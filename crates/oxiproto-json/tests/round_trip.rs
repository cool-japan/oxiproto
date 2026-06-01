use oxiproto_json::{from_json, to_json, JsonCodec, JsonError};
use prost::bytes::Bytes;
use prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, Value};
use prost_types::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FileDescriptorProto, FileDescriptorSet,
};

// ---------------------------------------------------------------------------
// Pool builder helpers
// ---------------------------------------------------------------------------

/// A field descriptor for a primitive proto3 field.
fn field(
    name: &str,
    number: i32,
    ty: prost_types::field_descriptor_proto::Type,
    label: prost_types::field_descriptor_proto::Label,
    json_name: &str,
) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_owned()),
        number: Some(number),
        r#type: Some(ty as i32),
        label: Some(label as i32),
        json_name: Some(json_name.to_owned()),
        ..Default::default()
    }
}

/// Build a [`DescriptorPool`] that contains:
///
/// - `Status` enum with values UNKNOWN(0), ACTIVE(1), INACTIVE(2)
/// - `TestMsg` with:
///   - int32 field `count`          (field 1)
///   - int64 field `big_num`        (field 2)
///   - bytes field `data`           (field 3)
///   - enum field `status`          (field 4)
///   - repeated int32 `scores`      (field 5)
///   - string field `label`         (field 6)
fn build_test_pool() -> (DescriptorPool, prost_reflect::MessageDescriptor) {
    use prost_types::field_descriptor_proto::{Label, Type};

    let status_enum = EnumDescriptorProto {
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
            EnumValueDescriptorProto {
                name: Some("INACTIVE".to_owned()),
                number: Some(2),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let msg = DescriptorProto {
        name: Some("TestMsg".to_owned()),
        field: vec![
            field("count", 1, Type::Int32, Label::Optional, "count"),
            field("big_num", 2, Type::Int64, Label::Optional, "bigNum"),
            field("data", 3, Type::Bytes, Label::Optional, "data"),
            FieldDescriptorProto {
                name: Some("status".to_owned()),
                number: Some(4),
                r#type: Some(Type::Enum as i32),
                label: Some(Label::Optional as i32),
                type_name: Some("Status".to_owned()),
                json_name: Some("status".to_owned()),
                ..Default::default()
            },
            field("scores", 5, Type::Int32, Label::Repeated, "scores"),
            field("label", 6, Type::String, Label::Optional, "label"),
        ],
        enum_type: vec![status_enum],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("test.proto".to_owned()),
        syntax: Some("proto3".to_owned()),
        message_type: vec![msg],
        ..Default::default()
    };
    let fds = FileDescriptorSet { file: vec![file] };
    let bytes = fds.encode_to_vec();
    let pool = DescriptorPool::decode(bytes.as_ref()).expect("pool decode");
    let desc = pool.get_message_by_name("TestMsg").expect("TestMsg");
    (pool, desc)
}

// ---------------------------------------------------------------------------
// Round-trip test
// ---------------------------------------------------------------------------

#[test]
fn round_trip_all_field_types() {
    let (_pool, desc) = build_test_pool();
    let mut orig = DynamicMessage::new(desc.clone());

    // count: int32
    orig.try_set_field(&desc.get_field_by_name("count").unwrap(), Value::I32(42))
        .unwrap();

    // big_num: int64
    orig.try_set_field(
        &desc.get_field_by_name("big_num").unwrap(),
        Value::I64(9_007_199_254_740_993_i64),
    )
    .unwrap();

    // data: bytes
    orig.try_set_field(
        &desc.get_field_by_name("data").unwrap(),
        Value::Bytes(Bytes::from_static(b"hello")),
    )
    .unwrap();

    // status: enum ACTIVE = 1
    orig.try_set_field(
        &desc.get_field_by_name("status").unwrap(),
        Value::EnumNumber(1),
    )
    .unwrap();

    // scores: repeated int32
    orig.try_set_field(
        &desc.get_field_by_name("scores").unwrap(),
        Value::List(vec![Value::I32(10), Value::I32(20), Value::I32(30)]),
    )
    .unwrap();

    // label: string
    orig.try_set_field(
        &desc.get_field_by_name("label").unwrap(),
        Value::String("hello world".to_owned()),
    )
    .unwrap();

    // Serialize to JSON
    let codec = JsonCodec::default();
    let json_val = to_json(&orig, &codec);

    // Deserialize back
    let rebuilt = from_json(&json_val, &desc, &codec).expect("from_json");

    // Compare via wire encoding (robust against PartialEq ordering quirks)
    assert_eq!(
        orig.encode_to_vec(),
        rebuilt.encode_to_vec(),
        "round-trip mismatch:\norig  = {orig:?}\nbuilt = {rebuilt:?}"
    );
}

// ---------------------------------------------------------------------------
// Error test: wrong type for string field
// ---------------------------------------------------------------------------

#[test]
fn from_json_wrong_type_returns_error() {
    let (_pool, desc) = build_test_pool();

    // Pass a JSON number where a string is expected for 'label'
    let json_val = serde_json::json!({
        "label": 42
    });

    let codec = JsonCodec::default();
    let result = from_json(&json_val, &desc, &codec);
    assert!(
        matches!(result, Err(JsonError::WrongType { ref field, .. }) if field == "label"),
        "expected WrongType for label, got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// CamelCase field names
// ---------------------------------------------------------------------------

#[test]
fn camel_case_field_name_in_json() {
    let (_pool, desc) = build_test_pool();
    let mut msg = DynamicMessage::new(desc.clone());
    msg.try_set_field(
        &desc.get_field_by_name("big_num").unwrap(),
        Value::I64(1234),
    )
    .unwrap();

    let codec = JsonCodec::default();
    let json_val = to_json(&msg, &codec);
    let obj = json_val.as_object().unwrap();

    assert!(
        obj.contains_key("bigNum"),
        "expected camelCase key 'bigNum', got keys: {:?}",
        obj.keys().collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// preserve_proto_field_names
// ---------------------------------------------------------------------------

#[test]
fn preserve_proto_field_names_emits_snake_case() {
    let (_pool, desc) = build_test_pool();
    let mut msg = DynamicMessage::new(desc.clone());
    msg.try_set_field(
        &desc.get_field_by_name("big_num").unwrap(),
        Value::I64(1234),
    )
    .unwrap();

    let codec = JsonCodec::default().preserve_proto_field_names(true);
    let json_val = to_json(&msg, &codec);
    let obj = json_val.as_object().unwrap();

    assert!(
        obj.contains_key("big_num"),
        "expected snake_case key 'big_num', got keys: {:?}",
        obj.keys().collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// always_print_fields
// ---------------------------------------------------------------------------

#[test]
fn always_print_fields_includes_defaults() {
    let (_pool, desc) = build_test_pool();
    let msg = DynamicMessage::new(desc.clone());

    // With always_print_fields, even default-valued fields appear
    let codec = JsonCodec::default().always_print_fields(true);
    let json_val = to_json(&msg, &codec);
    let obj = json_val.as_object().unwrap();

    assert!(
        obj.contains_key("count"),
        "always_print_fields should include 'count' even at default 0"
    );
}

// ---------------------------------------------------------------------------
// enum_as_number
// ---------------------------------------------------------------------------

#[test]
fn enum_as_number_emits_integer() {
    let (_pool, desc) = build_test_pool();
    let mut msg = DynamicMessage::new(desc.clone());
    msg.try_set_field(
        &desc.get_field_by_name("status").unwrap(),
        Value::EnumNumber(1),
    )
    .unwrap();

    let codec = JsonCodec::default().emit_enum_as_number(true);
    let json_val = to_json(&msg, &codec);
    let obj = json_val.as_object().unwrap();
    let v = obj.get("status").unwrap();
    assert!(
        v.is_number(),
        "emit_enum_as_number should produce a JSON number"
    );
    assert_eq!(v.as_i64().unwrap(), 1);
}

// ---------------------------------------------------------------------------
// Enum string round-trip
// ---------------------------------------------------------------------------

#[test]
fn enum_string_round_trip() {
    let (_pool, desc) = build_test_pool();
    let mut orig = DynamicMessage::new(desc.clone());
    orig.try_set_field(
        &desc.get_field_by_name("status").unwrap(),
        Value::EnumNumber(2),
    )
    .unwrap();

    let codec = JsonCodec::default();
    let json_val = to_json(&orig, &codec);
    let obj = json_val.as_object().unwrap();
    assert_eq!(obj.get("status").unwrap().as_str().unwrap(), "INACTIVE");

    let rebuilt = from_json(&json_val, &desc, &codec).expect("from_json");
    assert_eq!(orig.encode_to_vec(), rebuilt.encode_to_vec());
}
