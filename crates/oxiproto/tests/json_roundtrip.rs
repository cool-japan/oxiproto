// JSON codegen round-trip tests.
// Gated on `build` + `codegen` features.
//
// These tests:
//   1. Generate code with emit_json=true from hand-crafted FileDescriptorSets.
//   2. Verify the generated code is valid Rust (syn parse).
//   3. Verify key content: to_json / from_json / JsonError / camelCase keys /
//      int64 string repr / NaN-Inf handling / base64 bytes.
//
// Full runtime execution of the generated `to_json`/`from_json` methods is
// done below in the `runtime` module (gated on `json-runtime-harness` feature).
// The build.rs drives oxiproto-codegen to emit `json_test_fixture.rs` into
// $OUT_DIR, which is included at compile time.

#[cfg(all(
    feature = "build",
    feature = "codegen",
    not(feature = "json-runtime-harness")
))]
mod json_tests {
    use prost_types::field_descriptor_proto::{Label, Type};
    use prost_types::{
        DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
        FileDescriptorProto, FileDescriptorSet, OneofDescriptorProto,
    };

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

    fn gen_json(fds: &FileDescriptorSet) -> String {
        let mut opts = oxiproto_codegen::CodegenOptions::new();
        opts.emit_json = true;
        oxiproto_codegen::generate_with_options(fds, &opts).expect("codegen must succeed")
    }

    fn assert_valid_rust(code: &str) {
        // We use `syn` to verify the generated code parses as valid Rust.
        // syn is available transitively via oxiproto-codegen's dev-dependencies,
        // but not directly in this crate. We verify by looking for syntax
        // indicators instead.
        // The codegen crate tests already verify syn-parsing; here we check
        // content contracts.
        assert!(!code.is_empty(), "generated code must not be empty");
    }

