//! Integration tests for the `OxiOneof` trait.
//!
//! Tests a hand-implemented `TestOneof` enum with three variants:
//!   - field 10: i32
//!   - field 11: String
//!   - field 12: bool

use oxiproto_core::wire::varint::encoded_len_varint;
use oxiproto_core::wire::{self, WireType};
use oxiproto_core::{OxiOneof, OxiProtoResult};

// ── Hand-implemented TestOneof ────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone)]
enum TestOneof {
    /// field 10: int32
    Int(i32),
    /// field 11: string
    Str(String),
    /// field 12: bool
    Flag(bool),
}

impl OxiOneof for TestOneof {
    fn discriminant(&self) -> u32 {
        match self {
            TestOneof::Int(_) => 10,
            TestOneof::Str(_) => 11,
            TestOneof::Flag(_) => 12,
        }
    }

    fn encoded_len(&self) -> usize {
        match self {
            TestOneof::Int(v) => {
                let tag_value = (10u64 << 3) | u64::from(WireType::Varint.value());
                encoded_len_varint(tag_value) + encoded_len_varint(*v as i64 as u64)
            }
            TestOneof::Str(s) => {
                let tag_value = (11u64 << 3) | u64::from(WireType::Len.value());
                encoded_len_varint(tag_value) + wire::encoded_len_length_delimited(s.len())
            }
            TestOneof::Flag(_) => {
                // tag + varint 0 or 1 (both 1 byte)
                let tag_value = (12u64 << 3) | u64::from(WireType::Varint.value());
                encoded_len_varint(tag_value) + 1
            }
        }
    }

    fn encode(&self, buf: &mut wire::EncodeBuffer) {
        match self {
            TestOneof::Int(v) => {
                let _ = buf.write_tag(10, WireType::Varint);
                buf.write_varint_i32(*v);
            }
            TestOneof::Str(s) => {
                let _ = buf.write_tag(11, WireType::Len);
                buf.write_string(s);
            }
            TestOneof::Flag(b) => {
                let _ = buf.write_tag(12, WireType::Varint);
                buf.write_bool(*b);
            }
        }
    }

