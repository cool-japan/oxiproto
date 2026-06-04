//! Conformance test suite for the OxiProto wire format codec.
//!
//! This file contains test vectors drawn directly from the Protocol Buffers
//! binary encoding specification:
//!   <https://protobuf.dev/programming-guides/encoding/>
//!
//! Each test vector specifies:
//! - A human-readable description (message type and field values)
//! - The expected wire bytes as a hex literal
//! - The expected decoded field values
//!
//! Tests verify three properties:
//! 1. **Encode correctness** — OxiProto produces the canonical bytes.
//! 2. **Decode correctness** — The canonical bytes decode to the expected values.
//! 3. **Round-trip stability** — encode(decode(bytes)) == bytes.
//!
//! No external files or network access are used.  All test vectors are
//! self-contained byte literals.

#![forbid(unsafe_code)]

use oxiproto::{
    wire::{DecodeBuffer, EncodeBuffer, WireType},
    OxiMessage, OxiProtoError, OxiProtoResult,
};

// ─── Wire-level helper: encode a tag + varint value ──────────────────────────

fn tag_varint(field: u32, value: u64) -> Vec<u8> {
    let mut buf = EncodeBuffer::new();
    buf.write_tag(field, WireType::Varint)
        .expect("write_tag for conformance test");
    buf.write_varint(value);
    buf.into_vec()
}

fn tag_len_string(field: u32, s: &str) -> Vec<u8> {
    let mut buf = EncodeBuffer::new();
    buf.write_tag(field, WireType::Len)
        .expect("write_tag for conformance test");
    buf.write_string(s);
    buf.into_vec()
}

fn tag_len_bytes(field: u32, data: &[u8]) -> Vec<u8> {
    let mut buf = EncodeBuffer::new();
    buf.write_tag(field, WireType::Len)
        .expect("write_tag for conformance test");
    buf.write_length_delimited(data);
    buf.into_vec()
}

// ─── Section 1: Varint encoding (from encoding guide §Varints) ───────────────

/// Spec example: value 1 encodes as `\x01` (single byte).
/// Source: <https://protobuf.dev/programming-guides/encoding/#varints>
#[test]
fn conformance_varint_1() {
    let expected: &[u8] = &[0x01];
    let mut enc = Vec::new();
    oxiproto::wire::varint::encode_varint(1u64, &mut enc);
    assert_eq!(enc.as_slice(), expected, "varint(1) must be \\x01");

    let (val, consumed) =
        oxiproto::wire::varint::decode_varint(expected).expect("decode varint(1)");
    assert_eq!(val, 1);
    assert_eq!(consumed, 1);
}

/// Spec example: value 150 encodes as `\x96\x01` (two bytes).
/// Source: <https://protobuf.dev/programming-guides/encoding/#varints>
#[test]
fn conformance_varint_150() {
    let expected: &[u8] = &[0x96, 0x01];
    let mut enc = Vec::new();
    oxiproto::wire::varint::encode_varint(150u64, &mut enc);
    assert_eq!(enc.as_slice(), expected, "varint(150) must be \\x96\\x01");

    let (val, consumed) =
        oxiproto::wire::varint::decode_varint(expected).expect("decode varint(150)");
    assert_eq!(val, 150);
    assert_eq!(consumed, 2);
}

/// The maximum u64 value (2^64 − 1) uses exactly 10 bytes.
#[test]
fn conformance_varint_u64_max() {
    let expected: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01];
    let mut enc = Vec::new();
    let written = oxiproto::wire::varint::encode_varint(u64::MAX, &mut enc);
    assert_eq!(written, 10, "u64::MAX must encode in exactly 10 bytes");
    assert_eq!(enc.as_slice(), expected);

    let (val, consumed) = oxiproto::wire::varint::decode_varint(expected).expect("decode");
    assert_eq!(val, u64::MAX);
    assert_eq!(consumed, 10);
}

// ─── Section 2: ZigZag encoding (sint32 / sint64) ────────────────────────────

