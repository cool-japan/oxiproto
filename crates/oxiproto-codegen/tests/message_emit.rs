//! Tests for `emit_oxi_message_impl` — verifying the OxiMessage/OxiName
//! generated code parses as valid Rust.
//!
//! These tests use hand-crafted `FileDescriptorSet` values (not compiled from
//! .proto files) so they can run without protoc in CI.

use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet};

fn make_field(name: &str, number: i32, r#type: Type, label: Label) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_string()),
        number: Some(number),
        label: Some(label as i32),
        r#type: Some(r#type as i32),
        ..Default::default()
    }
}

/// Build an FDS with all scalar types for OxiMessage impl testing.
fn build_scalars_fds() -> FileDescriptorSet {
    let msg = DescriptorProto {
        name: Some("Scalars".to_string()),
        field: vec![
            make_field("int32_field", 1, Type::Int32, Label::Optional),
            make_field("int64_field", 2, Type::Int64, Label::Optional),
            make_field("uint32_field", 3, Type::Uint32, Label::Optional),
            make_field("uint64_field", 4, Type::Uint64, Label::Optional),
            make_field("sint32_field", 5, Type::Sint32, Label::Optional),
            make_field("sint64_field", 6, Type::Sint64, Label::Optional),
            make_field("bool_field", 7, Type::Bool, Label::Optional),
            make_field("fixed32_field", 8, Type::Fixed32, Label::Optional),
            make_field("sfixed32_field", 9, Type::Sfixed32, Label::Optional),
            make_field("float_field", 10, Type::Float, Label::Optional),
            make_field("fixed64_field", 11, Type::Fixed64, Label::Optional),
            make_field("sfixed64_field", 12, Type::Sfixed64, Label::Optional),
            make_field("double_field", 13, Type::Double, Label::Optional),
            make_field("string_field", 14, Type::String, Label::Optional),
            make_field("bytes_field", 15, Type::Bytes, Label::Optional),
            make_field("repeated_int32", 16, Type::Int32, Label::Repeated),
            make_field("repeated_string", 17, Type::String, Label::Repeated),
            make_field("optional_int32", 18, Type::Int32, Label::Optional),
        ],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("scalars.proto".to_string()),
        package: Some("scalars".to_string()),
        message_type: vec![msg],
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

/// Build an FDS with a nested message for OxiMessage impl testing.
fn build_nested_fds() -> FileDescriptorSet {
    let inner = DescriptorProto {
        name: Some("Inner".to_string()),
        field: vec![make_field("value", 1, Type::Int32, Label::Optional)],
        ..Default::default()
    };

    let mut inner_field = make_field("inner", 2, Type::Message, Label::Optional);
    inner_field.type_name = Some(".nested.Outer.Inner".to_string());

    let outer = DescriptorProto {
        name: Some("Outer".to_string()),
        field: vec![
            make_field("id", 1, Type::Int32, Label::Optional),
            inner_field,
        ],
        nested_type: vec![inner],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("nested.proto".to_string()),
        package: Some("nested".to_string()),
        message_type: vec![outer],
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

/// Build an FDS with a oneof group for OxiMessage impl testing.
fn build_oneof_fds() -> FileDescriptorSet {
    let mut text_field = make_field("text_val", 1, Type::String, Label::Optional);
    text_field.oneof_index = Some(0);
    let mut num_field = make_field("int_val", 2, Type::Int32, Label::Optional);
    num_field.oneof_index = Some(0);

    let msg = DescriptorProto {
        name: Some("WithOneof".to_string()),
        field: vec![text_field, num_field],
        oneof_decl: vec![prost_types::OneofDescriptorProto {
            name: Some("kind".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("oneof.proto".to_string()),
        package: Some("oneof_test".to_string()),
        message_type: vec![msg],
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

/// Generate code with emit_oxi_message_impl=true and verify it parses.
fn gen_with_oxi_impl(fds: &FileDescriptorSet) -> String {
    let options = oxiproto_codegen::CodegenOptions {
        emit_oxi_message_impl: true,
        ..Default::default()
    };
    oxiproto_codegen::generate_with_options(fds, &options)
        .expect("generate_with_options should succeed")
}

#[test]
fn scalars_oxi_message_impl_parses() {
    let fds = build_scalars_fds();
    let code = gen_with_oxi_impl(&fds);

    let parse_result = syn::parse_str::<syn::File>(&code);
    assert!(
        parse_result.is_ok(),
        "Generated code failed to parse: {}\n\nCode:\n{code}",
        parse_result.err().unwrap()
    );

    // OxiMessage impl must be present
    assert!(
        code.contains("impl ::oxiproto_core::OxiMessage for Scalars"),
        "Expected OxiMessage impl in:\n{code}"
    );
    // OxiName impl must be present
    assert!(
        code.contains("impl ::oxiproto_core::OxiName for Scalars"),
        "Expected OxiName impl in:\n{code}"
    );
    // _unknown field must be present
    assert!(
        code.contains("_unknown"),
        "Expected _unknown field in:\n{code}"
    );
}

#[test]
fn nested_oxi_message_impl_parses() {
    let fds = build_nested_fds();
    let code = gen_with_oxi_impl(&fds);

    let parse_result = syn::parse_str::<syn::File>(&code);
    assert!(
        parse_result.is_ok(),
        "Nested OxiMessage code failed to parse: {}\n\nCode:\n{code}",
        parse_result.err().unwrap()
    );

    // Both Outer and Outer_Inner should have OxiMessage impls
    assert!(
        code.contains("impl ::oxiproto_core::OxiMessage for Outer"),
        "Expected OxiMessage for Outer in:\n{code}"
    );
    assert!(
        code.contains("impl ::oxiproto_core::OxiMessage for Outer_Inner"),
        "Expected OxiMessage for Outer_Inner in:\n{code}"
    );
}

#[test]
fn oneof_oxi_message_impl_parses() {
    let fds = build_oneof_fds();
    let code = gen_with_oxi_impl(&fds);

    let parse_result = syn::parse_str::<syn::File>(&code);
    assert!(
        parse_result.is_ok(),
        "Oneof OxiMessage code failed to parse: {}\n\nCode:\n{code}",
        parse_result.err().unwrap()
    );

    assert!(
        code.contains("impl ::oxiproto_core::OxiMessage for WithOneof"),
        "Expected OxiMessage for WithOneof in:\n{code}"
    );
}

#[test]
fn oxi_name_constants_correct() {
    let fds = build_scalars_fds();
    let code = gen_with_oxi_impl(&fds);

    // NAME = "Scalars", PACKAGE = "scalars"
    assert!(
        code.contains("const NAME: &'static str = \"Scalars\""),
        "Expected NAME constant in:\n{code}"
    );
    assert!(
        code.contains("const PACKAGE: &'static str = \"scalars\""),
        "Expected PACKAGE constant in:\n{code}"
    );
}

#[test]
fn map_field_oxi_message_impl_parses() {
    // Build an FDS with a map field
    let map_entry = DescriptorProto {
        name: Some("LabelsEntry".to_string()),
        field: vec![
            make_field("key", 1, Type::String, Label::Optional),
            make_field("value", 2, Type::Int32, Label::Optional),
        ],
        options: Some(prost_types::MessageOptions {
            map_entry: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };

    let mut map_field = make_field("labels", 1, Type::Message, Label::Repeated);
    map_field.type_name = Some(".maps.Container.LabelsEntry".to_string());

    let container = DescriptorProto {
        name: Some("Container".to_string()),
        field: vec![map_field],
        nested_type: vec![map_entry],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("map.proto".to_string()),
        package: Some("maps".to_string()),
        message_type: vec![container],
        ..Default::default()
    };

    let fds = FileDescriptorSet { file: vec![file] };
    let code = gen_with_oxi_impl(&fds);

    let parse_result = syn::parse_str::<syn::File>(&code);
    assert!(
        parse_result.is_ok(),
        "Map OxiMessage code failed to parse: {}\n\nCode:\n{code}",
        parse_result.err().unwrap()
    );

    assert!(
        code.contains("impl ::oxiproto_core::OxiMessage for Container"),
        "Expected OxiMessage for Container in:\n{code}"
    );
}

#[test]
fn oxi_message_impl_deterministic() {
    let fds = build_scalars_fds();
    let code1 = gen_with_oxi_impl(&fds);
    let code2 = gen_with_oxi_impl(&fds);
    assert_eq!(code1, code2, "OxiMessage codegen must be deterministic");
}

#[test]
fn encoded_len_method_present() {
    let fds = build_scalars_fds();
    let code = gen_with_oxi_impl(&fds);
    assert!(
        code.contains("fn encoded_len(&self) -> usize"),
        "Expected encoded_len in generated code:\n{code}"
    );
}

#[test]
fn encode_raw_method_present() {
    let fds = build_scalars_fds();
    let code = gen_with_oxi_impl(&fds);
    assert!(
        code.contains("fn encode_raw(&self, buf: &mut"),
        "Expected encode_raw in generated code:\n{code}"
    );
}

#[test]
fn merge_method_present() {
    let fds = build_scalars_fds();
    let code = gen_with_oxi_impl(&fds);
    assert!(
        code.contains("fn merge(&mut self, buf: &mut"),
        "Expected merge in generated code:\n{code}"
    );
}

#[test]
fn clear_method_present() {
    let fds = build_scalars_fds();
    let code = gen_with_oxi_impl(&fds);
    assert!(
        code.contains("fn clear(&mut self)"),
        "Expected clear in generated code:\n{code}"
    );
}
