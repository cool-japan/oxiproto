//! Wire-format compatibility tests: golden byte vectors.
//!
//! These tests verify that the native `oxiproto_core::wire` codec produces
//! byte-for-byte identical output to the canonical protobuf binary encoding
//! as specified in the protobuf binary format documentation and as implemented
//! by the reference implementations (Go, C++, Java).
//!
//! All golden bytes were derived from the canonical protobuf encoding rules:
//!   - <https://protobuf.dev/programming-guides/encoding/>
//!
//! Each test encodes a value, checks it against the known golden bytes, and
//! then decodes the golden bytes to verify round-trip correctness.

use oxiproto_core::wire::{
    decode_double, decode_fixed32, decode_fixed64, decode_float, decode_sfixed32, decode_sfixed64,
    decode_string, decode_varint, decode_varint_i32, decode_varint_i64, encode_fixed32,
    encode_fixed64, encode_sfixed32, encode_sfixed64, encode_varint, encode_varint_i32,
    encode_varint_i64, zigzag_decode32, zigzag_decode64, zigzag_encode32, zigzag_encode64,
    DecodeBuffer, EncodeBuffer, WireType,
};

// ---------------------------------------------------------------------------
// Varint golden bytes (canonical LEB128 encoding)
// ---------------------------------------------------------------------------

/// From the protobuf encoding guide: the value 1 encodes as `\x01` (1 byte).
#[test]
fn compat_varint_one() {
    let golden = [0x01_u8];
    let mut buf = Vec::new();
    encode_varint(1, &mut buf);
    assert_eq!(buf.as_slice(), &golden, "encode");
    let (val, consumed) = decode_varint(&golden).expect("decode");
    assert_eq!(val, 1);
    assert_eq!(consumed, 1);
}

/// The value 150 (used in the encoding guide example) encodes as `\x96\x01`.
#[test]
fn compat_varint_150() {
    // 150 = 0b10010110 = 96 01 in varint
    // byte 0: 0x96 = 10010110 → 7 bits = 0010110 = 22, continuation set
    // byte 1: 0x01 = 00000001 → 7 bits = 0000001 = 1
    // value = 1 << 7 | 22 = 128 + 22 = 150
    let golden = [0x96_u8, 0x01];
    let mut buf = Vec::new();
    encode_varint(150, &mut buf);
    assert_eq!(buf.as_slice(), &golden, "encode 150");
    let (val, _) = decode_varint(&golden).expect("decode");
    assert_eq!(val, 150);
}

/// Value 300 encodes as `\xac\x02`.
#[test]
fn compat_varint_300() {
    let golden = [0xAC_u8, 0x02];
    let mut buf = Vec::new();
    encode_varint(300, &mut buf);
    assert_eq!(buf.as_slice(), &golden);
    let (val, consumed) = decode_varint(&golden).expect("decode");
    assert_eq!(val, 300);
    assert_eq!(consumed, 2);
}

/// Value 0 encodes as a single zero byte.
#[test]
fn compat_varint_zero() {
    let golden = [0x00_u8];
    let mut buf = Vec::new();
    encode_varint(0, &mut buf);
    assert_eq!(buf.as_slice(), &golden);
    let (val, _) = decode_varint(&golden).expect("decode");
    assert_eq!(val, 0);
}

/// Value 127 (0x7F) encodes as a single `\x7f` byte (no continuation).
#[test]
fn compat_varint_127() {
    let golden = [0x7F_u8];
    let mut buf = Vec::new();
    encode_varint(127, &mut buf);
    assert_eq!(buf.as_slice(), &golden);
}

/// Value 128 (first 2-byte varint) encodes as `\x80\x01`.
#[test]
fn compat_varint_128() {
    let golden = [0x80_u8, 0x01];
    let mut buf = Vec::new();
    encode_varint(128, &mut buf);
    assert_eq!(buf.as_slice(), &golden);
}

/// Maximum u64 value (u64::MAX = 0xFFFF_FFFF_FFFF_FFFF) encodes as 10 bytes,
/// all `\xff` except the final `\x01`.
#[test]
fn compat_varint_u64_max() {
    // u64::MAX = 2^64 - 1 = 18446744073709551615
    // LEB128 encoding: 10 bytes of 0xFF then 0x01
    let golden = [
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01_u8,
    ];
    let mut buf = Vec::new();
    let n = encode_varint(u64::MAX, &mut buf);
    assert_eq!(n, 10);
    assert_eq!(buf.as_slice(), &golden);
    let (val, consumed) = decode_varint(&golden).expect("decode");
    assert_eq!(val, u64::MAX);
    assert_eq!(consumed, 10);
}