/// Spec: ZigZag encoding maps n → (n << 1) ^ (n >> 31) for sint32.
/// Canonical test vectors:
///   0 → 0,  -1 → 1,  1 → 2,  -2 → 3,  2147483647 → 4294967294,  -2147483648 → 4294967295
/// Source: <https://protobuf.dev/programming-guides/encoding/#signed-ints>
#[test]
fn conformance_zigzag_sint32() {
    let cases: &[(i32, u32)] = &[
        (0, 0),
        (-1, 1),
        (1, 2),
        (-2, 3),
        (2, 4),
        (i32::MAX, u32::MAX - 1),
        (i32::MIN, u32::MAX),
    ];
    for &(input, expected) in cases {
        let encoded = oxiproto::wire::zigzag::zigzag_encode32(input);
        assert_eq!(
            encoded, expected,
            "zigzag_encode32({input}) = {encoded}, expected {expected}"
        );
        let decoded = oxiproto::wire::zigzag::zigzag_decode32(expected);
        assert_eq!(
            decoded, input,
            "zigzag_decode32({expected}) = {decoded}, expected {input}"
        );
    }
}

/// sint64 zigzag round-trips.
#[test]
fn conformance_zigzag_sint64() {
    let cases: &[(i64, u64)] = &[
        (0, 0),
        (-1, 1),
        (1, 2),
        (-2, 3),
        (i64::MAX, u64::MAX - 1),
        (i64::MIN, u64::MAX),
    ];
    for &(input, expected) in cases {
        let encoded = oxiproto::wire::zigzag::zigzag_encode64(input);
        assert_eq!(encoded, expected, "zigzag_encode64({input}) != {expected}");
        let decoded = oxiproto::wire::zigzag::zigzag_decode64(expected);
        assert_eq!(decoded, input, "zigzag_decode64({expected}) != {input}");
    }
}

// ─── Section 3: Field tag encoding ───────────────────────────────────────────

/// Spec: tag = (field_number << 3) | wire_type
/// field 1, Varint: (1 << 3) | 0 = 8 = `\x08`
/// Source: <https://protobuf.dev/programming-guides/encoding/#structure>
#[test]
fn conformance_tag_field1_varint() {
    let expected: &[u8] = &[0x08];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::Varint).expect("write_tag");
    assert_eq!(enc.as_bytes(), expected, "tag(1, Varint) must be \\x08");

    let (tag, consumed) = oxiproto::wire::tag::decode_tag(expected).expect("decode_tag");
    assert_eq!(tag.field_number, 1);
    assert_eq!(tag.wire_type, WireType::Varint);
    assert_eq!(consumed, 1);
}

/// field 2, Len: (2 << 3) | 2 = 18 = `\x12`
#[test]
fn conformance_tag_field2_len() {
    let expected: &[u8] = &[0x12];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(2, WireType::Len).expect("write_tag");
    assert_eq!(enc.as_bytes(), expected);

    let (tag, _) = oxiproto::wire::tag::decode_tag(expected).expect("decode_tag");
    assert_eq!(tag.field_number, 2);
    assert_eq!(tag.wire_type, WireType::Len);
}

/// field 1, I64: (1 << 3) | 1 = 9 = `\x09`
#[test]
fn conformance_tag_field1_i64() {
    let expected: &[u8] = &[0x09];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::I64).expect("write_tag");
    assert_eq!(enc.as_bytes(), expected);
}

/// field 1, I32: (1 << 3) | 5 = 13 = `\x0d`
#[test]
fn conformance_tag_field1_i32() {
    let expected: &[u8] = &[0x0D];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::I32).expect("write_tag");
    assert_eq!(enc.as_bytes(), expected);
}

// ─── Section 4: Complete message test vectors from the encoding guide ─────────

