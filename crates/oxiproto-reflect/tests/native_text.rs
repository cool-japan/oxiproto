//! Tests for native DynamicMessage protobuf text format encoding/decoding.

use oxiproto_reflect::native::{DescriptorPool, DynamicMessage, MapKey, Value};
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FileDescriptorProto, FileDescriptorSet,
};

// ---------------------------------------------------------------------------
// Helpers — same FDS structure as native_json tests
// ---------------------------------------------------------------------------

fn make_fds() -> FileDescriptorSet {
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("test.proto".to_owned()),
            syntax: Some("proto3".to_owned()),
            enum_type: vec![EnumDescriptorProto {
                name: Some("Status".to_owned()),
                value: vec![
                    EnumValueDescriptorProto {
                        name: Some("UNKNOWN".to_owned()),
                        number: Some(0),
                        ..Default::default()
                    },
                    EnumValueDescriptorProto {
                        name: Some("ACTIVE".to_owned()),
                        number: Some(1),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            message_type: vec![
                // Synthetic map entry
                DescriptorProto {
                    name: Some("MsgCountsEntry".to_owned()),
                    field: vec![
                        FieldDescriptorProto {
                            name: Some("key".to_owned()),
                            number: Some(1),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::String as i32),
                            json_name: Some("key".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("value".to_owned()),
                            number: Some(2),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Int32 as i32),
                            json_name: Some("value".to_owned()),
                            ..Default::default()
                        },
                    ],
                    options: Some(prost_types::MessageOptions {
                        map_entry: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                DescriptorProto {
                    name: Some("Msg".to_owned()),
                    field: vec![
                        FieldDescriptorProto {
                            name: Some("id".to_owned()),
                            number: Some(1),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Int32 as i32),
                            json_name: Some("id".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("name".to_owned()),
                            number: Some(2),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::String as i32),
                            json_name: Some("name".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("big".to_owned()),
                            number: Some(3),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Int64 as i32),
                            json_name: Some("big".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("flag".to_owned()),
                            number: Some(4),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Bool as i32),
                            json_name: Some("flag".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("ratio".to_owned()),
                            number: Some(5),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Double as i32),
                            json_name: Some("ratio".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("data".to_owned()),
                            number: Some(6),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Bytes as i32),
                            json_name: Some("data".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("status".to_owned()),
                            number: Some(7),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Enum as i32),
                            type_name: Some(".Status".to_owned()),
                            json_name: Some("status".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("tags".to_owned()),
                            number: Some(8),
                            label: Some(Label::Repeated as i32),
                            r#type: Some(Type::String as i32),
                            json_name: Some("tags".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("counts".to_owned()),
                            number: Some(9),
                            label: Some(Label::Repeated as i32),
                            r#type: Some(Type::Message as i32),
                            type_name: Some(".MsgCountsEntry".to_owned()),
                            json_name: Some("counts".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("nested".to_owned()),
                            number: Some(10),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Message as i32),
                            type_name: Some(".Msg".to_owned()),
                            json_name: Some("nested".to_owned()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
    }
}

fn make_pool() -> DescriptorPool {
    DescriptorPool::from_file_descriptor_set(make_fds()).expect("pool construction failed")
}

// ---------------------------------------------------------------------------
// Encoding tests
// ---------------------------------------------------------------------------

#[test]
fn empty_message_encodes_to_empty_string() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let msg = DynamicMessage::new(desc);
    let text = msg.to_text().expect("to_text");
    assert_eq!(text, "");
}

#[test]
fn int32_field_encodes_correctly() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let id_field = desc.get_field(1).expect("field 1");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&id_field, Value::I32(42));
    let text = msg.to_text().expect("to_text");
    assert!(text.contains("id: 42"), "got: {text}");
}

#[test]
fn string_field_encodes_with_quotes() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let name_field = desc.get_field(2).expect("field 2");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&name_field, Value::String("hello world".to_owned()));
    let text = msg.to_text().expect("to_text");
    assert!(text.contains(r#"name: "hello world""#), "got: {text}");
}

#[test]
fn string_with_special_chars_escaped() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let name_field = desc.get_field(2).expect("field 2");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(
        &name_field,
        Value::String("line1\nline2\ttab\"quote".to_owned()),
    );
    let text = msg.to_text().expect("to_text");
    assert!(
        text.contains(r#"name: "line1\nline2\ttab\"quote""#),
        "got: {text}"
    );
}

#[test]
fn bool_true_encodes_as_true() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let flag_field = desc.get_field(4).expect("field 4");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&flag_field, Value::Bool(true));
    let text = msg.to_text().expect("to_text");
    assert!(text.contains("flag: true"), "got: {text}");
}

#[test]
fn float_nan_encodes_as_nan() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let ratio_field = desc.get_field(5).expect("field 5");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&ratio_field, Value::F64(f64::NAN));
    let text = msg.to_text().expect("to_text");
    assert!(text.contains("ratio: nan"), "got: {text}");
}

#[test]
fn float_infinity_encodes_as_inf() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let ratio_field = desc.get_field(5).expect("field 5");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&ratio_field, Value::F64(f64::INFINITY));
    let text = msg.to_text().expect("to_text");
    assert!(text.contains("ratio: inf"), "got: {text}");
}

#[test]
fn float_neg_infinity_encodes_as_neg_inf() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let ratio_field = desc.get_field(5).expect("field 5");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&ratio_field, Value::F64(f64::NEG_INFINITY));
    let text = msg.to_text().expect("to_text");
    assert!(text.contains("ratio: -inf"), "got: {text}");
}