// ---------------------------------------------------------------------------
// Negative int32/int64 varint encoding (sign-extension to 64 bits)
// ---------------------------------------------------------------------------

/// int32 value -1 encodes as 10 bytes (sign-extended to i64 = u64::MAX).
#[test]
fn compat_int32_negative_one() {
    // proto3: int32 -1 sign-extends to i64 then reinterprets as u64 (= u64::MAX)
    let golden = [
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01_u8,
    ];
    let mut buf = Vec::new();
    encode_varint_i32(-1, &mut buf);
    assert_eq!(buf.as_slice(), &golden, "encode int32 -1");
    let (val, _) = decode_varint_i32(&golden).expect("decode");
    assert_eq!(val, -1);
}

/// int64 value -1 also encodes as 10 bytes.
#[test]
fn compat_int64_negative_one() {
    let golden = [
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01_u8,
    ];
    let mut buf = Vec::new();
    encode_varint_i64(-1, &mut buf);
    assert_eq!(buf.as_slice(), &golden, "encode int64 -1");
    let (val, _) = decode_varint_i64(&golden).expect("decode");
    assert_eq!(val, -1i64);
}

/// int32 value i32::MIN (-2147483648) encodes as 10 bytes.
#[test]
fn compat_int32_min() {
    // i32::MIN as u64 = 0xFFFF_FFFF_8000_0000
    // LEB128 of 0xFFFF_FFFF_8000_0000:
    let golden = [
        0x80, 0x80, 0x80, 0x80, 0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0x01_u8,
    ];
    let mut buf = Vec::new();
    encode_varint_i32(i32::MIN, &mut buf);
    assert_eq!(buf.as_slice(), &golden, "encode i32::MIN");
    let (val, _) = decode_varint_i32(&golden).expect("decode");
    assert_eq!(val, i32::MIN);
}

// ---------------------------------------------------------------------------
// ZigZag encoding (sint32 / sint64)
// ---------------------------------------------------------------------------

/// ZigZag(0) = 0.
#[test]
fn compat_zigzag_zero() {
    assert_eq!(zigzag_encode32(0), 0);
    assert_eq!(zigzag_decode32(0), 0);
    assert_eq!(zigzag_encode64(0), 0);
    assert_eq!(zigzag_decode64(0), 0);
}

/// ZigZag(-1) = 1 (first positive).
#[test]
fn compat_zigzag_minus_one() {
    assert_eq!(zigzag_encode32(-1), 1);
    assert_eq!(zigzag_decode32(1), -1);
    assert_eq!(zigzag_encode64(-1i64), 1);
    assert_eq!(zigzag_decode64(1), -1i64);
}

/// ZigZag(1) = 2.
#[test]
fn compat_zigzag_one() {
    assert_eq!(zigzag_encode32(1), 2);
    assert_eq!(zigzag_decode32(2), 1);
    assert_eq!(zigzag_encode64(1i64), 2);
    assert_eq!(zigzag_decode64(2), 1i64);
}

/// ZigZag(-2) = 3.
#[test]
fn compat_zigzag_minus_two() {
    assert_eq!(zigzag_encode32(-2), 3);
    assert_eq!(zigzag_decode32(3), -2);
}

/// ZigZag(i32::MIN) = u32::MAX (maximum zigzag value).
#[test]
fn compat_zigzag_i32_min() {
    assert_eq!(zigzag_encode32(i32::MIN), u32::MAX);
    assert_eq!(zigzag_decode32(u32::MAX), i32::MIN);
}

/// ZigZag(i32::MAX) = u32::MAX - 1.
#[test]
fn compat_zigzag_i32_max() {
    assert_eq!(zigzag_encode32(i32::MAX), u32::MAX - 1);
    assert_eq!(zigzag_decode32(u32::MAX - 1), i32::MAX);
}

/// sint32 -1 encodes as varint(1) = `\x01`.
#[test]
fn compat_sint32_minus_one_wire() {
    let golden = [0x01_u8];
    let mut buf = Vec::new();
    encode_varint(u64::from(zigzag_encode32(-1)), &mut buf);
    assert_eq!(buf.as_slice(), &golden);
}

/// sint32 -2147483648 (i32::MIN) encodes as varint(u32::MAX).
#[test]
fn compat_sint32_i32_min_wire() {
    // zigzag(i32::MIN) = u32::MAX = 4294967295
    // varint(4294967295) = 5 bytes: \xff\xff\xff\xff\x0f
    let golden = [0xFF, 0xFF, 0xFF, 0xFF, 0x0F_u8];
    let mut buf = Vec::new();
    encode_varint(u64::from(zigzag_encode32(i32::MIN)), &mut buf);
    assert_eq!(buf.as_slice(), &golden);
    let (raw, _) = decode_varint(&golden).expect("decode");
    assert_eq!(zigzag_decode32(raw as u32), i32::MIN);
}