/// Spec example: `message Test1 { optional int32 a = 1; }` with a = 150.
/// Wire: `\x08\x96\x01`
/// Source: <https://protobuf.dev/programming-guides/encoding/#simple>
#[test]
fn conformance_test1_a_150() {
    let expected: &[u8] = &[0x08, 0x96, 0x01];

    // Encode
    let encoded = tag_varint(1, 150);
    assert_eq!(
        encoded.as_slice(),
        expected,
        "Test1{{a=150}} must encode to \\x08\\x96\\x01"
    );

    // Decode
    let mut dec = DecodeBuffer::new(expected);
    let tag = dec.read_tag().expect("read_tag");
    assert_eq!(tag.field_number, 1);
    assert_eq!(tag.wire_type, WireType::Varint);
    let a = dec.read_varint().expect("read_varint");
    assert_eq!(a, 150);
    assert!(dec.is_empty(), "buffer must be consumed");
}

/// Spec example: `message Test2 { optional string b = 2; }` with b = "testing".
/// Wire: `\x12\x07testing`
/// Source: <https://protobuf.dev/programming-guides/encoding/#strings>
#[test]
fn conformance_test2_b_testing() {
    let expected: &[u8] = &[0x12, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g'];

    // Encode
    let encoded = tag_len_string(2, "testing");
    assert_eq!(
        encoded.as_slice(),
        expected,
        "Test2{{b='testing'}} must encode to \\x12\\x07testing"
    );

    // Decode
    let mut dec = DecodeBuffer::new(expected);
    let tag = dec.read_tag().expect("read_tag");
    assert_eq!(tag.field_number, 2);
    assert_eq!(tag.wire_type, WireType::Len);
    let b = dec.read_string().expect("read_string");
    assert_eq!(b, "testing");
    assert!(dec.is_empty());
}

/// Spec example: embedded message `Test3 { optional Test1 c = 3; }` where c.a = 150.
/// Wire: `\x1a\x03\x08\x96\x01`
/// Source: <https://protobuf.dev/programming-guides/encoding/#embedded>
#[test]
fn conformance_test3_embedded_test1() {
    let expected: &[u8] = &[0x1A, 0x03, 0x08, 0x96, 0x01];

    // Build inner message bytes
    let inner_bytes: &[u8] = &[0x08, 0x96, 0x01]; // Test1 { a = 150 }

    // Encode outer
    let encoded = tag_len_bytes(3, inner_bytes);
    assert_eq!(
        encoded.as_slice(),
        expected,
        "Test3{{c=Test1{{a=150}}}} must match canonical bytes"
    );

    // Decode outer
    let mut dec = DecodeBuffer::new(expected);
    let tag = dec.read_tag().expect("read_tag");
    assert_eq!(tag.field_number, 3);
    assert_eq!(tag.wire_type, WireType::Len);
    let payload = dec.read_length_delimited().expect("read_length_delimited");
    assert_eq!(payload, inner_bytes);

    // Decode inner
    let mut inner_dec = DecodeBuffer::new(payload);
    let inner_tag = inner_dec.read_tag().expect("inner_tag");
    assert_eq!(inner_tag.field_number, 1);
    assert_eq!(inner_tag.wire_type, WireType::Varint);
    let a = inner_dec.read_varint().expect("inner a");
    assert_eq!(a, 150);
    assert!(inner_dec.is_empty());
    assert!(dec.is_empty());
}

/// Spec example: packed repeated int32.
/// `message Test4 { repeated int32 d = 4; }` with d = [3, 270, 86942].
/// Wire: `\x22\x06\x03\x8e\x02\x9e\xa7\x05`
/// Source: <https://protobuf.dev/programming-guides/encoding/#packed>
#[test]
fn conformance_test4_packed_repeated_int32() {
    let expected: &[u8] = &[0x22, 0x06, 0x03, 0x8E, 0x02, 0x9E, 0xA7, 0x05];

    // Build packed payload
    let mut payload_buf = EncodeBuffer::new();
    payload_buf.write_varint(3);
    payload_buf.write_varint(270);
    payload_buf.write_varint(86942);
    assert_eq!(
        payload_buf.as_bytes(),
        &[0x03, 0x8E, 0x02, 0x9E, 0xA7, 0x05]
    );

    // Encode full message
    let mut enc = EncodeBuffer::new();
    enc.write_tag(4, WireType::Len).expect("tag");
    enc.write_length_delimited(payload_buf.as_bytes());
    assert_eq!(
        enc.as_bytes(),
        expected,
        "packed repeated int32 must match canonical"
    );

    // Decode
    let mut dec = DecodeBuffer::new(expected);
    let tag = dec.read_tag().expect("tag");
    assert_eq!(tag.field_number, 4);
    assert_eq!(tag.wire_type, WireType::Len);
    let blob = dec.read_length_delimited().expect("blob");
    let mut inner = DecodeBuffer::new(blob);
    let d1 = inner.read_varint().expect("d1");
    assert_eq!(d1, 3);
    let d2 = inner.read_varint().expect("d2");
    assert_eq!(d2, 270);
    let d3 = inner.read_varint().expect("d3");
    assert_eq!(d3, 86942);
    assert!(inner.is_empty());
    assert!(dec.is_empty());
}

// ─── Section 5: Fixed-width types ────────────────────────────────────────────

/// fixed32 is stored in 4 bytes, little-endian.
/// 0x01020304 → `\x04\x03\x02\x01`
/// Source: <https://protobuf.dev/programming-guides/encoding/#non-varint-nums>
#[test]
fn conformance_fixed32_little_endian() {
    let value: u32 = 0x0102_0304;
    let expected: &[u8] = &[0x04, 0x03, 0x02, 0x01]; // LE

    // Raw encoding (no tag)
    let mut raw = Vec::new();
    oxiproto::wire::fixed::encode_fixed32(value, &mut raw);
    assert_eq!(raw.as_slice(), expected, "fixed32 must be little-endian");

    let (decoded, consumed) =
        oxiproto::wire::fixed::decode_fixed32(expected).expect("decode_fixed32");
    assert_eq!(decoded, value);
    assert_eq!(consumed, 4);
}

/// fixed64 is stored in 8 bytes, little-endian.
#[test]
fn conformance_fixed64_little_endian() {
    let value: u64 = 0x0102_0304_0506_0708;
    let expected: &[u8] = &[0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01];

    let mut raw = Vec::new();
    oxiproto::wire::fixed::encode_fixed64(value, &mut raw);
    assert_eq!(raw.as_slice(), expected, "fixed64 must be little-endian");

    let (decoded, consumed) =
        oxiproto::wire::fixed::decode_fixed64(expected).expect("decode_fixed64");
    assert_eq!(decoded, value);
    assert_eq!(consumed, 8);
}

/// float 1.0 in IEEE 754: `\x00\x00\x80\x3f`
#[test]
fn conformance_float_one() {
    let expected: &[u8] = &[0x00, 0x00, 0x80, 0x3F];
    let mut enc = EncodeBuffer::new();
    enc.write_float(1.0f32);
    assert_eq!(enc.as_bytes(), expected, "float 1.0 must match IEEE 754 LE");

    let (decoded, consumed) = oxiproto::wire::fixed::decode_float(expected).expect("decode_float");
    assert_eq!(decoded, 1.0f32);
    assert_eq!(consumed, 4);
}

/// double 1.0 in IEEE 754: `\x00\x00\x00\x00\x00\x00\xf0\x3f`
#[test]
fn conformance_double_one() {
    let expected: &[u8] = &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F];
    let mut enc = EncodeBuffer::new();
    enc.write_double(1.0f64);
    assert_eq!(
        enc.as_bytes(),
        expected,
        "double 1.0 must match IEEE 754 LE"
    );

    let (decoded, consumed) =
        oxiproto::wire::fixed::decode_double(expected).expect("decode_double");
    assert_eq!(decoded, 1.0f64);
    assert_eq!(consumed, 8);
}

