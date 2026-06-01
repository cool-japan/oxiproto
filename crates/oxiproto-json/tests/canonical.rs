use oxiproto_json::{to_json, JsonCodec};
use prost::bytes::Bytes;
use prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, Value};
use prost_types::{DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet};

// ---------------------------------------------------------------------------
// Helper: build a pool with a custom message
// ---------------------------------------------------------------------------

fn make_pool_with_message(
    name: &str,
    fields: Vec<FieldDescriptorProto>,
) -> (DescriptorPool, prost_reflect::MessageDescriptor) {
    let file = FileDescriptorProto {
        name: Some("test.proto".to_owned()),
        syntax: Some("proto3".to_owned()),
        message_type: vec![DescriptorProto {
            name: Some(name.to_owned()),
            field: fields,
            ..Default::default()
        }],
        ..Default::default()
    };
    let fds = FileDescriptorSet { file: vec![file] };
    let bytes = fds.encode_to_vec();

    use prost::Message as _;
    let pool = DescriptorPool::decode(bytes.as_ref()).expect("pool decode");
    let desc = pool.get_message_by_name(name).expect("message descriptor");
    (pool, desc)
}

// ---------------------------------------------------------------------------
// Timestamp
// ---------------------------------------------------------------------------

/// Test that Timestamp { seconds: 1700000000, nanos: 0 } → "2023-11-14T22:13:20Z"
#[test]
fn timestamp_to_rfc3339_exact() {
    // Build the google.protobuf.Timestamp descriptor from prost_types.
    let _ts = prost_types::Timestamp {
        seconds: 1_700_000_000,
        nanos: 0,
    };

    // Use prost_reflect's well-known pool (available via the file descriptor
    // set that prost_types provides)
    let fds_bytes = prost_types::FileDescriptorSet {
        file: vec![prost_types::FileDescriptorProto {
            name: Some("google/protobuf/timestamp.proto".to_owned()),
            package: Some("google.protobuf".to_owned()),
            syntax: Some("proto3".to_owned()),
            message_type: vec![prost_types::DescriptorProto {
                name: Some("Timestamp".to_owned()),
                field: vec![
                    prost_types::FieldDescriptorProto {
                        name: Some("seconds".to_owned()),
                        number: Some(1),
                        r#type: Some(prost_types::field_descriptor_proto::Type::Int64 as i32),
                        label: Some(prost_types::field_descriptor_proto::Label::Optional as i32),
                        json_name: Some("seconds".to_owned()),
                        ..Default::default()
                    },
                    prost_types::FieldDescriptorProto {
                        name: Some("nanos".to_owned()),
                        number: Some(2),
                        r#type: Some(prost_types::field_descriptor_proto::Type::Int32 as i32),
                        label: Some(prost_types::field_descriptor_proto::Label::Optional as i32),
                        json_name: Some("nanos".to_owned()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }],
    };
    let fds_encoded = fds_bytes.encode_to_vec();
    let pool = DescriptorPool::decode(fds_encoded.as_ref()).expect("pool");
    let desc = pool
        .get_message_by_name("google.protobuf.Timestamp")
        .expect("Timestamp descriptor");

    let mut msg = DynamicMessage::new(desc.clone());
    let secs_field = desc.get_field_by_name("seconds").expect("seconds");
    let nanos_field = desc.get_field_by_name("nanos").expect("nanos");
    msg.try_set_field(&secs_field, Value::I64(1_700_000_000))
        .expect("set seconds");
    msg.try_set_field(&nanos_field, Value::I32(0))
        .expect("set nanos");

    let codec = JsonCodec::default();
    let json = to_json(&msg, &codec);
    assert_eq!(
        json.as_str().unwrap(),
        "2023-11-14T22:13:20Z",
        "Timestamp(1700000000s) should format as 2023-11-14T22:13:20Z"
    );
}

// ---------------------------------------------------------------------------
// Duration
// ---------------------------------------------------------------------------

fn make_duration_pool() -> (DescriptorPool, prost_reflect::MessageDescriptor) {
    let fds = prost_types::FileDescriptorSet {
        file: vec![prost_types::FileDescriptorProto {
            name: Some("google/protobuf/duration.proto".to_owned()),
            package: Some("google.protobuf".to_owned()),
            syntax: Some("proto3".to_owned()),
            message_type: vec![prost_types::DescriptorProto {
                name: Some("Duration".to_owned()),
                field: vec![
                    prost_types::FieldDescriptorProto {
                        name: Some("seconds".to_owned()),
                        number: Some(1),
                        r#type: Some(prost_types::field_descriptor_proto::Type::Int64 as i32),
                        label: Some(prost_types::field_descriptor_proto::Label::Optional as i32),
                        json_name: Some("seconds".to_owned()),
                        ..Default::default()
                    },
                    prost_types::FieldDescriptorProto {
                        name: Some("nanos".to_owned()),
                        number: Some(2),
                        r#type: Some(prost_types::field_descriptor_proto::Type::Int32 as i32),
                        label: Some(prost_types::field_descriptor_proto::Label::Optional as i32),
                        json_name: Some("nanos".to_owned()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }],
    };
    let encoded = fds.encode_to_vec();
    let pool = DescriptorPool::decode(encoded.as_ref()).expect("pool");
    let desc = pool
        .get_message_by_name("google.protobuf.Duration")
        .expect("Duration descriptor");
    (pool, desc)
}

#[test]
fn duration_1_5s() {
    let (_pool, desc) = make_duration_pool();
    let mut msg = DynamicMessage::new(desc.clone());
    let secs_field = desc.get_field_by_name("seconds").expect("seconds");
    let nanos_field = desc.get_field_by_name("nanos").expect("nanos");
    msg.try_set_field(&secs_field, Value::I64(1))
        .expect("set seconds");
    msg.try_set_field(&nanos_field, Value::I32(500_000_000))
        .expect("set nanos");

    let codec = JsonCodec::default();
    let json = to_json(&msg, &codec);
    assert_eq!(json.as_str().unwrap(), "1.5s");
}

#[test]
fn duration_negative_1s() {
    let (_pool, desc) = make_duration_pool();
    let mut msg = DynamicMessage::new(desc.clone());
    let secs_field = desc.get_field_by_name("seconds").expect("seconds");
    let nanos_field = desc.get_field_by_name("nanos").expect("nanos");
    msg.try_set_field(&secs_field, Value::I64(-1))
        .expect("set seconds");
    msg.try_set_field(&nanos_field, Value::I32(0))
        .expect("set nanos");

    let codec = JsonCodec::default();
    let json = to_json(&msg, &codec);
    assert_eq!(json.as_str().unwrap(), "-1s");
}

// ---------------------------------------------------------------------------
// int64 → JSON string precision test
// ---------------------------------------------------------------------------

#[test]
fn int64_emitted_as_string() {
    use prost_types::field_descriptor_proto::{Label, Type};

    let (_pool, desc) = make_pool_with_message(
        "Int64Msg",
        vec![FieldDescriptorProto {
            name: Some("val".to_owned()),
            number: Some(1),
            r#type: Some(Type::Int64 as i32),
            label: Some(Label::Optional as i32),
            json_name: Some("val".to_owned()),
            ..Default::default()
        }],
    );

    let large: i64 = 9_007_199_254_740_993;
    let mut msg = DynamicMessage::new(desc.clone());
    let field = desc.get_field_by_name("val").expect("val");
    msg.try_set_field(&field, Value::I64(large)).expect("set");

    let codec = JsonCodec::default();
    let json = to_json(&msg, &codec);
    let obj = json.as_object().expect("object");
    let v = obj.get("val").expect("val key");
    // Must be a JSON string, not a number
    assert!(v.is_string(), "int64 must be emitted as string, got: {v:?}");
    assert_eq!(v.as_str().unwrap(), "9007199254740993");
}

// ---------------------------------------------------------------------------
// bytes → base64
// ---------------------------------------------------------------------------

#[test]
fn bytes_to_base64_standard() {
    use prost_types::field_descriptor_proto::{Label, Type};

    let (_pool, desc) = make_pool_with_message(
        "BytesMsg",
        vec![FieldDescriptorProto {
            name: Some("data".to_owned()),
            number: Some(1),
            r#type: Some(Type::Bytes as i32),
            label: Some(Label::Optional as i32),
            json_name: Some("data".to_owned()),
            ..Default::default()
        }],
    );

    let mut msg = DynamicMessage::new(desc.clone());
    let field = desc.get_field_by_name("data").expect("data");
    msg.try_set_field(&field, Value::Bytes(Bytes::from_static(b"hello")))
        .expect("set");

    let codec = JsonCodec::default();
    let json = to_json(&msg, &codec);
    let obj = json.as_object().expect("object");
    let v = obj.get("data").expect("data key");
    assert_eq!(v.as_str().unwrap(), "aGVsbG8=");
}