// ---------------------------------------------------------------------------
// Field tag encoding  (field_number << 3 | wire_type)
// ---------------------------------------------------------------------------

/// Field 1 with wire type Varint (0): tag = (1 << 3) | 0 = 8 = `\x08`.
#[test]
fn compat_tag_field1_varint() {
    let golden = [0x08_u8];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::Varint).expect("write_tag");
    assert_eq!(enc.as_bytes(), &golden);
}

/// Field 2 with wire type Len (2): tag = (2 << 3) | 2 = 18 = `\x12`.
#[test]
fn compat_tag_field2_len() {
    let golden = [0x12_u8];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(2, WireType::Len).expect("write_tag");
    assert_eq!(enc.as_bytes(), &golden);
}

/// Field 1 with wire type I64 (1): tag = (1 << 3) | 1 = 9 = `\x09`.
#[test]
fn compat_tag_field1_i64() {
    let golden = [0x09_u8];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::I64).expect("write_tag");
    assert_eq!(enc.as_bytes(), &golden);
}

/// Field 1 with wire type I32 (5): tag = (1 << 3) | 5 = 13 = `\x0d`.
#[test]
fn compat_tag_field1_i32() {
    let golden = [0x0D_u8];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::I32).expect("write_tag");
    assert_eq!(enc.as_bytes(), &golden);
}

/// Field 16 with wire type Varint: tag = (16 << 3) | 0 = 128 = `\x80\x01` (2-byte varint).
#[test]
fn compat_tag_field16_varint() {
    let golden = [0x80_u8, 0x01];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(16, WireType::Varint).expect("write_tag");
    assert_eq!(enc.as_bytes(), &golden);
}

// ---------------------------------------------------------------------------
// Fixed32 / fixed64 / sfixed32 / sfixed64 golden bytes (little-endian)
// ---------------------------------------------------------------------------

/// fixed32 value 0x01020304 encodes as `\x04\x03\x02\x01` (little-endian).
#[test]
fn compat_fixed32_little_endian() {
    let golden = [0x04_u8, 0x03, 0x02, 0x01];
    let mut buf = Vec::new();
    encode_fixed32(0x01020304, &mut buf);
    assert_eq!(buf.as_slice(), &golden);
    let (val, consumed) = decode_fixed32(&golden).expect("decode");
    assert_eq!(val, 0x01020304);
    assert_eq!(consumed, 4);
}

/// fixed64 value 0x0102030405060708 encodes in little-endian order.
#[test]
fn compat_fixed64_little_endian() {
    let golden = [0x08_u8, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01];
    let mut buf = Vec::new();
    encode_fixed64(0x0102_0304_0506_0708, &mut buf);
    assert_eq!(buf.as_slice(), &golden);
    let (val, consumed) = decode_fixed64(&golden).expect("decode");
    assert_eq!(val, 0x0102_0304_0506_0708);
    assert_eq!(consumed, 8);
}

/// sfixed32 value -1 encodes as `\xff\xff\xff\xff`.
#[test]
fn compat_sfixed32_minus_one() {
    let golden = [0xFF_u8, 0xFF, 0xFF, 0xFF];
    let mut buf = Vec::new();
    encode_sfixed32(-1i32, &mut buf);
    assert_eq!(buf.as_slice(), &golden);
    let (val, _) = decode_sfixed32(&golden).expect("decode");
    assert_eq!(val, -1i32);
}

/// sfixed64 value -1 encodes as 8 `\xff` bytes.
#[test]
fn compat_sfixed64_minus_one() {
    let golden = [0xFF_u8; 8];
    let mut buf = Vec::new();
    encode_sfixed64(-1i64, &mut buf);
    assert_eq!(buf.as_slice(), &golden);
    let (val, _) = decode_sfixed64(&golden).expect("decode");
    assert_eq!(val, -1i64);
}

/// float 1.0 encodes as IEEE 754 little-endian: `\x00\x00\x80\x3f`.
#[test]
fn compat_float_one() {
    // 1.0f32 IEEE 754 = 0x3F800000 → LE = [0x00, 0x00, 0x80, 0x3F]
    let golden = [0x00_u8, 0x00, 0x80, 0x3F];
    let mut enc = EncodeBuffer::new();
    enc.write_float(1.0f32);
    assert_eq!(enc.as_bytes(), &golden);
    let dec_val = {
        let bits = u32::from_le_bytes(golden);
        f32::from_bits(bits)
    };
    assert_eq!(dec_val, 1.0f32);
}

