#![forbid(unsafe_code)]

//! `format` subcommand — read `.proto` files and emit canonically formatted
//! proto3/proto2 text.

use clap::Args;
use std::{collections::HashMap, path::PathBuf};

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

/// Arguments for the `format` subcommand.
#[derive(Args)]
pub struct FormatArgs {
    /// Input .proto files to format.
    #[arg(value_name = "PROTO_FILE", required = true)]
    pub protos: Vec<PathBuf>,
    /// Include paths for import resolution.
    #[arg(short = 'I', long = "include")]
    pub include: Vec<PathBuf>,
    /// Rewrite files in-place (default: print to stdout).
    #[arg(long)]
    pub in_place: bool,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run the `format` subcommand.
///
/// # Errors
///
/// Returns an error if a file is missing, compilation fails, or an in-place
/// write fails.
pub fn run(
    args: FormatArgs,
    _verbosity: crate::util::Verbosity,
) -> Result<(), Box<dyn std::error::Error>> {
    for proto in &args.protos {
        if !proto.exists() {
            return Err(format!("proto file not found: {}", proto.display()).into());
        }
    }

    for proto in &args.protos {
        let fds = oxiproto_build::compile_to_fds(std::slice::from_ref(proto), &args.include)?;

        // Find the FileDescriptorProto that corresponds to this input file.
        let proto_name = proto
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        let fdp = fds
            .file
            .iter()
            .find(|f| {
                let fname = f.name.as_deref().unwrap_or("");
                // Match by full path basename or by tail of the relative name.
                fname == proto_name
                    || fname.ends_with(&format!("/{proto_name}"))
                    || f.name
                        .as_deref()
                        .map(|n| {
                            std::path::Path::new(n).file_name().and_then(|s| s.to_str())
                                == Some(proto_name)
                        })
                        .unwrap_or(false)
            })
            .ok_or_else(|| {
                format!(
                    "could not locate '{}' in compiled FileDescriptorSet",
                    proto.display()
                )
            })?;

        let formatted = format_file_descriptor(fdp);

        if args.in_place {
            std::fs::write(proto, &formatted)
                .map_err(|e| format!("failed to write {}: {e}", proto.display()))?;
        } else {
            print!("{formatted}");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Core formatter
// ---------------------------------------------------------------------------

/// Location-map: proto path vector → leading comment text.
type LocationMap = HashMap<Vec<i32>, String>;

/// Build a `path → leading_comments` map from `source_code_info`.
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

/// Format a leading-comment string as `// …` lines.
fn emit_comment(comment: &str, buf: &mut String) {
    for line in comment.lines() {
        let stripped = line.strip_prefix(' ').unwrap_or(line).trim_end();
        buf.push_str("// ");
        buf.push_str(stripped);
        buf.push('\n');
    }
}

/// Produce a canonical proto3/proto2 text representation of `fdp`.
pub fn format_file_descriptor(fdp: &prost_types::FileDescriptorProto) -> String {
    let mut out = String::new();
    let loc_map = build_location_map(fdp);

    // --- syntax ---
    let syntax = fdp.syntax.as_deref().unwrap_or("proto3");
    out.push_str(&format!("syntax = \"{syntax}\";\n"));

    // --- edition (proto editions, if present) ---
    // Not exposed in prost_types < proto edition support; skip.

    // --- package ---
    if let Some(pkg) = &fdp.package {
        if !pkg.is_empty() {
            out.push('\n');
            out.push_str(&format!("package {pkg};\n"));
        }
    }

    // --- imports: plain → public → weak (alphabetical within each group) ---
    let plain_deps: Vec<&str> =
        fdp.dependency
            .iter()
            .filter(|d| {
                !fdp.public_dependency.iter().any(|&i| {
                    fdp.dependency.get(i as usize).map(|s| s.as_str()) == Some(d.as_str())
                }) && !fdp.weak_dependency.iter().any(|&i| {
                    fdp.dependency.get(i as usize).map(|s| s.as_str()) == Some(d.as_str())
                })
            })
            .map(|d| d.as_str())
            .collect();

    // Collect public dep paths by index.
    let public_deps: Vec<&str> = fdp
        .public_dependency
        .iter()
        .filter_map(|&i| fdp.dependency.get(i as usize).map(|s| s.as_str()))
        .collect();

    let weak_deps: Vec<&str> = fdp
        .weak_dependency
        .iter()
        .filter_map(|&i| fdp.dependency.get(i as usize).map(|s| s.as_str()))
        .collect();

    let mut sorted_plain = plain_deps.clone();
    sorted_plain.sort_unstable();
    let mut sorted_public = public_deps.clone();
    sorted_public.sort_unstable();
    let mut sorted_weak = weak_deps.clone();
    sorted_weak.sort_unstable();

    let has_imports =
        !sorted_plain.is_empty() || !sorted_public.is_empty() || !sorted_weak.is_empty();
    if has_imports {
        out.push('\n');
        for dep in &sorted_plain {
            out.push_str(&format!("import \"{dep}\";\n"));
        }
        for dep in &sorted_public {
            out.push_str(&format!("import public \"{dep}\";\n"));
        }
        for dep in &sorted_weak {
            out.push_str(&format!("import weak \"{dep}\";\n"));
        }
    }

    // --- file-level options ---
    if let Some(opts) = &fdp.options {
        let mut option_lines: Vec<String> = Vec::new();
        if let Some(java_pkg) = &opts.java_package {
            option_lines.push(format!("option java_package = \"{java_pkg}\";"));
        }
        if let Some(java_outer) = &opts.java_outer_classname {
            option_lines.push(format!("option java_outer_classname = \"{java_outer}\";"));
        }
        if opts.java_multiple_files() {
            option_lines.push("option java_multiple_files = true;".to_string());
        }
        if opts.optimize_for() != prost_types::file_options::OptimizeMode::Speed {
            let mode = match opts.optimize_for() {
                prost_types::file_options::OptimizeMode::CodeSize => "CODE_SIZE",
                prost_types::file_options::OptimizeMode::LiteRuntime => "LITE_RUNTIME",
                _ => "SPEED",
            };
            option_lines.push(format!("option optimize_for = {mode};"));
        }
        if let Some(go_pkg) = &opts.go_package {
            option_lines.push(format!("option go_package = \"{go_pkg}\";"));
        }
        if opts.cc_generic_services() {
            option_lines.push("option cc_generic_services = true;".to_string());
        }
        if opts.java_generic_services() {
            option_lines.push("option java_generic_services = true;".to_string());
        }
        if opts.py_generic_services() {
            option_lines.push("option py_generic_services = true;".to_string());
        }
        if opts.java_generate_equals_and_hash() {
            option_lines.push("option java_generate_equals_and_hash = true;".to_string());
        }
        if opts.java_string_check_utf8() {
            option_lines.push("option java_string_check_utf8 = true;".to_string());
        }
        if opts.cc_enable_arenas() {
            option_lines.push("option cc_enable_arenas = true;".to_string());
        }
        if let Some(objc_cls) = &opts.objc_class_prefix {
            option_lines.push(format!("option objc_class_prefix = \"{objc_cls}\";"));
        }
        if let Some(csharp_ns) = &opts.csharp_namespace {
            option_lines.push(format!("option csharp_namespace = \"{csharp_ns}\";"));
        }
        if let Some(swift_prefix) = &opts.swift_prefix {
            option_lines.push(format!("option swift_prefix = \"{swift_prefix}\";"));
        }
        if !option_lines.is_empty() {
            out.push('\n');
            for line in &option_lines {
                out.push_str(line);
                out.push('\n');
            }
        }
    }

    // --- top-level messages ---
    for (i, msg) in fdp.message_type.iter().enumerate() {
        if msg.options.as_ref().is_some_and(|o| o.map_entry()) {
            continue;
        }
        out.push('\n');
        let path = vec![4i32, i as i32];
        if let Some(comment) = loc_map.get(&path) {
            emit_comment(comment, &mut out);
        }
        format_message(msg, fdp, &path, &loc_map, 0, &mut out);
    }

    // --- top-level enums ---
    for (i, en) in fdp.enum_type.iter().enumerate() {
        out.push('\n');
        let path = vec![5i32, i as i32];
        if let Some(comment) = loc_map.get(&path) {
            emit_comment(comment, &mut out);
        }
        format_enum(en, &path, &loc_map, 0, &mut out);
    }

    // --- services ---
    for (i, svc) in fdp.service.iter().enumerate() {
        out.push('\n');
        let path = vec![6i32, i as i32];
        if let Some(comment) = loc_map.get(&path) {
            emit_comment(comment, &mut out);
        }
        format_service(svc, &path, &loc_map, 0, &mut out);
    }

    out
}

// ---------------------------------------------------------------------------
// Message formatting
// ---------------------------------------------------------------------------

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn format_message(
    msg: &prost_types::DescriptorProto,
    file: &prost_types::FileDescriptorProto,
    path: &[i32],
    loc_map: &LocationMap,
    depth: usize,
    out: &mut String,
) {
    let ind = indent(depth);
    let name = msg.name.as_deref().unwrap_or("(unnamed)");
    out.push_str(&format!("{ind}message {name} {{\n"));

    // Build a set of field indices that belong to a oneof (excluding synthetic
    // proto3_optional oneofs which are single-field containing the optional field).
    // Fields in synthetic oneofs are emitted inline with `optional`.
    let synthetic_oneof_indices: std::collections::HashSet<i32> = msg
        .oneof_decl
        .iter()
        .enumerate()
        .filter(|(oi, _)| {
            // A synthetic oneof has exactly one field that has proto3_optional = true.
            msg.field
                .iter()
                .filter(|f| f.oneof_index == Some(*oi as i32))
                .all(|f| f.proto3_optional())
                && msg
                    .field
                    .iter()
                    .filter(|f| f.oneof_index == Some(*oi as i32))
                    .count()
                    == 1
        })
        .map(|(oi, _)| oi as i32)
        .collect();

    // Collect which field indices are already covered by non-synthetic oneofs.
    let oneof_field_indices: std::collections::HashSet<usize> = msg
        .field
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            if let Some(oi) = f.oneof_index {
                !synthetic_oneof_indices.contains(&oi)
            } else {
                false
            }
        })
        .map(|(i, _)| i)
        .collect();

    // Track which oneofs we've already emitted.
    let mut emitted_oneofs: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for (fi, field) in msg.field.iter().enumerate() {
        let field_path: Vec<i32> = {
            let mut p = path.to_vec();
            p.push(2);
            p.push(fi as i32);
            p
        };

        if let Some(comment) = loc_map.get(&field_path) {
            emit_comment(comment, out);
        }

        if let Some(oi) = field.oneof_index {
            if synthetic_oneof_indices.contains(&oi) {
                // proto3_optional field — emit `optional type name = N;`
                let type_str = field_type_keyword(field, file);
                let fname = field.name.as_deref().unwrap_or("(unnamed)");
                let num = field.number.unwrap_or(0);
                out.push_str(&format!("{ind}  optional {type_str} {fname} = {num};\n"));
                continue;
            }

            // Non-synthetic oneof field.
            let oi_usize = oi as usize;
            if !emitted_oneofs.contains(&oi_usize) {
                emitted_oneofs.insert(oi_usize);
                if let Some(od) = msg.oneof_decl.get(oi_usize) {
                    let oname = od.name.as_deref().unwrap_or("(unnamed)");
                    let oneof_path: Vec<i32> = {
                        let mut p = path.to_vec();
                        p.push(8);
                        p.push(oi_usize as i32);
                        p
                    };
                    if let Some(comment) = loc_map.get(&oneof_path) {
                        emit_comment(comment, out);
                    }
                    out.push_str(&format!("{ind}  oneof {oname} {{\n"));
                    // Emit all fields belonging to this oneof.
                    for (fi2, f2) in msg.field.iter().enumerate() {
                        if f2.oneof_index == Some(oi) {
                            let f2_path: Vec<i32> = {
                                let mut p = path.to_vec();
                                p.push(2);
                                p.push(fi2 as i32);
                                p
                            };
                            if let Some(comment) = loc_map.get(&f2_path) {
                                emit_comment(comment, out);
                            }
                            let type_str = field_type_keyword(f2, file);
                            let fname2 = f2.name.as_deref().unwrap_or("(unnamed)");
                            let num2 = f2.number.unwrap_or(0);
                            out.push_str(&format!("{ind}    {type_str} {fname2} = {num2};\n"));
                        }
                    }
                    out.push_str(&format!("{ind}  }}\n"));
                }
            }
            continue;
        }

        // Regular field (not in any oneof).
        if oneof_field_indices.contains(&fi) {
            // Already emitted inside a oneof block above.
            continue;
        }

        // Check for map field: repeated message where the nested message has
        // options.map_entry = true.
        if is_map_field(field, msg) {
            emit_map_field(field, msg, file, out, &ind);
            continue;
        }

        emit_regular_field(field, file, out, &ind);
    }

    // --- reserved ranges ---
    for rr in &msg.reserved_range {
        let start = rr.start.unwrap_or(0);
        let end = rr.end.unwrap_or(0);
        // protobuf reserved range end is exclusive; "reserved 4 to 9" means [4, 10).
        if end == start + 1 {
            out.push_str(&format!("{ind}  reserved {start};\n"));
        } else {
            out.push_str(&format!("{ind}  reserved {start} to {};\n", end - 1));
        }
    }

    // --- reserved names ---
    if !msg.reserved_name.is_empty() {
        let names: Vec<String> = msg
            .reserved_name
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect();
        out.push_str(&format!("{ind}  reserved {};\n", names.join(", ")));
    }

    // --- nested enums ---
    for (ei, en) in msg.enum_type.iter().enumerate() {
        out.push('\n');
        let en_path: Vec<i32> = {
            let mut p = path.to_vec();
            p.push(4);
            p.push(ei as i32);
            p
        };
        if let Some(comment) = loc_map.get(&en_path) {
            emit_comment(comment, out);
        }
        format_enum(en, &en_path, loc_map, depth + 1, out);
    }

    // --- nested messages ---
    for (ni, nested) in msg.nested_type.iter().enumerate() {
        if nested.options.as_ref().is_some_and(|o| o.map_entry()) {
            continue;
        }
        out.push('\n');
        let nested_path: Vec<i32> = {
            let mut p = path.to_vec();
            p.push(3);
            p.push(ni as i32);
            p
        };
        if let Some(comment) = loc_map.get(&nested_path) {
            emit_comment(comment, out);
        }
        format_message(nested, file, &nested_path, loc_map, depth + 1, out);
    }

    out.push_str(&format!("{ind}}}\n"));
}

/// Return `true` if the field is a map field (repeated message with map_entry
/// nested message).
fn is_map_field(
    field: &prost_types::FieldDescriptorProto,
    msg: &prost_types::DescriptorProto,
) -> bool {
    use prost_types::field_descriptor_proto::{Label, Type};
    if field.label() != Label::Repeated {
        return false;
    }
    if field.r#type() != Type::Message {
        return false;
    }
    // The type_name for a map field refers to a nested MapEntry message.
    let type_name = field.type_name.as_deref().unwrap_or("");
    // Find the nested type with a matching name and map_entry option set.
    msg.nested_type.iter().any(|nt| {
        nt.options.as_ref().is_some_and(|o| o.map_entry())
            && type_name.ends_with(nt.name.as_deref().unwrap_or(""))
    })
}

/// Emit a map field: `  map<key_type, value_type> name = N;`
fn emit_map_field(
    field: &prost_types::FieldDescriptorProto,
    msg: &prost_types::DescriptorProto,
    file: &prost_types::FileDescriptorProto,
    out: &mut String,
    ind: &str,
) {
    let type_name = field.type_name.as_deref().unwrap_or("");
    // Locate the nested map-entry message.
    let entry_msg = msg.nested_type.iter().find(|nt| {
        nt.options.as_ref().is_some_and(|o| o.map_entry())
            && type_name.ends_with(nt.name.as_deref().unwrap_or(""))
    });

    let (key_type, val_type) = entry_msg
        .map(|em| {
            let key = em
                .field
                .iter()
                .find(|f| f.number == Some(1))
                .map(|f| field_type_keyword(f, file))
                .unwrap_or_else(|| "string".to_string());
            let val = em
                .field
                .iter()
                .find(|f| f.number == Some(2))
                .map(|f| field_type_keyword(f, file))
                .unwrap_or_else(|| "string".to_string());
            (key, val)
        })
        .unwrap_or_else(|| ("string".to_string(), "string".to_string()));

    let fname = field.name.as_deref().unwrap_or("(unnamed)");
    let num = field.number.unwrap_or(0);
    out.push_str(&format!(
        "{ind}  map<{key_type}, {val_type}> {fname} = {num};\n"
    ));
}

/// Emit a regular (non-map, non-oneof, non-optional) field line.
fn emit_regular_field(
    field: &prost_types::FieldDescriptorProto,
    file: &prost_types::FileDescriptorProto,
    out: &mut String,
    ind: &str,
) {
    use prost_types::field_descriptor_proto::Label;

    let type_str = field_type_keyword(field, file);
    let fname = field.name.as_deref().unwrap_or("(unnamed)");
    let num = field.number.unwrap_or(0);

    let label = field.label();
    let prefix = match label {
        Label::Repeated => "repeated ",
        Label::Required => "required ",
        // In proto3 `optional` is the default; only emit for proto2.
        Label::Optional => "",
    };

    out.push_str(&format!("{ind}  {prefix}{type_str} {fname} = {num};\n"));
}

/// Convert a field's type (and possibly `type_name`) to a proto keyword string.
fn field_type_keyword(
    field: &prost_types::FieldDescriptorProto,
    _file: &prost_types::FileDescriptorProto,
) -> String {
    use prost_types::field_descriptor_proto::Type;
    match field.r#type() {
        Type::Double => "double".to_string(),
        Type::Float => "float".to_string(),
        Type::Int64 => "int64".to_string(),
        Type::Uint64 => "uint64".to_string(),
        Type::Int32 => "int32".to_string(),
        Type::Fixed64 => "fixed64".to_string(),
        Type::Fixed32 => "fixed32".to_string(),
        Type::Bool => "bool".to_string(),
        Type::String => "string".to_string(),
        Type::Bytes => "bytes".to_string(),
        Type::Uint32 => "uint32".to_string(),
        Type::Sfixed32 => "sfixed32".to_string(),
        Type::Sfixed64 => "sfixed64".to_string(),
        Type::Sint32 => "sint32".to_string(),
        Type::Sint64 => "sint64".to_string(),
        Type::Group => "/* group */".to_string(),
        Type::Message | Type::Enum => field
            .type_name
            .as_deref()
            .unwrap_or("?")
            .trim_start_matches('.')
            .to_string(),
    }
}