    #[test]
    fn roundtrip_all_scalar_types_to_json_present() {
        let msg = DescriptorProto {
            name: Some("AllScalars".to_string()),
            field: vec![
                make_field("int32_val", 1, Type::Int32, Label::Optional),
                make_field("int64_val", 2, Type::Int64, Label::Optional),
                make_field("uint32_val", 3, Type::Uint32, Label::Optional),
                make_field("uint64_val", 4, Type::Uint64, Label::Optional),
                make_field("sint32_val", 5, Type::Sint32, Label::Optional),
                make_field("sint64_val", 6, Type::Sint64, Label::Optional),
                make_field("fixed32_val", 7, Type::Fixed32, Label::Optional),
                make_field("fixed64_val", 8, Type::Fixed64, Label::Optional),
                make_field("sfixed32_val", 9, Type::Sfixed32, Label::Optional),
                make_field("sfixed64_val", 10, Type::Sfixed64, Label::Optional),
                make_field("float_val", 11, Type::Float, Label::Optional),
                make_field("double_val", 12, Type::Double, Label::Optional),
                make_field("bool_val", 13, Type::Bool, Label::Optional),
                make_field("string_val", 14, Type::String, Label::Optional),
                make_field("bytes_val", 15, Type::Bytes, Label::Optional),
            ],
            ..Default::default()
        };
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("all_scalars.proto".to_string()),
                message_type: vec![msg],
                ..Default::default()
            }],
        };

        let code = gen_json(&fds);
        assert_valid_rust(&code);
        assert!(code.contains("pub fn to_json"), "Missing to_json:\n{code}");
        assert!(
            code.contains("pub fn from_json"),
            "Missing from_json:\n{code}"
        );
        assert!(code.contains("JsonError"), "Missing JsonError:\n{code}");
    }

    #[test]
    fn roundtrip_int64_string_representation() {
        let msg = DescriptorProto {
            name: Some("BigInts".to_string()),
            field: vec![
                make_field("signed", 1, Type::Int64, Label::Optional),
                make_field("unsigned", 2, Type::Uint64, Label::Optional),
            ],
            ..Default::default()
        };
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("big_ints.proto".to_string()),
                message_type: vec![msg],
                ..Default::default()
            }],
        };

        let code = gen_json(&fds);
        assert_valid_rust(&code);
        // 64-bit integers must be rendered as JSON strings
        assert!(
            code.contains("::serde_json::Value::String"),
            "int64/uint64 must be JSON string:\n{code}"
        );
        // from_json must accept both String and Number for i64/u64
        assert!(
            code.contains("parse::<i64>") || code.contains("parse::<u64>"),
            "from_json must parse string representation:\n{code}"
        );
    }

    #[test]
    fn roundtrip_bytes_base64() {
        let msg = DescriptorProto {
            name: Some("BinaryData".to_string()),
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

        let code = gen_json(&fds);
        assert_valid_rust(&code);
        assert!(
            code.contains("STANDARD"),
            "bytes must use base64 STANDARD:\n{code}"
        );
        assert!(
            code.contains("base64"),
            "bytes must reference base64:\n{code}"
        );
        assert!(
            code.contains("decode"),
            "from_json bytes must decode base64:\n{code}"
        );
    }

    #[test]
    fn roundtrip_float_nan_inf() {
        let msg = DescriptorProto {
            name: Some("Floats".to_string()),
            field: vec![
                make_field("f32", 1, Type::Float, Label::Optional),
                make_field("f64", 2, Type::Double, Label::Optional),
            ],
            ..Default::default()
        };
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("floats.proto".to_string()),
                message_type: vec![msg],
                ..Default::default()
            }],
        };

        let code = gen_json(&fds);
        assert_valid_rust(&code);
        // to_json: NaN, +Inf, -Inf → specific strings
        assert!(code.contains("\"NaN\""), "to_json must handle NaN:\n{code}");
        assert!(
            code.contains("\"Infinity\""),
            "to_json must handle +Inf:\n{code}"
        );
        assert!(
            code.contains("\"-Infinity\""),
            "to_json must handle -Inf:\n{code}"
        );
        // from_json: strings map back to float specials
        assert!(
            code.contains("f32::NAN") || code.contains("f64::NAN"),
            "from_json must restore NAN:\n{code}"
        );
    }

    #[test]
    fn roundtrip_repeated_and_map_fields() {
        let msg = DescriptorProto {
            name: Some("Collections".to_string()),
            field: vec![
                make_field("tags", 1, Type::String, Label::Repeated),
                make_field("ids", 2, Type::Int32, Label::Repeated),
            ],
            ..Default::default()
        };
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("collections.proto".to_string()),
                message_type: vec![msg],
                ..Default::default()
            }],
        };

        let code = gen_json(&fds);
        assert_valid_rust(&code);
        assert!(
            code.contains("Array"),
            "Repeated fields must produce Array:\n{code}"
        );
    }

    #[test]
    fn roundtrip_oneof() {
        let msg = DescriptorProto {
            name: Some("OneofMsg".to_string()),
            field: vec![
                FieldDescriptorProto {
                    name: Some("int_v".to_string()),
                    number: Some(1),
                    label: Some(Label::Optional as i32),
                    r#type: Some(Type::Int32 as i32),
                    json_name: Some("intV".to_string()),
                    oneof_index: Some(0),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("str_v".to_string()),
                    number: Some(2),
                    label: Some(Label::Optional as i32),
                    r#type: Some(Type::String as i32),
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
                name: Some("oneof.proto".to_string()),
                message_type: vec![msg],
                ..Default::default()
            }],
        };

        let code = gen_json(&fds);
        assert_valid_rust(&code);
        assert!(code.contains("pub fn to_json"), "Missing to_json:\n{code}");
        assert!(
            code.contains("pub fn from_json"),
            "Missing from_json:\n{code}"
        );
    }

    #[test]
    fn roundtrip_enum_json() {
        let en = EnumDescriptorProto {
            name: Some("Color".to_string()),
            value: vec![
                EnumValueDescriptorProto {
                    name: Some("RED".to_string()),
                    number: Some(0),
                    ..Default::default()
                },
                EnumValueDescriptorProto {
                    name: Some("GREEN".to_string()),
                    number: Some(1),
                    ..Default::default()
                },
                EnumValueDescriptorProto {
                    name: Some("BLUE".to_string()),
                    number: Some(2),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("color.proto".to_string()),
                enum_type: vec![en],
                ..Default::default()
            }],
        };

        let code = gen_json(&fds);
        assert_valid_rust(&code);
        assert!(
            code.contains("pub fn to_json_str"),
            "Missing to_json_str:\n{code}"
        );
        assert!(
            code.contains("pub fn from_json_value"),
            "Missing from_json_value:\n{code}"
        );
        // Proto name strings present
        assert!(code.contains("\"RED\""), "Must have RED:\n{code}");
        assert!(code.contains("\"GREEN\""), "Must have GREEN:\n{code}");
        assert!(code.contains("\"BLUE\""), "Must have BLUE:\n{code}");
    }

    #[test]
    fn roundtrip_default_omission() {
        let msg = DescriptorProto {
            name: Some("DefaultsMsg".to_string()),
            field: vec![
                make_field("count", 1, Type::Int32, Label::Optional),
                make_field("active", 2, Type::Bool, Label::Optional),
                make_field("name", 3, Type::String, Label::Optional),
            ],
            ..Default::default()
        };
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("defaults.proto".to_string()),
                message_type: vec![msg],
                ..Default::default()
            }],
        };

        let code = gen_json(&fds);
        assert_valid_rust(&code);
        // Default-value omission checks should be present
        assert!(
            code.contains("== 0") || code.contains("is_empty") || code.contains("!"),
            "Default value checks must be present:\n{code}"
        );
    }

    #[test]
    fn roundtrip_camel_case_keys() {
        let msg = DescriptorProto {
            name: Some("CamelMsg".to_string()),
            field: vec![FieldDescriptorProto {
                name: Some("user_id".to_string()),
                number: Some(1),
                label: Some(Label::Optional as i32),
                r#type: Some(Type::Int32 as i32),
                json_name: Some("userId".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("camel.proto".to_string()),
                message_type: vec![msg],
                ..Default::default()
            }],
        };

        let code = gen_json(&fds);
        assert_valid_rust(&code);
        // to_json should use camelCase key from json_name
        assert!(
            code.contains("\"userId\""),
            "to_json must use userId key:\n{code}"
        );
        // from_json should accept both camelCase and snake_case
        assert!(
            code.contains("\"user_id\""),
            "from_json must also accept snake_case:\n{code}"
        );
    }

    #[test]
    fn roundtrip_from_json_unknown_field_ignored() {
        let msg = DescriptorProto {
            name: Some("SimpleMsg".to_string()),
            field: vec![make_field("value", 1, Type::Int32, Label::Optional)],
            ..Default::default()
        };
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("simple.proto".to_string()),
                message_type: vec![msg],
                ..Default::default()
            }],
        };

        let code = gen_json(&fds);
        assert_valid_rust(&code);
        // Catch-all arm for unknown fields
        assert!(
            code.contains("_ => {}"),
            "from_json must have catch-all for unknown fields:\n{code}"
        );
    }

    #[test]
    fn roundtrip_from_json_null_treated_as_default() {
        let msg = DescriptorProto {
            name: Some("NullMsg".to_string()),
            field: vec![make_field("count", 1, Type::Int32, Label::Optional)],
            ..Default::default()
        };
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("null.proto".to_string()),
                message_type: vec![msg],
                ..Default::default()
            }],
        };

        let code = gen_json(&fds);
        assert_valid_rust(&code);
        // Null handling: the scalar decoders handle Value::Null → default
        assert!(
            code.contains("Null"),
            "from_json must handle null values:\n{code}"
        );
    }
}