// ─── Section 6: Negative integer encoding ────────────────────────────────────

/// int32 -1 sign-extends to i64, which is encoded as u64::MAX (10 bytes).
/// Source: <https://protobuf.dev/programming-guides/encoding/#signed-ints>
#[test]
fn conformance_int32_negative_one_is_10_bytes() {
    let expected: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01];

    let mut enc = Vec::new();
    oxiproto::wire::varint::encode_varint_i32(-1, &mut enc);
    assert_eq!(
        enc.as_slice(),
        expected,
        "int32 -1 must sign-extend to 10-byte varint"
    );

    let (val, consumed) =
        oxiproto::wire::varint::decode_varint_i32(expected).expect("decode_varint_i32");
    assert_eq!(val, -1i32);
    assert_eq!(consumed, 10);
}

/// sint32 -1 (zigzag) encodes as varint(1) = single byte `\x01`.
#[test]
fn conformance_sint32_minus_one_zigzag_is_1_byte() {
    let expected: &[u8] = &[0x01];
    let zz = oxiproto::wire::zigzag::zigzag_encode32(-1i32);
    assert_eq!(zz, 1u32);

    let mut enc = Vec::new();
    oxiproto::wire::varint::encode_varint(u64::from(zz), &mut enc);
    assert_eq!(enc.as_slice(), expected, "sint32 -1 must encode as \\x01");
}