/// double 1.0 encodes as IEEE 754 little-endian: `\x00\x00\x00\x00\x00\x00\xf0\x3f`.
#[test]
fn compat_double_one() {
    // 1.0f64 IEEE 754 = 0x3FF0000000000000 → LE = [0x00,0x00,0x00,0x00,0x00,0x00,0xF0,0x3F]
    let golden = [0x00_u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F];
    let mut enc = EncodeBuffer::new();
    enc.write_double(1.0f64);
    assert_eq!(enc.as_bytes(), &golden);
    let dec_val = {
        let bits = u64::from_le_bytes(golden);
        f64::from_bits(bits)
    };
    assert_eq!(dec_val, 1.0f64);
}

/// float -1.5 encodes as `\x00\x00\xc0\xbf`.
#[test]
fn compat_float_minus_1_5() {
    // -1.5f32 IEEE 754 = 0xBFC00000 → LE = [0x00, 0x00, 0xC0, 0xBF]
    let golden = [0x00_u8, 0x00, 0xC0, 0xBF];
    let mut enc = EncodeBuffer::new();
    enc.write_float(-1.5f32);
    assert_eq!(enc.as_bytes(), &golden);
}

/// fixed32 all-zeros encodes as 4 zero bytes.
#[test]
fn compat_fixed32_zero() {
    let golden = [0x00_u8, 0x00, 0x00, 0x00];
    let mut buf = Vec::new();
    encode_fixed32(0, &mut buf);
    assert_eq!(buf.as_slice(), &golden);
}

/// fixed64 all-zeros encodes as 8 zero bytes.
#[test]
fn compat_fixed64_zero() {
    let golden = [0x00_u8; 8];
    let mut buf = Vec::new();
    encode_fixed64(0, &mut buf);
    assert_eq!(buf.as_slice(), &golden);
}

// ---------------------------------------------------------------------------
// Length-delimited field encoding
// ---------------------------------------------------------------------------

/// An empty byte sequence encodes as `\x00` (length 0).
#[test]
fn compat_length_delimited_empty() {
    let golden = [0x00_u8];
    let mut enc = EncodeBuffer::new();
    enc.write_length_delimited(&[]);
    assert_eq!(enc.as_bytes(), &golden);
}

/// The string "testing" (7 bytes) encodes as `\x07testing`.
#[test]
fn compat_string_testing() {
    // tag for field 2, wire type Len: 0x12, then \x07, then "testing"
    let golden = [0x07_u8, b't', b'e', b's', b't', b'i', b'n', b'g'];
    let mut enc = EncodeBuffer::new();
    enc.write_string("testing");
    assert_eq!(enc.as_bytes(), &golden);
    let (s, consumed) = decode_string(&golden).expect("decode");
    assert_eq!(s, "testing");
    assert_eq!(consumed, 8);
}

/// A 128-byte payload: length prefix encodes as `\x80\x01` (2-byte varint).
#[test]
fn compat_length_delimited_128_bytes() {
    let payload = vec![0xAA_u8; 128];
    let mut enc = EncodeBuffer::new();
    enc.write_length_delimited(&payload);
    let bytes = enc.as_bytes();
    // First two bytes are the varint-encoded length 128 = 0x80 0x01
    assert_eq!(bytes[0], 0x80);
    assert_eq!(bytes[1], 0x01);
    // Remaining 128 bytes are the payload
    assert_eq!(&bytes[2..], payload.as_slice());
}

// ---------------------------------------------------------------------------
// Complete message encoding golden tests
// ---------------------------------------------------------------------------

/// Protobuf encoding guide example: `message Test1 { int32 a = 1; }` with a = 150.
/// Encodes to `\x08\x96\x01`.
#[test]
fn compat_message_test1_a_150() {
    // tag for field 1, Varint: (1 << 3) | 0 = 8 = \x08
    // varint(150) = \x96\x01
    let golden = [0x08_u8, 0x96, 0x01];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::Varint).expect("tag");
    enc.write_varint(150);
    assert_eq!(enc.as_bytes(), &golden);
    // Decode round-trip
    let mut dec = DecodeBuffer::new(&golden);
    let tag = dec.read_tag().expect("tag");
    assert_eq!(tag.field_number, 1);
    assert_eq!(tag.wire_type, WireType::Varint);
    let val = dec.read_varint().expect("val");
    assert_eq!(val, 150);
    assert!(dec.is_empty());
}

