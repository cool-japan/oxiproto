//! Tests for native DynamicMessage JSON encoding/decoding.

use oxiproto_reflect::native::{DescriptorPool, DynamicMessage, Value};
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FileDescriptorProto, FileDescriptorSet,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a simple FDS for:
/// ```text
/// syntax = "proto3";
/// enum Status { UNKNOWN = 0; ACTIVE = 1; }
/// message Msg {
///   int32 id = 1;
///   string name = 2;
///   int64 big = 3;
///   uint64 ubig = 4;
///   float score = 5;
///   double ratio = 6;
///   bool flag = 7;
///   bytes data = 8;
///   Status status = 9;
///   repeated string tags = 10;
///   map<string, int32> counts = 11;
///   Msg nested = 12;
/// }
/// ```
fn make_fds_full() -> FileDescriptorSet {
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
                // Synthetic map entry for counts: map<string,int32>
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
                            name: Some("ubig".to_owned()),
                            number: Some(4),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Uint64 as i32),
                            json_name: Some("ubig".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("score".to_owned()),
                            number: Some(5),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Float as i32),
                            json_name: Some("score".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("ratio".to_owned()),
                            number: Some(6),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Double as i32),
                            json_name: Some("ratio".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("flag".to_owned()),
                            number: Some(7),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Bool as i32),
                            json_name: Some("flag".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("data".to_owned()),
                            number: Some(8),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Bytes as i32),
                            json_name: Some("data".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("status".to_owned()),
                            number: Some(9),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Enum as i32),
                            type_name: Some(".Status".to_owned()),
                            json_name: Some("status".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("tags".to_owned()),
                            number: Some(10),
                            label: Some(Label::Repeated as i32),
                            r#type: Some(Type::String as i32),
                            json_name: Some("tags".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("counts".to_owned()),
                            number: Some(11),
                            label: Some(Label::Repeated as i32),
                            r#type: Some(Type::Message as i32),
                            type_name: Some(".MsgCountsEntry".to_owned()),
                            json_name: Some("counts".to_owned()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("nested".to_owned()),
                            number: Some(12),
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
    DescriptorPool::from_file_descriptor_set(make_fds_full()).expect("pool construction failed")
}

// ---------------------------------------------------------------------------
// Basic encoding tests
// ---------------------------------------------------------------------------

#[test]
fn empty_message_encodes_to_empty_object() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let msg = DynamicMessage::new(desc);
    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json, serde_json::json!({}));
}

#[test]
fn scalar_int32_encodes_as_number() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let id_field = desc.get_field(1).expect("field 1");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&id_field, Value::I32(42));
    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json["id"], serde_json::json!(42));
}

#[test]
fn int64_encodes_as_string() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let big_field = desc.get_field(3).expect("field 3");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&big_field, Value::I64(i64::MAX));
    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json["big"], serde_json::json!(i64::MAX.to_string()));
}

#[test]
fn uint64_encodes_as_string() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let ubig_field = desc.get_field(4).expect("field 4");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&ubig_field, Value::U64(u64::MAX));
    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json["ubig"], serde_json::json!(u64::MAX.to_string()));
}

#[test]
fn float_nan_encodes_as_string() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let score_field = desc.get_field(5).expect("field 5");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&score_field, Value::F32(f32::NAN));
    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json["score"], serde_json::json!("NaN"));
}

#[test]
fn float_infinity_encodes_as_string() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let ratio_field = desc.get_field(6).expect("field 6");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&ratio_field, Value::F64(f64::INFINITY));
    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json["ratio"], serde_json::json!("Infinity"));
    msg.set_field(&ratio_field, Value::F64(f64::NEG_INFINITY));
    let json2 = msg.to_json().expect("to_json failed");
    assert_eq!(json2["ratio"], serde_json::json!("-Infinity"));
}

#[test]
fn bool_encodes_as_bool() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let flag_field = desc.get_field(7).expect("field 7");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&flag_field, Value::Bool(true));
    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json["flag"], serde_json::json!(true));
}

#[test]
fn bytes_encodes_as_base64() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let data_field = desc.get_field(8).expect("field 8");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&data_field, Value::Bytes(vec![0u8, 1, 2, 255]));
    let json = msg.to_json().expect("to_json failed");
    // base64 of [0,1,2,255] = "AAEC/w=="
    assert_eq!(json["data"], serde_json::json!("AAEC/w=="));
}

#[test]
fn enum_encodes_as_name() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let status_field = desc.get_field(9).expect("field 9");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&status_field, Value::EnumNumber(1));
    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json["status"], serde_json::json!("ACTIVE"));
}

#[test]
fn unknown_enum_number_encodes_as_integer() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let status_field = desc.get_field(9).expect("field 9");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&status_field, Value::EnumNumber(99));
    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json["status"], serde_json::json!(99));
}