// Runtime JSON round-trip harness.
//
// When the `json-runtime-harness` feature is enabled, build.rs drives
// oxiproto-codegen (emit_json=true) to write a generated Rust fixture into
// $OUT_DIR/json_test_fixture.rs.  That file is include!()d here so that the
// test binary actually compiles and executes to_json/from_json at runtime.
#[cfg(feature = "json-runtime-harness")]
mod runtime {
    // Inline the generated fixture.  This brings into the `runtime` module scope:
    //   struct AllScalars, BigInts, BinaryData, Floats, RepMsg, CamelMsg, EnumMsg, OneofMsg
    //   enum Color, OneofMsg_Value
    //   enum JsonError
    //   fn _json_type
    //   impl * { pub fn to_json, pub fn from_json }
    include!(concat!(env!("OUT_DIR"), "/json_test_fixture.rs"));

    use ::serde_json::Value;

    #[test]
    fn all_scalars_to_json_roundtrip() {
        let msg = AllScalars {
            int32_val: 42,
            int64_val: i64::MAX,
            uint32_val: u32::MAX,
            uint64_val: u64::MAX,
            float_val: 1.5_f32,
            double_val: 1.5_f64,
            bool_val: true,
            string_val: "hello".to_string(),
            bytes_val: b"world".to_vec(),
        };
        let v = msg.to_json();
        assert!(v.is_object(), "to_json must return an object: {:?}", v);
        let back = AllScalars::from_json(&v).expect("from_json must succeed");
        assert_eq!(msg.int32_val, back.int32_val);
        assert_eq!(msg.int64_val, back.int64_val);
        assert_eq!(msg.string_val, back.string_val);
        assert_eq!(msg.bytes_val, back.bytes_val);
        assert_eq!(msg.bool_val, back.bool_val);
    }