// ─── Section 7: Field ordering and unknown fields ─────────────────────────────

/// Proto3 mandates that field order in a message is well-defined (ascending by
/// field number in the canonical encoding, but parsers must accept any order).
///
/// This test verifies that a decoder reading fields out of order still produces
/// correct results.
#[test]
fn conformance_field_order_independence() {
    // Encode field 2 before field 1
    let mut enc = EncodeBuffer::new();
    enc.write_tag(2, WireType::Len).expect("tag 2");
    enc.write_string("world");
    enc.write_tag(1, WireType::Varint).expect("tag 1");
    enc.write_varint(99);
    let bytes = enc.into_vec();

    // Decode and collect by field number
    let mut dec = DecodeBuffer::new(&bytes);
    let t2 = dec.read_tag().expect("t2");
    assert_eq!(t2.field_number, 2);
    let s = dec.read_string().expect("s");
    assert_eq!(s, "world");
    let t1 = dec.read_tag().expect("t1");
    assert_eq!(t1.field_number, 1);
    let n = dec.read_varint().expect("n");
    assert_eq!(n, 99);
    assert!(dec.is_empty());
}

/// Unknown fields (fields with unrecognised field numbers) must be preserved /
/// skipped without error, per the protobuf forward-compatibility rule.
/// Source: <https://protobuf.dev/programming-guides/encoding/#unknown>
#[test]
fn conformance_unknown_fields_skipped_gracefully() {
    // Build a message with known field 1 and unknown fields 100, 200, 300.
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::Varint).expect("f1");
    enc.write_varint(7);
    enc.write_tag(100, WireType::Varint).expect("f100");
    enc.write_varint(42);
    enc.write_tag(200, WireType::Len).expect("f200");
    enc.write_string("unknown string");
    enc.write_tag(300, WireType::I64).expect("f300");
    enc.write_fixed64(0xDEAD_BEEF);
    let bytes = enc.into_vec();

    // A consumer that only knows field 1 must skip the rest.
    let mut dec = DecodeBuffer::new(&bytes);
    let mut known_value = 0u64;
    while !dec.is_empty() {
        let tag = dec.read_tag().expect("read_tag");
        match tag.field_number {
            1 => known_value = dec.read_varint().expect("known field"),
            _ => dec.skip_field(tag.wire_type).expect("skip unknown"),
        }
    }
    assert_eq!(known_value, 7, "known field 1 must decode correctly");
}

// ─── Section 8: Protobuf wire format edge cases ───────────────────────────────

