#![forbid(unsafe_code)]

use clap::Args;
use std::path::PathBuf;

use crate::util::Verbosity;

/// Arguments for the `describe` subcommand.
#[derive(Args)]
pub struct DescribeArgs {
    /// Input .proto files (at least one required).
    #[arg(required = true)]
    pub protos: Vec<PathBuf>,

    /// Include paths for resolving proto imports (may be repeated).
    #[arg(short = 'I', long)]
    pub include: Vec<PathBuf>,
}

/// Run the `describe` subcommand: print a human-readable summary of every
/// message, enum, and service defined in the given `.proto` files.
///
/// # Errors
///
/// Returns an error if any proto file is missing or parsing fails.
pub fn run(args: DescribeArgs, verbosity: Verbosity) -> Result<(), Box<dyn std::error::Error>> {
    let _ = verbosity; // reserved for future verbose progress messages
    for proto in &args.protos {
        if !proto.exists() {
            return Err(format!("proto file not found: {}", proto.display()).into());
        }
    }

    let fds = oxiproto_build::compile_to_fds(&args.protos, &args.include)?;

    for file in &fds.file {
        let pkg = file.package.as_deref().unwrap_or("(no package)");
        let fname = file.name.as_deref().unwrap_or("(unknown)");
        println!("File: {fname}");
        println!("  Package: {pkg}");
        if let Some(syntax) = &file.syntax {
            println!("  Syntax: {syntax}");
        }

        if !file.dependency.is_empty() {
            println!("  Imports:");
            for dep in &file.dependency {
                println!("    - {dep}");
            }
        }

        for msg in &file.message_type {
            describe_message(msg, 1);
        }

        for en in &file.enum_type {
            describe_enum(en, 1);
        }

        for svc in &file.service {
            describe_service(svc);
        }

        println!();
    }

    Ok(())
}

fn describe_message(msg: &prost_types::DescriptorProto, depth: usize) {
    let indent = "  ".repeat(depth);
    let name = msg.name.as_deref().unwrap_or("(unnamed)");

    // Skip map entry synthetic types
    if msg.options.as_ref().is_some_and(|o| o.map_entry()) {
        return;
    }

    println!("{indent}Message: {name}");

    for field in &msg.field {
        let fname = field.name.as_deref().unwrap_or("(unnamed)");
        let fnum = field.number.unwrap_or(0);
        let ftype = field_type_name(field);
        let label = field_label_name(field);
        println!("{indent}  {label}{ftype} {fname} = {fnum};");
    }

    // Nested types
    for nested in &msg.nested_type {
        describe_message(nested, depth + 1);
    }
    for en in &msg.enum_type {
        describe_enum(en, depth + 1);
    }
}

fn describe_enum(en: &prost_types::EnumDescriptorProto, depth: usize) {
    let indent = "  ".repeat(depth);
    let name = en.name.as_deref().unwrap_or("(unnamed)");
    println!("{indent}Enum: {name}");
    for val in &en.value {
        let vname = val.name.as_deref().unwrap_or("(unnamed)");
        let vnum = val.number.unwrap_or(0);
        println!("{indent}  {vname} = {vnum};");
    }
}

fn describe_service(svc: &prost_types::ServiceDescriptorProto) {
    let name = svc.name.as_deref().unwrap_or("(unnamed)");
    println!("  Service: {name}");
    for method in &svc.method {
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
        let client_stream = if method.client_streaming.unwrap_or(false) {
            "stream "
        } else {
            ""
        };
        let server_stream = if method.server_streaming.unwrap_or(false) {
            "stream "
        } else {
            ""
        };
        println!("    rpc {mname}({client_stream}{input}) returns ({server_stream}{output});");
    }
}

fn field_type_name(field: &prost_types::FieldDescriptorProto) -> String {
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

fn field_label_name(field: &prost_types::FieldDescriptorProto) -> &'static str {
    use prost_types::field_descriptor_proto::Label;
    let l = field.label.unwrap_or(Label::Optional as i32);
    if l == Label::Repeated as i32 {
        "repeated "
    } else if l == Label::Required as i32 {
        "required "
    } else {
        ""
    }
}
