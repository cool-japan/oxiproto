#![cfg(feature = "format")]
#![forbid(unsafe_code)]

use oxiproto_codegen::{generate_with_options, CodegenOptions};
use prost_types::{DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet};

fn minimal_fds() -> FileDescriptorSet {
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("test.proto".to_owned()),
            syntax: Some("proto3".to_owned()),
            message_type: vec![DescriptorProto {
                name: Some("HelloMsg".to_owned()),
                field: vec![FieldDescriptorProto {
                    name: Some("value".to_owned()),
                    number: Some(1),
                    r#type: Some(prost_types::field_descriptor_proto::Type::Int32 as i32),
                    label: Some(prost_types::field_descriptor_proto::Label::Optional as i32),
                    json_name: Some("value".to_owned()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
}

/// Strips lines that are plain `//` comments (not doc-comments `///`/`//!`) then
/// collapses all whitespace so token content can be compared across formatting.
fn token_content(src: &str) -> String {
    src.lines()
        .filter(|line| {
            let t = line.trim();
            // Keep the line unless it's a plain `//` comment
            !(t.starts_with("//") && !t.starts_with("///") && !t.starts_with("//!"))
        })
        .flat_map(|line| line.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn formatted_output_is_valid_rust() {
    let opts = CodegenOptions {
        format_output: true,
        ..CodegenOptions::new()
    };
    let code = generate_with_options(&minimal_fds(), &opts).expect("codegen failed");
    // Verify that the output is valid Rust by parsing it again
    let parsed = syn::parse_file(&code);
    assert!(parsed.is_ok(), "formatted output is not valid Rust: {code}");
}

#[test]
fn formatted_is_idempotent() {
    let opts = CodegenOptions {
        format_output: true,
        ..CodegenOptions::new()
    };
    let fds = minimal_fds();
    let code1 = generate_with_options(&fds, &opts).expect("first gen");
    let code2 = generate_with_options(&fds, &opts).expect("second gen");
    assert_eq!(code1, code2, "codegen should be deterministic");
}

#[test]
fn unformatted_differs_only_in_whitespace() {
    let opts_raw = CodegenOptions {
        format_output: false,
        ..CodegenOptions::new()
    };
    let opts_fmt = CodegenOptions {
        format_output: true,
        ..CodegenOptions::new()
    };
    let fds = minimal_fds();
    let raw = generate_with_options(&fds, &opts_raw).expect("raw gen");
    let fmt = generate_with_options(&fds, &opts_fmt).expect("fmt gen");
    // prettyplease drops plain `//` comments; compare only token content,
    // excluding ordinary comment lines from both sides.
    let raw_tokens = token_content(&raw);
    let fmt_tokens = token_content(&fmt);
    assert_eq!(
        raw_tokens, fmt_tokens,
        "formatted and unformatted should have the same tokens (excluding plain comments)"
    );
}