/// Empty (zero-length) message is valid in proto3: all fields have their
/// default values.
#[test]
fn conformance_empty_message_is_valid() {
    let bytes: &[u8] = &[];
    let mut dec = DecodeBuffer::new(bytes);
    assert!(
        dec.is_empty(),
        "empty slice must produce an immediately-empty DecodeBuffer"
    );
    // Attempting to read a tag from an empty buffer must return UnexpectedEof,
    // not panic.
    let result = dec.read_tag();
    assert!(
        result.is_err(),
        "read_tag on empty buffer must return Err (UnexpectedEof)"
    );
}

/// Duplicate field numbers are legal: last-write-wins for singular fields,
/// appended for repeated.
/// Source: <https://protobuf.dev/programming-guides/encoding/#last-one-wins>
#[test]
fn conformance_duplicate_field_last_write_wins() {
    // Encode field 1 twice with different values.
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::Varint).expect("tag first");
    enc.write_varint(10);
    enc.write_tag(1, WireType::Varint).expect("tag second");
    enc.write_varint(20);
    let bytes = enc.into_vec();

    // Read both; the caller is responsible for last-write-wins logic.
    let mut dec = DecodeBuffer::new(&bytes);
    let mut last_val = 0u64;
    while !dec.is_empty() {
        let tag = dec.read_tag().expect("tag");
        assert_eq!(tag.field_number, 1);
        last_val = dec.read_varint().expect("val");
    }
    assert_eq!(last_val, 20, "last-write-wins: second value must be 20");
}

// ─── Section 9: OxiMessage trait conformance ─────────────────────────────────
//
// These tests verify that a hand-written `OxiMessage` impl produces conformant
// wire bytes (identical to the spec vectors from §4 above).

/// A hand-written OxiMessage for the encoding guide's Test1 message.
#[derive(Debug, Default, PartialEq, Clone)]
struct ConformanceTest1 {
    a: i32,
}

impl OxiMessage for ConformanceTest1 {
    fn encoded_len(&self) -> usize {
        use oxiproto::wire::varint::encoded_len_varint;
        if self.a != 0 {
            encoded_len_varint(8u64) + encoded_len_varint(self.a as i64 as u64)
        } else {
            0
        }
    }

    fn encode_raw(&self, buf: &mut EncodeBuffer) {
        if self.a != 0 {
            buf.write_tag(1, WireType::Varint).expect("tag");
            buf.write_varint_i32(self.a);
        }
    }

    fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = match buf.read_tag() {
                Ok(t) => t,
                Err(oxiproto::wire::WireError::UnexpectedEof) => break,
                Err(e) => return Err(OxiProtoError::WireFormatError(e)),
            };
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Varint) => {
                    self.a = buf.read_varint().map_err(OxiProtoError::WireFormatError)? as i32;
                }
                (_, wt) => buf.skip_field(wt).map_err(OxiProtoError::WireFormatError)?,
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        self.a = 0;
    }
}

/// OxiMessage impl for the encoding guide's Test2 message.
#[derive(Debug, Default, PartialEq, Clone)]
struct ConformanceTest2 {
    b: String,
}

impl OxiMessage for ConformanceTest2 {
    fn encoded_len(&self) -> usize {
        use oxiproto::wire::length_delimited::encoded_len_length_delimited;
        use oxiproto::wire::varint::encoded_len_varint;
        if !self.b.is_empty() {
            encoded_len_varint((2u64 << 3) | 2u64) + encoded_len_length_delimited(self.b.len())
        } else {
            0
        }
    }

    fn encode_raw(&self, buf: &mut EncodeBuffer) {
        if !self.b.is_empty() {
            buf.write_tag(2, WireType::Len).expect("tag");
            buf.write_string(&self.b);
        }
    }

    fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = match buf.read_tag() {
                Ok(t) => t,
                Err(oxiproto::wire::WireError::UnexpectedEof) => break,
                Err(e) => return Err(OxiProtoError::WireFormatError(e)),
            };
            match (tag.field_number, tag.wire_type) {
                (2, WireType::Len) => {
                    self.b = buf
                        .read_string()
                        .map_err(OxiProtoError::WireFormatError)?
                        .to_owned();
                }
                (_, wt) => buf.skip_field(wt).map_err(OxiProtoError::WireFormatError)?,
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        self.b.clear();
    }
}