#[test]
fn bytes_field_encodes_as_hex_escaped() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let data_field = desc.get_field(6).expect("field 6");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&data_field, Value::Bytes(vec![0xAB, 0xCD]));
    let text = msg.to_text().expect("to_text");
    assert!(text.contains(r#"data: "\xab\xcd""#), "got: {text}");
}

#[test]
fn enum_field_encodes_as_name() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let status_field = desc.get_field(7).expect("field 7");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&status_field, Value::EnumNumber(1));
    let text = msg.to_text().expect("to_text");
    assert!(text.contains("status: ACTIVE"), "got: {text}");
}

#[test]
fn repeated_field_encodes_as_multiple_lines() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let tags_field = desc.get_field(8).expect("field 8");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(
        &tags_field,
        Value::List(vec![
            Value::String("a".to_owned()),
            Value::String("b".to_owned()),
        ]),
    );
    let text = msg.to_text().expect("to_text");
    assert!(text.contains(r#"tags: "a""#), "got: {text}");
    assert!(text.contains(r#"tags: "b""#), "got: {text}");
}

#[test]
fn nested_message_encodes_with_braces() {
    let pool = make_pool();
    let outer_desc = pool.get_message_by_name("Msg").expect("Msg");
    let nested_field = outer_desc.get_field(10).expect("field 10");

    let inner_desc = pool.get_message_by_name("Msg").expect("Msg");
    let inner_id = inner_desc.get_field(1).expect("field 1");
    let mut inner = DynamicMessage::new(inner_desc);
    inner.set_field(&inner_id, Value::I32(99));

    let mut msg = DynamicMessage::new(outer_desc);
    msg.set_field(&nested_field, Value::Message(Box::new(inner)));

    let text = msg.to_text().expect("to_text");
    assert!(text.contains("nested {"), "got: {text}");
    assert!(text.contains("id: 99"), "got: {text}");
    assert!(text.contains('}'), "got: {text}");
}

// ---------------------------------------------------------------------------
// Decoding tests
// ---------------------------------------------------------------------------

#[test]
fn decode_int32_field() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let text = "id: 42";
    let msg = DynamicMessage::from_text(desc.clone(), text).expect("from_text");
    let id_field = desc.get_field(1).expect("field 1");
    assert_eq!(*msg.get_field(&id_field), Value::I32(42));
}

#[test]
fn decode_string_field() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let text = r#"name: "hello""#;
    let msg = DynamicMessage::from_text(desc.clone(), text).expect("from_text");
    let name_field = desc.get_field(2).expect("field 2");
    assert_eq!(
        *msg.get_field(&name_field),
        Value::String("hello".to_owned())
    );
}

#[test]
fn decode_bool_field() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let text = "flag: true";
    let msg = DynamicMessage::from_text(desc.clone(), text).expect("from_text");
    let flag_field = desc.get_field(4).expect("field 4");
    assert_eq!(*msg.get_field(&flag_field), Value::Bool(true));
}