    fn merge_field(
        field_number: u32,
        wire_type: WireType,
        buf: &mut wire::DecodeBuffer,
        slot: &mut Option<Self>,
    ) -> OxiProtoResult<bool> {
        // wire_type is available for callers that need it (e.g. to reject
        // unexpected wire types). We don't enforce it here for test simplicity.
        let _ = wire_type;
        match field_number {
            10 => {
                let v = buf.read_varint_i32()?;
                *slot = Some(TestOneof::Int(v));
                Ok(true)
            }
            11 => {
                let s = buf.read_string()?;
                *slot = Some(TestOneof::Str(s.to_owned()));
                Ok(true)
            }
            12 => {
                let b = buf.read_bool()?;
                *slot = Some(TestOneof::Flag(b));
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Encode a single `TestOneof` variant and return the bytes.
fn encode_variant(variant: &TestOneof) -> Vec<u8> {
    let mut buf = wire::EncodeBuffer::new();
    variant.encode(&mut buf);
    buf.into_vec()
}

/// Decode a single `TestOneof` from raw bytes (field_number already known).
fn decode_variant(field_number: u32, bytes: &[u8]) -> OxiProtoResult<Option<TestOneof>> {
    let mut buf = wire::DecodeBuffer::new(bytes);
    // Read the tag to get the wire type.
    let tag = buf.read_tag()?;
    assert_eq!(
        tag.field_number, field_number,
        "unexpected field number in encoded bytes"
    );
    let mut slot: Option<TestOneof> = None;
    let recognised = TestOneof::merge_field(tag.field_number, tag.wire_type, &mut buf, &mut slot)?;
    assert!(recognised, "field_number {field_number} must be recognised");
    Ok(slot)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn oneof_int_variant_round_trip() {
    let variant = TestOneof::Int(42);
    let bytes = encode_variant(&variant);

    let decoded = decode_variant(10, &bytes)
        .expect("decode must succeed")
        .expect("slot must be filled");

    assert_eq!(decoded, variant);
}

#[test]
fn oneof_str_variant_round_trip() {
    let variant = TestOneof::Str("hello".to_owned());
    let bytes = encode_variant(&variant);

    let decoded = decode_variant(11, &bytes)
        .expect("decode must succeed")
        .expect("slot must be filled");

    assert_eq!(decoded, variant);
}

#[test]
fn oneof_bool_variant_round_trip_true() {
    let variant = TestOneof::Flag(true);
    let bytes = encode_variant(&variant);

    let decoded = decode_variant(12, &bytes)
        .expect("decode must succeed")
        .expect("slot must be filled");

    assert_eq!(decoded, variant);
}

#[test]
fn oneof_bool_variant_round_trip_false() {
    let variant = TestOneof::Flag(false);
    let bytes = encode_variant(&variant);
    let decoded = decode_variant(12, &bytes)
        .expect("decode must succeed")
        .expect("slot must be filled");
    assert_eq!(decoded, variant);
}

#[test]
fn oneof_negative_int_round_trip() {
    let variant = TestOneof::Int(-100);
    let bytes = encode_variant(&variant);
    let decoded = decode_variant(10, &bytes).expect("decode").expect("slot");
    assert_eq!(decoded, variant);
}

#[test]
fn oneof_later_field_number_wins() {
    // Encode field 10 first, then field 11. In a message merge loop both would
    // arrive; the second should overwrite the first slot.
    let mut enc = wire::EncodeBuffer::new();
    // First: Int(7)
    enc.write_tag(10, WireType::Varint).expect("tag 10");
    enc.write_varint_i32(7);
    // Second: Str("winner")
    enc.write_tag(11, WireType::Len).expect("tag 11");
    enc.write_string("winner");

    let bytes = enc.into_vec();
    let mut buf = wire::DecodeBuffer::new(&bytes);
    let mut slot: Option<TestOneof> = None;

    // Simulate message merge loop.
    while !buf.is_empty() {
        let tag = buf.read_tag().expect("read tag");
        let recognised =
            TestOneof::merge_field(tag.field_number, tag.wire_type, &mut buf, &mut slot)
                .expect("merge_field");
        assert!(recognised, "all fields must be recognised");
    }

    // The slot should hold the last written variant.
    assert_eq!(slot, Some(TestOneof::Str("winner".to_owned())));
}

#[test]
fn oneof_unknown_field_number_returns_false() {
    // Field 99 is not part of TestOneof.
    let mut enc = wire::EncodeBuffer::new();
    enc.write_tag(99, WireType::Varint).expect("tag 99");
    enc.write_varint(123);

    let bytes = enc.into_vec();
    let mut buf = wire::DecodeBuffer::new(&bytes);
    let tag = buf.read_tag().expect("read tag");

    let mut slot: Option<TestOneof> = None;
    let recognised = TestOneof::merge_field(tag.field_number, tag.wire_type, &mut buf, &mut slot)
        .expect("merge_field ok");

    // Must return false and leave the slot unchanged.
    assert!(
        !recognised,
        "unrecognised field_number must return Ok(false)"
    );
    assert!(slot.is_none(), "slot must be untouched");
}

#[test]
fn oneof_discriminant_values() {
    assert_eq!(TestOneof::Int(0).discriminant(), 10);
    assert_eq!(TestOneof::Str(String::new()).discriminant(), 11);
    assert_eq!(TestOneof::Flag(false).discriminant(), 12);
}

#[test]
fn oneof_encoded_len_matches_actual() {
    for variant in [
        TestOneof::Int(0),
        TestOneof::Int(i32::MAX),
        TestOneof::Int(i32::MIN),
        TestOneof::Str(String::new()),
        TestOneof::Str("hello world".to_owned()),
        TestOneof::Flag(true),
        TestOneof::Flag(false),
    ] {
        let declared = variant.encoded_len();
        let actual = encode_variant(&variant).len();
        assert_eq!(
            declared, actual,
            "encoded_len mismatch for {:?}: declared={declared} actual={actual}",
            variant
        );
    }
}
