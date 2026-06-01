//! Zero-copy read/write buffer abstractions for wire format operations.
//!
//! [`DecodeBuffer`] wraps a `&[u8]` and provides cursor-based reading of
//! protobuf wire format primitives. [`EncodeBuffer`] wraps a `Vec<u8>` and
//! provides append-based writing.

use super::fixed::{decode_fixed32, decode_fixed64, encode_fixed32, encode_fixed64};
use super::length_delimited::{decode_length_delimited, encode_length_delimited};
use super::tag::{decode_tag, encode_tag, Tag};
use super::varint::{decode_varint, decode_varint32, encode_varint};
use super::wire_type::WireType;
use super::WireError;
use prost::alloc::vec::Vec;

/// A cursor-based reader for decoding protobuf wire format from a byte slice.
///
/// `DecodeBuffer` maintains an internal position and provides methods to read
/// wire format primitives sequentially. It does not copy data; string and
/// bytes fields are returned as slices into the original buffer.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::{DecodeBuffer, EncodeBuffer, WireType};
///
/// let mut enc = EncodeBuffer::new();
/// enc.write_tag(1, WireType::Varint).unwrap();
/// enc.write_varint(42);
///
/// let mut dec = DecodeBuffer::new(enc.as_bytes());
/// let tag = dec.read_tag().unwrap();
/// assert_eq!(tag.field_number, 1);
/// let value = dec.read_varint().unwrap();
/// assert_eq!(value, 42);
/// assert!(dec.is_empty());
/// ```
pub struct DecodeBuffer<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> DecodeBuffer<'a> {
    /// Create a new `DecodeBuffer` wrapping the given byte slice.
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Returns `true` if all bytes have been consumed.
    pub fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }

    /// Returns the number of bytes remaining.
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// Returns the current read position.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Returns a slice of the remaining unconsumed bytes.
    pub fn remaining_bytes(&self) -> &'a [u8] {
        &self.buf[self.pos..]
    }

    /// Read a field tag from the current position.
    ///
    /// # Errors
    ///
    /// See [`decode_tag`].
    pub fn read_tag(&mut self) -> Result<Tag, WireError> {
        let (tag, consumed) = decode_tag(&self.buf[self.pos..])?;
        self.pos += consumed;
        Ok(tag)
    }

    /// Read a varint (`u64`) from the current position.
    ///
    /// # Errors
    ///
    /// See [`decode_varint`].
    pub fn read_varint(&mut self) -> Result<u64, WireError> {
        let (val, consumed) = decode_varint(&self.buf[self.pos..])?;
        self.pos += consumed;
        Ok(val)
    }

    /// Read a varint as `u32` from the current position.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::OutOfRange`] if the value exceeds `u32::MAX`.
    pub fn read_varint32(&mut self) -> Result<u32, WireError> {
        let (val, consumed) = decode_varint32(&self.buf[self.pos..])?;
        self.pos += consumed;
        Ok(val)
    }

    /// Read a varint as `i64` (two's complement reinterpretation).
    pub fn read_varint_i64(&mut self) -> Result<i64, WireError> {
        let val = self.read_varint()?;
        Ok(val as i64)
    }

    /// Read a varint as `i32`.
    pub fn read_varint_i32(&mut self) -> Result<i32, WireError> {
        let val = self.read_varint()?;
        Ok(val as i32)
    }

    /// Read a varint as `bool`.
    pub fn read_bool(&mut self) -> Result<bool, WireError> {
        let val = self.read_varint()?;
        Ok(val != 0)
    }

    /// Read a fixed 32-bit little-endian value.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::UnexpectedEof`] if fewer than 4 bytes remain.
    pub fn read_fixed32(&mut self) -> Result<u32, WireError> {
        let (val, consumed) = decode_fixed32(&self.buf[self.pos..])?;
        self.pos += consumed;
        Ok(val)
    }

    /// Read a fixed 64-bit little-endian value.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::UnexpectedEof`] if fewer than 8 bytes remain.
    pub fn read_fixed64(&mut self) -> Result<u64, WireError> {
        let (val, consumed) = decode_fixed64(&self.buf[self.pos..])?;
        self.pos += consumed;
        Ok(val)
    }

    /// Read a 32-bit float (IEEE 754 little-endian).
    pub fn read_float(&mut self) -> Result<f32, WireError> {
        let bits = self.read_fixed32()?;
        Ok(f32::from_bits(bits))
    }

    /// Read a 64-bit double (IEEE 754 little-endian).
    pub fn read_double(&mut self) -> Result<f64, WireError> {
        let bits = self.read_fixed64()?;
        Ok(f64::from_bits(bits))
    }

    /// Read a length-delimited field, returning the payload as a byte slice.
    ///
    /// # Errors
    ///
    /// See [`decode_length_delimited`].
    pub fn read_length_delimited(&mut self) -> Result<&'a [u8], WireError> {
        let (payload, consumed) = decode_length_delimited(&self.buf[self.pos..])?;
        self.pos += consumed;
        Ok(payload)
    }

    /// Read a length-delimited field as a UTF-8 string slice.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::InvalidUtf8`] if the bytes are not valid UTF-8.
    pub fn read_string(&mut self) -> Result<&'a str, WireError> {
        let bytes = self.read_length_delimited()?;
        core::str::from_utf8(bytes).map_err(WireError::InvalidUtf8)
    }

    /// Skip a field based on its wire type.
    ///
    /// This advances the cursor past the field's value without interpreting it.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::UnexpectedEof`] if the field extends beyond the
    /// buffer.
    pub fn skip_field(&mut self, wire_type: WireType) -> Result<(), WireError> {
        match wire_type {
            WireType::Varint => {
                let _ = self.read_varint()?;
            }
            WireType::I64 => {
                let _ = self.read_fixed64()?;
            }
            WireType::Len => {
                let _ = self.read_length_delimited()?;
            }
            WireType::SGroup => {
                // Skip fields until we hit the matching EGroup tag.
                loop {
                    let tag = self.read_tag()?;
                    if tag.wire_type == WireType::EGroup {
                        break;
                    }
                    self.skip_field(tag.wire_type)?;
                }
            }
            WireType::EGroup => {
                // EGroup is the end marker — nothing to skip.
            }
            WireType::I32 => {
                let _ = self.read_fixed32()?;
            }
        }
        Ok(())
    }
}