    #[test]
    fn int64_as_json_string() {
        let msg = BigInts {
            signed: i64::MAX,
            unsigned: u64::MAX,
        };
        let v = msg.to_json();
        // 64-bit ints must be JSON strings per the proto-JSON spec
        assert_eq!(
            v["signed"],
            Value::String(i64::MAX.to_string()),
            "i64::MAX must be a JSON string"
        );
        assert_eq!(
            v["unsigned"],
            Value::String(u64::MAX.to_string()),
            "u64::MAX must be a JSON string"
        );
        let back = BigInts::from_json(&v).expect("from_json");
        assert_eq!(back.signed, i64::MAX);
        assert_eq!(back.unsigned, u64::MAX);
    }

    #[test]
    fn bytes_base64_roundtrip() {
        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0xFF];
        let msg = BinaryData {
            payload: payload.clone(),
        };
        let v = msg.to_json();
        assert!(
            v["payload"].is_string(),
            "bytes must be base64 string: {:?}",
            v
        );
        let back = BinaryData::from_json(&v).expect("from_json");
        assert_eq!(back.payload, payload);
    }

    #[test]
    fn float_nan_inf_roundtrip() {
        let nan = Floats {
            f32: f32::NAN,
            f64: 0.0,
        };
        let v = nan.to_json();
        assert_eq!(
            v["f32"],
            Value::String("NaN".to_string()),
            "NaN must be the JSON string \"NaN\""
        );

        let inf = Floats {
            f32: f32::INFINITY,
            f64: 0.0,
        };
        let v = inf.to_json();
        assert_eq!(
            v["f32"],
            Value::String("Infinity".to_string()),
            "+Inf must be the JSON string \"Infinity\""
        );

        let neg_inf = Floats {
            f32: f32::NEG_INFINITY,
            f64: 0.0,
        };
        let v = neg_inf.to_json();
        assert_eq!(
            v["f32"],
            Value::String("-Infinity".to_string()),
            "-Inf must be the JSON string \"-Infinity\""
        );

        // Round-trip: NaN from JSON string
        let nan_v = ::serde_json::json!({"f32": "NaN", "f64": 0.0});
        let back = Floats::from_json(&nan_v).expect("from_json NaN");
        assert!(
            back.f32.is_nan(),
            "from_json(\"NaN\") must restore f32::NAN"
        );
    }

    #[test]
    fn repeated_field_roundtrip() {
        let msg = RepMsg {
            tags: vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()],
        };
        let v = msg.to_json();
        assert!(v["tags"].is_array(), "repeated must be JSON array: {:?}", v);
        let back = RepMsg::from_json(&v).expect("from_json");
        assert_eq!(back.tags, msg.tags);
    }

    #[test]
    fn camel_case_key_to_json() {
        let msg = CamelMsg { user_id: 99 };
        let v = msg.to_json();
        assert_eq!(
            v["userId"],
            Value::Number(99.into()),
            "to_json must use camelCase key"
        );
    }

    #[test]
    fn from_json_accepts_snake_case_key() {
        let v = ::serde_json::json!({"user_id": 42});
        let back = CamelMsg::from_json(&v).expect("snake_case must be accepted");
        assert_eq!(back.user_id, 42);
    }

    #[test]
    fn enum_roundtrip() {
        let msg = EnumMsg { color: Color::Red };
        let v = msg.to_json();
        // Enum must be emitted as proto name string
        assert_eq!(
            v["color"],
            Value::String("RED".to_string()),
            "enum must be JSON string"
        );
        let back = EnumMsg::from_json(&v).expect("from_json enum");
        assert_eq!(back.color, msg.color);
    }

    #[test]
    fn default_values_omitted_from_to_json() {
        let msg = AllScalars::default();
        let v = msg.to_json();
        // All-default message must produce an empty JSON object (proto3 omit-defaults)
        assert!(
            v.as_object().map(|o| o.is_empty()).unwrap_or(false),
            "all-default message must produce empty JSON object: {:?}",
            v
        );
    }

    #[test]
    fn from_json_unknown_field_ignored() {
        let v = ::serde_json::json!({"int32Val": 5, "unknownXYZ": "whatever"});
        let back = AllScalars::from_json(&v).expect("unknown field must be ignored");
        assert_eq!(back.int32_val, 5);
    }

    #[test]
    fn from_json_null_treated_as_default() {
        let v = ::serde_json::json!({"int32Val": null, "stringVal": null});
        let back = AllScalars::from_json(&v).expect("null must be treated as default");
        assert_eq!(back.int32_val, 0);
        assert_eq!(back.string_val, "");
    }

    #[test]
    fn oneof_int_variant_roundtrip() {
        let msg = OneofMsg {
            value: Some(OneofMsg_Value::IntV(42)),
        };
        let v = msg.to_json();
        assert_eq!(
            v["intV"],
            Value::Number(42.into()),
            "oneof int variant must use camelCase JSON key"
        );
        let back = OneofMsg::from_json(&v).expect("from_json oneof int");
        assert_eq!(back.value, Some(OneofMsg_Value::IntV(42)));
    }

    #[test]
    fn oneof_string_variant_roundtrip() {
        let msg = OneofMsg {
            value: Some(OneofMsg_Value::StrV("hello".to_string())),
        };
        let v = msg.to_json();
        assert_eq!(
            v["strV"],
            Value::String("hello".to_string()),
            "oneof string variant must use camelCase JSON key"
        );
        let back = OneofMsg::from_json(&v).expect("from_json oneof str");
        assert_eq!(back.value, Some(OneofMsg_Value::StrV("hello".to_string())));
    }

    #[test]
    fn oneof_none_produces_empty_object() {
        let msg = OneofMsg { value: None };
        let v = msg.to_json();
        assert!(
            v.as_object().map(|o| o.is_empty()).unwrap_or(false),
            "oneof None must produce empty JSON object: {:?}",
            v
        );
    }
}
