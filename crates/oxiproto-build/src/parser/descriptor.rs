#![forbid(unsafe_code)]

//! Convert a resolved [`ProtoFile`] AST into a [`prost_types::FileDescriptorSet`].
//!
//! The `proto_file` passed in must have already been through [`super::resolve::resolve`]
//! so that all `FieldType::Named(s)` strings already carry leading-dot FQNs.

use std::collections::{HashMap, HashSet};

use prost_types::{
    field_descriptor_proto::{Label, Type},
    uninterpreted_option::NamePart,
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FileDescriptorProto, FileDescriptorSet, MessageOptions, MethodDescriptorProto,
    OneofDescriptorProto, ServiceDescriptorProto, UninterpretedOption,
};

use crate::parser::ast::{
    Enum, EnumValue, ExtendBlock, Field, FieldLabel, FieldType, ImportModifier, Message,
    OptionValue, ProtoFile, ProtoOption, Reserved, ReservedRangeTo, ScalarType, Service,
};

#[cfg(feature = "native-parser")]
use crate::parser::{comments::CommentMap, span::LineTable};
#[cfg(feature = "native-parser")]
use prost_types::source_code_info::Location;

// ---------------------------------------------------------------------------
// Option value helper
// ---------------------------------------------------------------------------

/// Extract a boolean from an `OptionValue`, accepting both `Bool` and
/// `Ident("true"/"false")` variants (since the parser may emit either).
fn option_bool(val: &OptionValue) -> Option<bool> {
    match val {
        OptionValue::Bool(b) => Some(*b),
        OptionValue::Ident(s) if s == "true" => Some(true),
        OptionValue::Ident(s) if s == "false" => Some(false),
        _ => None,
    }
}

