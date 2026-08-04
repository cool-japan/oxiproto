//! Reflection behaviour for Protobuf Editions (`edition = "2023";`) files.
//!
//! Each test compiles an edition `.proto` with `oxiproto-build`, builds a native
//! descriptor pool from the resulting `FileDescriptorSet`, and then asserts on
//! *observable wire bytes* — not on descriptor bookkeeping — so that a feature
//! that resolves correctly but is never applied would still fail.

use oxiproto_reflect::native::{DescriptorPool, DynamicMessage, Kind, Value};

fn pool_of(src: &str) -> DescriptorPool {
    let fds = oxiproto_build::compile_str_native(src).expect("edition source must compile");
    DescriptorPool::from_file_descriptor_set(fds).expect("pool")
}

fn message(pool: &DescriptorPool, name: &str) -> oxiproto_reflect::native::MessageDescriptor {
    pool.get_message_by_name(name)
        .unwrap_or_else(|| panic!("message {name} not found"))
}

// ---------------------------------------------------------------------------
// field_presence
// ---------------------------------------------------------------------------

/// Edition 2023 defaults to EXPLICIT presence, so a field *set to zero* must
/// still appear on the wire — the proto3 "omit the default" rule does not apply.
#[test]
fn explicit_presence_serializes_a_zero_value() {
    let pool = pool_of(
        r#"edition = "2023";
message M {
  int32 a = 1;
}
"#,
    );
    let desc = message(&pool, "M");
    let field = desc.get_field_by_name("a").expect("field a");
    assert!(field.has_presence(), "edition default presence is EXPLICIT");

    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&field, Value::I32(0));
    let bytes = msg.encode_to_vec().expect("encode");
    // tag(1, varint) = 0x08, value 0x00
    assert_eq!(bytes, vec![0x08, 0x00]);
}

/// `features.field_presence = IMPLICIT` restores the proto3 rule: a zero is
/// indistinguishable from unset and is dropped from the encoding.
#[test]
fn implicit_presence_omits_a_zero_value() {
    let pool = pool_of(
        r#"edition = "2023";
message M {
  int32 a = 1 [features.field_presence = IMPLICIT];
}
"#,
    );
    let desc = message(&pool, "M");
    let field = desc.get_field_by_name("a").expect("field a");
    assert!(!field.has_presence());

    let mut msg = DynamicMessage::new(desc.clone());
    msg.set_field(&field, Value::I32(0));
    assert!(msg.encode_to_vec().expect("encode").is_empty());

    // A non-zero value is of course still encoded.
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&field, Value::I32(7));
    assert_eq!(msg.encode_to_vec().expect("encode"), vec![0x08, 0x07]);
}

/// The same distinction has to show up in the JSON mapping.
#[test]
fn presence_controls_json_default_emission() {
    let pool = pool_of(
        r#"edition = "2023";
message M {
  int32 kept = 1;
  int32 dropped = 2 [features.field_presence = IMPLICIT];
}
"#,
    );
    let desc = message(&pool, "M");
    let kept = desc.get_field_by_name("kept").expect("kept");
    let dropped = desc.get_field_by_name("dropped").expect("dropped");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&kept, Value::I32(0));
    msg.set_field(&dropped, Value::I32(0));

    let json = msg.to_json().expect("to_json");
    let obj = json.as_object().expect("object");
    assert!(obj.contains_key("kept"), "EXPLICIT zero must be emitted");
    assert!(
        !obj.contains_key("dropped"),
        "IMPLICIT zero must be omitted, got {json}"
    );
}

/// A `LEGACY_REQUIRED` field is a proto2 `required` field: cardinality Required.
#[test]
fn legacy_required_is_reported_as_required_cardinality() {
    use oxiproto_reflect::native::Cardinality;
    let pool = pool_of(
        r#"edition = "2023";
message M {
  int32 a = 1 [features.field_presence = LEGACY_REQUIRED];
}
"#,
    );
    let desc = message(&pool, "M");
    let field = desc.get_field_by_name("a").expect("field a");
    assert_eq!(field.cardinality(), Cardinality::Required);
    assert!(field.has_presence());
}

// ---------------------------------------------------------------------------
// repeated_field_encoding
// ---------------------------------------------------------------------------

/// PACKED (the edition default) and EXPANDED must produce different bytes.
#[test]
fn repeated_field_encoding_changes_the_wire_bytes() {
    let pool = pool_of(
        r#"edition = "2023";
message M {
  repeated int32 packed = 1;
  repeated int32 expanded = 2 [features.repeated_field_encoding = EXPANDED];
}
"#,
    );
    let desc = message(&pool, "M");
    let packed = desc.get_field_by_name("packed").expect("packed");
    let expanded = desc.get_field_by_name("expanded").expect("expanded");
    assert!(packed.is_packed());
    assert!(!expanded.is_packed());

    let mut msg = DynamicMessage::new(desc.clone());
    msg.set_field(&packed, Value::List(vec![Value::I32(1), Value::I32(2)]));
    // tag(1, Len)=0x0a, len=2, 0x01, 0x02
    assert_eq!(msg.encode_to_vec().expect("encode"), vec![0x0a, 0x02, 1, 2]);

    let mut msg = DynamicMessage::new(desc.clone());
    msg.set_field(&expanded, Value::List(vec![Value::I32(1), Value::I32(2)]));
    // tag(2, Varint)=0x10 repeated once per element
    assert_eq!(msg.encode_to_vec().expect("encode"), vec![0x10, 1, 0x10, 2]);

    // Both forms decode back to the same values (decoders accept either).
    let decoded = DynamicMessage::decode(desc, &[0x10, 1, 0x10, 2]).expect("decode");
    let field = decoded
        .descriptor()
        .get_field_by_name("expanded")
        .expect("expanded");
    assert_eq!(
        decoded.get_field(&field).into_owned(),
        Value::List(vec![Value::I32(1), Value::I32(2)])
    );
}

