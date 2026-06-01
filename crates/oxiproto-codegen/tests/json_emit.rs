// Tests that verify JSON codegen produces valid Rust code containing the
// expected method signatures.  These tests check syntax (via `syn`) and
// content (via substring matching) but do NOT execute the generated code.

use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FileDescriptorProto, FileDescriptorSet, OneofDescriptorProto,
};

// ── helpers ────────────────────────────────────────────────────────────────────

fn make_field(name: &str, number: i32, r#type: Type, label: Label) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_string()),
        number: Some(number),
        label: Some(label as i32),
        r#type: Some(r#type as i32),
        json_name: Some(to_camel_case(name)),
        ..Default::default()
    }
}

fn make_enum_field(name: &str, number: i32, type_name: &str) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_string()),
        number: Some(number),
        label: Some(Label::Optional as i32),
        r#type: Some(Type::Enum as i32),
        type_name: Some(type_name.to_string()),
        json_name: Some(to_camel_case(name)),
        ..Default::default()
    }
}

fn make_message_field(name: &str, number: i32, type_name: &str) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_string()),
        number: Some(number),
        label: Some(Label::Optional as i32),
        r#type: Some(Type::Message as i32),
        type_name: Some(type_name.to_string()),
        json_name: Some(to_camel_case(name)),
        ..Default::default()
    }
}

fn make_repeated_field(name: &str, number: i32, r#type: Type) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_string()),
        number: Some(number),
        label: Some(Label::Repeated as i32),
        r#type: Some(r#type as i32),
        json_name: Some(to_camel_case(name)),
        ..Default::default()
    }
}

fn make_oneof_field(
    name: &str,
    number: i32,
    r#type: Type,
    oneof_index: i32,
) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_string()),
        number: Some(number),
        label: Some(Label::Optional as i32),
        r#type: Some(r#type as i32),
        json_name: Some(to_camel_case(name)),
        oneof_index: Some(oneof_index),
        ..Default::default()
    }
}

fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut next_upper = false;
    for c in s.chars() {
        if c == '_' {
            next_upper = true;
        } else if next_upper {
            result.extend(c.to_uppercase());
            next_upper = false;
        } else {
            result.push(c);
        }
    }
    result
}

fn make_status_enum() -> EnumDescriptorProto {
    EnumDescriptorProto {
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
    }
}

fn gen_with_json(fds: &FileDescriptorSet) -> String {
    let mut opts = oxiproto_codegen::CodegenOptions::new();
    opts.emit_json = true;
    oxiproto_codegen::generate_with_options(fds, &opts).expect("codegen must succeed")
}

fn assert_valid_rust(code: &str) {
    syn::parse_str::<syn::File>(code)
        .unwrap_or_else(|e| panic!("Generated code failed to parse: {e}\n\nCode:\n{code}"));
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[test]
fn json_emit_produces_to_json_and_from_json() {
    let msg = DescriptorProto {
        name: Some("Scalars".to_string()),
        field: vec![
            make_field("name", 1, Type::String, Label::Optional),
            make_field("count", 2, Type::Int32, Label::Optional),
            make_field("big_count", 3, Type::Int64, Label::Optional),
            make_field("active", 4, Type::Bool, Label::Optional),
            make_field("score", 5, Type::Float, Label::Optional),
            make_field("ratio", 6, Type::Double, Label::Optional),
            make_field("data", 7, Type::Bytes, Label::Optional),
        ],
        ..Default::default()
    };
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("scalars.proto".to_string()),
            package: Some("".to_string()),
            message_type: vec![msg],
            ..Default::default()
        }],
    };

    let code = gen_with_json(&fds);
    assert_valid_rust(&code);
    assert!(
        code.contains("pub fn to_json"),
        "Missing to_json in:\n{code}"
    );
    assert!(
        code.contains("pub fn from_json"),
        "Missing from_json in:\n{code}"
    );
    assert!(code.contains("JsonError"), "Missing JsonError in:\n{code}");
    assert!(
        code.contains("_json_type"),
        "Missing _json_type in:\n{code}"
    );
}