/// Extract the json_name override from field options if present.
fn extract_json_name_override(options: &[ProtoOption]) -> Option<String> {
    for opt in options {
        if opt.name == "json_name" {
            if let OptionValue::Str(s) = &opt.value {
                return Some(s.clone());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Field options builder
// ---------------------------------------------------------------------------

fn build_field_options(options: &[ProtoOption]) -> Option<prost_types::FieldOptions> {
    let mut deprecated = None;
    let mut packed = None;
    let mut uninterpreted: Vec<UninterpretedOption> = Vec::new();
    for opt in options {
        match opt.name.as_str() {
            "deprecated" => {
                deprecated = option_bool(&opt.value);
            }
            "packed" => {
                packed = option_bool(&opt.value);
            }
            // "json_name" and "default" are handled separately — not option proto fields.
            "json_name" | "default" => {}
            _ => {
                if let Some(uninterp) = build_uninterpreted_option(opt) {
                    uninterpreted.push(uninterp);
                }
            }
        }
    }
    if deprecated.is_some() || packed.is_some() || !uninterpreted.is_empty() {
        Some(prost_types::FieldOptions {
            deprecated,
            packed,
            uninterpreted_option: uninterpreted,
            ..Default::default()
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Message-literal aggregate_value formatting
// ---------------------------------------------------------------------------

/// Format a message-literal option value as a protobuf text-proto string.
///
/// The output follows the text-proto convention used by `aggregate_value` in
/// `UninterpretedOption`: fields separated by spaces, strings double-quoted
/// with backslash escaping, nested messages recursively formatted.
fn format_aggregate_value(pairs: &[(String, OptionValue)]) -> String {
    let mut out = String::from("{ ");
    for (key, val) in pairs {
        out.push_str(key);
        out.push_str(": ");
        format_option_value_into(val, &mut out);
        out.push(' ');
    }
    out.push('}');
    out
}

/// Append a single `OptionValue` in text-proto format to `buf`.
fn format_option_value_into(val: &OptionValue, buf: &mut String) {
    match val {
        OptionValue::Str(s) => {
            buf.push('"');
            for ch in s.chars() {
                match ch {
                    '\\' => buf.push_str("\\\\"),
                    '"' => buf.push_str("\\\""),
                    '\n' => buf.push_str("\\n"),
                    '\r' => buf.push_str("\\r"),
                    '\t' => buf.push_str("\\t"),
                    c => buf.push(c),
                }
            }
            buf.push('"');
        }
        OptionValue::Int(n) => buf.push_str(&n.to_string()),
        OptionValue::Float(f) => buf.push_str(&format!("{f}")),
        OptionValue::Bool(b) => buf.push_str(if *b { "true" } else { "false" }),
        OptionValue::Ident(s) => buf.push_str(s),
        OptionValue::MessageLiteral(pairs) => {
            buf.push_str(&format_aggregate_value(pairs));
        }
    }
}

/// Parse a proto option name string into a list of `NamePart` entries.
///
/// Handles the `(extension.name).plain_field` syntax correctly:
/// - The portion inside `(...)` is one extension `NamePart` with `is_extension=true`.
///   The dotted name inside the parens is kept as-is (e.g. `"my.pkg.option"`).
/// - Plain dotted segments outside parens become individual `NamePart`s with
///   `is_extension=false`.
///
/// Examples:
/// - `"deprecated"` → `[{name="deprecated", ext=false}]`
/// - `"(my.fake_option)"` → `[{name="my.fake_option", ext=true}]`
/// - `"(my.ext).field"` → `[{name="my.ext", ext=true}, {name="field", ext=false}]`
fn parse_option_name_parts(name: &str) -> Vec<NamePart> {
    let mut parts = Vec::new();
    let mut remaining = name;

    while !remaining.is_empty() {
        // Skip leading dots between parts
        if remaining.starts_with('.') {
            remaining = &remaining[1..];
            continue;
        }

        if remaining.starts_with('(') {
            // Find the matching closing paren
            if let Some(close) = remaining.find(')') {
                let inner = &remaining[1..close];
                parts.push(NamePart {
                    name_part: inner.to_owned(),
                    is_extension: true,
                });
                remaining = &remaining[close + 1..];
            } else {
                // Malformed — no closing paren; treat whole remainder as one part
                parts.push(NamePart {
                    name_part: remaining.trim_matches('(').to_owned(),
                    is_extension: true,
                });
                break;
            }
        } else {
            // Plain segment — consume until next dot or paren
            let end = remaining.find(['.', '(']).unwrap_or(remaining.len());
            let seg = &remaining[..end];
            if !seg.is_empty() {
                parts.push(NamePart {
                    name_part: seg.to_owned(),
                    is_extension: false,
                });
            }
            remaining = &remaining[end..];
        }
    }

    parts
}

/// Build an `UninterpretedOption` from a `ProtoOption` whose value is a
/// `MessageLiteral`.  Returns `None` if the value is not a `MessageLiteral`.
fn build_uninterpreted_option_from_literal(opt: &ProtoOption) -> Option<UninterpretedOption> {
    if let OptionValue::MessageLiteral(pairs) = &opt.value {
        let aggregate = format_aggregate_value(pairs);
        let name_parts = parse_option_name_parts(&opt.name);
        Some(UninterpretedOption {
            name: name_parts,
            aggregate_value: Some(aggregate),
            ..Default::default()
        })
    } else {
        None
    }
}

/// Build an `UninterpretedOption` from a `ProtoOption` with a scalar value.
///
/// NOTE: protox errors on undefined extension options (e.g. `(my.fake_option) = true`)
/// because its interpretation phase requires all extension names to be resolved.
/// The native parser does not have an interpretation phase — it preserves scalar
/// custom options as `uninterpreted_option` entries using the appropriate typed
/// value field, which is internally consistent with how it handles message-literal
/// custom options.
///
/// Maps `OptionValue` variants to `UninterpretedOption` typed fields:
/// - `Ident(s)` → `identifier_value`
/// - `Bool(b)` → `identifier_value = "true"/"false"`
/// - `Int(n >= 0)` → `positive_int_value`
/// - `Int(n < 0)` → `negative_int_value` (absolute value)
/// - `Float(f)` → `double_value`
/// - `Str(s)` → `string_value` (UTF-8 bytes)
/// - `MessageLiteral` → not handled here (use `build_uninterpreted_option_from_literal`)
fn build_uninterpreted_option_from_scalar(opt: &ProtoOption) -> Option<UninterpretedOption> {
    let name_parts = parse_option_name_parts(&opt.name);
    let mut u = UninterpretedOption {
        name: name_parts,
        ..Default::default()
    };
    match &opt.value {
        OptionValue::Ident(s) => {
            u.identifier_value = Some(s.clone());
        }
        OptionValue::Bool(b) => {
            u.identifier_value = Some(if *b {
                "true".to_owned()
            } else {
                "false".to_owned()
            });
        }
        OptionValue::Int(n) if *n >= 0 => {
            u.positive_int_value = Some(*n as u64);
        }
        OptionValue::Int(n) => {
            // Negative integer: store absolute value in negative_int_value (i64).
            u.negative_int_value = Some(n.unsigned_abs() as i64);
        }
        OptionValue::Float(f) => {
            u.double_value = Some(*f);
        }
        OptionValue::Str(s) => {
            u.string_value = Some(s.as_bytes().to_vec());
        }
        OptionValue::MessageLiteral(_) => {
            // Not handled here — caller should use build_uninterpreted_option_from_literal.
            return None;
        }
    }
    Some(u)
}

// ---------------------------------------------------------------------------
// Message options builder
// ---------------------------------------------------------------------------

/// Build an `UninterpretedOption` for any option not handled as a known
/// interpreted option.  Tries `MessageLiteral` first, then scalar.
fn build_uninterpreted_option(opt: &ProtoOption) -> Option<UninterpretedOption> {
    build_uninterpreted_option_from_literal(opt)
        .or_else(|| build_uninterpreted_option_from_scalar(opt))
}

/// Build `MessageOptions` from user-declared message options.
///
/// Does NOT set `map_entry` — that is done separately by the map-entry
/// synthesis path. When merging with a pre-existing `MessageOptions` that
/// carries `map_entry: Some(true)`, use `merge_message_options`.
fn build_message_options(options: &[ProtoOption]) -> Option<prost_types::MessageOptions> {
    let mut deprecated = None;
    let mut uninterpreted: Vec<UninterpretedOption> = Vec::new();
    for opt in options {
        if opt.name == "deprecated" {
            deprecated = option_bool(&opt.value);
        } else if let Some(uninterp) = build_uninterpreted_option(opt) {
            uninterpreted.push(uninterp);
        }
    }
    if deprecated.is_some() || !uninterpreted.is_empty() {
        Some(prost_types::MessageOptions {
            deprecated,
            uninterpreted_option: uninterpreted,
            ..Default::default()
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Enum options builder
// ---------------------------------------------------------------------------

fn build_enum_options(options: &[ProtoOption]) -> Option<prost_types::EnumOptions> {
    let mut deprecated = None;
    let mut allow_alias = None;
    let mut uninterpreted: Vec<UninterpretedOption> = Vec::new();
    for opt in options {
        match opt.name.as_str() {
            "deprecated" => {
                deprecated = option_bool(&opt.value);
            }
            "allow_alias" => {
                allow_alias = option_bool(&opt.value);
            }
            _ => {
                if let Some(uninterp) = build_uninterpreted_option(opt) {
                    uninterpreted.push(uninterp);
                }
            }
        }
    }
    if deprecated.is_some() || allow_alias.is_some() || !uninterpreted.is_empty() {
        Some(prost_types::EnumOptions {
            deprecated,
            allow_alias,
            uninterpreted_option: uninterpreted,
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Service options builder
// ---------------------------------------------------------------------------

fn build_service_options(options: &[ProtoOption]) -> Option<prost_types::ServiceOptions> {
    let mut deprecated = None;
    let mut uninterpreted: Vec<UninterpretedOption> = Vec::new();
    for opt in options {
        if opt.name == "deprecated" {
            deprecated = option_bool(&opt.value);
        } else if let Some(uninterp) = build_uninterpreted_option(opt) {
            uninterpreted.push(uninterp);
        }
    }
    if deprecated.is_some() || !uninterpreted.is_empty() {
        Some(prost_types::ServiceOptions {
            deprecated,
            uninterpreted_option: uninterpreted,
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Method options builder
// ---------------------------------------------------------------------------

fn build_method_options(options: &[ProtoOption]) -> Option<prost_types::MethodOptions> {
    let mut deprecated = None;
    let mut uninterpreted: Vec<UninterpretedOption> = Vec::new();
    for opt in options {
        if opt.name == "deprecated" {
            deprecated = option_bool(&opt.value);
        } else if let Some(uninterp) = build_uninterpreted_option(opt) {
            uninterpreted.push(uninterp);
        }
    }
    if deprecated.is_some() || !uninterpreted.is_empty() {
        Some(prost_types::MethodOptions {
            deprecated,
            uninterpreted_option: uninterpreted,
            ..Default::default()
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// File options builder
// ---------------------------------------------------------------------------

fn build_file_options(options: &[ProtoOption]) -> Option<prost_types::FileOptions> {
    let mut java_package = None;
    let mut java_outer_classname = None;
    let mut go_package = None;
    let mut java_multiple_files = None;
    let mut optimize_for = None;
    let mut deprecated = None;
    let mut uninterpreted: Vec<UninterpretedOption> = Vec::new();

    for opt in options {
        match opt.name.as_str() {
            "java_package" => {
                if let OptionValue::Str(s) = &opt.value {
                    java_package = Some(s.clone());
                }
            }
            "java_outer_classname" => {
                if let OptionValue::Str(s) = &opt.value {
                    java_outer_classname = Some(s.clone());
                }
            }
            "go_package" => {
                if let OptionValue::Str(s) = &opt.value {
                    go_package = Some(s.clone());
                }
            }
            "java_multiple_files" => {
                java_multiple_files = option_bool(&opt.value);
            }
            "optimize_for" => {
                if let OptionValue::Int(n) = &opt.value {
                    optimize_for = Some(*n as i32);
                }
            }
            "deprecated" => {
                deprecated = option_bool(&opt.value);
            }
            _ => {
                if let Some(uninterp) = build_uninterpreted_option(opt) {
                    uninterpreted.push(uninterp);
                }
            }
        }
    }

    if java_package.is_some()
        || java_outer_classname.is_some()
        || go_package.is_some()
        || java_multiple_files.is_some()
        || optimize_for.is_some()
        || deprecated.is_some()
        || !uninterpreted.is_empty()
    {
        Some(prost_types::FileOptions {
            java_package,
            java_outer_classname,
            go_package,
            java_multiple_files,
            optimize_for,
            deprecated,
            uninterpreted_option: uninterpreted,
            ..Default::default()
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Reserved ranges/names builders
// ---------------------------------------------------------------------------

/// Build `(reserved_range, reserved_name)` for a **message** descriptor.
///
/// Message reserved ranges use **exclusive** end: `reserved 2 to 5` means
/// field numbers 2, 3, 4, 5 — stored as `[start=2, end=6)`.
/// A bare `reserved 2;` (single number) becomes `[start=2, end=3)`.
/// The `max` keyword maps to 536,870,912 (MAX_FIELD_NUMBER + 1).
fn build_message_reserved(
    reserved: &[Reserved],
) -> (
    Vec<prost_types::descriptor_proto::ReservedRange>,
    Vec<String>,
) {
    let mut ranges = Vec::new();
    let mut names = Vec::new();
    for r in reserved {
        match r {
            Reserved::Ranges(range_vec) => {
                for range in range_vec {
                    let end = match range.to {
                        ReservedRangeTo::Number(n) => n + 1,
                        ReservedRangeTo::Max => 536_870_912,
                    };
                    ranges.push(prost_types::descriptor_proto::ReservedRange {
                        start: Some(range.from),
                        end: Some(end),
                    });
                }
            }
            Reserved::Names(name_vec) => {
                names.extend(name_vec.iter().cloned());
            }
        }
    }
    (ranges, names)
}

/// Build `(reserved_range, reserved_name)` for an **enum** descriptor.
///
/// Enum reserved ranges use **inclusive** end (`EnumReservedRange` comment
/// says "inclusive such that it can appropriately represent the entire int32
/// domain").  A bare `reserved 0;` becomes `[start=0, end=0]`.
/// The `max` keyword maps to `i32::MAX`.
fn build_enum_reserved(
    reserved: &[Reserved],
) -> (
    Vec<prost_types::enum_descriptor_proto::EnumReservedRange>,
    Vec<String>,
) {
    let mut ranges = Vec::new();
    let mut names = Vec::new();
    for r in reserved {
        match r {
            Reserved::Ranges(range_vec) => {
                for range in range_vec {
                    let end = match range.to {
                        ReservedRangeTo::Number(n) => n,
                        ReservedRangeTo::Max => i32::MAX,
                    };
                    ranges.push(prost_types::enum_descriptor_proto::EnumReservedRange {
                        start: Some(range.from),
                        end: Some(end),
                    });
                }
            }
            Reserved::Names(name_vec) => {
                names.extend(name_vec.iter().cloned());
            }
        }
    }
    (ranges, names)
}

// ---------------------------------------------------------------------------
// Default value helper
// ---------------------------------------------------------------------------

/// Extract a formatted `default_value` string from field options if a
/// `"default"` option is present.  Returns `None` when no `default` option
/// exists or when `is_message_type` is true (message fields never carry
/// default values).
fn extract_default_value(options: &[ProtoOption], is_message_type: bool) -> Option<String> {
    if is_message_type {
        return None;
    }
    for opt in options {
        if opt.name == "default" {
            let formatted = match &opt.value {
                OptionValue::Str(s) => s.clone(),
                OptionValue::Ident(s) => s.clone(),
                OptionValue::Int(n) => n.to_string(),
                OptionValue::Float(f) => format!("{f}"),
                OptionValue::Bool(b) => if *b { "true" } else { "false" }.to_owned(),
                // Message literals are not valid as default values; skip them.
                OptionValue::MessageLiteral(_) => continue,
            };
            return Some(formatted);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Extension range builder
// ---------------------------------------------------------------------------

fn build_extension_ranges(
    extensions: &[crate::parser::ast::ExtensionRange],
) -> Vec<prost_types::descriptor_proto::ExtensionRange> {
    extensions
        .iter()
        .map(|er| {
            let proto_start = er.start as i32;
            // end is inclusive in the AST, but proto stores exclusive end.
            // bare number (None) → treat as single-value range [start, start+1)
            let proto_end = er.end.map(|e| e as i32 + 1).unwrap_or(er.start as i32 + 1);
            prost_types::descriptor_proto::ExtensionRange {
                start: Some(proto_start),
                end: Some(proto_end),
                options: None,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Extension field builder (for top-level extend blocks)
// ---------------------------------------------------------------------------

/// Fully-qualify an extendee name: prepend `.package.` if it doesn't start
/// with `.`.
fn fully_qualify_extendee(extendee: &str, pkg: Option<&str>) -> String {
    if extendee.starts_with('.') {
        extendee.to_owned()
    } else if let Some(p) = pkg {
        if p.is_empty() {
            format!(".{extendee}")
        } else {
            format!(".{p}.{extendee}")
        }
    } else {
        format!(".{extendee}")
    }
}

fn build_extension_fields(
    extends: &[ExtendBlock],
    pkg: Option<&str>,
    enum_fqns: &HashSet<String>,
    is_proto2: bool,
) -> Vec<FieldDescriptorProto> {
    let mut result = Vec::new();
    for eb in extends {
        let extendee_fqn = fully_qualify_extendee(&eb.extendee, pkg);
        for field in &eb.fields {
            let (proto_type, type_name) = field_type_to_proto_kind(&field.ty, enum_fqns);
            let is_msg = matches!(field.ty, FieldType::Named(_));
            let default_value = extract_default_value(&field.options, is_msg);
            let json_name = extract_json_name_override(&field.options)
                .unwrap_or_else(|| snake_to_camel_case(&field.name));
            let label = match &field.label {
                FieldLabel::Required => Label::Required as i32,
                FieldLabel::Repeated => Label::Repeated as i32,
                FieldLabel::Optional | FieldLabel::Singular => Label::Optional as i32,
            };
            // In proto2, optional extension fields do NOT get synthetic oneofs.
            // Extensions are never in a oneof.
            let _ = is_proto2; // used conceptually (no oneof for extension fields)
            let fdp = FieldDescriptorProto {
                name: Some(field.name.clone()),
                number: Some(field.number),
                label: Some(label),
                r#type: Some(proto_type as i32),
                type_name,
                extendee: Some(extendee_fqn.clone()),
                default_value,
                oneof_index: None,
                json_name: Some(json_name),
                options: build_field_options(&field.options),
                proto3_optional: None,
            };
            result.push(fdp);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Convert a resolved [`ProtoFile`] to a [`FileDescriptorSet`] containing
/// exactly one [`FileDescriptorProto`].
///
/// `src` is the original source string used to produce `proto_file`.  It is
/// used to populate `source_code_info` when the `native-parser` feature is
/// enabled.  Pass `""` to get `source_code_info: None`.
pub fn build_file_descriptor_set(
    proto_file: &ProtoFile,
    file_name: &str,
    src: &str,
) -> FileDescriptorSet {
    let fdp = build_file_descriptor_proto(proto_file, file_name, src);
    FileDescriptorSet { file: vec![fdp] }
}

// ---------------------------------------------------------------------------
// File-level builder
// ---------------------------------------------------------------------------

fn build_file_descriptor_proto(
    proto_file: &ProtoFile,
    file_name: &str,
    src: &str,
) -> FileDescriptorProto {
    let pkg = proto_file.package.as_deref().unwrap_or("");
    let is_proto2 = proto_file.syntax.as_deref() == Some("proto2");

    // Build enum FQN set for type-kind disambiguation.
    let mut enum_fqns: HashSet<String> = HashSet::new();
    for en in &proto_file.enums {
        collect_enum_fqns_top(en, pkg, &mut enum_fqns);
    }
    for msg in &proto_file.messages {
        collect_enum_fqns_message(msg, pkg, &[], &mut enum_fqns);
    }

    // Build top-level messages.
    let message_type: Vec<DescriptorProto> = proto_file
        .messages
        .iter()
        .map(|msg| build_message_descriptor(msg, pkg, &[], &enum_fqns, is_proto2))
        .collect();

    // Build top-level enums.
    let enum_type: Vec<EnumDescriptorProto> =
        proto_file.enums.iter().map(build_enum_descriptor).collect();

    // Build services.
    let service: Vec<ServiceDescriptorProto> = proto_file
        .services
        .iter()
        .map(build_service_descriptor)
        .collect();

    // Build file-level extension fields from top-level extend blocks.
    let extension = build_extension_fields(
        &proto_file.extends,
        proto_file.package.as_deref(),
        &enum_fqns,
        is_proto2,
    );

    // Dependencies from imports.
    let dependency: Vec<String> = proto_file
        .imports
        .iter()
        .map(|imp| imp.path.clone())
        .collect();

    let mut public_dependency: Vec<i32> = Vec::new();
    let mut weak_dependency: Vec<i32> = Vec::new();
    for (i, imp) in proto_file.imports.iter().enumerate() {
        match imp.modifier {
            ImportModifier::Public => public_dependency.push(i as i32),
            ImportModifier::Weak => weak_dependency.push(i as i32),
            ImportModifier::None => {}
        }
    }

    // Build source_code_info when native-parser feature is active and src
    // is non-empty.
    #[cfg(feature = "native-parser")]
    let source_code_info = if src.is_empty() {
        None
    } else {
        let comments = CommentMap::extract(src);
        let line_table = LineTable::build(src);
        Some(build_source_code_info(
            proto_file,
            src,
            &comments,
            &line_table,
        ))
    };

    #[cfg(not(feature = "native-parser"))]
    let source_code_info = {
        let _ = src;
        None
    };

    FileDescriptorProto {
        name: Some(file_name.to_owned()),
        package: proto_file.package.clone(),
        dependency,
        public_dependency,
        weak_dependency,
        message_type,
        enum_type,
        service,
        extension,
        options: build_file_options(&proto_file.options),
        source_code_info,
        syntax: proto_file.syntax.clone(),
    }
}

// ---------------------------------------------------------------------------
// Enum FQN collection helpers
// ---------------------------------------------------------------------------

fn collect_enum_fqns_top(en: &Enum, pkg: &str, out: &mut HashSet<String>) {
    let fqn = make_fqn(pkg, &[], &en.name);
    out.insert(fqn);
}

fn collect_enum_fqns_message(msg: &Message, pkg: &str, scope: &[&str], out: &mut HashSet<String>) {
    let mut inner: Vec<&str> = scope.to_vec();
    inner.push(&msg.name);
    for en in &msg.nested_enums {
        let fqn = make_fqn(pkg, &inner, &en.name);
        out.insert(fqn);
    }
    for nested in &msg.nested_messages {
        collect_enum_fqns_message(nested, pkg, &inner, out);
    }
}

// ---------------------------------------------------------------------------
// FQN helpers
// ---------------------------------------------------------------------------

fn make_fqn(pkg: &str, scope: &[&str], name: &str) -> String {
    let mut fqn = String::new();
    fqn.push('.');
    if !pkg.is_empty() {
        fqn.push_str(pkg);
        fqn.push('.');
    }
    for part in scope {
        fqn.push_str(part);
        fqn.push('.');
    }
    fqn.push_str(name);
    fqn
}

// ---------------------------------------------------------------------------
// json_name helper
// ---------------------------------------------------------------------------

/// Capitalize the first letter of `s`, leaving the rest unchanged.
/// Used to recover the group name from the lowercased field name.
/// `"result"` → `"Result"`, `"inner"` → `"Inner"`.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => {
            let upper: String = c.to_uppercase().collect();
            upper + chars.as_str()
        }
    }
}

/// Convert a snake_case field name to camelCase json_name.
/// `"my_field"` → `"myField"`, `"id"` → `"id"`, `"user_id"` → `"userId"`.
fn snake_to_camel_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;
    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            for uc in ch.to_uppercase() {
                result.push(uc);
            }
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Scalar type mapping
// ---------------------------------------------------------------------------

fn scalar_to_proto_type(st: ScalarType) -> Type {
    match st {
        ScalarType::Double => Type::Double,
        ScalarType::Float => Type::Float,
        ScalarType::Int32 => Type::Int32,
        ScalarType::Int64 => Type::Int64,
        ScalarType::Uint32 => Type::Uint32,
        ScalarType::Uint64 => Type::Uint64,
        ScalarType::Sint32 => Type::Sint32,
        ScalarType::Sint64 => Type::Sint64,
        ScalarType::Fixed32 => Type::Fixed32,
        ScalarType::Fixed64 => Type::Fixed64,
        ScalarType::Sfixed32 => Type::Sfixed32,
        ScalarType::Sfixed64 => Type::Sfixed64,
        ScalarType::Bool => Type::Bool,
        ScalarType::String => Type::String,
        ScalarType::Bytes => Type::Bytes,
    }
}

// ---------------------------------------------------------------------------
// Entry message name helper  (e.g. "my_map" → "MyMapEntry")
// ---------------------------------------------------------------------------

/// Convert a snake_case field name to PascalCase then append "Entry".
/// `"counts"` → `"CountsEntry"`, `"my_map"` → `"MyMapEntry"`.
fn map_entry_message_name(field_name: &str) -> String {
    let mut result = String::with_capacity(field_name.len() + 5 + 1);
    let mut capitalize_next = true;
    for ch in field_name.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            for uc in ch.to_uppercase() {
                result.push(uc);
            }
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result.push_str("Entry");
    result
}

// ---------------------------------------------------------------------------
// Message descriptor builder
// ---------------------------------------------------------------------------

/// Build a [`DescriptorProto`] for `msg`.
///
/// `scope` is the list of ancestor message names (for FQN construction of
/// nested map-entry type_names).
/// `is_proto2` controls whether `optional` fields get a synthetic oneof.
fn build_message_descriptor(
    msg: &Message,
    pkg: &str,
    scope: &[&str],
    enum_fqns: &HashSet<String>,
    is_proto2: bool,
) -> DescriptorProto {
    // Scope INSIDE this message (used for map entry FQNs and nested recursion).
    let mut inner_scope: Vec<&str> = scope.to_vec();
    inner_scope.push(&msg.name);

    // --- Pass 1: build real oneofs ---
    let mut oneof_decl: Vec<OneofDescriptorProto> = msg
        .oneofs
        .iter()
        .map(|o| OneofDescriptorProto {
            name: Some(o.name.clone()),
            options: None,
        })
        .collect();

    // --- Pass 2: assemble fields in source declaration order ---
    //
    // The AST stores regular fields and oneof blocks in separate Vecs, losing
    // the interleaving from the source file.  We restore source order by
    // sorting on `span.start` (byte offset in the source file).  This is
    // correct even when field numbers are non-monotonic.

    // Collect (span_start, FieldDescriptorProto) tuples, then sort.
    let mut field_entries: Vec<(usize, FieldDescriptorProto)> = Vec::new();
    // Track synthetic oneof count (appended after real oneofs).
    let mut synthetic_oneofs: Vec<OneofDescriptorProto> = Vec::new();
    // Nested descriptors for map entry messages.
    let mut extra_nested: Vec<DescriptorProto> = Vec::new();

    let real_oneof_count = msg.oneofs.len();

    // Regular fields (including map fields).
    for field in &msg.fields {
        let mut ctx = FieldBuildCtx {
            pkg,
            inner_scope: &inner_scope,
            enum_fqns,
            real_oneof_count,
            synthetic_oneofs: &mut synthetic_oneofs,
            extra_nested: &mut extra_nested,
            is_proto2,
        };
        let entries = build_regular_field(field, &mut ctx);
        field_entries.extend(entries);
    }

    // Oneof member fields.
    for (oneof_idx, oneof) in msg.oneofs.iter().enumerate() {
        for field in &oneof.fields {
            let fdp = build_oneof_member_field(field, oneof_idx, enum_fqns);
            field_entries.push((field.span.start, fdp));
        }
    }

    // Sort by byte offset (span.start) to restore source declaration order.
    field_entries.sort_by_key(|(start, _)| *start);
    let fields: Vec<FieldDescriptorProto> = field_entries.into_iter().map(|(_, f)| f).collect();

    // Append synthetic oneofs after real oneofs.
    oneof_decl.extend(synthetic_oneofs);

    // Nested message types: explicit nested messages + map entry messages.
    let mut nested_type: Vec<DescriptorProto> = msg
        .nested_messages
        .iter()
        .map(|n| build_message_descriptor(n, pkg, &inner_scope, enum_fqns, is_proto2))
        .collect();
    nested_type.extend(extra_nested);

    // Nested enum types.
    let enum_type: Vec<EnumDescriptorProto> =
        msg.nested_enums.iter().map(build_enum_descriptor).collect();

    let (reserved_range, reserved_name) = build_message_reserved(&msg.reserved);

    // Build extension_range from proto2 `extensions` statements.
    let extension_range = build_extension_ranges(&msg.extensions);

    DescriptorProto {
        name: Some(msg.name.clone()),
        field: fields,
        extension: vec![],
        nested_type,
        enum_type,
        extension_range,
        oneof_decl,
        options: build_message_options(&msg.options),
        reserved_range,
        reserved_name,
    }
}

// ---------------------------------------------------------------------------
// Field builders
// ---------------------------------------------------------------------------

/// Mutable context for building fields within a message.
struct FieldBuildCtx<'a> {
    pkg: &'a str,
    inner_scope: &'a [&'a str],
    enum_fqns: &'a HashSet<String>,
    real_oneof_count: usize,
    synthetic_oneofs: &'a mut Vec<OneofDescriptorProto>,
    extra_nested: &'a mut Vec<DescriptorProto>,
    is_proto2: bool,
}

/// Build the `FieldDescriptorProto`(s) for a regular (non-oneof-member) field.
///
/// Returns a Vec because map fields produce exactly one field entry (the
/// repeated synthetic field), but also produce a side-effect (map entry nested
/// message pushed into `extra_nested`).
///
/// `ctx.is_proto2` controls whether `optional` fields get a synthetic oneof
/// (proto3 behaviour) or plain `LABEL_OPTIONAL` without one (proto2 behaviour).
fn build_regular_field(
    field: &Field,
    ctx: &mut FieldBuildCtx<'_>,
) -> Vec<(usize, FieldDescriptorProto)> {
    let pkg = ctx.pkg;
    let inner_scope = ctx.inner_scope;
    let enum_fqns = ctx.enum_fqns;
    let real_oneof_count = ctx.real_oneof_count;
    let synthetic_oneofs = &mut ctx.synthetic_oneofs;
    let extra_nested = &mut ctx.extra_nested;
    let is_proto2 = ctx.is_proto2;
    match &field.ty {
        FieldType::Map { key, value } => {
            // Map field desugaring.
            let entry_name = map_entry_message_name(&field.name);
            // FQN for the entry message inside the current scope.
            let entry_fqn = make_fqn(pkg, inner_scope, &entry_name);

            // Build the entry message (key=1, value=2, map_entry=true).
            let key_fdp = FieldDescriptorProto {
                name: Some("key".to_owned()),
                number: Some(1),
                label: Some(Label::Optional as i32),
                r#type: Some(scalar_to_proto_type(*key) as i32),
                type_name: None,
                extendee: None,
                default_value: None,
                oneof_index: None,
                json_name: Some("key".to_owned()),
                options: None,
                proto3_optional: None,
            };
            let (value_type, value_type_name) = field_type_to_proto_kind(value.as_ref(), enum_fqns);
            let value_fdp = FieldDescriptorProto {
                name: Some("value".to_owned()),
                number: Some(2),
                label: Some(Label::Optional as i32),
                r#type: Some(value_type as i32),
                type_name: value_type_name,
                extendee: None,
                default_value: None,
                oneof_index: None,
                json_name: Some("value".to_owned()),
                options: None,
                proto3_optional: None,
            };

            let entry_msg = DescriptorProto {
                name: Some(entry_name.clone()),
                field: vec![key_fdp, value_fdp],
                extension: vec![],
                nested_type: vec![],
                enum_type: vec![],
                extension_range: vec![],
                oneof_decl: vec![],
                options: Some(MessageOptions {
                    message_set_wire_format: None,
                    no_standard_descriptor_accessor: None,
                    deprecated: None,
                    map_entry: Some(true),
                    uninterpreted_option: vec![],
                }),
                reserved_range: vec![],
                reserved_name: vec![],
            };
            extra_nested.push(entry_msg);

            // The map field itself becomes a repeated Message field.
            let json_name = extract_json_name_override(&field.options)
                .unwrap_or_else(|| snake_to_camel_case(&field.name));
            let fdp = FieldDescriptorProto {
                name: Some(field.name.clone()),
                number: Some(field.number),
                label: Some(Label::Repeated as i32),
                r#type: Some(Type::Message as i32),
                type_name: Some(entry_fqn),
                extendee: None,
                default_value: None,
                oneof_index: None,
                json_name: Some(json_name),
                options: build_field_options(&field.options),
                proto3_optional: None,
            };
            vec![(field.span.start, fdp)]
        }

        FieldType::Group(_) => {
            // A proto2 group field.  TYPE_GROUP (10), type_name = FQN of the
            // synthesized nested message (already resolved at this point).
            // json_name = the capitalized group name (protoc convention, NOT
            // camelCase of the lowercased field name).
            //
            // After resolution, FieldType::Group(s) holds the FQN e.g.
            // ".pkg.Outer.Result".  The original capitalized group name is the
            // last segment of the FQN; alternatively we can capitalize the
            // first letter of field.name (which is always the lowercased name).
            let (proto_type, type_name) = field_type_to_proto_kind(&field.ty, enum_fqns);
            let json_name = extract_json_name_override(&field.options)
                .unwrap_or_else(|| capitalize_first(&field.name));
            let field_opts = build_field_options(&field.options);

            let label = match &field.label {
                FieldLabel::Repeated => Label::Repeated as i32,
                FieldLabel::Required => Label::Required as i32,
                FieldLabel::Optional | FieldLabel::Singular => Label::Optional as i32,
            };

            let fdp = FieldDescriptorProto {
                name: Some(field.name.clone()),
                number: Some(field.number),
                label: Some(label),
                r#type: Some(proto_type as i32),
                type_name,
                extendee: None,
                default_value: None,
                oneof_index: None,
                json_name: Some(json_name),
                options: field_opts,
                proto3_optional: None,
            };
            vec![(field.span.start, fdp)]
        }

        FieldType::Scalar(_) | FieldType::Named(_) => {
            let (proto_type, type_name) = field_type_to_proto_kind(&field.ty, enum_fqns);
            let json_name = extract_json_name_override(&field.options)
                .unwrap_or_else(|| snake_to_camel_case(&field.name));
            let field_opts = build_field_options(&field.options);
            let is_msg = matches!(field.ty, FieldType::Named(_));
            let default_value = extract_default_value(&field.options, is_msg);

            match &field.label {
                FieldLabel::Repeated => {
                    let fdp = FieldDescriptorProto {
                        name: Some(field.name.clone()),
                        number: Some(field.number),
                        label: Some(Label::Repeated as i32),
                        r#type: Some(proto_type as i32),
                        type_name,
                        extendee: None,
                        default_value,
                        oneof_index: None,
                        json_name: Some(json_name),
                        options: field_opts,
                        proto3_optional: None,
                    };
                    vec![(field.span.start, fdp)]
                }

                FieldLabel::Required => {
                    // proto2 required field: LABEL_REQUIRED, no synthetic oneof.
                    let fdp = FieldDescriptorProto {
                        name: Some(field.name.clone()),
                        number: Some(field.number),
                        label: Some(Label::Required as i32),
                        r#type: Some(proto_type as i32),
                        type_name,
                        extendee: None,
                        default_value,
                        oneof_index: None,
                        json_name: Some(json_name),
                        options: field_opts,
                        proto3_optional: None,
                    };
                    vec![(field.span.start, fdp)]
                }

                FieldLabel::Optional => {
                    if is_proto2 {
                        // proto2 optional: LABEL_OPTIONAL, no synthetic oneof.
                        let fdp = FieldDescriptorProto {
                            name: Some(field.name.clone()),
                            number: Some(field.number),
                            label: Some(Label::Optional as i32),
                            r#type: Some(proto_type as i32),
                            type_name,
                            extendee: None,
                            default_value,
                            oneof_index: None,
                            json_name: Some(json_name),
                            options: field_opts,
                            proto3_optional: None,
                        };
                        vec![(field.span.start, fdp)]
                    } else {
                        // proto3 optional: create a synthetic oneof.
                        let synth_oneof_name = format!("_{}", field.name);
                        let synth_oneof_idx = (real_oneof_count + synthetic_oneofs.len()) as i32;
                        synthetic_oneofs.push(OneofDescriptorProto {
                            name: Some(synth_oneof_name),
                            options: None,
                        });
                        let fdp = FieldDescriptorProto {
                            name: Some(field.name.clone()),
                            number: Some(field.number),
                            label: Some(Label::Optional as i32),
                            r#type: Some(proto_type as i32),
                            type_name,
                            extendee: None,
                            default_value,
                            oneof_index: Some(synth_oneof_idx),
                            json_name: Some(json_name),
                            options: field_opts,
                            proto3_optional: Some(true),
                        };
                        vec![(field.span.start, fdp)]
                    }
                }

                FieldLabel::Singular => {
                    // Proto3 singular (no label) → LABEL_OPTIONAL, no oneof.
                    let fdp = FieldDescriptorProto {
                        name: Some(field.name.clone()),
                        number: Some(field.number),
                        label: Some(Label::Optional as i32),
                        r#type: Some(proto_type as i32),
                        type_name,
                        extendee: None,
                        default_value,
                        oneof_index: None,
                        json_name: Some(json_name),
                        options: field_opts,
                        proto3_optional: None,
                    };
                    vec![(field.span.start, fdp)]
                }
            }
        }
    }
}

/// Build a `FieldDescriptorProto` for a member of a real `oneof` block.
fn build_oneof_member_field(
    field: &Field,
    oneof_idx: usize,
    enum_fqns: &HashSet<String>,
) -> FieldDescriptorProto {
    let (proto_type, type_name) = field_type_to_proto_kind(&field.ty, enum_fqns);
    let json_name = extract_json_name_override(&field.options)
        .unwrap_or_else(|| snake_to_camel_case(&field.name));
    FieldDescriptorProto {
        name: Some(field.name.clone()),
        number: Some(field.number),
        label: Some(Label::Optional as i32),
        r#type: Some(proto_type as i32),
        type_name,
        extendee: None,
        default_value: None,
        oneof_index: Some(oneof_idx as i32),
        json_name: Some(json_name),
        options: build_field_options(&field.options),
        proto3_optional: None,
    }
}

/// Map a `FieldType` to `(Type, Option<type_name>)`.
///
/// For scalars: `(scalar_type, None)`.
/// For Named (already FQN): `(Message or Enum, Some(fqn))`.
/// For Group (already FQN after resolve): `(Group, Some(fqn))`.
/// Map fields should not reach here (handled separately).
fn field_type_to_proto_kind(ft: &FieldType, enum_fqns: &HashSet<String>) -> (Type, Option<String>) {
    match ft {
        FieldType::Scalar(st) => (scalar_to_proto_type(*st), None),
        FieldType::Named(fqn) => {
            // fqn is already a leading-dot FQN after resolve.
            let kind = if enum_fqns.contains(fqn) {
                Type::Enum
            } else {
                Type::Message
            };
            (kind, Some(fqn.clone()))
        }
        FieldType::Group(fqn) => {
            // Group: TYPE_GROUP (10).  The type_name holds the FQN of the
            // synthesized nested message (set by the resolver).
            (Type::Group, Some(fqn.clone()))
        }
        FieldType::Map { .. } => {
            // Map-typed value fields (value side of a map<K,V>) could be a
            // message type; but the value is always FieldType::Scalar or
            // FieldType::Named — Map is only used at the outer field level.
            // This arm is unreachable in practice (value is never Map).
            (Type::Message, None)
        }
    }
}

// ---------------------------------------------------------------------------
// Enum descriptor builder
// ---------------------------------------------------------------------------

fn build_enum_descriptor(en: &Enum) -> EnumDescriptorProto {
    let value: Vec<EnumValueDescriptorProto> = en.values.iter().map(build_enum_value).collect();
    let (reserved_range, reserved_name) = build_enum_reserved(&en.reserved);
    EnumDescriptorProto {
        name: Some(en.name.clone()),
        value,
        options: build_enum_options(&en.options),
        reserved_range,
        reserved_name,
    }
}

fn build_enum_value(ev: &EnumValue) -> EnumValueDescriptorProto {
    EnumValueDescriptorProto {
        name: Some(ev.name.clone()),
        number: Some(ev.number),
        options: None,
    }
}

// ---------------------------------------------------------------------------
// Service descriptor builder
// ---------------------------------------------------------------------------

fn build_service_descriptor(svc: &Service) -> ServiceDescriptorProto {
    let method: Vec<MethodDescriptorProto> =
        svc.methods.iter().map(build_method_descriptor).collect();
    ServiceDescriptorProto {
        name: Some(svc.name.clone()),
        method,
        options: build_service_options(&svc.options),
    }
}

fn build_method_descriptor(method: &crate::parser::ast::Method) -> MethodDescriptorProto {
    MethodDescriptorProto {
        name: Some(method.name.clone()),
        input_type: Some(method.input_type.clone()),
        output_type: Some(method.output_type.clone()),
        options: build_method_options(&method.options),
        client_streaming: if method.client_streaming {
            Some(true)
        } else {
            None
        },
        server_streaming: if method.server_streaming {
            Some(true)
        } else {
            None
        },
    }
}

// ---------------------------------------------------------------------------
// SourceCodeInfo builder (native-parser feature)
// ---------------------------------------------------------------------------

/// Build the top-level `SourceCodeInfo` for `proto_file`.
///
/// Populates one `Location` per file-level declaration node using the
/// protobuf path conventions from the descriptor.proto spec:
/// - file top-level = `[]`
/// - file.message_type = `[4, i]`
/// - file.enum_type    = `[5, i]`
/// - file.service      = `[6, i]`
/// - message.field     = `[…, 2, i]`
/// - message.nested    = `[…, 3, i]`
/// - message.enum      = `[…, 4, i]`
/// - message.oneof     = `[…, 8, i]`
/// - enum.value        = `[…, 2, i]`
/// - service.method    = `[…, 2, i]`
///
/// Field indices mirror the source-order sort used in `build_message_descriptor`
/// (all regular fields + oneof member fields sorted by `span.start`).
#[cfg(feature = "native-parser")]
fn build_source_code_info(
    proto_file: &ProtoFile,
    src: &str,
    comments: &CommentMap,
    line_table: &LineTable,
) -> prost_types::SourceCodeInfo {
    let src_bytes = src.as_bytes();
    let mut locations: Vec<Location> = Vec::new();

    // File-level location (path = []).
    // Span covers the whole file.
    let file_span_end = src.len();
    let (file_leading, file_detached) = comments.leading_for(0, src_bytes);
    locations.push(Location {
        path: vec![],
        span: line_table.proto_span(0, file_span_end),
        leading_comments: file_leading,
        trailing_comments: None,
        leading_detached_comments: file_detached,
    });

    // Top-level messages (path = [4, idx]).
    for (msg_idx, msg) in proto_file.messages.iter().enumerate() {
        let path = vec![4i32, msg_idx as i32];
        add_message_locations(&mut locations, msg, &path, src_bytes, comments, line_table);
    }

    // Top-level enums (path = [5, idx]).
    for (en_idx, en) in proto_file.enums.iter().enumerate() {
        let path = vec![5i32, en_idx as i32];
        add_enum_locations(&mut locations, en, &path, src_bytes, comments, line_table);
    }

    // Services (path = [6, idx]).
    for (svc_idx, svc) in proto_file.services.iter().enumerate() {
        let path = vec![6i32, svc_idx as i32];
        add_service_locations(&mut locations, svc, &path, src_bytes, comments, line_table);
    }

    prost_types::SourceCodeInfo {
        location: locations,
    }
}

/// Emit `Location` entries for a message and all its children.
#[cfg(feature = "native-parser")]
fn add_message_locations(
    locs: &mut Vec<Location>,
    msg: &Message,
    path: &[i32],
    src: &[u8],
    comments: &CommentMap,
    line_table: &LineTable,
) {
    let (leading, detached) = comments.leading_for(msg.span.start, src);
    let trailing = comments.trailing_for(msg.span.end, src);
    locs.push(Location {
        path: path.to_vec(),
        span: line_table.proto_span(msg.span.start, msg.span.end),
        leading_comments: leading,
        trailing_comments: trailing,
        leading_detached_comments: detached,
    });

    // Build the field list in source-order (same sort as build_message_descriptor).
    // Collect (span.start, field_ref) for regular fields AND oneof member fields.
    let mut all_fields: Vec<(usize, &Field)> = Vec::new();
    for f in &msg.fields {
        all_fields.push((f.span.start, f));
    }
    for oneof in &msg.oneofs {
        for f in &oneof.fields {
            all_fields.push((f.span.start, f));
        }
    }
    all_fields.sort_by_key(|(start, _)| *start);

    for (field_idx, (_, field)) in all_fields.iter().enumerate() {
        let field_path: Vec<i32> = path
            .iter()
            .copied()
            .chain([2i32, field_idx as i32])
            .collect();
        let (fl, fd) = comments.leading_for(field.span.start, src);
        let ft = comments.trailing_for(field.span.end, src);
        locs.push(Location {
            path: field_path,
            span: line_table.proto_span(field.span.start, field.span.end),
            leading_comments: fl,
            trailing_comments: ft,
            leading_detached_comments: fd,
        });
    }

    // Oneofs (path + [8, oneof_idx]).
    for (oi, oneof) in msg.oneofs.iter().enumerate() {
        let oneof_path: Vec<i32> = path.iter().copied().chain([8i32, oi as i32]).collect();
        let (ol, od) = comments.leading_for(oneof.span.start, src);
        let ot = comments.trailing_for(oneof.span.end, src);
        locs.push(Location {
            path: oneof_path,
            span: line_table.proto_span(oneof.span.start, oneof.span.end),
            leading_comments: ol,
            trailing_comments: ot,
            leading_detached_comments: od,
        });
    }

    // Nested messages (path + [3, nested_idx]).
    for (ni, nested) in msg.nested_messages.iter().enumerate() {
        let nested_path: Vec<i32> = path.iter().copied().chain([3i32, ni as i32]).collect();
        add_message_locations(locs, nested, &nested_path, src, comments, line_table);
    }

    // Nested enums (path + [4, en_idx]).
    for (ei, en) in msg.nested_enums.iter().enumerate() {
        let en_path: Vec<i32> = path.iter().copied().chain([4i32, ei as i32]).collect();
        add_enum_locations(locs, en, &en_path, src, comments, line_table);
    }
}

/// Emit `Location` entries for an enum and all its values.
#[cfg(feature = "native-parser")]
fn add_enum_locations(
    locs: &mut Vec<Location>,
    en: &Enum,
    path: &[i32],
    src: &[u8],
    comments: &CommentMap,
    line_table: &LineTable,
) {
    let (leading, detached) = comments.leading_for(en.span.start, src);
    let trailing = comments.trailing_for(en.span.end, src);
    locs.push(Location {
        path: path.to_vec(),
        span: line_table.proto_span(en.span.start, en.span.end),
        leading_comments: leading,
        trailing_comments: trailing,
        leading_detached_comments: detached,
    });

    // Enum values (path + [2, val_idx]).
    for (vi, val) in en.values.iter().enumerate() {
        let val_path: Vec<i32> = path.iter().copied().chain([2i32, vi as i32]).collect();
        let (vl, vd) = comments.leading_for(val.span.start, src);
        let vt = comments.trailing_for(val.span.end, src);
        locs.push(Location {
            path: val_path,
            span: line_table.proto_span(val.span.start, val.span.end),
            leading_comments: vl,
            trailing_comments: vt,
            leading_detached_comments: vd,
        });
    }
}

/// Emit `Location` entries for a service and all its methods.
#[cfg(feature = "native-parser")]
fn add_service_locations(
    locs: &mut Vec<Location>,
    svc: &Service,
    path: &[i32],
    src: &[u8],
    comments: &CommentMap,
    line_table: &LineTable,
) {
    let (leading, detached) = comments.leading_for(svc.span.start, src);
    let trailing = comments.trailing_for(svc.span.end, src);
    locs.push(Location {
        path: path.to_vec(),
        span: line_table.proto_span(svc.span.start, svc.span.end),
        leading_comments: leading,
        trailing_comments: trailing,
        leading_detached_comments: detached,
    });

    // Methods (path + [2, method_idx]).
    for (mi, method) in svc.methods.iter().enumerate() {
        let method_path: Vec<i32> = path.iter().copied().chain([2i32, mi as i32]).collect();
        let (ml, md) = comments.leading_for(method.span.start, src);
        let mt = comments.trailing_for(method.span.end, src);
        locs.push(Location {
            path: method_path,
            span: line_table.proto_span(method.span.start, method.span.end),
            leading_comments: ml,
            trailing_comments: mt,
            leading_detached_comments: md,
        });
    }
}

// ---------------------------------------------------------------------------
// Multi-file support (native-parser feature)
// ---------------------------------------------------------------------------

/// Build a [`FileDescriptorProto`] using a pre-supplied global enum FQN set.
///
/// This is the multi-file variant of `build_file_descriptor_proto`; it accepts
/// `global_enum_fqns` (gathered from all loaded files) instead of computing
/// the enum set from this file alone, which is necessary for cross-file
/// `Type::Enum` disambiguation.
#[cfg(feature = "native-parser")]
pub(crate) fn build_file_descriptor_proto_with_global_enums(
    proto_file: &ProtoFile,
    file_name: &str,
    global_enum_fqns: &HashSet<String>,
    src: &str,
) -> FileDescriptorProto {
    let pkg = proto_file.package.as_deref().unwrap_or("");
    let is_proto2 = proto_file.syntax.as_deref() == Some("proto2");

    // Build top-level messages.
    let message_type: Vec<DescriptorProto> = proto_file
        .messages
        .iter()
        .map(|msg| build_message_descriptor(msg, pkg, &[], global_enum_fqns, is_proto2))
        .collect();

    // Build top-level enums.
    let enum_type: Vec<EnumDescriptorProto> =
        proto_file.enums.iter().map(build_enum_descriptor).collect();

    // Build services.
    let service: Vec<ServiceDescriptorProto> = proto_file
        .services
        .iter()
        .map(build_service_descriptor)
        .collect();

    // Build file-level extension fields from top-level extend blocks.
    let extension = build_extension_fields(
        &proto_file.extends,
        proto_file.package.as_deref(),
        global_enum_fqns,
        is_proto2,
    );

    // Dependencies from imports.
    let dependency: Vec<String> = proto_file
        .imports
        .iter()
        .map(|imp| imp.path.clone())
        .collect();

    let mut public_dependency: Vec<i32> = Vec::new();
    let mut weak_dependency: Vec<i32> = Vec::new();
    for (i, imp) in proto_file.imports.iter().enumerate() {
        match imp.modifier {
            ImportModifier::Public => public_dependency.push(i as i32),
            ImportModifier::Weak => weak_dependency.push(i as i32),
            ImportModifier::None => {}
        }
    }

    let source_code_info = if src.is_empty() {
        None
    } else {
        let comments = CommentMap::extract(src);
        let line_table = LineTable::build(src);
        Some(build_source_code_info(
            proto_file,
            src,
            &comments,
            &line_table,
        ))
    };

    FileDescriptorProto {
        name: Some(file_name.to_owned()),
        package: proto_file.package.clone(),
        dependency,
        public_dependency,
        weak_dependency,
        message_type,
        enum_type,
        service,
        extension,
        options: build_file_options(&proto_file.options),
        source_code_info,
        syntax: proto_file.syntax.clone(),
    }
}

/// Build a multi-file [`Vec<FileDescriptorProto>`] in topological order.
///
/// Prebuilt WKT files are pushed verbatim; user-authored parsed files are
/// built with the global enum FQN set for correct `Type::Enum` classification.
#[cfg(feature = "native-parser")]
pub(crate) fn build_fds_multi(
    order: &[String],
    files: &HashMap<String, crate::parser::loader::LoadedFile>,
    resolved: &HashMap<String, ProtoFile>,
    global_enum_fqns: &HashSet<String>,
) -> Vec<FileDescriptorProto> {
    use crate::parser::loader::LoadedFile;
    let mut result = Vec::new();
    for name in order {
        match files.get(name) {
            Some(LoadedFile::Prebuilt { fdp }) => {
                result.push((**fdp).clone()); // push verbatim — byte-identical to protox
            }
            Some(LoadedFile::Parsed { source, .. }) => {
                if let Some(ast) = resolved.get(name) {
                    let fdp = build_file_descriptor_proto_with_global_enums(
                        ast,
                        name,
                        global_enum_fqns,
                        source,
                    );
                    result.push(fdp);
                }
            }
            None => {}
        }
    }
    result
}