#[test]
fn decode_nan_float() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let text = "ratio: nan";
    let msg = DynamicMessage::from_text(desc.clone(), text).expect("from_text");
    let ratio_field = desc.get_field(5).expect("field 5");
    if let Value::F64(v) = *msg.get_field(&ratio_field) {
        assert!(v.is_nan());
    } else {
        panic!("expected F64");
    }
}

#[test]
fn decode_enum_by_name() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let text = "status: ACTIVE";
    let msg = DynamicMessage::from_text(desc.clone(), text).expect("from_text");
    let status_field = desc.get_field(7).expect("field 7");
    assert_eq!(*msg.get_field(&status_field), Value::EnumNumber(1));
}

#[test]
fn decode_repeated_field() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let text = r#"tags: "a"
tags: "b"
tags: "c"
"#;
    let msg = DynamicMessage::from_text(desc.clone(), text).expect("from_text");
    let tags_field = desc.get_field(8).expect("field 8");
    assert_eq!(
        *msg.get_field(&tags_field),
        Value::List(vec![
            Value::String("a".to_owned()),
            Value::String("b".to_owned()),
            Value::String("c".to_owned()),
        ])
    );
}

#[test]
fn decode_nested_message() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let text = "nested {\n  id: 77\n}\n";
    let msg = DynamicMessage::from_text(desc.clone(), text).expect("from_text");
    let nested_field = desc.get_field(10).expect("field 10");
    if let Value::Message(nested) = msg.get_field(&nested_field).as_ref() {
        let id_f = nested.descriptor().get_field(1).expect("id");
        assert_eq!(*nested.get_field(&id_f), Value::I32(77));
    } else {
        panic!("expected nested message");
    }
}

#[test]
fn decode_skips_unknown_fields() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let text = "id: 5\nunknown_field: 42\n";
    let msg = DynamicMessage::from_text(desc.clone(), text).expect("from_text");
    let id_field = desc.get_field(1).expect("field 1");
    assert_eq!(*msg.get_field(&id_field), Value::I32(5));
}

#[test]
fn decode_skips_comment_lines() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let text = "# this is a comment\nid: 10\n# another comment\n";
    let msg = DynamicMessage::from_text(desc.clone(), text).expect("from_text");
    let id_field = desc.get_field(1).expect("field 1");
    assert_eq!(*msg.get_field(&id_field), Value::I32(10));
}

#[test]
fn decode_hex_escape_in_bytes() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let text = r#"data: "\xde\xad\xbe\xef""#;
    let msg = DynamicMessage::from_text(desc.clone(), text).expect("from_text");
    let data_field = desc.get_field(6).expect("field 6");
    assert_eq!(
        *msg.get_field(&data_field),
        Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF])
    );
}

