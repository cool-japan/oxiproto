// Build script for the `oxiproto` facade crate.
//
// Compiles `tests/fixtures/user.proto` via `prost-build` so that the
// integration test can `include!()` the generated code and perform
// wire-byte cross-validation against the hand-written `OxiMessage` impl.

fn main() {
    let proto_dir = "tests/fixtures";
    let proto_file = "tests/fixtures/user.proto";

    // Re-run if the proto changes or if this script itself changes.
    println!("cargo:rerun-if-changed={proto_file}");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR must be set by Cargo");

    prost_build::Config::new()
        .out_dir(&out_dir)
        // Accept proto3 `optional` fields even on older bundled `protoc` (< 3.15).
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile_protos(&[proto_file], &[proto_dir])
        .expect("prost-build failed to compile user.proto");

    // JSON runtime harness (only when json-runtime-harness feature is active)
    if std::env::var("CARGO_FEATURE_JSON_RUNTIME_HARNESS").is_ok() {
        emit_json_test_fixture();
    }
}

fn emit_json_test_fixture() {
    use prost_types::field_descriptor_proto::{Label, Type};
    use prost_types::{
        DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
        FileDescriptorProto, FileDescriptorSet, OneofDescriptorProto,
    };

    fn field(
        name: &str,
        num: i32,
        ty: Type,
        label: Label,
        json_name: &str,
    ) -> FieldDescriptorProto {
        FieldDescriptorProto {
            name: Some(name.to_string()),
            number: Some(num),
            r#type: Some(ty as i32),
            label: Some(label as i32),
            json_name: Some(json_name.to_string()),
            ..Default::default()
        }
    }

    // Message: AllScalars — one of each scalar type
    let all_scalars = DescriptorProto {
        name: Some("AllScalars".to_string()),
        field: vec![
            field("int32_val", 1, Type::Int32, Label::Optional, "int32Val"),
            field("int64_val", 2, Type::Int64, Label::Optional, "int64Val"),
            field("uint32_val", 3, Type::Uint32, Label::Optional, "uint32Val"),
            field("uint64_val", 4, Type::Uint64, Label::Optional, "uint64Val"),
            field("float_val", 5, Type::Float, Label::Optional, "floatVal"),
            field("double_val", 6, Type::Double, Label::Optional, "doubleVal"),
            field("bool_val", 7, Type::Bool, Label::Optional, "boolVal"),
            field("string_val", 8, Type::String, Label::Optional, "stringVal"),
            field("bytes_val", 9, Type::Bytes, Label::Optional, "bytesVal"),
        ],
        ..Default::default()
    };

    // Message: BigInts — int64/uint64 for string-repr testing
    let big_ints = DescriptorProto {
        name: Some("BigInts".to_string()),
        field: vec![
            field("signed", 1, Type::Int64, Label::Optional, "signed"),
            field("unsigned", 2, Type::Uint64, Label::Optional, "unsigned"),
        ],
        ..Default::default()
    };

    // Message: BinaryData — bytes field
    let binary_data = DescriptorProto {
        name: Some("BinaryData".to_string()),
        field: vec![field("payload", 1, Type::Bytes, Label::Optional, "payload")],
        ..Default::default()
    };

    // Message: Floats — float/double for NaN/Inf testing
    let floats = DescriptorProto {
        name: Some("Floats".to_string()),
        field: vec![
            field("f32", 1, Type::Float, Label::Optional, "f32"),
            field("f64", 2, Type::Double, Label::Optional, "f64"),
        ],
        ..Default::default()
    };

    // Message: RepMsg — repeated field
    let rep_msg = DescriptorProto {
        name: Some("RepMsg".to_string()),
        field: vec![field("tags", 1, Type::String, Label::Repeated, "tags")],
        ..Default::default()
    };

    // Message: CamelMsg — camelCase json_name test
    let camel_msg = DescriptorProto {
        name: Some("CamelMsg".to_string()),
        field: vec![FieldDescriptorProto {
            name: Some("user_id".to_string()),
            number: Some(1),
            r#type: Some(Type::Int32 as i32),
            label: Some(Label::Optional as i32),
            json_name: Some("userId".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    // Enum: Color
    let color_enum = EnumDescriptorProto {
        name: Some("Color".to_string()),
        value: vec![
            EnumValueDescriptorProto {
                name: Some("COLOR_UNSPECIFIED".to_string()),
                number: Some(0),
                ..Default::default()
            },
            EnumValueDescriptorProto {
                name: Some("RED".to_string()),
                number: Some(1),
                ..Default::default()
            },
            EnumValueDescriptorProto {
                name: Some("GREEN".to_string()),
                number: Some(2),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    // Message: EnumMsg — has a Color field
    let enum_msg = DescriptorProto {
        name: Some("EnumMsg".to_string()),
        field: vec![FieldDescriptorProto {
            name: Some("color".to_string()),
            number: Some(1),
            r#type: Some(Type::Enum as i32),
            label: Some(Label::Optional as i32),
            type_name: Some(".harness.Color".to_string()),
            json_name: Some("color".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    // Message: OneofMsg — tests oneof JSON roundtrip
    let oneof_msg = DescriptorProto {
        name: Some("OneofMsg".to_string()),
        field: vec![
            FieldDescriptorProto {
                name: Some("int_v".to_string()),
                number: Some(1),
                r#type: Some(Type::Int32 as i32),
                label: Some(Label::Optional as i32),
                json_name: Some("intV".to_string()),
                oneof_index: Some(0),
                ..Default::default()
            },
            FieldDescriptorProto {
                name: Some("str_v".to_string()),
                number: Some(2),
                r#type: Some(Type::String as i32),
                label: Some(Label::Optional as i32),
                json_name: Some("strV".to_string()),
                oneof_index: Some(0),
                ..Default::default()
            },
        ],
        oneof_decl: vec![OneofDescriptorProto {
            name: Some("value".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("harness.proto".to_string()),
            package: Some("harness".to_string()),
            syntax: Some("proto3".to_string()),
            message_type: vec![
                all_scalars,
                big_ints,
                binary_data,
                floats,
                rep_msg,
                camel_msg,
                enum_msg,
                oneof_msg,
            ],
            enum_type: vec![color_enum],
            ..Default::default()
        }],
    };

    let mut opts = oxiproto_codegen::CodegenOptions::new();
    opts.emit_json = true;
    let code =
        oxiproto_codegen::generate_with_options(&fds, &opts).expect("json fixture codegen failed");

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR must be set"));
    std::fs::write(out_dir.join("json_test_fixture.rs"), code)
        .expect("failed to write json_test_fixture.rs");
}