// ---------------------------------------------------------------------------
// Enum formatting
// ---------------------------------------------------------------------------

fn format_enum(
    en: &prost_types::EnumDescriptorProto,
    path: &[i32],
    loc_map: &LocationMap,
    depth: usize,
    out: &mut String,
) {
    let ind = indent(depth);
    let name = en.name.as_deref().unwrap_or("(unnamed)");
    out.push_str(&format!("{ind}enum {name} {{\n"));

    for (vi, val) in en.value.iter().enumerate() {
        let val_path: Vec<i32> = {
            let mut p = path.to_vec();
            p.push(2);
            p.push(vi as i32);
            p
        };
        if let Some(comment) = loc_map.get(&val_path) {
            emit_comment(comment, out);
        }
        let vname = val.name.as_deref().unwrap_or("(unnamed)");
        let vnum = val.number.unwrap_or(0);
        out.push_str(&format!("{ind}  {vname} = {vnum};\n"));
    }

    // Reserved enum ranges.
    for rr in &en.reserved_range {
        let start = rr.start.unwrap_or(0);
        let end = rr.end.unwrap_or(0);
        if end == start + 1 {
            out.push_str(&format!("{ind}  reserved {start};\n"));
        } else {
            out.push_str(&format!("{ind}  reserved {start} to {end};\n"));
        }
    }

    // Reserved enum names.
    if !en.reserved_name.is_empty() {
        let names: Vec<String> = en
            .reserved_name
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect();
        out.push_str(&format!("{ind}  reserved {};\n", names.join(", ")));
    }

    out.push_str(&format!("{ind}}}\n"));
}

// ---------------------------------------------------------------------------
// Service formatting
// ---------------------------------------------------------------------------

fn format_service(
    svc: &prost_types::ServiceDescriptorProto,
    path: &[i32],
    loc_map: &LocationMap,
    depth: usize,
    out: &mut String,
) {
    let ind = indent(depth);
    let name = svc.name.as_deref().unwrap_or("(unnamed)");
    out.push_str(&format!("{ind}service {name} {{\n"));

    for (mi, method) in svc.method.iter().enumerate() {
        let m_path: Vec<i32> = {
            let mut p = path.to_vec();
            p.push(2);
            p.push(mi as i32);
            p
        };
        if let Some(comment) = loc_map.get(&m_path) {
            emit_comment(comment, out);
        }
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

        let client_stream = method.client_streaming();
        let server_stream = method.server_streaming();

        let req = if client_stream {
            format!("stream {input}")
        } else {
            input.to_string()
        };
        let resp = if server_stream {
            format!("stream {output}")
        } else {
            output.to_string()
        };

        out.push_str(&format!("{ind}  rpc {mname}({req}) returns ({resp});\n"));
    }

    out.push_str(&format!("{ind}}}\n"));
}