#[test]
fn conformance_oxi_message_test1_matches_spec() {
    let msg = ConformanceTest1 { a: 150 };
    let bytes = msg.encode_to_vec();
    let expected: &[u8] = &[0x08, 0x96, 0x01];
    assert_eq!(
        bytes.as_slice(),
        expected,
        "OxiMessage Test1{{a=150}} must produce canonical spec bytes"
    );

    let decoded = ConformanceTest1::decode(expected).expect("decode");
    assert_eq!(decoded.a, 150);
}

#[test]
fn conformance_oxi_message_test2_matches_spec() {
    let msg = ConformanceTest2 {
        b: "testing".to_owned(),
    };
    let bytes = msg.encode_to_vec();
    let expected: &[u8] = &[0x12, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g'];
    assert_eq!(
        bytes.as_slice(),
        expected,
        "OxiMessage Test2{{b='testing'}} must produce canonical spec bytes"
    );

    let decoded = ConformanceTest2::decode(expected).expect("decode");
    assert_eq!(decoded.b, "testing");
}

#[test]
fn conformance_oxi_message_encoded_len_matches_actual_test1() {
    let msg = ConformanceTest1 { a: 150 };
    let bytes = msg.encode_to_vec();
    assert_eq!(
        msg.encoded_len(),
        bytes.len(),
        "encoded_len() must equal actual byte count"
    );
}

#[test]
fn conformance_oxi_message_round_trip_test1() {
    let original = ConformanceTest1 { a: -500 };
    let bytes = original.encode_to_vec();
    let decoded = ConformanceTest1::decode(&bytes).expect("decode");
    assert_eq!(original, decoded, "OxiMessage round-trip must be lossless");
}

#[test]
fn conformance_oxi_message_round_trip_test2() {
    let original = ConformanceTest2 {
        b: "hello, protobuf!".to_owned(),
    };
    let bytes = original.encode_to_vec();
    let decoded = ConformanceTest2::decode(&bytes).expect("decode");
    assert_eq!(original, decoded, "OxiMessage round-trip must be lossless");
}

// ─── Section 10: Spec-specified wire type numbers ─────────────────────────────

/// Wire type numbers are defined by the spec:
/// 0 = Varint, 1 = I64, 2 = Len, 3 = SGroup, 4 = EGroup, 5 = I32.
#[test]
fn conformance_wire_type_values() {
    assert_eq!(WireType::Varint.value(), 0, "Varint wire type must be 0");
    assert_eq!(WireType::I64.value(), 1, "I64 wire type must be 1");
    assert_eq!(WireType::Len.value(), 2, "Len wire type must be 2");
    assert_eq!(WireType::SGroup.value(), 3, "SGroup wire type must be 3");
    assert_eq!(WireType::EGroup.value(), 4, "EGroup wire type must be 4");
    assert_eq!(WireType::I32.value(), 5, "I32 wire type must be 5");
}

// ─── Section 11: 128-byte-boundary varint length prefix ──────────────────────

/// A length-delimited field whose payload is exactly 128 bytes uses a 2-byte
/// varint for the length: `\x80\x01` (= 128 in LEB128).
#[test]
fn conformance_length_128_uses_two_byte_varint() {
    let payload = vec![0xAA_u8; 128];
    let mut enc = EncodeBuffer::new();
    enc.write_length_delimited(&payload);
    let bytes = enc.into_vec();
    // Length prefix is 2 bytes: \x80 \x01
    assert_eq!(bytes[0], 0x80, "128-byte varint: low byte must be 0x80");
    assert_eq!(bytes[1], 0x01, "128-byte varint: high byte must be 0x01");
    assert_eq!(&bytes[2..], payload.as_slice());
}