#[test]
fn repeated_strings_encode_as_array() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let tags_field = desc.get_field(10).expect("field 10");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(
        &tags_field,
        Value::List(vec![
            Value::String("a".to_owned()),
            Value::String("b".to_owned()),
        ]),
    );
    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json["tags"], serde_json::json!(["a", "b"]));
}

#[test]
fn map_field_encodes_as_object() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let counts_field = desc.get_field(11).expect("field 11");
    let mut map = std::collections::HashMap::new();
    map.insert(
        oxiproto_reflect::native::MapKey::String("alpha".to_owned()),
        Value::I32(1),
    );
    map.insert(
        oxiproto_reflect::native::MapKey::String("beta".to_owned()),
        Value::I32(2),
    );
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&counts_field, Value::Map(map));
    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json["counts"]["alpha"], serde_json::json!(1));
    assert_eq!(json["counts"]["beta"], serde_json::json!(2));
}

#[test]
fn nested_message_encodes_recursively() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let nested_field = desc.get_field(12).expect("field 12");

    // Build inner message.
    let inner_desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let inner_id_field = inner_desc.get_field(1).expect("inner field 1");
    let mut inner = DynamicMessage::new(inner_desc);
    inner.set_field(&inner_id_field, Value::I32(99));

    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&nested_field, Value::Message(Box::new(inner)));

    let json = msg.to_json().expect("to_json failed");
    assert_eq!(json["nested"]["id"], serde_json::json!(99));
}

#[test]
fn default_values_omitted_from_output() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let id_field = desc.get_field(1).expect("field 1");
    let name_field = desc.get_field(2).expect("field 2");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&id_field, Value::I32(0)); // default for int32
    msg.set_field(&name_field, Value::String(String::new())); // default for string
    let json = msg.to_json().expect("to_json failed");
    // Both defaults should be omitted.
    assert!(!json.as_object().expect("object").contains_key("id"));
    assert!(!json.as_object().expect("object").contains_key("name"));
}

// ---------------------------------------------------------------------------
// Basic decoding tests
// ---------------------------------------------------------------------------

#[test]
fn decode_empty_object_gives_empty_message() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    assert!(msg.iter_fields().count() == 0);
}

#[test]
fn decode_int32_field() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({"id": 7});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    let id_field = msg.descriptor().get_field(1).expect("field 1");
    assert_eq!(*msg.get_field(&id_field), Value::I32(7));
}

#[test]
fn decode_int64_from_string() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({"big": "9223372036854775807"});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    let big_field = msg.descriptor().get_field(3).expect("field 3");
    assert_eq!(*msg.get_field(&big_field), Value::I64(i64::MAX));
}

#[test]
fn decode_nan_from_string() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({"ratio": "NaN"});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    let ratio_field = msg.descriptor().get_field(6).expect("field 6");
    if let Value::F64(v) = *msg.get_field(&ratio_field) {
        assert!(v.is_nan());
    } else {
        panic!("expected F64");
    }
}

#[test]
fn decode_infinity_from_string() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({"ratio": "Infinity"});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    let ratio_field = msg.descriptor().get_field(6).expect("field 6");
    assert_eq!(*msg.get_field(&ratio_field), Value::F64(f64::INFINITY));
}

#[test]
fn decode_bytes_from_base64() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({"data": "AAEC/w=="});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    let data_field = msg.descriptor().get_field(8).expect("field 8");
    assert_eq!(
        *msg.get_field(&data_field),
        Value::Bytes(vec![0, 1, 2, 255])
    );
}

#[test]
fn decode_enum_from_name() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({"status": "ACTIVE"});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    let status_field = msg.descriptor().get_field(9).expect("field 9");
    assert_eq!(*msg.get_field(&status_field), Value::EnumNumber(1));
}

#[test]
fn decode_enum_from_number() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({"status": 1});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    let status_field = msg.descriptor().get_field(9).expect("field 9");
    assert_eq!(*msg.get_field(&status_field), Value::EnumNumber(1));
}

#[test]
fn decode_repeated_strings() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({"tags": ["x", "y", "z"]});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    let tags_field = msg.descriptor().get_field(10).expect("field 10");
    assert_eq!(
        *msg.get_field(&tags_field),
        Value::List(vec![
            Value::String("x".to_owned()),
            Value::String("y".to_owned()),
            Value::String("z".to_owned()),
        ])
    );
}