/// An append-only buffer for encoding protobuf wire format.
///
/// `EncodeBuffer` wraps a `Vec<u8>` and provides methods to write wire format
/// primitives.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::{EncodeBuffer, WireType};
///
/// let mut buf = EncodeBuffer::new();
/// buf.write_tag(1, WireType::Varint).unwrap();
/// buf.write_varint(42);
/// buf.write_tag(2, WireType::Len).unwrap();
/// buf.write_string("hello");
///
/// let bytes = buf.into_vec();
/// assert!(!bytes.is_empty());
/// ```
pub struct EncodeBuffer {
    buf: Vec<u8>,
}

impl EncodeBuffer {
    /// Create a new empty `EncodeBuffer`.
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Create a new `EncodeBuffer` with the given capacity hint.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
        }
    }

    /// Returns the current length of the buffer in bytes.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Returns `true` if the buffer contains no bytes.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Returns a slice view of the encoded bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Consume the buffer and return the underlying `Vec<u8>`.
    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }

    /// Write a field tag.
    ///
    /// # Errors
    ///
    /// See [`encode_tag`].
    pub fn write_tag(&mut self, field_number: u32, wire_type: WireType) -> Result<(), WireError> {
        encode_tag(field_number, wire_type, &mut self.buf)?;
        Ok(())
    }

    /// Write a varint value.
    pub fn write_varint(&mut self, value: u64) {
        encode_varint(value, &mut self.buf);
    }

    /// Write a `u32` as a varint.
    pub fn write_varint32(&mut self, value: u32) {
        encode_varint(u64::from(value), &mut self.buf);
    }

    /// Write an `i64` as a varint (two's complement).
    pub fn write_varint_i64(&mut self, value: i64) {
        encode_varint(value as u64, &mut self.buf);
    }

    /// Write an `i32` as a varint (sign-extended to 64 bits).
    pub fn write_varint_i32(&mut self, value: i32) {
        encode_varint(value as i64 as u64, &mut self.buf);
    }

    /// Write a `bool` as a varint (0 or 1).
    pub fn write_bool(&mut self, value: bool) {
        encode_varint(u64::from(value), &mut self.buf);
    }

    /// Write a 32-bit fixed value (little-endian).
    pub fn write_fixed32(&mut self, value: u32) {
        encode_fixed32(value, &mut self.buf);
    }

    /// Write a 64-bit fixed value (little-endian).
    pub fn write_fixed64(&mut self, value: u64) {
        encode_fixed64(value, &mut self.buf);
    }

    /// Write a 32-bit float (IEEE 754 little-endian).
    pub fn write_float(&mut self, value: f32) {
        self.write_fixed32(value.to_bits());
    }

    /// Write a 64-bit double (IEEE 754 little-endian).
    pub fn write_double(&mut self, value: f64) {
        self.write_fixed64(value.to_bits());
    }

    /// Write a length-delimited value (varint length prefix + raw bytes).
    pub fn write_length_delimited(&mut self, data: &[u8]) {
        encode_length_delimited(data, &mut self.buf);
    }

    /// Write a string as a length-delimited field.
    pub fn write_string(&mut self, s: &str) {
        self.write_length_delimited(s.as_bytes());
    }

    /// Write raw bytes without any framing.
    pub fn write_raw(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }
}

