use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FileDescriptorProto, FileDescriptorSet,
};
use std::collections::BTreeMap;

fn make_field(name: &str, number: i32, r#type: Type, label: Label) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_string()),
        number: Some(number),
        label: Some(label as i32),
        r#type: Some(r#type as i32),
        ..Default::default()
    }
}

fn build_test_fds() -> FileDescriptorSet {
    // Message: TestMessage { string name = 1; int64 count = 2; repeated string tags = 3; }
    let msg = DescriptorProto {
        name: Some("TestMessage".to_string()),
        field: vec![
            make_field("name", 1, Type::String, Label::Optional),
            make_field("count", 2, Type::Int64, Label::Optional),
            make_field("tags", 3, Type::String, Label::Repeated),
        ],
        ..Default::default()
    };

    // Enum: Status { UNKNOWN = 0; ACTIVE = 1; INACTIVE = 2; }
    let en = EnumDescriptorProto {
        name: Some("Status".to_string()),
        value: vec![
            EnumValueDescriptorProto {
                name: Some("UNKNOWN".to_string()),
                number: Some(0),
                ..Default::default()
            },
            EnumValueDescriptorProto {
                name: Some("ACTIVE".to_string()),
                number: Some(1),
                ..Default::default()
            },
            EnumValueDescriptorProto {
                name: Some("INACTIVE".to_string()),
                number: Some(2),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("test.proto".to_string()),
        package: Some("test".to_string()),
        message_type: vec![msg],
        enum_type: vec![en],
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

#[test]
fn emit_generates_valid_rust() {
    let fds = build_test_fds();
    let code = oxiproto_codegen::generate(&fds).expect("generate should succeed");

    // 1. The output parses as valid Rust with syn
    let syntax: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("Generated code failed to parse: {e}\n\nCode:\n{code}"));

    // 2. Contains the struct name TestMessage
    let has_struct = syntax.items.iter().any(|item| {
        if let syn::Item::Struct(s) = item {
            s.ident == "TestMessage"
        } else {
            false
        }
    });
    assert!(has_struct, "Expected struct TestMessage in:\n{code}");

    // 3. TestMessage has 3 fields: name (String), count (i64), tags (Vec<String>)
    let test_struct = syntax
        .items
        .iter()
        .find_map(|item| {
            if let syn::Item::Struct(s) = item {
                if s.ident == "TestMessage" {
                    Some(s)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .expect("TestMessage struct");

    let fields: Vec<_> = if let syn::Fields::Named(f) = &test_struct.fields {
        f.named.iter().collect()
    } else {
        panic!("Expected named fields")
    };
    assert_eq!(fields.len(), 3, "Expected 3 fields, got {}", fields.len());

    // 4. Contains the enum Status with 3 variants
    let has_enum = syntax.items.iter().any(|item| {
        if let syn::Item::Enum(e) = item {
            e.ident == "Status"
        } else {
            false
        }
    });
    assert!(has_enum, "Expected enum Status in:\n{code}");

    let status_enum = syntax
        .items
        .iter()
        .find_map(|item| {
            if let syn::Item::Enum(e) = item {
                if e.ident == "Status" {
                    Some(e)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .expect("Status enum");
    assert_eq!(status_enum.variants.len(), 3, "Expected 3 enum variants");

    // 5. Variants have explicit discriminants
    let unknown = status_enum
        .variants
        .iter()
        .find(|v| v.ident == "Unknown")
        .expect("Unknown variant");
    assert!(
        unknown.discriminant.is_some(),
        "Expected explicit discriminant on Unknown"
    );
}

#[test]
fn write_to_file() {
    let fds = build_test_fds();
    let path = std::env::temp_dir().join("oxiproto_codegen_test.rs");
    oxiproto_codegen::generate_to_file(&fds, &path).expect("write_to_file should succeed");
    let content = std::fs::read_to_string(&path).expect("read generated file");
    assert!(
        content.contains("TestMessage"),
        "File should contain TestMessage"
    );
}

/// Build an FDS exercising map fields. The map field references a synthetic
/// nested map entry message named `LabelsEntry` with `map_entry` option set.
fn build_map_fds() -> FileDescriptorSet {
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
    map_field.type_name = Some(".test.Container.LabelsEntry".to_string());

    let container = DescriptorProto {
        name: Some("Container".to_string()),
        field: vec![map_field],
        nested_type: vec![map_entry],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("map.proto".to_string()),
        package: Some("test".to_string()),
        message_type: vec![container],
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

#[test]
fn map_field_generates_hashmap() {
    let fds = build_map_fds();
    let code = oxiproto_codegen::generate(&fds).expect("generate");

    // Output must parse as valid Rust
    let _: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("Generated code failed to parse: {e}\n\nCode:\n{code}"));

    assert!(
        code.contains("HashMap<String, i32>"),
        "expected map<string,int32> -> HashMap<String, i32> in:\n{code}"
    );
    // The synthetic LabelsEntry must NOT be emitted as a struct
    assert!(
        !code.contains("struct Container_LabelsEntry"),
        "map entry type should be inlined, not emitted as a struct:\n{code}"
    );
}

#[test]
fn map_field_with_btree_option() {
    let fds = build_map_fds();
    let options = oxiproto_codegen::CodegenOptions {
        use_btree_map: true,
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &options).expect("generate");
    let _: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("BTreeMap code failed to parse: {e}\n\nCode:\n{code}"));
    assert!(
        code.contains("BTreeMap<String, i32>"),
        "expected BTreeMap with btree option in:\n{code}"
    );
}

/// Build an FDS exercising a oneof group.
fn build_oneof_fds() -> FileDescriptorSet {
    let mut text_field = make_field("text", 1, Type::String, Label::Optional);
    text_field.oneof_index = Some(0);
    let mut number_field = make_field("number", 2, Type::Int32, Label::Optional);
    number_field.oneof_index = Some(0);

    let msg = DescriptorProto {
        name: Some("Payload".to_string()),
        field: vec![text_field, number_field],
        oneof_decl: vec![prost_types::OneofDescriptorProto {
            name: Some("content".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("oneof.proto".to_string()),
        package: Some("test".to_string()),
        message_type: vec![msg],
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

#[test]
fn oneof_generates_enum() {
    let fds = build_oneof_fds();
    let code = oxiproto_codegen::generate(&fds).expect("generate");

    let syntax: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("oneof code failed to parse: {e}\n\nCode:\n{code}"));

    // A oneof enum named Payload_Content must exist with 2 variants
    let oneof_enum = syntax.items.iter().find_map(|item| {
        if let syn::Item::Enum(e) = item {
            if e.ident == "Payload_Content" {
                return Some(e);
            }
        }
        None
    });
    let oneof_enum =
        oneof_enum.unwrap_or_else(|| panic!("expected Payload_Content enum in:\n{code}"));
    assert_eq!(oneof_enum.variants.len(), 2, "oneof should have 2 variants");

    // The Payload struct must have a `content: Option<Payload_Content>` field
    assert!(
        code.contains("content: Option<Payload_Content>"),
        "expected oneof field in struct:\n{code}"
    );
}

#[test]
fn enum_has_default_impl() {
    let fds = build_test_fds();
    let code = oxiproto_codegen::generate(&fds).expect("generate");
    let _: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("code failed to parse: {e}\n\nCode:\n{code}"));

    // Default impl: first variant (Unknown) is the default
    assert!(
        code.contains("impl Default for Status"),
        "expected Default impl for Status enum:\n{code}"
    );
    // from_i32 helper
    assert!(
        code.contains("pub fn from_i32"),
        "expected from_i32 helper:\n{code}"
    );
}

/// With `generate_docs` enabled and `source_code_info` populated, the emitted
/// doc comments must be correctly indented so the output still parses as
/// valid Rust (top-level items have no indent; members get 4 spaces).
#[test]
fn doc_comments_produce_valid_rust() {
    use prost_types::source_code_info::Location;
    use prost_types::SourceCodeInfo;

    let mut fds = build_test_fds();

    // Attach source code info: comment on the top-level message (path [4, 0]),
    // on its first field (path [4, 0, 2, 0]), on the top-level enum (path
    // [5, 0]) and the first enum value (path [5, 0, 2, 0]).
    let file = &mut fds.file[0];
    file.source_code_info = Some(SourceCodeInfo {
        location: vec![
            Location {
                path: vec![4, 0],
                leading_comments: Some(" The primary test message.".to_string()),
                ..Default::default()
            },
            Location {
                path: vec![4, 0, 2, 0],
                leading_comments: Some(" The name of the greeting.".to_string()),
                ..Default::default()
            },
            Location {
                path: vec![5, 0],
                leading_comments: Some(" Lifecycle status.".to_string()),
                ..Default::default()
            },
            Location {
                path: vec![5, 0, 2, 0],
                leading_comments: Some(" Unset / unknown.".to_string()),
                ..Default::default()
            },
        ],
    });

    let options = oxiproto_codegen::CodegenOptions {
        generate_docs: true,
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &options).expect("generate");

    // The whole point of the indent fix: this must still parse.
    let _: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("doc-commented code failed to parse: {e}\n\nCode:\n{code}"));

    // Top-level message comment must be column 0 (no leading spaces).
    assert!(
        code.contains("/// The primary test message."),
        "expected message doc comment:\n{code}"
    );
    // Field comment must be indented 4 spaces inside the struct body.
    assert!(
        code.contains("    /// The name of the greeting."),
        "expected indented field doc comment:\n{code}"
    );
}

// ── New CG-1 tests ────────────────────────────────────────────────────────────

/// Build a 3-level nested FDS for nested-message codegen tests.
fn build_nested_fds() -> FileDescriptorSet {
    let level3 = DescriptorProto {
        name: Some("Level3".to_string()),
        field: vec![make_field("flag", 1, Type::Bool, Label::Optional)],
        ..Default::default()
    };

    let mut level3_field = make_field("inner", 2, Type::Message, Label::Optional);
    level3_field.type_name = Some(".nested.Level1.Level2.Level3".to_string());

    let level2 = DescriptorProto {
        name: Some("Level2".to_string()),
        field: vec![
            make_field("name", 1, Type::String, Label::Optional),
            level3_field,
        ],
        nested_type: vec![level3],
        ..Default::default()
    };

    let mut level2_field = make_field("child", 2, Type::Message, Label::Optional);
    level2_field.type_name = Some(".nested.Level1.Level2".to_string());

    let level1 = DescriptorProto {
        name: Some("Level1".to_string()),
        field: vec![
            make_field("id", 1, Type::Int32, Label::Optional),
            level2_field,
        ],
        nested_type: vec![level2],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("nested.proto".to_string()),
        package: Some("nested".to_string()),
        message_type: vec![level1],
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

#[test]
fn nested_messages_codegen() {
    let fds = build_nested_fds();
    let code = oxiproto_codegen::generate(&fds).expect("generate");

    let syntax: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("Nested code failed to parse: {e}\n\nCode:\n{code}"));

    // Level1 should exist
    let has_level1 = syntax.items.iter().any(|item| {
        if let syn::Item::Struct(s) = item {
            s.ident == "Level1"
        } else {
            false
        }
    });
    assert!(has_level1, "Expected Level1 struct in:\n{code}");

    // Level1_Level2 should exist (nested struct with prefix)
    let has_level1_level2 = syntax.items.iter().any(|item| {
        if let syn::Item::Struct(s) = item {
            s.ident == "Level1_Level2"
        } else {
            false
        }
    });
    assert!(
        has_level1_level2,
        "Expected Level1_Level2 struct in:\n{code}"
    );

    // Level1_Level2_Level3 should exist
    let has_l3 = syntax.items.iter().any(|item| {
        if let syn::Item::Struct(s) = item {
            s.ident == "Level1_Level2_Level3"
        } else {
            false
        }
    });
    assert!(has_l3, "Expected Level1_Level2_Level3 struct in:\n{code}");
}

/// Build a service FDS with all streaming variants.
fn build_service_fds() -> FileDescriptorSet {
    let req = DescriptorProto {
        name: Some("Req".to_string()),
        field: vec![make_field("text", 1, Type::String, Label::Optional)],
        ..Default::default()
    };
    let resp = DescriptorProto {
        name: Some("Resp".to_string()),
        field: vec![make_field("code", 1, Type::Int32, Label::Optional)],
        ..Default::default()
    };

    let make_method =
        |name: &str, client_stream: bool, server_stream: bool| prost_types::MethodDescriptorProto {
            name: Some(name.to_string()),
            input_type: Some(".svc.Req".to_string()),
            output_type: Some(".svc.Resp".to_string()),
            client_streaming: Some(client_stream),
            server_streaming: Some(server_stream),
            ..Default::default()
        };

    let svc = prost_types::ServiceDescriptorProto {
        name: Some("Echo".to_string()),
        method: vec![
            make_method("Unary", false, false),
            make_method("ServerStream", false, true),
            make_method("ClientStream", true, false),
            make_method("Bidi", true, true),
        ],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("services.proto".to_string()),
        package: Some("svc".to_string()),
        message_type: vec![req, resp],
        service: vec![svc],
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

#[test]
fn service_trait_codegen() {
    let fds = build_service_fds();
    let code = oxiproto_codegen::generate(&fds).expect("generate");

    let _: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("Service code failed to parse: {e}\n\nCode:\n{code}"));

    // Echo trait should be present
    assert!(
        code.contains("pub trait Echo"),
        "Expected Echo trait in:\n{code}"
    );
    // All four methods
    assert!(
        code.contains("fn unary("),
        "Expected unary method in:\n{code}"
    );
    assert!(
        code.contains("fn server_stream("),
        "Expected server_stream method in:\n{code}"
    );
    assert!(
        code.contains("fn client_stream("),
        "Expected client_stream method in:\n{code}"
    );
    assert!(
        code.contains("fn bidi("),
        "Expected bidi method in:\n{code}"
    );
    // Streaming types
    assert!(
        code.contains("Vec<Resp>"),
        "Expected Vec<Resp> for server streaming in:\n{code}"
    );
    assert!(
        code.contains("Vec<Req>"),
        "Expected Vec<Req> for client streaming in:\n{code}"
    );
}

/// Build an FDS with a google.protobuf.Timestamp field to test WKT mapping.
fn build_wkt_fds() -> FileDescriptorSet {
    let mut ts_field = make_field("created_at", 1, Type::Message, Label::Optional);
    ts_field.type_name = Some(".google.protobuf.Timestamp".to_string());

    let msg = DescriptorProto {
        name: Some("Event".to_string()),
        field: vec![ts_field],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("event.proto".to_string()),
        package: Some("events".to_string()),
        message_type: vec![msg],
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

#[test]
fn wkt_timestamp_field_mapping() {
    let fds = build_wkt_fds();
    let code = oxiproto_codegen::generate(&fds).expect("generate");

    let _: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("WKT code failed to parse: {e}\n\nCode:\n{code}"));

    // Should use the WKT path, not Option<Box<Timestamp>>
    assert!(
        code.contains("::oxiproto_wkt::Timestamp"),
        "Expected WKT Timestamp type in:\n{code}"
    );
    assert!(
        !code.contains("Option<Box<Timestamp>>"),
        "WKT field should not be Option<Box<Timestamp>>:\n{code}"
    );
}

/// Test package namespacing: foo.bar package → pub mod foo { pub mod bar { ... } }
#[test]
fn package_namespacing_generates_modules() {
    let fds = build_test_fds(); // package = "test"
    let options = oxiproto_codegen::CodegenOptions {
        package_namespacing: true,
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &options).expect("generate");

    let _: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("Namespaced code failed to parse: {e}\n\nCode:\n{code}"));

    assert!(
        code.contains("pub mod test {"),
        "Expected pub mod test in:\n{code}"
    );
}

/// Test that reserved fields are not emitted as struct fields.
/// We construct a descriptor where a field has the same number as a reserved
/// range entry — the codegen must skip it and emit a comment instead.
fn build_reserved_fds() -> FileDescriptorSet {
    use prost_types::descriptor_proto::ReservedRange;

    // Field 2 appears in the reserved_range [2,4), so codegen should skip it
    // and emit "// reserved field 2" instead.
    let msg = DescriptorProto {
        name: Some("WithReserved".to_string()),
        field: vec![
            make_field("active_field", 1, Type::String, Label::Optional),
            // This field has number 2 which falls in the reserved range [2,4)
            make_field("legacy_field", 2, Type::Int32, Label::Optional),
            // This field has the reserved name "old_name"
            make_field("old_name", 5, Type::Bool, Label::Optional),
        ],
        reserved_range: vec![ReservedRange {
            start: Some(2),
            end: Some(4),
        }],
        reserved_name: vec!["old_name".to_string()],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("reserved.proto".to_string()),
        package: Some("test".to_string()),
        message_type: vec![msg],
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

#[test]
fn reserved_fields_skipped() {
    let fds = build_reserved_fds();
    let code = oxiproto_codegen::generate(&fds).expect("generate");

    let _: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("Reserved code failed to parse: {e}\n\nCode:\n{code}"));

    // active_field should be present
    assert!(
        code.contains("active_field"),
        "Expected active_field in:\n{code}"
    );
    // The reserved-number field must be replaced by a comment
    assert!(
        !code.contains("pub legacy_field"),
        "legacy_field (reserved number) must not appear as a pub field:\n{code}"
    );
    // The reserved-name field must be replaced by a comment
    assert!(
        !code.contains("pub old_name"),
        "old_name (reserved name) must not appear as a pub field:\n{code}"
    );
    // Reserved comment should appear
    assert!(
        code.contains("// reserved field"),
        "Expected reserved field comment in:\n{code}"
    );
}

/// Test custom type attribute injection.
#[test]
fn custom_type_attribute_injection() {
    let fds = build_test_fds();
    let mut type_attributes = BTreeMap::new();
    type_attributes.insert(
        "test.TestMessage".to_string(),
        vec!["#[derive(serde::Serialize)]".to_string()],
    );
    let options = oxiproto_codegen::CodegenOptions {
        type_attributes,
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &options).expect("generate");

    assert!(
        code.contains("#[derive(serde::Serialize)]"),
        "Expected custom attribute in:\n{code}"
    );
    // Must appear before the struct
    let attr_pos = code
        .find("#[derive(serde::Serialize)]")
        .unwrap_or(usize::MAX);
    let struct_pos = code.find("pub struct TestMessage").unwrap_or(usize::MAX);
    assert!(
        attr_pos < struct_pos,
        "Custom attribute must precede the struct declaration:\n{code}"
    );
}

/// Test custom field attribute injection.
#[test]
fn custom_field_attribute_injection() {
    let fds = build_test_fds();
    let mut field_attributes = BTreeMap::new();
    field_attributes.insert(
        "test.TestMessage.name".to_string(),
        vec!["#[serde(rename = \"n\")]".to_string()],
    );
    let options = oxiproto_codegen::CodegenOptions {
        field_attributes,
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &options).expect("generate");

    assert!(
        code.contains("#[serde(rename = \"n\")]"),
        "Expected custom field attribute in:\n{code}"
    );
}

/// Test deterministic output: same FDS produces identical output on two runs.
#[test]
fn deterministic_output() {
    let fds = build_test_fds();
    let code1 = oxiproto_codegen::generate(&fds).expect("generate 1");
    let code2 = oxiproto_codegen::generate(&fds).expect("generate 2");
    assert_eq!(code1, code2, "Code generation must be deterministic");
}

/// Test that oneof fields work with services in the same FDS.
#[test]
fn oneof_and_service_combined() {
    let fds = build_service_fds();
    let code = oxiproto_codegen::generate(&fds).expect("generate");
    let _: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("Combined code failed to parse: {e}\n\nCode:\n{code}"));
    assert!(code.contains("pub struct Req"), "Expected Req struct");
    assert!(code.contains("pub struct Resp"), "Expected Resp struct");
    assert!(code.contains("pub trait Echo"), "Expected Echo trait");
}

// ── emit_services toggle tests ────────────────────────────────────────────────

/// With `emit_services: true` (default), the service FDS must produce a
/// `pub trait Echo` in the output.
#[test]
fn test_emit_services_default_true() {
    let fds = build_service_fds();
    let options = oxiproto_codegen::CodegenOptions {
        emit_services: true,
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &options).expect("generate");

    let _: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("code failed to parse: {e}\n\nCode:\n{code}"));

    assert!(
        code.contains("pub trait Echo"),
        "emit_services=true must include the Echo service trait in:\n{code}"
    );
}

/// With `emit_services: false`, the service FDS must NOT produce any `pub trait`
/// definition, while message structs remain present.
#[test]
fn test_emit_services_false_suppresses_services() {
    let fds = build_service_fds();
    let options = oxiproto_codegen::CodegenOptions {
        emit_services: false,
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &options).expect("generate");

    let _: syn::File = syn::parse_str(&code)
        .unwrap_or_else(|e| panic!("code failed to parse: {e}\n\nCode:\n{code}"));

    assert!(
        !code.contains("pub trait"),
        "emit_services=false must suppress all service traits in:\n{code}"
    );
    // Message structs must still be present.
    assert!(
        code.contains("pub struct Req"),
        "Req struct must still appear when emit_services=false:\n{code}"
    );
    assert!(
        code.contains("pub struct Resp"),
        "Resp struct must still appear when emit_services=false:\n{code}"
    );
}

// ── TypeRegistry unit tests (via codegenerated output checking) ───────────────

fn assert_valid_rust(code: &str) {
    let _: syn::File = syn::parse_str(code)
        .unwrap_or_else(|e| panic!("code failed to parse: {e}\n\nCode:\n{code}"));
}

fn gen_namespaced(fds: &FileDescriptorSet) -> String {
    let mut opts = oxiproto_codegen::CodegenOptions::new();
    opts.package_namespacing = true;
    opts.emit_json = false;
    oxiproto_codegen::generate_with_options(fds, &opts).expect("codegen should succeed")
}

fn gen_namespaced_json(fds: &FileDescriptorSet) -> String {
    let mut opts = oxiproto_codegen::CodegenOptions::new();
    opts.package_namespacing = true;
    opts.emit_json = true;
    oxiproto_codegen::generate_with_options(fds, &opts).expect("codegen should succeed")
}

fn make_msg_with_field(
    msg_name: &str,
    field_name: &str,
    field_number: i32,
    ftype: Type,
    type_name: Option<&str>,
) -> DescriptorProto {
    DescriptorProto {
        name: Some(msg_name.to_string()),
        field: vec![FieldDescriptorProto {
            name: Some(field_name.to_string()),
            number: Some(field_number),
            label: Some(Label::Optional as i32),
            r#type: Some(ftype as i32),
            type_name: type_name.map(|s| s.to_string()),
            json_name: Some(field_name.to_string()),
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// Namespaced cross-package struct field: message in "foo" with field of type ".bar.B".
/// The generated field type must contain the relative path "super::bar::B".
#[test]
fn namespaced_struct_field_cross_package() {
    // Package "bar" has message B
    let bar_msg = DescriptorProto {
        name: Some("B".to_string()),
        ..Default::default()
    };
    // Package "foo" has message A with a field of type ".bar.B"
    let foo_msg = make_msg_with_field("A", "b_field", 1, Type::Message, Some(".bar.B"));

    let fds = FileDescriptorSet {
        file: vec![
            FileDescriptorProto {
                name: Some("bar.proto".to_string()),
                package: Some("bar".to_string()),
                message_type: vec![bar_msg],
                ..Default::default()
            },
            FileDescriptorProto {
                name: Some("foo.proto".to_string()),
                package: Some("foo".to_string()),
                message_type: vec![foo_msg],
                ..Default::default()
            },
        ],
    };

    let code = gen_namespaced(&fds);
    assert_valid_rust(&code);
    // The field type must use the relative module path, not the bare "B"
    assert!(
        code.contains("super::bar::B"),
        "Expected 'super::bar::B' in generated code:\n{code}"
    );
}

/// Namespaced cross-package enum field: message in "foo" with field of type ".bar.Color".
/// The generated field type must contain the relative path "super::bar::Color".
#[test]
fn namespaced_struct_field_cross_package_enum() {
    let color_enum = EnumDescriptorProto {
        name: Some("Color".to_string()),
        value: vec![EnumValueDescriptorProto {
            name: Some("RED".to_string()),
            number: Some(0),
            ..Default::default()
        }],
        ..Default::default()
    };
    let foo_msg = make_msg_with_field("A", "color", 1, Type::Enum, Some(".bar.Color"));

    let fds = FileDescriptorSet {
        file: vec![
            FileDescriptorProto {
                name: Some("bar.proto".to_string()),
                package: Some("bar".to_string()),
                enum_type: vec![color_enum],
                ..Default::default()
            },
            FileDescriptorProto {
                name: Some("foo.proto".to_string()),
                package: Some("foo".to_string()),
                message_type: vec![foo_msg],
                ..Default::default()
            },
        ],
    };

    let code = gen_namespaced(&fds);
    assert_valid_rust(&code);
    assert!(
        code.contains("super::bar::Color"),
        "Expected 'super::bar::Color' in generated code:\n{code}"
    );
}

/// Same-package struct field reference: no super:: prefix expected.
#[test]
fn namespaced_struct_field_same_package() {
    let b_msg = DescriptorProto {
        name: Some("B".to_string()),
        ..Default::default()
    };
    let a_msg = make_msg_with_field("A", "b_field", 1, Type::Message, Some(".foo.B"));

    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("foo.proto".to_string()),
            package: Some("foo".to_string()),
            message_type: vec![a_msg, b_msg],
            ..Default::default()
        }],
    };

    let code = gen_namespaced(&fds);
    assert_valid_rust(&code);
    // Same package: must NOT have super::foo::B, just B
    assert!(
        code.contains("Option<Box<B>>"),
        "Expected 'Option<Box<B>>' (no super::) in generated code:\n{code}"
    );
}

/// JSON codegen with cross-package type references under package_namespacing.
#[test]
fn namespaced_json_cross_package() {
    // Package "bar" has message B
    let bar_msg = DescriptorProto {
        name: Some("B".to_string()),
        ..Default::default()
    };
    // Package "foo" has message A with a field of type ".bar.B"
    let foo_msg = make_msg_with_field("A", "b_field", 1, Type::Message, Some(".bar.B"));

    let fds = FileDescriptorSet {
        file: vec![
            FileDescriptorProto {
                name: Some("bar.proto".to_string()),
                package: Some("bar".to_string()),
                message_type: vec![bar_msg],
                ..Default::default()
            },
            FileDescriptorProto {
                name: Some("foo.proto".to_string()),
                package: Some("foo".to_string()),
                message_type: vec![foo_msg],
                ..Default::default()
            },
        ],
    };

    let code = gen_namespaced_json(&fds);
    assert_valid_rust(&code);
    // The JSON impl (from_json) must use the relative path
    assert!(
        code.contains("super::bar::B"),
        "Expected 'super::bar::B' in generated JSON code:\n{code}"
    );
    // JSON prelude must be inside the module, not at root
    assert!(
        code.contains("pub mod foo"),
        "Expected 'pub mod foo' in:\n{code}"
    );
    assert!(
        code.contains("pub fn to_json"),
        "Expected 'to_json' in generated code:\n{code}"
    );
    assert!(
        code.contains("pub fn from_json"),
        "Expected 'from_json' in generated code:\n{code}"
    );
}

/// JSON prelude (JsonError) must appear in each package module under namespacing.
#[test]
fn namespaced_json_prelude_per_module() {
    let a_msg = DescriptorProto {
        name: Some("A".to_string()),
        ..Default::default()
    };
    let b_msg = DescriptorProto {
        name: Some("B".to_string()),
        ..Default::default()
    };

    let fds = FileDescriptorSet {
        file: vec![
            FileDescriptorProto {
                name: Some("foo.proto".to_string()),
                package: Some("foo".to_string()),
                message_type: vec![a_msg],
                ..Default::default()
            },
            FileDescriptorProto {
                name: Some("bar.proto".to_string()),
                package: Some("bar".to_string()),
                message_type: vec![b_msg],
                ..Default::default()
            },
        ],
    };

    let code = gen_namespaced_json(&fds);
    assert_valid_rust(&code);
    // Each module should have its own JsonError
    let count = code.matches("pub enum JsonError").count();
    assert!(
        count >= 2,
        "Expected JsonError defined in each package module, found only {count} occurrence(s):\n{code}"
    );
}

/// Flat layout JSON codegen regression guard: flat output still works correctly.
#[test]
fn flat_layout_json_unchanged() {
    let msg = DescriptorProto {
        name: Some("Item".to_string()),
        field: vec![FieldDescriptorProto {
            name: Some("name".to_string()),
            number: Some(1),
            label: Some(Label::Optional as i32),
            r#type: Some(Type::String as i32),
            json_name: Some("name".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("flat.proto".to_string()),
            package: Some("".to_string()),
            message_type: vec![msg],
            ..Default::default()
        }],
    };

    let mut opts = oxiproto_codegen::CodegenOptions::new();
    opts.emit_json = true;
    opts.package_namespacing = false;
    let code =
        oxiproto_codegen::generate_with_options(&fds, &opts).expect("codegen should succeed");

    assert_valid_rust(&code);
    // Flat layout must still produce to_json, from_json, and JsonError at root level
    assert!(
        code.contains("pub fn to_json"),
        "Expected 'to_json' in flat layout:\n{code}"
    );
    assert!(
        code.contains("pub fn from_json"),
        "Expected 'from_json' in flat layout:\n{code}"
    );
    assert!(
        code.contains("pub enum JsonError"),
        "Expected 'JsonError' in flat layout:\n{code}"
    );
    // No module wrapping
    assert!(
        !code.contains("pub mod"),
        "Expected no pub mod in flat layout:\n{code}"
    );
}

mod module_tree_tests {
    use super::*;

    fn simple_fds(pkg: &str, msg_name: &str) -> FileDescriptorSet {
        FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some(format!("{}.proto", msg_name.to_lowercase())),
                package: if pkg.is_empty() {
                    None
                } else {
                    Some(pkg.to_string())
                },
                syntax: Some("proto3".to_string()),
                message_type: vec![DescriptorProto {
                    name: Some(msg_name.to_string()),
                    field: vec![FieldDescriptorProto {
                        name: Some("value".to_string()),
                        number: Some(1),
                        r#type: Some(Type::Int32 as i32),
                        label: Some(Label::Optional as i32),
                        json_name: Some("value".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        }
    }

    #[test]
    fn generate_module_flat_layout() {
        // Single file, no package → root node with items, no children
        let fds = simple_fds("", "FlatMsg");
        let tree =
            oxiproto_codegen::generate_module(&fds, &oxiproto_codegen::CodegenOptions::new())
                .expect("generate_module must succeed");
        assert!(tree.name.is_empty(), "root must have empty name");
        assert!(!tree.items.is_empty(), "root must have items");
        assert!(tree.children.is_empty(), "no children for no-package");
    }

    #[test]
    fn generate_module_single_package() {
        // package "foo" → root has one child named "foo"
        let fds = simple_fds("foo", "FooMsg");
        let tree =
            oxiproto_codegen::generate_module(&fds, &oxiproto_codegen::CodegenOptions::new())
                .expect("generate_module");
        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].name, "foo");
        assert!(!tree.children[0].items.is_empty());
    }

    #[test]
    fn generate_module_nested_package() {
        // package "foo.bar" → root→foo→bar
        let fds = simple_fds("foo.bar", "BarMsg");
        let tree =
            oxiproto_codegen::generate_module(&fds, &oxiproto_codegen::CodegenOptions::new())
                .expect("generate_module");
        assert_eq!(tree.children.len(), 1, "root has one child: foo");
        assert_eq!(tree.children[0].name, "foo");
        assert_eq!(tree.children[0].children.len(), 1, "foo has one child: bar");
        assert_eq!(tree.children[0].children[0].name, "bar");
        assert!(!tree.children[0].children[0].items.is_empty());
    }

    #[test]
    fn generate_module_sibling_packages() {
        // Two files in "foo" and "bar" → root has two children
        let fds = FileDescriptorSet {
            file: vec![
                FileDescriptorProto {
                    name: Some("foo.proto".to_string()),
                    package: Some("foo".to_string()),
                    syntax: Some("proto3".to_string()),
                    message_type: vec![DescriptorProto {
                        name: Some("FooMsg".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                FileDescriptorProto {
                    name: Some("bar.proto".to_string()),
                    package: Some("bar".to_string()),
                    syntax: Some("proto3".to_string()),
                    message_type: vec![DescriptorProto {
                        name: Some("BarMsg".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
        };
        let tree =
            oxiproto_codegen::generate_module(&fds, &oxiproto_codegen::CodegenOptions::new())
                .expect("generate_module");
        let child_names: Vec<&str> = tree.children.iter().map(|c| c.name.as_str()).collect();
        assert!(child_names.contains(&"foo"), "must have foo child");
        assert!(child_names.contains(&"bar"), "must have bar child");
    }

    #[test]
    fn generate_module_multi_file_same_package() {
        // Two files in same package "pkg" → items kept as separate entries
        let fds = FileDescriptorSet {
            file: vec![
                FileDescriptorProto {
                    name: Some("a.proto".to_string()),
                    package: Some("pkg".to_string()),
                    syntax: Some("proto3".to_string()),
                    message_type: vec![DescriptorProto {
                        name: Some("MsgA".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                FileDescriptorProto {
                    name: Some("b.proto".to_string()),
                    package: Some("pkg".to_string()),
                    syntax: Some("proto3".to_string()),
                    message_type: vec![DescriptorProto {
                        name: Some("MsgB".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
        };
        let tree =
            oxiproto_codegen::generate_module(&fds, &oxiproto_codegen::CodegenOptions::new())
                .expect("generate_module");
        let pkg_node = tree
            .children
            .iter()
            .find(|c| c.name == "pkg")
            .expect("pkg node");
        assert_eq!(pkg_node.items.len(), 2, "two files → two items in pkg node");
    }

    #[test]
    fn generate_module_render_valid_rust() {
        // render() must produce syn-parseable Rust
        let fds = simple_fds("mypkg", "MyMsg");
        let tree =
            oxiproto_codegen::generate_module(&fds, &oxiproto_codegen::CodegenOptions::new())
                .expect("generate_module");
        let code = tree.render();
        assert_valid_rust(&code);
    }

    #[test]
    fn generate_module_is_additive() {
        // generate_with_options still works correctly after adding generate_module
        let fds = simple_fds("mypkg", "AnotherMsg");
        let opts = oxiproto_codegen::CodegenOptions::new();
        let code = oxiproto_codegen::generate_with_options(&fds, &opts)
            .expect("generate_with_options must still work");
        assert!(
            code.contains("AnotherMsg"),
            "generate_with_options regression: {code}"
        );
    }
}

// ─── Builder-pattern generation tests ─────────────────────────────────────────

mod builder_tests {
    use prost_types::field_descriptor_proto::{Label, Type};
    use prost_types::{
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
    };

    fn make_field(name: &str, number: i32, r#type: Type, label: Label) -> FieldDescriptorProto {
        FieldDescriptorProto {
            name: Some(name.to_string()),
            number: Some(number),
            label: Some(label as i32),
            r#type: Some(r#type as i32),
            ..Default::default()
        }
    }

    /// Build an FDS for `Foo` with scalar, repeated, singular-message and map fields.
    ///
    /// ```proto
    /// message Foo {
    ///     int32 count       = 1;
    ///     string label      = 2;
    ///     repeated int32 tags = 3;
    ///     // map<string, int32> attrs = 4  (synthetic nested map entry)
    ///     // (singular message field omitted intentionally — syn-checks are syntax-only)
    /// }
    /// ```
    fn build_builder_fds() -> FileDescriptorSet {
        // Synthetic map entry for attrs: map<string, int32>
        let map_entry = DescriptorProto {
            name: Some("AttrsEntry".to_string()),
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

        let mut map_field = make_field("attrs", 4, Type::Message, Label::Repeated);
        map_field.type_name = Some(".test.Foo.AttrsEntry".to_string());

        let msg = DescriptorProto {
            name: Some("Foo".to_string()),
            field: vec![
                make_field("count", 1, Type::Int32, Label::Optional),
                make_field("label", 2, Type::String, Label::Optional),
                make_field("tags", 3, Type::Int32, Label::Repeated),
                map_field,
            ],
            nested_type: vec![map_entry],
            ..Default::default()
        };

        let file = FileDescriptorProto {
            name: Some("builder.proto".to_string()),
            package: Some("test".to_string()),
            message_type: vec![msg],
            ..Default::default()
        };

        FileDescriptorSet { file: vec![file] }
    }

    fn builder_options() -> oxiproto_codegen::CodegenOptions {
        oxiproto_codegen::CodegenOptions {
            emit_builder: true,
            ..Default::default()
        }
    }

    /// `FooBuilder` struct and `impl FooBuilder` must appear in the output.
    #[test]
    fn builder_generates_struct() {
        let fds = build_builder_fds();
        let code = oxiproto_codegen::generate_with_options(&fds, &builder_options())
            .expect("generate_with_options");
        assert!(
            code.contains("FooBuilder"),
            "expected FooBuilder in output:\n{code}"
        );
        assert!(
            code.contains("impl FooBuilder"),
            "expected impl FooBuilder in output:\n{code}"
        );
    }

    /// The `build()` method must be present and return `Foo`.
    #[test]
    fn builder_has_build_method() {
        let fds = build_builder_fds();
        let code = oxiproto_codegen::generate_with_options(&fds, &builder_options())
            .expect("generate_with_options");
        assert!(
            code.contains("pub fn build(self) -> Foo"),
            "expected build() -> Foo in output:\n{code}"
        );
    }

    /// A scalar setter for `count` must appear.
    #[test]
    fn builder_has_scalar_setter() {
        let fds = build_builder_fds();
        let code = oxiproto_codegen::generate_with_options(&fds, &builder_options())
            .expect("generate_with_options");
        assert!(
            code.contains("pub fn count"),
            "expected scalar setter 'count' in output:\n{code}"
        );
    }

    /// With default options, `FooBuilder` must NOT appear.
    #[test]
    fn builder_disabled_by_default() {
        let fds = build_builder_fds();
        let code = oxiproto_codegen::generate(&fds).expect("generate");
        assert!(
            !code.contains("FooBuilder"),
            "FooBuilder should not appear when emit_builder is false:\n{code}"
        );
    }

    /// The generated code (struct + builder) must be syntactically valid Rust.
    #[test]
    fn builder_output_compiles() {
        let fds = build_builder_fds();
        let code = oxiproto_codegen::generate_with_options(&fds, &builder_options())
            .expect("generate_with_options");

        // Wrap in a module so the generated use-std import and HashMap are in scope.
        let wrapped = format!("use std::collections::HashMap;\n{code}");
        let _: syn::File = syn::parse_str(&wrapped)
            .unwrap_or_else(|e| panic!("Builder code failed to parse: {e}\n\nCode:\n{wrapped}"));
    }
}

// ─── Text-format generation tests ─────────────────────────────────────────────

mod text_format_tests {
    use oxiproto_codegen::CodegenOptions;
    use prost_types::field_descriptor_proto::{Label, Type};
    use prost_types::{
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
    };

    fn make_field(name: &str, number: i32, r#type: Type, label: Label) -> FieldDescriptorProto {
        FieldDescriptorProto {
            name: Some(name.to_string()),
            number: Some(number),
            label: Some(label as i32),
            r#type: Some(r#type as i32),
            ..Default::default()
        }
    }

    /// Build an FDS with a simple message that has a string field and an int field.
    fn build_simple_fds() -> FileDescriptorSet {
        let msg = DescriptorProto {
            name: Some("Greet".to_string()),
            field: vec![
                make_field("name", 1, Type::String, Label::Optional),
                make_field("value", 2, Type::Int32, Label::Optional),
            ],
            ..Default::default()
        };
        FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("greet.proto".to_string()),
                package: Some("test".to_string()),
                message_type: vec![msg],
                ..Default::default()
            }],
        }
    }

    /// Build options with emit_text_format = true.
    fn text_format_options() -> CodegenOptions {
        CodegenOptions {
            emit_text_format: true,
            ..CodegenOptions::new()
        }
    }

    /// With `emit_text_format = true`, the output contains `pub fn to_text_format`.
    #[test]
    fn text_format_generates_method() {
        let fds = build_simple_fds();
        let code = oxiproto_codegen::generate_with_options(&fds, &text_format_options())
            .expect("generate_with_options");
        assert!(
            code.contains("pub fn to_text_format"),
            "Expected 'pub fn to_text_format' in generated code:\n{code}"
        );
    }

    /// Without the flag (default false), the output must NOT contain `to_text_format`.
    #[test]
    fn text_format_disabled_by_default() {
        let fds = build_simple_fds();
        let code = oxiproto_codegen::generate(&fds).expect("generate");
        assert!(
            !code.contains("to_text_format"),
            "to_text_format must not appear when emit_text_format is false:\n{code}"
        );
    }

    /// A string field (`name`) should produce a pattern referencing the field name.
    #[test]
    fn text_format_has_string_field() {
        let fds = build_simple_fds();
        let code = oxiproto_codegen::generate_with_options(&fds, &text_format_options())
            .expect("generate_with_options");
        // The emitter guards on !self.name.is_empty() and uses "name:" literal
        assert!(
            code.contains("self.name"),
            "Expected reference to 'self.name' in text_format impl:\n{code}"
        );
        // The field name literal must appear in the format string
        assert!(
            code.contains("\"name:"),
            "Expected 'name:' literal in text_format impl:\n{code}"
        );
    }

    /// A message field generates an inner `to_text_format()` call.
    #[test]
    fn text_format_nested_message_field() {
        // Build: message Inner {} message Outer { Inner inner = 1; }
        let inner_msg = DescriptorProto {
            name: Some("Inner".to_string()),
            ..Default::default()
        };
        let mut msg_field = make_field("inner", 1, Type::Message, Label::Optional);
        msg_field.type_name = Some(".test.Inner".to_string());
        let outer_msg = DescriptorProto {
            name: Some("Outer".to_string()),
            field: vec![msg_field],
            ..Default::default()
        };
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("nested.proto".to_string()),
                package: Some("test".to_string()),
                message_type: vec![inner_msg, outer_msg],
                ..Default::default()
            }],
        };
        let code = oxiproto_codegen::generate_with_options(&fds, &text_format_options())
            .expect("generate_with_options");
        // The outer message impl should call inner.to_text_format()
        assert!(
            code.contains("to_text_format"),
            "Expected 'to_text_format' in nested message impl:\n{code}"
        );
        assert!(
            code.contains("_inner"),
            "Expected '_inner' variable in nested message impl:\n{code}"
        );
    }

    /// The generated code (struct + to_text_format impl) must be syntactically valid Rust.
    #[test]
    fn text_format_output_compiles() {
        let fds = build_simple_fds();
        let code = oxiproto_codegen::generate_with_options(&fds, &text_format_options())
            .expect("generate_with_options");
        let _: syn::File = syn::parse_str(&code)
            .unwrap_or_else(|e| panic!("text_format code failed to parse: {e}\n\nCode:\n{code}"));
    }

    /// Build an FDS exercising oneof, map, enum, repeated, and bool fields to
    /// validate that all emitter branches produce syntactically valid Rust.
    fn build_complex_fds() -> FileDescriptorSet {
        // Synthetic enum to be used as a field type
        let status_enum = prost_types::EnumDescriptorProto {
            name: Some("Status".to_string()),
            value: vec![
                prost_types::EnumValueDescriptorProto {
                    name: Some("UNKNOWN".to_string()),
                    number: Some(0),
                    ..Default::default()
                },
                prost_types::EnumValueDescriptorProto {
                    name: Some("ACTIVE".to_string()),
                    number: Some(1),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        // Synthetic map entry: map<string, int32>
        let map_entry = DescriptorProto {
            name: Some("TagsEntry".to_string()),
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

        let mut map_field = make_field("tags", 5, Type::Message, Label::Repeated);
        map_field.type_name = Some(".test.Complex.TagsEntry".to_string());

        // Oneof fields
        let mut oneof_str = make_field("text", 1, Type::String, Label::Optional);
        oneof_str.oneof_index = Some(0);
        let mut oneof_num = make_field("number", 2, Type::Int32, Label::Optional);
        oneof_num.oneof_index = Some(0);

        // Enum field (singular)
        let mut enum_field = make_field("status", 3, Type::Enum, Label::Optional);
        enum_field.type_name = Some(".test.Status".to_string());

        // Repeated scalar
        let repeated_field = make_field("scores", 4, Type::Int64, Label::Repeated);

        // Bool field
        let bool_field = make_field("active", 6, Type::Bool, Label::Optional);

        // Repeated bool
        let repeated_bool = make_field("flags", 7, Type::Bool, Label::Repeated);

        let msg = DescriptorProto {
            name: Some("Complex".to_string()),
            field: vec![
                oneof_str,
                oneof_num,
                enum_field,
                repeated_field,
                map_field,
                bool_field,
                repeated_bool,
            ],
            oneof_decl: vec![prost_types::OneofDescriptorProto {
                name: Some("payload".to_string()),
                ..Default::default()
            }],
            nested_type: vec![map_entry],
            ..Default::default()
        };

        FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("complex.proto".to_string()),
                package: Some("test".to_string()),
                message_type: vec![msg],
                enum_type: vec![status_enum],
                ..Default::default()
            }],
        }
    }

    /// The generated code for a message with oneof, map, enum, repeated, and
    /// bool fields must be syntactically valid Rust.
    #[test]
    fn text_format_complex_message_compiles() {
        let fds = build_complex_fds();
        let code = oxiproto_codegen::generate_with_options(&fds, &text_format_options())
            .expect("generate_with_options");
        // Wrap with the map import that the generated code needs.
        let wrapped = format!("use std::collections::HashMap;\n{code}");
        let _: syn::File = syn::parse_str(&wrapped).unwrap_or_else(|e| {
            panic!("Complex text_format code failed to parse: {e}\n\nCode:\n{wrapped}")
        });
        assert!(
            code.contains("pub fn to_text_format"),
            "Complex message must have to_text_format:\n{code}"
        );
        // Oneof match arm must reference the generated enum
        assert!(
            code.contains("Complex_Payload"),
            "Expected 'Complex_Payload' oneof enum reference:\n{code}"
        );
        // Map must iterate sorted keys
        assert!(
            code.contains("_keys.sort()"),
            "Expected sorted map iteration:\n{code}"
        );
    }
}