#[test]
fn decode_map_field() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({"counts": {"alpha": 1, "beta": 2}});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    let counts_field = msg.descriptor().get_field(11).expect("field 11");
    if let Value::Map(m) = msg.get_field(&counts_field).as_ref() {
        assert_eq!(
            m.get(&oxiproto_reflect::native::MapKey::String(
                "alpha".to_owned()
            )),
            Some(&Value::I32(1))
        );
        assert_eq!(
            m.get(&oxiproto_reflect::native::MapKey::String("beta".to_owned())),
            Some(&Value::I32(2))
        );
    } else {
        panic!("expected Map");
    }
}

#[test]
fn decode_unknown_keys_skipped() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({"id": 5, "nonexistent_field": "whatever"});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    let id_field = msg.descriptor().get_field(1).expect("field 1");
    assert_eq!(*msg.get_field(&id_field), Value::I32(5));
}

#[test]
fn decode_null_treated_as_default() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let json = serde_json::json!({"id": null});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    // id is not explicitly set (null means default).
    assert!(msg.iter_fields().count() == 0);
}

#[test]
fn decode_accepts_snake_case_key() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    // 'name' is both the snake_case and json_name here.
    let json = serde_json::json!({"name": "hello"});
    let msg = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    let name_field = msg.descriptor().get_field(2).expect("field 2");
    assert_eq!(
        *msg.get_field(&name_field),
        Value::String("hello".to_owned())
    );
}

// ---------------------------------------------------------------------------
// Round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn round_trip_scalars() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let id_field = desc.get_field(1).expect("field 1");
    let name_field = desc.get_field(2).expect("field 2");
    let big_field = desc.get_field(3).expect("field 3");
    let flag_field = desc.get_field(7).expect("field 7");
    let data_field = desc.get_field(8).expect("field 8");

    let mut original = DynamicMessage::new(desc.clone());
    original.set_field(&id_field, Value::I32(42));
    original.set_field(&name_field, Value::String("hello".to_owned()));
    original.set_field(&big_field, Value::I64(-9000000000000i64));
    original.set_field(&flag_field, Value::Bool(true));
    original.set_field(&data_field, Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]));

    let json = original.to_json().expect("to_json failed");
    let decoded = DynamicMessage::from_json(desc, &json).expect("from_json failed");

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
    assert_eq!(
        original.get_field(&data_field),
        decoded.get_field(&data_field)
    );
}

#[test]
fn round_trip_enum() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let status_field = desc.get_field(9).expect("field 9");

    let mut original = DynamicMessage::new(desc.clone());
    original.set_field(&status_field, Value::EnumNumber(1));

    let json = original.to_json().expect("to_json failed");
    // JSON should use the name "ACTIVE".
    assert_eq!(json["status"], serde_json::json!("ACTIVE"));

    let decoded = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    assert_eq!(
        original.get_field(&status_field),
        decoded.get_field(&status_field)
    );
}

#[test]
fn round_trip_repeated() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let tags_field = desc.get_field(10).expect("field 10");

    let mut original = DynamicMessage::new(desc.clone());
    original.set_field(
        &tags_field,
        Value::List(vec![
            Value::String("first".to_owned()),
            Value::String("second".to_owned()),
        ]),
    );

    let json = original.to_json().expect("to_json failed");
    let decoded = DynamicMessage::from_json(desc, &json).expect("from_json failed");
    assert_eq!(
        original.get_field(&tags_field),
        decoded.get_field(&tags_field)
    );
}

#[test]
fn round_trip_nested_message() {
    let pool = make_pool();
    let outer_desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let nested_field = outer_desc.get_field(12).expect("field 12");

    let inner_desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let inner_id = inner_desc.get_field(1).expect("inner field 1");
    let mut inner = DynamicMessage::new(inner_desc);
    inner.set_field(&inner_id, Value::I32(77));

    let mut original = DynamicMessage::new(outer_desc.clone());
    original.set_field(&nested_field, Value::Message(Box::new(inner)));

    let json = original.to_json().expect("to_json failed");
    let decoded = DynamicMessage::from_json(outer_desc, &json).expect("from_json failed");

    // Verify nested message round-tripped.
    if let Value::Message(nested) = decoded.get_field(&nested_field).as_ref() {
        let id_field = nested.descriptor().get_field(1).expect("inner field 1");
        assert_eq!(*nested.get_field(&id_field), Value::I32(77));
    } else {
        panic!("expected nested message");
    }
}

#[test]
fn to_json_string_round_trip() {
    let pool = make_pool();
    let desc = pool.get_message_by_name("Msg").expect("Msg not found");
    let id_field = desc.get_field(1).expect("field 1");
    let mut msg = DynamicMessage::new(desc.clone());
    msg.set_field(&id_field, Value::I32(123));

    let s = msg.to_json_string().expect("to_json_string failed");
    let decoded = DynamicMessage::from_json_str(desc, &s).expect("from_json_str failed");
    assert_eq!(decoded.get_field(&id_field), msg.get_field(&id_field));
}