impl Default for EncodeBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_varint_via_buffers() {
        let mut enc = EncodeBuffer::new();
        enc.write_tag(1, WireType::Varint).expect("tag");
        enc.write_varint(42);

        let mut dec = DecodeBuffer::new(enc.as_bytes());
        let tag = dec.read_tag().expect("read tag");
        assert_eq!(tag.field_number, 1);
        assert_eq!(tag.wire_type, WireType::Varint);
        let val = dec.read_varint().expect("read varint");
        assert_eq!(val, 42);
        assert!(dec.is_empty());
    }

    #[test]
    fn encode_decode_string_via_buffers() {
        let mut enc = EncodeBuffer::new();
        enc.write_tag(2, WireType::Len).expect("tag");
        enc.write_string("hello world");

        let mut dec = DecodeBuffer::new(enc.as_bytes());
        let tag = dec.read_tag().expect("read tag");
        assert_eq!(tag.field_number, 2);
        assert_eq!(tag.wire_type, WireType::Len);
        let s = dec.read_string().expect("read string");
        assert_eq!(s, "hello world");
        assert!(dec.is_empty());
    }

    #[test]
    fn encode_decode_fixed32_via_buffers() {
        let mut enc = EncodeBuffer::new();
        enc.write_tag(3, WireType::I32).expect("tag");
        enc.write_fixed32(0xDEAD_BEEF);

        let mut dec = DecodeBuffer::new(enc.as_bytes());
        let tag = dec.read_tag().expect("read tag");
        assert_eq!(tag.field_number, 3);
        assert_eq!(tag.wire_type, WireType::I32);
        let val = dec.read_fixed32().expect("read fixed32");
        assert_eq!(val, 0xDEAD_BEEF);
        assert!(dec.is_empty());
    }

    #[test]
    fn encode_decode_fixed64_via_buffers() {
        let mut enc = EncodeBuffer::new();
        enc.write_tag(4, WireType::I64).expect("tag");
        enc.write_fixed64(0xDEAD_BEEF_CAFE_BABE);

        let mut dec = DecodeBuffer::new(enc.as_bytes());
        let tag = dec.read_tag().expect("read tag");
        assert_eq!(tag.field_number, 4);
        assert_eq!(tag.wire_type, WireType::I64);
        let val = dec.read_fixed64().expect("read fixed64");
        assert_eq!(val, 0xDEAD_BEEF_CAFE_BABE);
        assert!(dec.is_empty());
    }

    #[test]
    fn multiple_fields() {
        let mut enc = EncodeBuffer::new();
        // Field 1: varint 100
        enc.write_tag(1, WireType::Varint).expect("tag1");
        enc.write_varint(100);
        // Field 2: string "proto"
        enc.write_tag(2, WireType::Len).expect("tag2");
        enc.write_string("proto");
        // Field 3: bool true
        enc.write_tag(3, WireType::Varint).expect("tag3");
        enc.write_bool(true);

        let mut dec = DecodeBuffer::new(enc.as_bytes());

        let tag1 = dec.read_tag().expect("tag1");
        assert_eq!(tag1.field_number, 1);
        assert_eq!(dec.read_varint().expect("val1"), 100);

        let tag2 = dec.read_tag().expect("tag2");
        assert_eq!(tag2.field_number, 2);
        assert_eq!(dec.read_string().expect("val2"), "proto");

        let tag3 = dec.read_tag().expect("tag3");
        assert_eq!(tag3.field_number, 3);
        assert!(dec.read_bool().expect("val3"));

        assert!(dec.is_empty());
    }

    #[test]
    fn skip_varint_field() {
        let mut enc = EncodeBuffer::new();
        enc.write_tag(1, WireType::Varint).expect("tag");
        enc.write_varint(999);
        enc.write_tag(2, WireType::Varint).expect("tag");
        enc.write_varint(42);

        let mut dec = DecodeBuffer::new(enc.as_bytes());
        let tag1 = dec.read_tag().expect("tag1");
        dec.skip_field(tag1.wire_type).expect("skip");
        let tag2 = dec.read_tag().expect("tag2");
        assert_eq!(tag2.field_number, 2);
        assert_eq!(dec.read_varint().expect("val"), 42);
    }

    #[test]
    fn skip_len_field() {
        let mut enc = EncodeBuffer::new();
        enc.write_tag(1, WireType::Len).expect("tag");
        enc.write_string("skip this");
        enc.write_tag(2, WireType::Varint).expect("tag");
        enc.write_varint(7);

        let mut dec = DecodeBuffer::new(enc.as_bytes());
        let tag1 = dec.read_tag().expect("tag1");
        dec.skip_field(tag1.wire_type).expect("skip");
        let tag2 = dec.read_tag().expect("tag2");
        assert_eq!(tag2.field_number, 2);
        assert_eq!(dec.read_varint().expect("val"), 7);
    }

    #[test]
    fn skip_fixed32_field() {
        let mut enc = EncodeBuffer::new();
        enc.write_tag(1, WireType::I32).expect("tag");
        enc.write_fixed32(0);
        enc.write_tag(2, WireType::Varint).expect("tag");
        enc.write_varint(1);

        let mut dec = DecodeBuffer::new(enc.as_bytes());
        let tag1 = dec.read_tag().expect("tag1");
        dec.skip_field(tag1.wire_type).expect("skip");
        let tag2 = dec.read_tag().expect("tag2");
        assert_eq!(tag2.field_number, 2);
        assert_eq!(dec.read_varint().expect("val"), 1);
    }

    #[test]
    fn remaining_bytes_and_position() {
        let data = [0x08, 0x01]; // tag=1/varint, value=1
        let mut dec = DecodeBuffer::new(&data);
        assert_eq!(dec.remaining(), 2);
        assert_eq!(dec.position(), 0);
        dec.read_tag().expect("tag");
        assert_eq!(dec.position(), 1);
        assert_eq!(dec.remaining(), 1);
        dec.read_varint().expect("val");
        assert_eq!(dec.position(), 2);
        assert_eq!(dec.remaining(), 0);
        assert!(dec.is_empty());
    }

    #[test]
    fn encode_buffer_capacity() {
        let buf = EncodeBuffer::with_capacity(100);
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn float_double_via_buffers() {
        let test_f32 = 12.5f32;
        let test_f64 = 98.765_432_1f64;

        let mut enc = EncodeBuffer::new();
        enc.write_tag(1, WireType::I32).expect("tag");
        enc.write_float(test_f32);
        enc.write_tag(2, WireType::I64).expect("tag");
        enc.write_double(test_f64);

        let mut dec = DecodeBuffer::new(enc.as_bytes());
        let tag1 = dec.read_tag().expect("tag1");
        assert_eq!(tag1.wire_type, WireType::I32);
        let f = dec.read_float().expect("float");
        assert_eq!(f, test_f32);

        let tag2 = dec.read_tag().expect("tag2");
        assert_eq!(tag2.wire_type, WireType::I64);
        let d = dec.read_double().expect("double");
        assert_eq!(d, test_f64);
    }
}