/// `message Test2 { string b = 2; }` with b = "testing".
/// Encodes to `\x12\x07testing`.
#[test]
fn compat_message_test2_b_testing() {
    // tag for field 2, Len: (2 << 3) | 2 = 18 = \x12
    // length-delimited "testing": \x07testing
    let golden = [0x12_u8, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g'];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(2, WireType::Len).expect("tag");
    enc.write_string("testing");
    assert_eq!(enc.as_bytes(), &golden);
    // Decode round-trip
    let mut dec = DecodeBuffer::new(&golden);
    let tag = dec.read_tag().expect("tag");
    assert_eq!(tag.field_number, 2);
    assert_eq!(tag.wire_type, WireType::Len);
    let s = dec.read_string().expect("string");
    assert_eq!(s, "testing");
    assert!(dec.is_empty());
}

/// `message Test3 { Test1 c = 3; }` where Test1.a = 150.
/// Encodes to `\x1a\x03\x08\x96\x01` (embedded message).
#[test]
fn compat_embedded_message_test3() {
    // Inner message Test1 { a: 150 } = \x08\x96\x01 (3 bytes)
    // tag for field 3, Len: (3 << 3) | 2 = 26 = \x1a
    // length: 3 = \x03
    let golden = [0x1A_u8, 0x03, 0x08, 0x96, 0x01];
    // Build inner
    let mut inner = EncodeBuffer::new();
    inner.write_tag(1, WireType::Varint).expect("inner tag");
    inner.write_varint(150);
    assert_eq!(inner.len(), 3);
    // Build outer
    let mut outer = EncodeBuffer::new();
    outer.write_tag(3, WireType::Len).expect("outer tag");
    outer.write_length_delimited(inner.as_bytes());
    assert_eq!(outer.as_bytes(), &golden);
    // Decode round-trip
    let mut dec = DecodeBuffer::new(&golden);
    let tag = dec.read_tag().expect("tag");
    assert_eq!(tag.field_number, 3);
    assert_eq!(tag.wire_type, WireType::Len);
    let payload = dec.read_length_delimited().expect("payload");
    assert_eq!(payload, &[0x08, 0x96, 0x01]);
    let mut inner_dec = DecodeBuffer::new(payload);
    let inner_tag = inner_dec.read_tag().expect("inner tag");
    assert_eq!(inner_tag.field_number, 1);
    let inner_val = inner_dec.read_varint().expect("inner val");
    assert_eq!(inner_val, 150);
    assert!(inner_dec.is_empty());
    assert!(dec.is_empty());
}

/// `message Test4 { repeated int32 d = 4; }` with d = [3, 270, 86942] (packed).
/// Packed encoding: `\x22\x06\x03\x8e\x02\x9e\xa7\x05`.
#[test]
fn compat_packed_repeated_int32() {
    // d = [3, 270, 86942]
    // varint(3)     = \x03
    // varint(270)   = \x8e\x02
    // varint(86942) = \x9e\xa7\x05
    // payload = \x03\x8e\x02\x9e\xa7\x05 (6 bytes)
    // tag for field 4, Len: (4 << 3) | 2 = 34 = \x22
    // length: 6 = \x06
    let golden = [0x22_u8, 0x06, 0x03, 0x8E, 0x02, 0x9E, 0xA7, 0x05];
    // Build packed payload
    let mut payload = EncodeBuffer::new();
    payload.write_varint(3);
    payload.write_varint(270);
    payload.write_varint(86942);
    assert_eq!(payload.len(), 6);
    assert_eq!(payload.as_bytes(), &[0x03, 0x8E, 0x02, 0x9E, 0xA7, 0x05]);
    // Build message
    let mut enc = EncodeBuffer::new();
    enc.write_tag(4, WireType::Len).expect("tag");
    enc.write_length_delimited(payload.as_bytes());
    assert_eq!(enc.as_bytes(), &golden);
    // Decode round-trip: parse packed blob
    let mut dec = DecodeBuffer::new(&golden);
    let tag = dec.read_tag().expect("tag");
    assert_eq!(tag.field_number, 4);
    assert_eq!(tag.wire_type, WireType::Len);
    let blob = dec.read_length_delimited().expect("blob");
    let mut inner = DecodeBuffer::new(blob);
    let v1 = inner.read_varint().expect("v1");
    assert_eq!(v1, 3);
    let v2 = inner.read_varint().expect("v2");
    assert_eq!(v2, 270);
    let v3 = inner.read_varint().expect("v3");
    assert_eq!(v3, 86942);
    assert!(inner.is_empty());
    assert!(dec.is_empty());
}

// ---------------------------------------------------------------------------
// bool encoding (proto3 rule: false is NOT emitted)
// ---------------------------------------------------------------------------

/// bool false encodes as varint 0 = `\x00`.
#[test]
fn compat_bool_false_varint() {
    let golden = [0x00_u8];
    let mut enc = EncodeBuffer::new();
    enc.write_bool(false);
    assert_eq!(enc.as_bytes(), &golden);
}

/// bool true encodes as varint 1 = `\x01`.
#[test]
fn compat_bool_true_varint() {
    let golden = [0x01_u8];
    let mut enc = EncodeBuffer::new();
    enc.write_bool(true);
    assert_eq!(enc.as_bytes(), &golden);
}

// ---------------------------------------------------------------------------
// Two's complement negative int32/int64 in message context
// ---------------------------------------------------------------------------

/// message field id=1 with int32 value -1.
/// Encodes as: `\x08` (tag 1/Varint) + 10 bytes of `\xff` then `\x01`.
#[test]
fn compat_message_int32_negative_one() {
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::Varint).expect("tag");
    enc.write_varint_i32(-1);
    let bytes = enc.as_bytes();
    assert_eq!(bytes[0], 0x08); // tag
    assert_eq!(bytes.len(), 11); // 1 byte tag + 10 bytes varint
    assert_eq!(
        &bytes[1..],
        &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]
    );
}

