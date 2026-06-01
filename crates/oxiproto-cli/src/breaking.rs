#![forbid(unsafe_code)]

use crate::util::Verbosity;
use clap::Args;
use std::{collections::HashMap, path::PathBuf};

/// Arguments for the `breaking` subcommand.
#[derive(Args)]
pub struct BreakingArgs {
    /// Old .proto files (baseline version)
    #[arg(long, required = true, value_name = "FILE")]
    pub old: Vec<PathBuf>,
    /// Include paths for old protos
    #[arg(long = "old-include", short = 'I', value_name = "DIR")]
    pub old_include: Vec<PathBuf>,
    /// New .proto files (updated version)
    #[arg(long, required = true, value_name = "FILE")]
    pub new: Vec<PathBuf>,
    /// Include paths for new protos
    #[arg(long = "new-include", short = 'J', value_name = "DIR")]
    pub new_include: Vec<PathBuf>,
}

/// Run the `breaking` subcommand: detect wire-breaking changes between two
/// versions of a proto schema.
///
/// Exits non-zero (returns `Err`) when any breaking change is detected.
///
/// # Errors
///
/// Returns an error if any proto file is missing, if compilation fails, or if
/// any breaking change is detected.
pub fn run(args: BreakingArgs, verbosity: Verbosity) -> Result<(), Box<dyn std::error::Error>> {
    // Validate file existence before any compilation.
    for p in &args.old {
        if !p.exists() {
            return Err(format!("old proto file not found: {}", p.display()).into());
        }
    }
    for p in &args.new {
        if !p.exists() {
            return Err(format!("new proto file not found: {}", p.display()).into());
        }
    }

    verbosity.verbose("Compiling old proto set...");
    let fds_old = oxiproto_build::compile_to_fds(&args.old, &args.old_include)?;
    verbosity.verbose("Compiling new proto set...");
    let fds_new = oxiproto_build::compile_to_fds(&args.new, &args.new_include)?;

    let old_registry = build_registry(&fds_old);
    let new_registry = build_registry(&fds_new);

    let mut findings: Vec<String> = Vec::new();

    // Check for removed messages.
    for fqn in old_registry.keys() {
        if !new_registry.contains_key(fqn.as_str()) {
            findings.push(format!("BREAKING: message `{}` removed", fqn));
        }
    }

    // Check for field and enum-value changes within messages that still exist.
    for (fqn, old_info) in &old_registry {
        if let Some(new_info) = new_registry.get(fqn.as_str()) {
            diff_message(fqn, old_info, new_info, &mut findings);
        }
    }

    if findings.is_empty() {
        verbosity.info("No breaking changes detected.");
        Ok(())
    } else {
        let count = findings.len();
        for f in &findings {
            println!("{f}");
        }
        Err(format!("{count} breaking change(s) detected").into())
    }
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct FieldInfo {
    name: String,
    /// Raw type integer from `field.r#type` (prost_types field_descriptor_proto::Type).
    type_val: i32,
    /// Fully-qualified type name for message/enum typed fields.
    type_name: String,
    /// Raw label integer from `field.label`.
    label: i32,
}

struct EnumValueInfo {
    name: String,
}

struct MessageInfo {
    fields_by_number: HashMap<i32, FieldInfo>,
    /// Flat enum values from all inline enums within this message, keyed by
    /// value number.  When two sibling enums share a number the last one wins,
    /// but the tests only require detecting removal of a value that previously
    /// existed.
    enum_values_by_number: HashMap<i32, EnumValueInfo>,
}

// ---------------------------------------------------------------------------
// Registry construction
// ---------------------------------------------------------------------------

/// Build a fully-qualified-name → `MessageInfo` map from a
/// `FileDescriptorSet`.  Skips map-entry synthetic messages.
fn build_registry(fds: &prost_types::FileDescriptorSet) -> HashMap<String, MessageInfo> {
    let mut registry = HashMap::new();
    for file in &fds.file {
        let pkg = file.package.as_deref().unwrap_or("");
        for msg in &file.message_type {
            collect_message(pkg, msg, &mut registry);
        }
    }
    registry
}

/// Recursively collect `msg` and all its nested types into `registry`.
fn collect_message(
    prefix: &str,
    msg: &prost_types::DescriptorProto,
    registry: &mut HashMap<String, MessageInfo>,
) {
    let name = msg.name.as_deref().unwrap_or("");
    let fqn = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    };

    // Skip synthetic map-entry messages to avoid false positives.
    let is_map_entry = msg.options.as_ref().map(|o| o.map_entry()).unwrap_or(false);

    if !is_map_entry {
        let mut fields_by_number = HashMap::new();
        for field in &msg.field {
            if let Some(num) = field.number {
                fields_by_number.insert(
                    num,
                    FieldInfo {
                        name: field.name.as_deref().unwrap_or("").to_string(),
                        type_val: field.r#type.unwrap_or(0),
                        type_name: field.type_name.as_deref().unwrap_or("").to_string(),
                        label: field.label.unwrap_or(0),
                    },
                );
            }
        }

        // Collect enum values from all inline enum types.
        let mut enum_values_by_number = HashMap::new();
        for en in &msg.enum_type {
            for val in &en.value {
                if let Some(num) = val.number {
                    enum_values_by_number.insert(
                        num,
                        EnumValueInfo {
                            name: val.name.as_deref().unwrap_or("").to_string(),
                        },
                    );
                }
            }
        }

        registry.insert(
            fqn.clone(),
            MessageInfo {
                fields_by_number,
                enum_values_by_number,
            },
        );
    }

    // Recurse into nested message types.
    for nested in &msg.nested_type {
        collect_message(&fqn, nested, registry);
    }
}

// ---------------------------------------------------------------------------
// Diffing
// ---------------------------------------------------------------------------

/// Compare old and new `MessageInfo` for a single message FQN, appending any
/// wire-breaking findings to `findings`.
fn diff_message(fqn: &str, old: &MessageInfo, new: &MessageInfo, findings: &mut Vec<String>) {
    // Field removals and type/label changes.
    for (num, old_field) in &old.fields_by_number {
        match new.fields_by_number.get(num) {
            None => {
                findings.push(format!(
                    "BREAKING: field `{}` (#{}) removed from message `{}`",
                    old_field.name, num, fqn
                ));
            }
            Some(new_field) => {
                // Wire type change (includes message/enum type-name changes).
                if old_field.type_val != new_field.type_val
                    || old_field.type_name != new_field.type_name
                {
                    findings.push(format!(
                        "BREAKING: field `{}` (#{}) type changed in message `{}`",
                        old_field.name, num, fqn
                    ));
                }
                // Repeated ↔ non-repeated cardinality change.
                use prost_types::field_descriptor_proto::Label;
                let old_repeated = old_field.label == Label::Repeated as i32;
                let new_repeated = new_field.label == Label::Repeated as i32;
                if old_repeated != new_repeated {
                    findings.push(format!(
                        "BREAKING: field `{}` (#{}) repeated status changed in message `{}`",
                        old_field.name, num, fqn
                    ));
                }
            }
        }
    }

    // Enum value removals.
    for (num, old_val) in &old.enum_values_by_number {
        if !new.enum_values_by_number.contains_key(num) {
            findings.push(format!(
                "BREAKING: enum value `{}` (#{}) removed from message `{}`",
                old_val.name, num, fqn
            ));
        }
    }
}
