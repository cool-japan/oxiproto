#![forbid(unsafe_code)]

//! `lint` subcommand — style and convention checks on `.proto` files
//! (buf-compatible subset: Google style guide rules applied from the FDS).

use clap::Args;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// CLI args and violation type
// ---------------------------------------------------------------------------

/// Arguments for the `lint` subcommand.
#[derive(Args)]
pub struct LintArgs {
    /// Input .proto files to lint.
    #[arg(value_name = "PROTO_FILE", required = true)]
    pub protos: Vec<PathBuf>,
    /// Include paths for import resolution.
    #[arg(short = 'I', long = "include")]
    pub include: Vec<PathBuf>,
    /// Output format: "text" (default) or "json".
    #[arg(long, default_value = "text")]
    pub output: String,
}

/// A single lint violation found in a proto file.
pub struct LintViolation {
    /// The proto file name (as it appears in the FDS).
    pub file: String,
    /// Line number (0 if unknown).
    pub line: u32,
    /// Rule identifier (e.g. `MESSAGE_NAMES_UPPER_CAMEL_CASE`).
    pub rule: String,
    /// Human-readable violation message.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run the `lint` subcommand.
///
/// Compiles all input `.proto` files to a `FileDescriptorSet`, walks every
/// message/enum/service in every non-WKT file, and applies the naming-
/// convention rules.  Prints violations in `text` or `json` format, then
/// returns an error if any violations were found.
///
/// # Errors
///
/// Returns an error if compilation fails or if any violations are found.
pub fn run(
    args: LintArgs,
    _verbosity: crate::util::Verbosity,
) -> Result<(), Box<dyn std::error::Error>> {
    for p in &args.protos {
        if !p.exists() {
            return Err(format!("proto file not found: {}", p.display()).into());
        }
    }

    let fds = oxiproto_build::compile_to_fds(&args.protos, &args.include)?;

    let mut violations: Vec<LintViolation> = Vec::new();

    for file in &fds.file {
        let fname = file.name.as_deref().unwrap_or("");
        // Skip well-known types.
        if fname.starts_with("google/protobuf/") {
            continue;
        }
        lint_file(file, fname, &mut violations);
    }

    match args.output.as_str() {
        "json" => print_json(&violations),
        _ => print_text(&violations),
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!("{} lint violation(s) found", violations.len()).into())
    }
}

// ---------------------------------------------------------------------------
// File-level linting
// ---------------------------------------------------------------------------

fn lint_file(file: &prost_types::FileDescriptorProto, fname: &str, out: &mut Vec<LintViolation>) {
    // Top-level messages.
    for msg in &file.message_type {
        lint_message(msg, fname, out);
    }

    // Top-level enums.
    for en in &file.enum_type {
        lint_enum(en, fname, out);
    }

    // Services.
    for svc in &file.service {
        lint_service(svc, fname, out);
    }
}

// ---------------------------------------------------------------------------
// Message linting (recursive)
// ---------------------------------------------------------------------------

fn lint_message(msg: &prost_types::DescriptorProto, fname: &str, out: &mut Vec<LintViolation>) {
    // Skip synthetic map-entry messages.
    if msg.options.as_ref().is_some_and(|o| o.map_entry()) {
        return;
    }

    let name = msg.name.as_deref().unwrap_or("");

    // Rule 1: MESSAGE_NAMES_UPPER_CAMEL_CASE
    if !is_upper_camel_case(name) {
        out.push(LintViolation {
            file: fname.to_owned(),
            line: 0,
            rule: "MESSAGE_NAMES_UPPER_CAMEL_CASE".to_owned(),
            message: format!(
                "message name '{name}' must be UpperCamelCase (first char uppercase, no underscores)"
            ),
        });
    }

    // Rule 2: FIELD_NAMES_LOWER_SNAKE_CASE (skip fields in map-entry messages)
    for field in &msg.field {
        let fname_field = field.name.as_deref().unwrap_or("");
        if !is_lower_snake_case(fname_field) {
            out.push(LintViolation {
                file: fname.to_owned(),
                line: 0,
                rule: "FIELD_NAMES_LOWER_SNAKE_CASE".to_owned(),
                message: format!(
                    "field name '{fname_field}' in message '{name}' must be lower_snake_case"
                ),
            });
        }
    }

    // Recurse into nested types.
    for nested in &msg.nested_type {
        lint_message(nested, fname, out);
    }

    // Recurse into nested enums.
    for en in &msg.enum_type {
        lint_enum(en, fname, out);
    }
}

// ---------------------------------------------------------------------------
// Enum linting
// ---------------------------------------------------------------------------