// ---------------------------------------------------------------------------
// bytes field encoding
// ---------------------------------------------------------------------------

/// bytes field {1: [0x01, 0x02, 0x03]} encodes as `\x0a\x03\x01\x02\x03`.
#[test]
fn compat_bytes_field() {
    // tag field 1 Len: (1<<3)|2 = 10 = \x0a
    // length: 3 = \x03
    // payload: \x01\x02\x03
    let golden = [0x0A_u8, 0x03, 0x01, 0x02, 0x03];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::Len).expect("tag");
    enc.write_length_delimited(&[0x01, 0x02, 0x03]);
    assert_eq!(enc.as_bytes(), &golden);
}

// ---------------------------------------------------------------------------
// Multiple fields in a message (field ordering, concatenation)
// ---------------------------------------------------------------------------

/// message { int32 a = 1; string b = 2; bool c = 3; } with a=42, b="hi", c=true.
/// Expected: tag1/varint + varint(42) + tag2/len + len(2) + "hi" + tag3/varint + varint(1).
#[test]
fn compat_multi_field_message() {
    // tag 1 Varint: \x08
    // varint(42):   \x2a
    // tag 2 Len:    \x12
    // length 2:     \x02
    // "hi":         \x68\x69
    // tag 3 Varint: \x18
    // bool true:    \x01
    let golden = [0x08, 0x2A, 0x12, 0x02, b'h', b'i', 0x18, 0x01];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::Varint).expect("tag1");
    enc.write_varint(42);
    enc.write_tag(2, WireType::Len).expect("tag2");
    enc.write_string("hi");
    enc.write_tag(3, WireType::Varint).expect("tag3");
    enc.write_bool(true);
    assert_eq!(enc.as_bytes(), &golden);
    // Decode
    let mut dec = DecodeBuffer::new(&golden);
    let t1 = dec.read_tag().expect("t1");
    assert_eq!(t1.field_number, 1);
    assert_eq!(dec.read_varint().expect("a"), 42);
    let t2 = dec.read_tag().expect("t2");
    assert_eq!(t2.field_number, 2);
    assert_eq!(dec.read_string().expect("b"), "hi");
    let t3 = dec.read_tag().expect("t3");
    assert_eq!(t3.field_number, 3);
    assert!(dec.read_bool().expect("c"));
    assert!(dec.is_empty());
}

// ---------------------------------------------------------------------------
// double field encoding golden test
// ---------------------------------------------------------------------------

/// double 1.0 as field 1: `\x09\x00\x00\x00\x00\x00\x00\xf0\x3f`.
#[test]
fn compat_double_field_1() {
    // tag 1 I64: (1<<3)|1 = 9 = \x09
    let golden = [0x09_u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::I64).expect("tag");
    enc.write_double(1.0f64);
    assert_eq!(enc.as_bytes(), &golden);
    let mut dec = DecodeBuffer::new(&golden);
    let tag = dec.read_tag().expect("tag");
    assert_eq!(tag.field_number, 1);
    assert_eq!(tag.wire_type, WireType::I64);
    assert_eq!(dec.read_double().expect("val"), 1.0f64);
    assert!(dec.is_empty());
}

// ---------------------------------------------------------------------------
// float field encoding golden test
// ---------------------------------------------------------------------------

