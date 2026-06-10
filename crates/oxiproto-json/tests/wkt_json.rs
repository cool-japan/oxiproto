//! Integration tests for the WKT JSON mapping additions:
//! - 2a: Inf/NaN float decode
//! - 2b: FieldMask encode/decode
//! - 2c: Struct / Value / ListValue encode/decode
//! - 7:  Any encode/decode (regular message and WKT-wrapping)

use oxiproto_json::{from_json, to_json, JsonCodec};
use prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, Value};
use prost_types::{
    field_descriptor_proto::{Label, Type},
    DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
};
use serde_json::json;

// ---------------------------------------------------------------------------
// Helper: pool builder for simple custom messages
// ---------------------------------------------------------------------------

/// Build a one-file pool with the given message name and fields.
/// The file is registered under `"test.proto"`.
fn make_simple_pool(
    msg_name: &str,
    fields: Vec<FieldDescriptorProto>,
) -> (DescriptorPool, prost_reflect::MessageDescriptor) {
    let file = FileDescriptorProto {
        name: Some("test.proto".to_owned()),
        syntax: Some("proto3".to_owned()),
        message_type: vec![DescriptorProto {
            name: Some(msg_name.to_owned()),
            field: fields,
            ..Default::default()
        }],
        ..Default::default()
    };
    let fds = FileDescriptorSet { file: vec![file] };
    let encoded = fds.encode_to_vec();
    let pool = DescriptorPool::decode(encoded.as_ref()).expect("pool decode");
    let desc = pool.get_message_by_name(msg_name).expect("descriptor");
    (pool, desc)
}

/// Build a pool that includes all google WKTs (via global pool clone) plus a
/// custom message named `msg_name` with the given fields.
fn make_wkt_pool(
    msg_name: &str,
    fields: Vec<FieldDescriptorProto>,
) -> (DescriptorPool, prost_reflect::MessageDescriptor) {
    let mut pool = DescriptorPool::global();

    let file = FileDescriptorProto {
        name: Some(format!("{msg_name}.proto")),
        syntax: Some("proto3".to_owned()),
        dependency: vec![
            "google/protobuf/timestamp.proto".to_owned(),
            "google/protobuf/any.proto".to_owned(),
            "google/protobuf/struct.proto".to_owned(),
            "google/protobuf/field_mask.proto".to_owned(),
        ],
        message_type: vec![DescriptorProto {
            name: Some(msg_name.to_owned()),
            field: fields,
            ..Default::default()
        }],
        ..Default::default()
    };

    pool.add_file_descriptor_proto(file)
        .expect("add descriptor proto");
    let desc = pool.get_message_by_name(msg_name).expect("descriptor");
    (pool, desc)
}

// ---------------------------------------------------------------------------
// 2a: Inf/NaN decode
// ---------------------------------------------------------------------------