// ---------------------------------------------------------------------------
// message_encoding
// ---------------------------------------------------------------------------

/// `features.message_encoding = DELIMITED` is the group wire format: the field
/// is framed by start/end-group tags and the descriptor reports `Kind::Group`.
#[test]
fn delimited_message_encoding_round_trips_as_a_group() {
    let pool = pool_of(
        r#"edition = "2023";
message Inner {
  int32 x = 1;
}
message M {
  Inner sub = 1 [features.message_encoding = DELIMITED];
}
"#,
    );
    let desc = message(&pool, "M");
    let field = desc.get_field_by_name("sub").expect("sub");
    assert!(
        matches!(field.kind(), Kind::Group(_)),
        "DELIMITED must resolve to a group-kind field"
    );

    let inner_desc = field.message_type().expect("group message");
    let inner_x = inner_desc.get_field_by_name("x").expect("x");
    let mut inner = DynamicMessage::new(inner_desc);
    inner.set_field(&inner_x, Value::I32(9));

    let mut msg = DynamicMessage::new(desc.clone());
    msg.set_field(&field, Value::Message(Box::new(inner)));

    let bytes = msg.encode_to_vec().expect("encode");
    // SGroup tag (1<<3|3 = 0x0b), inner varint field 1 = 9, EGroup tag (0x0c).
    assert_eq!(bytes, vec![0x0b, 0x08, 0x09, 0x0c]);

    let decoded = DynamicMessage::decode(desc, &bytes).expect("decode");
    let field = decoded.descriptor().get_field_by_name("sub").expect("sub");
    let value = decoded.get_field(&field).into_owned();
    match value {
        Value::Message(inner) => {
            let x = inner.descriptor().get_field_by_name("x").expect("x");
            assert_eq!(inner.get_field(&x).into_owned(), Value::I32(9));
        }
        other => panic!("expected a message, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Legacy syntaxes keep their behaviour
// ---------------------------------------------------------------------------

/// proto3 singular scalars still have no presence, and proto2 optional ones do.
#[test]
fn legacy_syntax_presence_is_unchanged() {
    let p3 = pool_of(
        r#"syntax = "proto3";
message M { int32 a = 1; }
"#,
    );
    let desc = message(&p3, "M");
    assert!(!desc.get_field_by_name("a").expect("a").has_presence());

    let p2 = pool_of(
        r#"syntax = "proto2";
message M { optional int32 a = 1; }
"#,
    );
    let desc = message(&p2, "M");
    let field = desc.get_field_by_name("a").expect("a");
    assert!(field.has_presence());

    // proto2 explicit presence: a zero that was set is still serialized.
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&field, Value::I32(0));
    assert_eq!(msg.encode_to_vec().expect("encode"), vec![0x08, 0x00]);
}

// ---------------------------------------------------------------------------
// has_field / encode agreement
// ---------------------------------------------------------------------------

/// `has_field` must use the same predicate as the encoders: a field that is
/// reported present has to appear in the output, and vice versa. Before
/// presence was modelled, a proto2 `optional` set to zero answered "absent"
/// while the encoder dropped it — consistent but wrong; answering "absent"
/// while the encoder emitted it would be worse.
#[test]
fn has_field_agrees_with_the_encoders() {
    let pool = pool_of(
        r#"edition = "2023";
message M {
  int32 explicit = 1;
  int32 implicit = 2 [features.field_presence = IMPLICIT];
  repeated int32 list = 3;
}
"#,
    );
    let desc = message(&pool, "M");
    for name in ["explicit", "implicit"] {
        let field = desc.get_field_by_name(name).expect("field");
        let mut msg = DynamicMessage::new(desc.clone());
        assert!(!msg.has_field(&field), "{name}: unset must not be present");
        msg.set_field(&field, Value::I32(0));
        let encoded = !msg.encode_to_vec().expect("encode").is_empty();
        assert_eq!(
            msg.has_field(&field),
            encoded,
            "{name}: has_field and the wire encoder must agree on a zero value"
        );
    }

    let list = desc.get_field_by_name("list").expect("list");
    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&list, Value::List(Vec::new()));
    assert!(
        !msg.has_field(&list),
        "an empty repeated field has no presence"
    );
    assert!(msg.encode_to_vec().expect("encode").is_empty());
}