/// float 1.0 as field 5: `\x2d\x00\x00\x80\x3f`.
#[test]
fn compat_float_field_5() {
    // tag 5 I32: (5<<3)|5 = 45 = \x2d
    let golden = [0x2D_u8, 0x00, 0x00, 0x80, 0x3F];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(5, WireType::I32).expect("tag");
    enc.write_float(1.0f32);
    assert_eq!(enc.as_bytes(), &golden);
    let mut dec = DecodeBuffer::new(&golden);
    let tag = dec.read_tag().expect("tag");
    assert_eq!(tag.field_number, 5);
    assert_eq!(tag.wire_type, WireType::I32);
    assert_eq!(dec.read_float().expect("val"), 1.0f32);
    assert!(dec.is_empty());
}

// ---------------------------------------------------------------------------
// Unknown field skip: conformance with forward-compatibility rule
// ---------------------------------------------------------------------------

/// A consumer that knows only field 1 must skip unknown field 3 (I64 wire type).
#[test]
fn compat_unknown_field_skip() {
    // Encode: field 1 varint 42, field 3 fixed64 0xDEAD
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::Varint).expect("tag1");
    enc.write_varint(42);
    enc.write_tag(3, WireType::I64).expect("tag3");
    enc.write_fixed64(0xDEAD);
    let bytes = enc.into_vec();
    // Consumer knowing only field 1
    let mut dec = DecodeBuffer::new(&bytes);
    let t1 = dec.read_tag().expect("t1");
    assert_eq!(t1.field_number, 1);
    let val = dec.read_varint().expect("val");
    assert_eq!(val, 42);
    // Unknown field 3 — must skip without error
    let t3 = dec.read_tag().expect("t3");
    assert_eq!(t3.field_number, 3);
    dec.skip_field(t3.wire_type).expect("skip_field");
    assert!(dec.is_empty());
}

/// A consumer must skip an unknown length-delimited field.
#[test]
fn compat_skip_unknown_len_field() {
    let mut enc = EncodeBuffer::new();
    enc.write_tag(99, WireType::Len).expect("tag");
    enc.write_string("unknown payload");
    enc.write_tag(1, WireType::Varint).expect("tag1");
    enc.write_varint(7);
    let bytes = enc.into_vec();
    let mut dec = DecodeBuffer::new(&bytes);
    // Skip unknown field 99
    let t99 = dec.read_tag().expect("t99");
    assert_eq!(t99.field_number, 99);
    dec.skip_field(t99.wire_type).expect("skip len");
    // Read known field 1
    let t1 = dec.read_tag().expect("t1");
    assert_eq!(t1.field_number, 1);
    let val = dec.read_varint().expect("val");
    assert_eq!(val, 7);
    assert!(dec.is_empty());
}

// ---------------------------------------------------------------------------
// High field numbers (varint-encoded tag)
// ---------------------------------------------------------------------------

/// Field number 2047 (max single-byte-field = 15 for 1-byte tag; 2047 = 2-byte tag).
/// tag = (2047 << 3) | 0 = 16376 = varint [0xF8, 0x7F].
#[test]
fn compat_field_number_2047() {
    // (2047 << 3) = 16376
    // varint(16376): 16376 = 0x3FF8 → [0xF8, 0x7F]
    let golden = [0xF8_u8, 0x7F];
    let mut enc = EncodeBuffer::new();
    enc.write_tag(2047, WireType::Varint).expect("tag");
    assert_eq!(enc.as_bytes(), &golden);
}

// ---------------------------------------------------------------------------
// Decode-only canonical byte sequences from the spec
// ---------------------------------------------------------------------------

/// Canonical decode of the encoding guide example:
/// a two-byte varint `\x96\x01` decodes to 150.
#[test]
fn compat_canonical_decode_varint_150() {
    let bytes = [0x96_u8, 0x01];
    let (v, c) = decode_varint(&bytes).expect("decode");
    assert_eq!(v, 150);
    assert_eq!(c, 2);
}

/// Canonical decode of a fixed32 `\x01\x00\x00\x00` (1 in little-endian).
#[test]
fn compat_canonical_decode_fixed32_one() {
    let bytes = [0x01_u8, 0x00, 0x00, 0x00];
    let (v, c) = decode_fixed32(&bytes).expect("decode");
    assert_eq!(v, 1u32);
    assert_eq!(c, 4);
}

