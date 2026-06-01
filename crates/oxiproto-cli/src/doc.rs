#![forbid(unsafe_code)]

use crate::util::Verbosity;
use clap::Args;
use std::{collections::HashMap, io::Write as _, path::PathBuf};

/// Arguments for the `doc` subcommand.
#[derive(Args)]
pub struct DocArgs {
    /// .proto files to document
    #[arg(value_name = "PROTO_FILE", required = true)]
    pub protos: Vec<PathBuf>,
    /// Include directories for import resolution
    #[arg(short = 'I', long = "include", value_name = "DIR")]
    pub include: Vec<PathBuf>,
    /// Write output to this file (default: stdout)
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    pub output: Option<PathBuf>,
}

/// A map from protobuf path vector to leading comment text.
type LocationMap = HashMap<Vec<i32>, String>;

/// Run the `doc` subcommand: compile the given `.proto` files and render
/// Markdown documentation, including `source_code_info` leading comments.
///
/// Output goes to `args.output` (a file) when specified, otherwise stdout.
///
/// # Errors
///
/// Returns an error if any proto file is missing, compilation fails, or the
/// output file cannot be written.
pub fn run(args: DocArgs, verbosity: Verbosity) -> Result<(), Box<dyn std::error::Error>> {
    // Validate file existence before any compilation.
    for p in &args.protos {
        if !p.exists() {
            return Err(format!("proto file not found: {}", p.display()).into());
        }
    }

    verbosity.verbose("Compiling proto sources for documentation...");
    let fds = oxiproto_build::compile_to_fds(&args.protos, &args.include)?;

    let mut buf = String::new();
    for file in &fds.file {
        render_file(file, &mut buf);
    }

    match &args.output {
        Some(path) => {
            let mut f = std::fs::File::create(path)
                .map_err(|e| format!("cannot create output file {}: {e}", path.display()))?;
            f.write_all(buf.as_bytes())
                .map_err(|e| format!("write error for {}: {e}", path.display()))?;
            verbosity.info(&format!("Documentation written to {}", path.display()));
        }
        None => {
            print!("{buf}");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Per-file rendering
// ---------------------------------------------------------------------------

fn render_file(file: &prost_types::FileDescriptorProto, buf: &mut String) {
    let name = file.name.as_deref().unwrap_or("(unknown)");

    // Skip WKT files — not interesting to document.
    if name.starts_with("google/protobuf/") {
        return;
    }

    buf.push_str(&format!("# {name}\n\n"));

    let loc_map = build_location_map(file);

    // Top-level messages.
    for (m, msg) in file.message_type.iter().enumerate() {
        render_message(msg, m, &[4, m as i32], &loc_map, buf);
    }

    // Top-level enums.
    for (e, en) in file.enum_type.iter().enumerate() {
        render_file_enum(en, e, &loc_map, buf);
    }

    // Services.
    for (s, svc) in file.service.iter().enumerate() {
        render_service(svc, s, &loc_map, buf);
    }
}

// ---------------------------------------------------------------------------
// LocationMap builder
// ---------------------------------------------------------------------------

/// Build a path → leading_comments map from `source_code_info`.
fn build_location_map(file: &prost_types::FileDescriptorProto) -> LocationMap {
    let mut map = LocationMap::new();
    if let Some(sci) = &file.source_code_info {
        for loc in &sci.location {
            let comment = loc.leading_comments.as_deref().unwrap_or("");
            if !comment.is_empty() {
                map.insert(loc.path.clone(), comment.to_string());
            }
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Message rendering
// ---------------------------------------------------------------------------

fn render_message(
    msg: &prost_types::DescriptorProto,
    _idx: usize,
    path: &[i32],
    loc_map: &LocationMap,
    buf: &mut String,
) {
    // Skip synthetic map-entry messages.
    if msg.options.as_ref().is_some_and(|o| o.map_entry()) {
        return;
    }

    let name = msg.name.as_deref().unwrap_or("(unnamed)");
    buf.push_str(&format!("## {name}\n\n"));

    if let Some(comment) = loc_map.get(path) {
        let cleaned = strip_comment(comment);
        if !cleaned.is_empty() {
            buf.push_str(&cleaned);
            buf.push_str("\n\n");
        }
    }

    // Fields table.
    if !msg.field.is_empty() {
        buf.push_str("### Fields\n\n");
        buf.push_str("| Field | Number | Type | Description |\n");
        buf.push_str("|-------|--------|------|-------------|\n");
        for (f, field) in msg.field.iter().enumerate() {
            let mut field_path = path.to_vec();
            field_path.push(2);
            field_path.push(f as i32);
            render_field_row(field, &field_path, loc_map, buf);
        }
        buf.push('\n');
    }

    // Nested enums.
    for (e, en) in msg.enum_type.iter().enumerate() {
        let mut enum_path = path.to_vec();
        enum_path.push(4);
        enum_path.push(e as i32);
        render_nested_enum(en, &enum_path, loc_map, buf);
    }

    // Nested messages.
    for (n, nested) in msg.nested_type.iter().enumerate() {
        if nested.options.as_ref().is_some_and(|o| o.map_entry()) {
            continue;
        }
        let mut nested_path = path.to_vec();
        nested_path.push(3);
        nested_path.push(n as i32);
        render_message(nested, n, &nested_path, loc_map, buf);
    }
}

fn render_field_row(
    field: &prost_types::FieldDescriptorProto,
    path: &[i32],
    loc_map: &LocationMap,
    buf: &mut String,
) {
    let name = field.name.as_deref().unwrap_or("(unnamed)");
    let number = field.number.unwrap_or(0);
    let type_str = field_type_str(field);
    let comment = loc_map.get(path).map(|c| first_line(c)).unwrap_or_default();
    buf.push_str(&format!("| {name} | {number} | {type_str} | {comment} |\n"));
}

// ---------------------------------------------------------------------------
// Enum rendering
// ---------------------------------------------------------------------------

fn render_file_enum(
    en: &prost_types::EnumDescriptorProto,
    e: usize,
    loc_map: &LocationMap,
    buf: &mut String,
) {
    let path = vec![5i32, e as i32];
    let name = en.name.as_deref().unwrap_or("(unnamed)");
    buf.push_str(&format!("## {name}\n\n"));

    if let Some(comment) = loc_map.get(&path) {
        let cleaned = strip_comment(comment);
        if !cleaned.is_empty() {
            buf.push_str(&cleaned);
            buf.push_str("\n\n");
        }
    }

    render_enum_values(en, &path, loc_map, buf);
}

fn render_nested_enum(
    en: &prost_types::EnumDescriptorProto,
    path: &[i32],
    loc_map: &LocationMap,
    buf: &mut String,
) {
    let name = en.name.as_deref().unwrap_or("(unnamed)");
    buf.push_str(&format!("## {name}\n\n"));

    if let Some(comment) = loc_map.get(path) {
        let cleaned = strip_comment(comment);
        if !cleaned.is_empty() {
            buf.push_str(&cleaned);
            buf.push_str("\n\n");
        }
    }

    render_enum_values(en, path, loc_map, buf);
}

fn render_enum_values(
    en: &prost_types::EnumDescriptorProto,
    enum_path: &[i32],
    loc_map: &LocationMap,
    buf: &mut String,
) {
    if en.value.is_empty() {
        return;
    }
    buf.push_str("### Values\n\n");
    buf.push_str("| Value | Number | Description |\n");
    buf.push_str("|-------|--------|-------------|\n");
    for (v, val) in en.value.iter().enumerate() {
        let vname = val.name.as_deref().unwrap_or("(unnamed)");
        let vnum = val.number.unwrap_or(0);
        let mut val_path = enum_path.to_vec();
        val_path.push(2);
        val_path.push(v as i32);
        let comment = loc_map
            .get(&val_path)
            .map(|c| first_line(c))
            .unwrap_or_default();
        buf.push_str(&format!("| {vname} | {vnum} | {comment} |\n"));
    }
    buf.push('\n');
}

// ---------------------------------------------------------------------------
// Service rendering
// ---------------------------------------------------------------------------

fn render_service(
    svc: &prost_types::ServiceDescriptorProto,
    s: usize,
    loc_map: &LocationMap,
    buf: &mut String,
) {
    let path = vec![6i32, s as i32];
    let name = svc.name.as_deref().unwrap_or("(unnamed)");
    buf.push_str(&format!("## {name}\n\n"));

    if let Some(comment) = loc_map.get(&path) {
        let cleaned = strip_comment(comment);
        if !cleaned.is_empty() {
            buf.push_str(&cleaned);
            buf.push_str("\n\n");
        }
    }

    if svc.method.is_empty() {
        return;
    }
    buf.push_str("### Methods\n\n");
    buf.push_str("| Method | Request | Response | Description |\n");
    buf.push_str("|--------|---------|----------|-------------|\n");
    for (m2, method) in svc.method.iter().enumerate() {
        let mname = method.name.as_deref().unwrap_or("(unnamed)");
        let input = method
            .input_type
            .as_deref()
            .unwrap_or("?")
            .trim_start_matches('.');
        let output = method
            .output_type
            .as_deref()
            .unwrap_or("?")
            .trim_start_matches('.');
        let mut method_path = path.clone();
        method_path.push(2);
        method_path.push(m2 as i32);
        let comment = loc_map
            .get(&method_path)
            .map(|c| first_line(c))
            .unwrap_or_default();
        buf.push_str(&format!("| {mname} | {input} | {output} | {comment} |\n"));
    }
    buf.push('\n');
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a field descriptor to a human-readable type string.
fn field_type_str(field: &prost_types::FieldDescriptorProto) -> String {
    use prost_types::field_descriptor_proto::Type;
    let t = field.r#type.unwrap_or(Type::String as i32);

    if t == Type::Message as i32 || t == Type::Enum as i32 {
        return field
            .type_name
            .as_deref()
            .unwrap_or("?")
            .trim_start_matches('.')
            .to_string();
    }
    if t == Type::Group as i32 {
        return "group".to_string();
    }

    match t {
        x if x == Type::Double as i32 => "double",
        x if x == Type::Float as i32 => "float",
        x if x == Type::Int64 as i32 => "int64",
        x if x == Type::Uint64 as i32 => "uint64",
        x if x == Type::Int32 as i32 => "int32",
        x if x == Type::Fixed64 as i32 => "fixed64",
        x if x == Type::Fixed32 as i32 => "fixed32",
        x if x == Type::Bool as i32 => "bool",
        x if x == Type::String as i32 => "string",
        x if x == Type::Bytes as i32 => "bytes",
        x if x == Type::Uint32 as i32 => "uint32",
        x if x == Type::Sfixed32 as i32 => "sfixed32",
        x if x == Type::Sfixed64 as i32 => "sfixed64",
        x if x == Type::Sint32 as i32 => "sint32",
        x if x == Type::Sint64 as i32 => "sint64",
        _ => "unknown",
    }
    .to_string()
}

/// Clean up a `leading_comments` string for use as a Markdown paragraph.
///
/// Strips a single leading space from each line (the standard protobuf
/// comment convention) and trims trailing whitespace.
fn strip_comment(s: &str) -> String {
    s.lines()
        .map(|line| {
            let stripped = line.strip_prefix(' ').unwrap_or(line);
            stripped.trim_end()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// Return only the first non-empty line of a comment, for use in table cells.
fn first_line(s: &str) -> String {
    s.lines()
        .map(|line| {
            let stripped = line.strip_prefix(' ').unwrap_or(line);
            stripped.trim()
        })
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}