fn make_float_pool() -> (DescriptorPool, prost_reflect::MessageDescriptor) {
    make_simple_pool(
        "FloatMsg",
        vec![
            FieldDescriptorProto {
                name: Some("f32_val".to_owned()),
                number: Some(1),
                r#type: Some(Type::Float as i32),
                label: Some(Label::Optional as i32),
                json_name: Some("f32Val".to_owned()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name: Some("f64_val".to_owned()),
                number: Some(2),
                r#type: Some(Type::Double as i32),
                label: Some(Label::Optional as i32),
                json_name: Some("f64Val".to_owned()),
                ..Default::default()
            },
        ],
    )
}

#[test]
fn f32_nan_decode() {
    let (_pool, desc) = make_float_pool();
    let codec = JsonCodec::default();
    let json = json!({ "f32Val": "NaN" });
    let msg = from_json(&json, &desc, &codec).expect("from_json");
    let v = msg.get_field_by_name("f32_val").expect("field");
    if let Value::F32(f) = v.as_ref() {
        assert!(f.is_nan(), "expected NaN, got {f}");
    } else {
        panic!("expected F32, got {v:?}");
    }
}

#[test]
fn f32_infinity_decode() {
    let (_pool, desc) = make_float_pool();
    let codec = JsonCodec::default();
    let json = json!({ "f32Val": "Infinity" });
    let msg = from_json(&json, &desc, &codec).expect("from_json");
    let v = msg.get_field_by_name("f32_val").expect("field");
    if let Value::F32(f) = v.as_ref() {
        assert!(f.is_infinite() && *f > 0.0, "expected +Inf, got {f}");
    } else {
        panic!("expected F32, got {v:?}");
    }
}

#[test]
fn f32_neg_infinity_decode() {
    let (_pool, desc) = make_float_pool();
    let codec = JsonCodec::default();
    let json = json!({ "f32Val": "-Infinity" });
    let msg = from_json(&json, &desc, &codec).expect("from_json");
    let v = msg.get_field_by_name("f32_val").expect("field");
    if let Value::F32(f) = v.as_ref() {
        assert!(f.is_infinite() && *f < 0.0, "expected -Inf, got {f}");
    } else {
        panic!("expected F32, got {v:?}");
    }
}

#[test]
fn f64_nan_decode() {
    let (_pool, desc) = make_float_pool();
    let codec = JsonCodec::default();
    let json = json!({ "f64Val": "NaN" });
    let msg = from_json(&json, &desc, &codec).expect("from_json");
    let v = msg.get_field_by_name("f64_val").expect("field");
    if let Value::F64(f) = v.as_ref() {
        assert!(f.is_nan(), "expected NaN, got {f}");
    } else {
        panic!("expected F64, got {v:?}");
    }
}

#[test]
fn f64_infinity_decode() {
    let (_pool, desc) = make_float_pool();
    let codec = JsonCodec::default();
    let json = json!({ "f64Val": "Infinity" });
    let msg = from_json(&json, &desc, &codec).expect("from_json");
    let v = msg.get_field_by_name("f64_val").expect("field");
    if let Value::F64(f) = v.as_ref() {
        assert!(f.is_infinite() && *f > 0.0, "expected +Inf, got {f}");
    } else {
        panic!("expected F64, got {v:?}");
    }
}

#[test]
fn f64_neg_infinity_decode() {
    let (_pool, desc) = make_float_pool();
    let codec = JsonCodec::default();
    let json = json!({ "f64Val": "-Infinity" });
    let msg = from_json(&json, &desc, &codec).expect("from_json");
    let v = msg.get_field_by_name("f64_val").expect("field");
    if let Value::F64(f) = v.as_ref() {
        assert!(f.is_infinite() && *f < 0.0, "expected -Inf, got {f}");
    } else {
        panic!("expected F64, got {v:?}");
    }
}

// ---------------------------------------------------------------------------
// 2b: FieldMask
// ---------------------------------------------------------------------------

fn make_field_mask_pool() -> (DescriptorPool, prost_reflect::MessageDescriptor) {
    let mut pool = DescriptorPool::global();

    let file = FileDescriptorProto {
        name: Some("field_mask_test.proto".to_owned()),
        syntax: Some("proto3".to_owned()),
        dependency: vec!["google/protobuf/field_mask.proto".to_owned()],
        message_type: vec![DescriptorProto {
            name: Some("FieldMaskWrapper".to_owned()),
            field: vec![FieldDescriptorProto {
                name: Some("mask".to_owned()),
                number: Some(1),
                r#type: Some(Type::Message as i32),
                label: Some(Label::Optional as i32),
                type_name: Some(".google.protobuf.FieldMask".to_owned()),
                json_name: Some("mask".to_owned()),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };

    pool.add_file_descriptor_proto(file)
        .expect("add field_mask_test");
    let desc = pool
        .get_message_by_name("FieldMaskWrapper")
        .expect("FieldMaskWrapper");
    (pool, desc)
}

#[test]
fn field_mask_encode_round_trip() {
    let (pool, _wrapper_desc) = make_field_mask_pool();
    let codec = JsonCodec::default();

    // Build a FieldMask DynamicMessage directly
    let fm_desc = pool
        .get_message_by_name("google.protobuf.FieldMask")
        .expect("FieldMask");
    let mut fm_msg = DynamicMessage::new(fm_desc.clone());

    let paths_field = fm_desc.get_field_by_name("paths").expect("paths field");
    fm_msg
        .try_set_field(
            &paths_field,
            Value::List(vec![
                Value::String("foo_bar".to_owned()),
                Value::String("baz_qux".to_owned()),
            ]),
        )
        .expect("set paths");

    // Encode → should be "fooBar,bazQux"
    let json = to_json(&fm_msg, &codec);
    assert_eq!(
        json.as_str().unwrap(),
        "fooBar,bazQux",
        "FieldMask encode: expected 'fooBar,bazQux', got: {json}"
    );

    // Decode → paths should be ["foo_bar", "baz_qux"]
    let decoded = from_json(&json, &fm_desc, &codec).expect("from_json FieldMask");
    let v = decoded.get_field_by_name("paths").expect("paths");
    if let Value::List(list) = v.as_ref() {
        let paths: Vec<&str> = list
            .iter()
            .filter_map(|item| {
                if let Value::String(s) = item {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(paths, vec!["foo_bar", "baz_qux"]);
    } else {
        panic!("expected List, got {v:?}");
    }
}

#[test]
fn field_mask_empty_paths() {
    let (pool, _) = make_field_mask_pool();
    let codec = JsonCodec::default();

    let fm_desc = pool
        .get_message_by_name("google.protobuf.FieldMask")
        .expect("FieldMask");
    let fm_msg = DynamicMessage::new(fm_desc.clone());

    // Empty mask → ""
    let json = to_json(&fm_msg, &codec);
    assert_eq!(json.as_str().unwrap(), "", "empty FieldMask should be ''");

    // Round-trip empty
    let decoded = from_json(&json, &fm_desc, &codec).expect("from_json empty FieldMask");
    let v = decoded.get_field_by_name("paths").expect("paths");
    if let Value::List(list) = v.as_ref() {
        assert!(list.is_empty(), "expected empty list");
    } else {
        panic!("expected List, got {v:?}");
    }
}

// ---------------------------------------------------------------------------
// 2c: Struct / Value / ListValue
// ---------------------------------------------------------------------------

fn get_wkt_descs(
    pool: &DescriptorPool,
) -> (
    prost_reflect::MessageDescriptor,
    prost_reflect::MessageDescriptor,
    prost_reflect::MessageDescriptor,
) {
    let value_desc = pool
        .get_message_by_name("google.protobuf.Value")
        .expect("Value");
    let list_desc = pool
        .get_message_by_name("google.protobuf.ListValue")
        .expect("ListValue");
    let struct_desc = pool
        .get_message_by_name("google.protobuf.Struct")
        .expect("Struct");
    (value_desc, list_desc, struct_desc)
}

#[test]
fn value_null_round_trip() {
    let pool = DescriptorPool::global();
    let (value_desc, _, _) = get_wkt_descs(&pool);
    let codec = JsonCodec::default();

    // Null Value: no kind set → encodes as JSON null
    let msg = DynamicMessage::new(value_desc.clone());
    let json = to_json(&msg, &codec);
    assert_eq!(json, serde_json::Value::Null);

    // Decode null back
    let decoded = from_json(&json, &value_desc, &codec).expect("from_json null Value");
    let re_json = to_json(&decoded, &codec);
    assert_eq!(re_json, serde_json::Value::Null);
}

#[test]
fn value_bool_round_trip() {
    let pool = DescriptorPool::global();
    let (value_desc, _, _) = get_wkt_descs(&pool);
    let codec = JsonCodec::default();

    let json_in = serde_json::Value::Bool(true);
    let decoded = from_json(&json_in, &value_desc, &codec).expect("from_json bool Value");
    let json_out = to_json(&decoded, &codec);
    assert_eq!(json_out, json_in, "bool Value round-trip mismatch");
}

#[test]
fn value_number_round_trip() {
    let pool = DescriptorPool::global();
    let (value_desc, _, _) = get_wkt_descs(&pool);
    let codec = JsonCodec::default();

    let json_in = json!(42.5);
    let decoded = from_json(&json_in, &value_desc, &codec).expect("from_json number Value");
    let json_out = to_json(&decoded, &codec);
    assert_eq!(json_out, json_in, "number Value round-trip mismatch");
}

#[test]
fn value_string_round_trip() {
    let pool = DescriptorPool::global();
    let (value_desc, _, _) = get_wkt_descs(&pool);
    let codec = JsonCodec::default();

    let json_in = json!("hello");
    let decoded = from_json(&json_in, &value_desc, &codec).expect("from_json string Value");
    let json_out = to_json(&decoded, &codec);
    assert_eq!(json_out, json_in, "string Value round-trip mismatch");
}

#[test]
fn value_struct_round_trip() {
    let pool = DescriptorPool::global();
    let (value_desc, _, _) = get_wkt_descs(&pool);
    let codec = JsonCodec::default();

    let json_in = json!({"name": "Alice", "age": 30.0});
    let decoded = from_json(&json_in, &value_desc, &codec).expect("from_json struct Value");
    let json_out = to_json(&decoded, &codec);
    assert_eq!(json_out, json_in, "struct Value round-trip mismatch");
}

#[test]
fn value_list_round_trip() {
    let pool = DescriptorPool::global();
    let (value_desc, _, _) = get_wkt_descs(&pool);
    let codec = JsonCodec::default();

    let json_in = json!([1.0, "two", true, null]);
    let decoded = from_json(&json_in, &value_desc, &codec).expect("from_json list Value");
    let json_out = to_json(&decoded, &codec);
    assert_eq!(json_out, json_in, "list Value round-trip mismatch");
}

#[test]
fn list_value_round_trip() {
    let pool = DescriptorPool::global();
    let (_, list_desc, _) = get_wkt_descs(&pool);
    let codec = JsonCodec::default();

    let json_in = json!([1.0, 2.0, 3.0]);
    let decoded = from_json(&json_in, &list_desc, &codec).expect("from_json ListValue");
    let json_out = to_json(&decoded, &codec);
    assert_eq!(json_out, json_in, "ListValue round-trip mismatch");
}

#[test]
fn struct_round_trip() {
    let pool = DescriptorPool::global();
    let (_, _, struct_desc) = get_wkt_descs(&pool);
    let codec = JsonCodec::default();

    let json_in = json!({ "x": 1.0, "y": "hello", "z": true });
    let decoded = from_json(&json_in, &struct_desc, &codec).expect("from_json Struct");
    let json_out = to_json(&decoded, &codec);
    assert_eq!(json_out, json_in, "Struct round-trip mismatch");
}

#[test]
fn nested_struct_round_trip() {
    let pool = DescriptorPool::global();
    let (_, _, struct_desc) = get_wkt_descs(&pool);
    let codec = JsonCodec::default();

    // Nested: outer.inner is a struct with a number
    let json_in = json!({ "outer": { "inner": 42.0 } });
    let decoded = from_json(&json_in, &struct_desc, &codec).expect("from_json nested Struct");
    let json_out = to_json(&decoded, &codec);
    assert_eq!(json_out, json_in, "nested Struct round-trip mismatch");
}

// ---------------------------------------------------------------------------
// 7: Any
// ---------------------------------------------------------------------------

/// Build a pool with WKTs + a simple `Inner` message (int32 field `id`).
fn make_any_test_pool() -> (DescriptorPool, prost_reflect::MessageDescriptor) {
    make_wkt_pool(
        "Inner",
        vec![FieldDescriptorProto {
            name: Some("id".to_owned()),
            number: Some(1),
            r#type: Some(Type::Int32 as i32),
            label: Some(Label::Optional as i32),
            json_name: Some("id".to_owned()),
            ..Default::default()
        }],
    )
}

#[test]
fn any_round_trip_regular_message() {
    let (pool, inner_desc) = make_any_test_pool();
    let codec = JsonCodec::default();

    let any_desc = pool
        .get_message_by_name("google.protobuf.Any")
        .expect("Any descriptor");

    // Build an Inner DynamicMessage with id=99
    let mut inner = DynamicMessage::new(inner_desc.clone());
    inner
        .try_set_field(
            &inner_desc.get_field_by_name("id").expect("id"),
            Value::I32(99),
        )
        .expect("set id");

    // Encode inner → bytes, build Any manually
    let inner_bytes = inner.encode_to_vec();
    let type_url = "type.googleapis.com/Inner".to_owned();

    let type_url_field = any_desc.get_field_by_name("type_url").expect("type_url");
    let value_field = any_desc.get_field_by_name("value").expect("value");

    let mut any_msg = DynamicMessage::new(any_desc.clone());
    any_msg
        .try_set_field(&type_url_field, Value::String(type_url.clone()))
        .expect("set type_url");
    any_msg
        .try_set_field(
            &value_field,
            Value::Bytes(prost::bytes::Bytes::from(inner_bytes)),
        )
        .expect("set value");

    // to_json → {"@type": "...", "id": 99}
    let json = to_json(&any_msg, &codec);
    let obj = json.as_object().expect("JSON object");
    assert_eq!(
        obj.get("@type").and_then(|v| v.as_str()),
        Some(type_url.as_str())
    );
    assert_eq!(obj.get("id").and_then(|v| v.as_i64()), Some(99));

    // from_json round-trip
    let decoded = from_json(&json, &any_desc, &codec).expect("from_json Any");
    let rt_json = to_json(&decoded, &codec);
    assert_eq!(json, rt_json, "Any round-trip mismatch");
}

#[test]
fn any_round_trip_timestamp_wkt() {
    let pool = DescriptorPool::global();
    let codec = JsonCodec::default();

    let any_desc = pool
        .get_message_by_name("google.protobuf.Any")
        .expect("Any descriptor");
    let ts_desc = pool
        .get_message_by_name("google.protobuf.Timestamp")
        .expect("Timestamp descriptor");

    // Build a Timestamp DynamicMessage: 2023-11-14T22:13:20Z (seconds=1700000000)
    let mut ts_msg = DynamicMessage::new(ts_desc.clone());
    ts_msg
        .try_set_field(
            &ts_desc.get_field_by_name("seconds").expect("seconds"),
            Value::I64(1_700_000_000),
        )
        .expect("set seconds");
    ts_msg
        .try_set_field(
            &ts_desc.get_field_by_name("nanos").expect("nanos"),
            Value::I32(0),
        )
        .expect("set nanos");

    let ts_bytes = ts_msg.encode_to_vec();
    let type_url = "type.googleapis.com/google.protobuf.Timestamp".to_owned();

    let type_url_field = any_desc.get_field_by_name("type_url").expect("type_url");
    let value_field = any_desc.get_field_by_name("value").expect("value");

    let mut any_msg = DynamicMessage::new(any_desc.clone());
    any_msg
        .try_set_field(&type_url_field, Value::String(type_url.clone()))
        .expect("set type_url");
    any_msg
        .try_set_field(
            &value_field,
            Value::Bytes(prost::bytes::Bytes::from(ts_bytes)),
        )
        .expect("set value");

    // to_json → {"@type": "...", "value": "2023-11-14T22:13:20Z"}
    let json = to_json(&any_msg, &codec);
    let obj = json.as_object().expect("JSON object");
    assert_eq!(
        obj.get("@type").and_then(|v| v.as_str()),
        Some(type_url.as_str()),
        "@type mismatch"
    );
    let inner_val = obj.get("value").expect("'value' key missing for WKT Any");
    assert_eq!(
        inner_val.as_str(),
        Some("2023-11-14T22:13:20Z"),
        "Timestamp in Any should be RFC3339 string under 'value'"
    );

    // from_json round-trip
    let decoded = from_json(&json, &any_desc, &codec).expect("from_json Any Timestamp");
    let rt_json = to_json(&decoded, &codec);
    assert_eq!(json, rt_json, "Any(Timestamp) round-trip mismatch");
}