/// Canonical decode of a fixed64 `\x01\x00\x00\x00\x00\x00\x00\x00` (1 LE).
#[test]
fn compat_canonical_decode_fixed64_one() {
    let bytes = [0x01_u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let (v, c) = decode_fixed64(&bytes).expect("decode");
    assert_eq!(v, 1u64);
    assert_eq!(c, 8);
}

/// Canonical decode of a float `\x00\x00\x80\x3f` (1.0f32 little-endian).
#[test]
fn compat_canonical_decode_float_one() {
    let bytes = [0x00_u8, 0x00, 0x80, 0x3F];
    let (v, c) = decode_float(&bytes).expect("decode");
    assert_eq!(v, 1.0f32);
    assert_eq!(c, 4);
}

/// Canonical decode of a double `\x00\x00\x00\x00\x00\x00\xf0\x3f` (1.0f64).
#[test]
fn compat_canonical_decode_double_one() {
    let bytes = [0x00_u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F];
    let (v, c) = decode_double(&bytes).expect("decode");
    assert_eq!(v, 1.0f64);
    assert_eq!(c, 8);
}

// ---------------------------------------------------------------------------
// Interop with prost-generated bytes (cross-validation)
// ---------------------------------------------------------------------------

/// Encode a message using prost::Message derive, then decode with our wire module.
/// Verifies byte-for-byte compatibility between prost and native codec.
#[test]
fn compat_prost_interop_encode_decode() {
    use prost::Message as _;

    // prost-generated message: { int32 id = 1; string name = 2; }
    #[derive(Clone, prost::Message)]
    struct PersonProto {
        #[prost(int32, tag = "1")]
        id: i32,
        #[prost(string, tag = "2")]
        name: String,
    }

    let msg = PersonProto {
        id: 42,
        name: "Alice".to_owned(),
    };
    let prost_bytes = msg.encode_to_vec();

    // Decode with our native wire module
    let mut dec = DecodeBuffer::new(&prost_bytes);
    // Field 1: id = 42
    let t1 = dec.read_tag().expect("t1");
    assert_eq!(t1.field_number, 1);
    assert_eq!(t1.wire_type, WireType::Varint);
    let id_val = dec.read_varint().expect("id");
    assert_eq!(id_val, 42);
    // Field 2: name = "Alice"
    let t2 = dec.read_tag().expect("t2");
    assert_eq!(t2.field_number, 2);
    assert_eq!(t2.wire_type, WireType::Len);
    let name_str = dec.read_string().expect("name");
    assert_eq!(name_str, "Alice");
    assert!(dec.is_empty());

    // Encode with our native codec, compare bytes
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::Varint).expect("tag1");
    enc.write_varint(42);
    enc.write_tag(2, WireType::Len).expect("tag2");
    enc.write_string("Alice");
    assert_eq!(enc.as_bytes(), prost_bytes.as_slice());
}

/// Encode a prost message with repeated int32, then decode packed bytes natively.
#[test]
fn compat_prost_interop_packed_repeated() {
    use prost::Message as _;

    #[derive(Clone, prost::Message)]
    struct WithRepeated {
        #[prost(int32, repeated, packed = "true", tag = "1")]
        values: Vec<i32>,
    }

    let msg = WithRepeated {
        values: vec![1, 2, 3, 4, 5],
    };
    let prost_bytes = msg.encode_to_vec();

    // Decode with native module
    let mut dec = DecodeBuffer::new(&prost_bytes);
    let tag = dec.read_tag().expect("tag");
    assert_eq!(tag.field_number, 1);
    assert_eq!(tag.wire_type, WireType::Len);
    let payload = dec.read_length_delimited().expect("payload");
    let mut inner = DecodeBuffer::new(payload);
    let mut values = Vec::new();
    while !inner.is_empty() {
        values.push(inner.read_varint().expect("elem") as i32);
    }
    assert_eq!(values, vec![1, 2, 3, 4, 5]);
    assert!(dec.is_empty());
}

/// Native-encoded bytes are accepted by prost decoder.
#[test]
fn compat_native_encode_prost_decode() {
    use prost::Message as _;

    #[derive(Clone, prost::Message)]
    struct CounterProto {
        #[prost(uint64, tag = "1")]
        count: u64,
        #[prost(bool, tag = "2")]
        active: bool,
    }

    // Encode natively
    let mut enc = EncodeBuffer::new();
    enc.write_tag(1, WireType::Varint).expect("tag1");
    enc.write_varint(99_999_999u64);
    enc.write_tag(2, WireType::Varint).expect("tag2");
    enc.write_bool(true);
    let native_bytes = enc.into_vec();

    // Decode with prost
    let decoded = CounterProto::decode(native_bytes.as_slice()).expect("prost decode");
    assert_eq!(decoded.count, 99_999_999u64);
    assert!(decoded.active);
}