// ---------------------------------------------------------------------------
// Round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn round_trip_scalars() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let id_field = desc.get_field(1).expect("field 1");
    let name_field = desc.get_field(2).expect("field 2");
    let big_field = desc.get_field(3).expect("field 3");
    let flag_field = desc.get_field(4).expect("field 4");

    let mut original = DynamicMessage::new(desc.clone());
    original.set_field(&id_field, Value::I32(-7));
    original.set_field(&name_field, Value::String("test\nmultiline".to_owned()));
    original.set_field(&big_field, Value::I64(i64::MIN));
    original.set_field(&flag_field, Value::Bool(true));

    let text = original.to_text().expect("to_text");
    let decoded = DynamicMessage::from_text(desc, &text).expect("from_text");

    assert_eq!(original.get_field(&id_field), decoded.get_field(&id_field));
    assert_eq!(
        original.get_field(&name_field),
        decoded.get_field(&name_field)
    );
    assert_eq!(
        original.get_field(&big_field),
        decoded.get_field(&big_field)
    );
    assert_eq!(
        original.get_field(&flag_field),
        decoded.get_field(&flag_field)
    );
}

#[test]
fn round_trip_enum() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let status_field = desc.get_field(7).expect("field 7");

    let mut original = DynamicMessage::new(desc.clone());
    original.set_field(&status_field, Value::EnumNumber(1));

    let text = original.to_text().expect("to_text");
    assert!(text.contains("ACTIVE"), "encoded: {text}");

    let decoded = DynamicMessage::from_text(desc, &text).expect("from_text");
    assert_eq!(
        original.get_field(&status_field),
        decoded.get_field(&status_field)
    );
}

#[test]
fn round_trip_repeated() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let tags_field = desc.get_field(8).expect("field 8");

    let mut original = DynamicMessage::new(desc.clone());
    original.set_field(
        &tags_field,
        Value::List(vec![
            Value::String("x".to_owned()),
            Value::String("y".to_owned()),
            Value::String("z".to_owned()),
        ]),
    );

    let text = original.to_text().expect("to_text");
    let decoded = DynamicMessage::from_text(desc, &text).expect("from_text");
    assert_eq!(
        original.get_field(&tags_field),
        decoded.get_field(&tags_field)
    );
}

#[test]
fn round_trip_map_field() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg");
    let counts_field = desc.get_field(9).expect("field 9");

    let mut map = std::collections::HashMap::new();
    map.insert(MapKey::String("alpha".to_owned()), Value::I32(100));
    map.insert(MapKey::String("beta".to_owned()), Value::I32(200));

    let mut original = DynamicMessage::new(desc.clone());
    original.set_field(&counts_field, Value::Map(map));

    let text = original.to_text().expect("to_text");
    let decoded = DynamicMessage::from_text(desc, &text).expect("from_text");

    // Compare map contents after round-trip.
    if let (Value::Map(orig_map), Value::Map(dec_map)) = (
        original.get_field(&counts_field).as_ref(),
        decoded.get_field(&counts_field).as_ref(),
    ) {
        assert_eq!(orig_map.len(), dec_map.len());
        for (k, v) in orig_map {
            assert_eq!(dec_map.get(k), Some(v));
        }
    } else {
        panic!("expected maps");
    }
}

#[test]
fn round_trip_nested_message() {
    let pool = make_pool();
    let outer_desc = pool.get_message_by_name("Msg").expect("Msg");
    let nested_field = outer_desc.get_field(10).expect("field 10");

    let inner_desc = pool.get_message_by_name("Msg").expect("Msg");
    let inner_id = inner_desc.get_field(1).expect("field 1");
    let mut inner = DynamicMessage::new(inner_desc);
    inner.set_field(&inner_id, Value::I32(42));

    let mut original = DynamicMessage::new(outer_desc.clone());
    original.set_field(&nested_field, Value::Message(Box::new(inner)));

    let text = original.to_text().expect("to_text");
    let decoded = DynamicMessage::from_text(outer_desc, &text).expect("from_text");

    if let Value::Message(nested) = decoded.get_field(&nested_field).as_ref() {
        let id_f = nested.descriptor().get_field(1).expect("id");
        assert_eq!(*nested.get_field(&id_f), Value::I32(42));
    } else {
        panic!("expected nested message");
    }
}