fn lint_enum(en: &prost_types::EnumDescriptorProto, fname: &str, out: &mut Vec<LintViolation>) {
    let name = en.name.as_deref().unwrap_or("");

    // Rule 3: ENUM_NAMES_UPPER_CAMEL_CASE
    if !is_upper_camel_case(name) {
        out.push(LintViolation {
            file: fname.to_owned(),
            line: 0,
            rule: "ENUM_NAMES_UPPER_CAMEL_CASE".to_owned(),
            message: format!("enum name '{name}' must be UpperCamelCase"),
        });
    }

    let prefix = to_screaming_snake_case(name);

    for val in &en.value {
        let vname = val.name.as_deref().unwrap_or("");

        // Rule 4: ENUM_VALUE_NAMES_UPPER_SNAKE_CASE
        if !is_screaming_snake_case(vname) {
            out.push(LintViolation {
                file: fname.to_owned(),
                line: 0,
                rule: "ENUM_VALUE_NAMES_UPPER_SNAKE_CASE".to_owned(),
                message: format!(
                    "enum value '{vname}' in enum '{name}' must be SCREAMING_SNAKE_CASE"
                ),
            });
        }

        // Rule 5: ENUM_VALUE_PREFIX
        let expected_prefix = format!("{prefix}_");
        if !vname.starts_with(&expected_prefix) {
            out.push(LintViolation {
                file: fname.to_owned(),
                line: 0,
                rule: "ENUM_VALUE_PREFIX".to_owned(),
                message: format!(
                    "enum value '{vname}' must start with '{expected_prefix}' (prefix of enum '{name}')"
                ),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Service linting
// ---------------------------------------------------------------------------

fn lint_service(
    svc: &prost_types::ServiceDescriptorProto,
    fname: &str,
    out: &mut Vec<LintViolation>,
) {
    let name = svc.name.as_deref().unwrap_or("");

    // Rule 6: SERVICE_NAMES_UPPER_CAMEL_CASE
    if !is_upper_camel_case(name) {
        out.push(LintViolation {
            file: fname.to_owned(),
            line: 0,
            rule: "SERVICE_NAMES_UPPER_CAMEL_CASE".to_owned(),
            message: format!("service name '{name}' must be UpperCamelCase"),
        });
    }

    // Rule 7: RPC_NAMES_UPPER_CAMEL_CASE
    for method in &svc.method {
        let mname = method.name.as_deref().unwrap_or("");
        if !is_upper_camel_case(mname) {
            out.push(LintViolation {
                file: fname.to_owned(),
                line: 0,
                rule: "RPC_NAMES_UPPER_CAMEL_CASE".to_owned(),
                message: format!("rpc name '{mname}' in service '{name}' must be UpperCamelCase"),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Output printers
// ---------------------------------------------------------------------------

fn print_text(violations: &[LintViolation]) {
    for v in violations {
        if v.line > 0 {
            println!("{}:{}: [{}] {}", v.file, v.line, v.rule, v.message);
        } else {
            println!("{}: [{}] {}", v.file, v.rule, v.message);
        }
    }
}

fn print_json(violations: &[LintViolation]) {
    let arr: Vec<serde_json::Value> = violations
        .iter()
        .map(|v| {
            serde_json::json!({
                "file": v.file,
                "line": v.line,
                "rule": v.rule,
                "message": v.message,
            })
        })
        .collect();
    // serde_json serialisation of a Vec<Value> is infallible; use match to
    // avoid unwrap/expect in production code.
    match serde_json::to_string(&arr) {
        Ok(s) => println!("{s}"),
        Err(_) => println!("[]"),
    }
}

// ---------------------------------------------------------------------------
// Naming-convention helpers
// ---------------------------------------------------------------------------

/// `UpperCamelCase`: first char uppercase ASCII alpha, no underscores, all
/// alphanumeric ASCII (digits allowed after the first char).
pub fn is_upper_camel_case(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        None => false,
        Some(first) if !first.is_ascii_uppercase() => false,
        Some(_) => chars.all(|c| c.is_ascii_alphanumeric()),
    }
}

/// `lower_snake_case`: all lowercase ASCII, digits, and underscores; no
/// uppercase; no leading or trailing underscores; no consecutive underscores.
pub fn is_lower_snake_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.starts_with('_') || s.ends_with('_') {
        return false;
    }
    if s.contains("__") {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// `SCREAMING_SNAKE_CASE`: all uppercase ASCII, digits, and underscores.
pub fn is_screaming_snake_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Convert `UpperCamelCase` to `UPPER_SNAKE_CASE`.
///
/// Inserts an underscore before each uppercase letter that follows a lowercase
/// letter or digit, then uppercases the whole string.
pub fn to_screaming_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    let mut prev_lower = false;
    for ch in s.chars() {
        if ch.is_ascii_uppercase() {
            if prev_lower {
                result.push('_');
            }
            result.push(ch);
            prev_lower = false;
        } else if ch.is_ascii_lowercase() {
            result.push(ch.to_ascii_uppercase());
            prev_lower = true;
        } else {
            result.push(ch);
            prev_lower = false;
        }
    }
    result
}