#[test]
fn json_emit_repeated_fields() {
    let msg = DescriptorProto {
        name: Some("Lists".to_string()),
        field: vec![
            make_repeated_field("tags", 1, Type::String),
            make_repeated_field("scores", 2, Type::Int32),
            make_repeated_field("ids", 3, Type::Int64),
            make_repeated_field("flags", 4, Type::Bool),
        ],
        ..Default::default()
    };
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("lists.proto".to_string()),
            message_type: vec![msg],
            ..Default::default()
        }],
    };

    let code = gen_with_json(&fds);
    assert_valid_rust(&code);
    assert!(code.contains("pub fn to_json"), "Missing to_json:\n{code}");
    assert!(
        code.contains("pub fn from_json"),
        "Missing from_json:\n{code}"
    );
    assert!(
        code.contains("Array"),
        "Repeated fields should use Array:\n{code}"
    );
}

#[test]
fn json_emit_enum_has_to_json_str_and_from_json_value() {
    let en = make_status_enum();
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("status.proto".to_string()),
            enum_type: vec![en],
            ..Default::default()
        }],
    };

    let code = gen_with_json(&fds);
    assert_valid_rust(&code);
    assert!(
        code.contains("pub fn to_json_str"),
        "Missing to_json_str:\n{code}"
    );
    assert!(
        code.contains("pub fn from_json_value"),
        "Missing from_json_value:\n{code}"
    );
    assert!(
        code.contains("\"UNKNOWN\""),
        "Should contain UNKNOWN variant name:\n{code}"
    );
    assert!(
        code.contains("\"ACTIVE\""),
        "Should contain ACTIVE variant name:\n{code}"
    );
}

#[test]
fn json_emit_nested_message() {
    let inner = DescriptorProto {
        name: Some("Address".to_string()),
        field: vec![make_field("street", 1, Type::String, Label::Optional)],
        ..Default::default()
    };
    let outer = DescriptorProto {
        name: Some("Person".to_string()),
        field: vec![
            make_field("name", 1, Type::String, Label::Optional),
            make_message_field("address", 2, ".Address"),
        ],
        ..Default::default()
    };
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("nested.proto".to_string()),
            message_type: vec![inner, outer],
            ..Default::default()
        }],
    };

    let code = gen_with_json(&fds);
    assert_valid_rust(&code);
    assert!(code.contains("pub fn to_json"), "Missing to_json:\n{code}");
}

#[test]
fn json_emit_oneof() {
    let msg = DescriptorProto {
        name: Some("OneofMsg".to_string()),
        field: vec![
            make_oneof_field("int_val", 1, Type::Int32, 0),
            make_oneof_field("str_val", 2, Type::String, 0),
        ],
        oneof_decl: vec![OneofDescriptorProto {
            name: Some("value".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("oneof.proto".to_string()),
            message_type: vec![msg],
            ..Default::default()
        }],
    };

    let code = gen_with_json(&fds);
    assert_valid_rust(&code);
    assert!(code.contains("pub fn to_json"), "Missing to_json:\n{code}");
    assert!(
        code.contains("pub fn from_json"),
        "Missing from_json:\n{code}"
    );
}

#[test]
fn json_emit_enum_field_in_message() {
    let en = make_status_enum();
    let msg = DescriptorProto {
        name: Some("Task".to_string()),
        field: vec![
            make_field("title", 1, Type::String, Label::Optional),
            make_enum_field("status", 2, ".Status"),
        ],
        ..Default::default()
    };
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("task.proto".to_string()),
            message_type: vec![msg],
            enum_type: vec![en],
            ..Default::default()
        }],
    };

    let code = gen_with_json(&fds);
    assert_valid_rust(&code);
    assert!(code.contains("pub fn to_json"), "Missing to_json:\n{code}");
    assert!(
        code.contains("to_json_str"),
        "Enum field should use to_json_str:\n{code}"
    );
}

#[test]
fn json_emit_bytes_field_uses_base64() {
    let msg = DescriptorProto {
        name: Some("BinaryMsg".to_string()),
        field: vec![make_field("payload", 1, Type::Bytes, Label::Optional)],
        ..Default::default()
    };
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("binary.proto".to_string()),
            message_type: vec![msg],
            ..Default::default()
        }],
    };

    let code = gen_with_json(&fds);
    assert_valid_rust(&code);
    assert!(
        code.contains("STANDARD"),
        "bytes field should use STANDARD base64:\n{code}"
    );
    assert!(code.contains("base64"), "Should reference base64:\n{code}");
}

#[test]
fn json_emit_int64_uses_string_repr() {
    let msg = DescriptorProto {
        name: Some("BigNums".to_string()),
        field: vec![
            make_field("big_signed", 1, Type::Int64, Label::Optional),
            make_field("big_unsigned", 2, Type::Uint64, Label::Optional),
            make_field("fixed64_val", 3, Type::Fixed64, Label::Optional),
        ],
        ..Default::default()
    };
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("bignums.proto".to_string()),
            message_type: vec![msg],
            ..Default::default()
        }],
    };

    let code = gen_with_json(&fds);
    assert_valid_rust(&code);
    // i64/u64 must be serialised as JSON strings
    assert!(
        code.contains("::serde_json::Value::String"),
        "int64/uint64 should be JSON string:\n{code}"
    );
}

#[test]
fn json_emit_float_nan_inf() {
    let msg = DescriptorProto {
        name: Some("FloatMsg".to_string()),
        field: vec![
            make_field("f32_val", 1, Type::Float, Label::Optional),
            make_field("f64_val", 2, Type::Double, Label::Optional),
        ],
        ..Default::default()
    };
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("float.proto".to_string()),
            message_type: vec![msg],
            ..Default::default()
        }],
    };

    let code = gen_with_json(&fds);
    assert_valid_rust(&code);
    assert!(code.contains("\"NaN\""), "Should handle NaN:\n{code}");
    assert!(code.contains("\"Infinity\""), "Should handle +Inf:\n{code}");
    assert!(
        code.contains("\"-Infinity\""),
        "Should handle -Inf:\n{code}"
    );
}

#[test]
fn json_emit_camel_case_keys() {
    let msg = DescriptorProto {
        name: Some("CamelTest".to_string()),
        field: vec![
            FieldDescriptorProto {
                name: Some("user_id".to_string()),
                number: Some(1),
                label: Some(Label::Optional as i32),
                r#type: Some(Type::Int32 as i32),
                json_name: Some("userId".to_string()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name: Some("first_name".to_string()),
                number: Some(2),
                label: Some(Label::Optional as i32),
                r#type: Some(Type::String as i32),
                json_name: Some("firstName".to_string()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("camel.proto".to_string()),
            message_type: vec![msg],
            ..Default::default()
        }],
    };

    let code = gen_with_json(&fds);
    assert_valid_rust(&code);
    // to_json should use camelCase keys from json_name
    assert!(
        code.contains("\"userId\""),
        "Expected userId key in:\n{code}"
    );
    assert!(
        code.contains("\"firstName\""),
        "Expected firstName key in:\n{code}"
    );
    // from_json should accept both camelCase and snake_case
    assert!(
        code.contains("\"userId\" | \"user_id\"") || code.contains("\"user_id\" | \"userId\""),
        "Expected both userId and user_id in from_json:\n{code}"
    );
}

#[test]
fn package_namespacing_and_emit_json_no_error() {
    // The guard has been lifted: emit_json + package_namespacing must NOT return Err.
    let msg = DescriptorProto {
        name: Some("Msg".to_string()),
        ..Default::default()
    };
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("pkg.proto".to_string()),
            package: Some("foo".to_string()),
            message_type: vec![msg],
            ..Default::default()
        }],
    };

    let mut opts = oxiproto_codegen::CodegenOptions::new();
    opts.emit_json = true;
    opts.package_namespacing = true;

    let result = oxiproto_codegen::generate_with_options(&fds, &opts);
    assert!(
        result.is_ok(),
        "emit_json + package_namespacing should succeed after guard removal, got: {:?}",
        result.err()
    );
    let code = result.unwrap();
    assert_valid_rust(&code);
    // JSON prelude must appear inside the module, not at the root
    assert!(
        code.contains("pub mod foo"),
        "Expected module 'foo' in:\n{code}"
    );
    assert!(
        code.contains("pub fn to_json"),
        "Expected to_json in:\n{code}"
    );
    assert!(
        code.contains("pub fn from_json"),
        "Expected from_json in:\n{code}"
    );
}
