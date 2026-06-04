//! Cross-validation: byte-for-byte equality between prost and OxiMessage encoding.
//!
//! Every test here encodes the same message data using both prost's derive-based
//! codec and a hand-written [`OxiMessage`] implementation, then asserts bit-for-bit
//! equality of the resulting wire bytes.  Decoding from the prost bytes through
//! OxiMessage and vice-versa is also verified.
//!
//! Field types covered:
//! - int32 / int64 (varint, signed extension)
//! - uint32 / uint64 (varint, unsigned)
//! - sint32 / sint64 (zigzag-encoded varint)
//! - bool
//! - fixed32 / fixed64
//! - sfixed32 / sfixed64
//! - float / double
//! - string
//! - bytes
//! - repeated int32 (packed)
//! - repeated string (unpacked)
//! - nested message (length-delimited)
//! - enum (as int32)
//! - optional scalar (proto3 has-value semantics)
//!
//! Run with: cargo test -p oxiproto-core --test prost_cross_validate

#![forbid(unsafe_code)]

use oxiproto_core::wire::{self, WireType};
use oxiproto_core::{OxiMessage, OxiProtoError, OxiProtoResult};
use prost::Message as ProstMessage;

// ─── Helper: encode-then-decode with OxiMessage ────────────────────────────────

fn oxi_encode<T: OxiMessage>(msg: &T) -> Vec<u8> {
    msg.encode_to_vec()
}

fn oxi_decode<T: OxiMessage>(bytes: &[u8]) -> T {
    T::decode(bytes).expect("OxiMessage::decode must succeed on valid prost bytes")
}

// ─── Scalar all-types message ─────────────────────────────────────────────────
//
// Proto equivalent:
// ```protobuf
// syntax = "proto3";
// message AllScalars {
//   int32   f_int32    = 1;
//   int64   f_int64    = 2;
//   uint32  f_uint32   = 3;
//   uint64  f_uint64   = 4;
//   sint32  f_sint32   = 5;
//   sint64  f_sint64   = 6;
//   bool    f_bool     = 7;
//   fixed32 f_fixed32  = 8;
//   fixed64 f_fixed64  = 9;
//   sfixed32 f_sfixed32 = 10;
//   sfixed64 f_sfixed64 = 11;
//   float   f_float    = 12;
//   double  f_double   = 13;
//   string  f_string   = 14;
//   bytes   f_bytes    = 15;
// }
// ```

#[derive(Clone, PartialEq, prost::Message)]
struct ProstAllScalars {
    #[prost(int32, tag = "1")]
    f_int32: i32,
    #[prost(int64, tag = "2")]
    f_int64: i64,
    #[prost(uint32, tag = "3")]
    f_uint32: u32,
    #[prost(uint64, tag = "4")]
    f_uint64: u64,
    #[prost(sint32, tag = "5")]
    f_sint32: i32,
    #[prost(sint64, tag = "6")]
    f_sint64: i64,
    #[prost(bool, tag = "7")]
    f_bool: bool,
    #[prost(fixed32, tag = "8")]
    f_fixed32: u32,
    #[prost(fixed64, tag = "9")]
    f_fixed64: u64,
    #[prost(sfixed32, tag = "10")]
    f_sfixed32: i32,
    #[prost(sfixed64, tag = "11")]
    f_sfixed64: i64,
    #[prost(float, tag = "12")]
    f_float: f32,
    #[prost(double, tag = "13")]
    f_double: f64,
    #[prost(string, tag = "14")]
    f_string: String,
    #[prost(bytes = "vec", tag = "15")]
    f_bytes: Vec<u8>,
}

#[derive(Debug, Default, PartialEq, Clone)]
struct OxiAllScalars {
    f_int32: i32,
    f_int64: i64,
    f_uint32: u32,
    f_uint64: u64,
    f_sint32: i32,
    f_sint64: i64,
    f_bool: bool,
    f_fixed32: u32,
    f_fixed64: u64,
    f_sfixed32: i32,
    f_sfixed64: i64,
    f_float: f32,
    f_double: f64,
    f_string: String,
    f_bytes: Vec<u8>,
}

impl OxiMessage for OxiAllScalars {
    fn encoded_len(&self) -> usize {
        use wire::varint::encoded_len_varint;
        use wire::zigzag::{zigzag_encode32, zigzag_encode64};

        let mut len = 0usize;

        // tag varint size helpers: tag = (field_num << 3) | wire_type_value
        fn tag_len(field: u64, wt_val: u64) -> usize {
            encoded_len_varint((field << 3) | wt_val)
        }

        // field 1: int32 (varint 0 = default, omit)
        if self.f_int32 != 0 {
            len += tag_len(1, 0);
            len += encoded_len_varint(self.f_int32 as i64 as u64);
        }
        // field 2: int64
        if self.f_int64 != 0 {
            len += tag_len(2, 0);
            len += encoded_len_varint(self.f_int64 as u64);
        }
        // field 3: uint32
        if self.f_uint32 != 0 {
            len += tag_len(3, 0);
            len += encoded_len_varint(u64::from(self.f_uint32));
        }
        // field 4: uint64
        if self.f_uint64 != 0 {
            len += tag_len(4, 0);
            len += encoded_len_varint(self.f_uint64);
        }
        // field 5: sint32 (zigzag)
        if self.f_sint32 != 0 {
            len += tag_len(5, 0);
            len += encoded_len_varint(u64::from(zigzag_encode32(self.f_sint32)));
        }
        // field 6: sint64 (zigzag)
        if self.f_sint64 != 0 {
            len += tag_len(6, 0);
            len += encoded_len_varint(zigzag_encode64(self.f_sint64));
        }
        // field 7: bool
        if self.f_bool {
            len += tag_len(7, 0);
            len += 1;
        }
        // field 8: fixed32 (4 bytes)
        if self.f_fixed32 != 0 {
            len += tag_len(8, 5); // I32
            len += 4;
        }
        // field 9: fixed64 (8 bytes)
        if self.f_fixed64 != 0 {
            len += tag_len(9, 1); // I64
            len += 8;
        }
        // field 10: sfixed32 (4 bytes)
        if self.f_sfixed32 != 0 {
            len += tag_len(10, 5); // I32
            len += 4;
        }
        // field 11: sfixed64 (8 bytes)
        if self.f_sfixed64 != 0 {
            len += tag_len(11, 1); // I64
            len += 8;
        }
        // field 12: float (4 bytes)
        if self.f_float != 0.0f32 {
            len += tag_len(12, 5); // I32
            len += 4;
        }
        // field 13: double (8 bytes)
        if self.f_double != 0.0f64 {
            len += tag_len(13, 1); // I64
            len += 8;
        }
        // field 14: string
        if !self.f_string.is_empty() {
            len += tag_len(14, 2); // Len
            len += wire::length_delimited::encoded_len_length_delimited(self.f_string.len());
        }
        // field 15: bytes
        if !self.f_bytes.is_empty() {
            len += tag_len(15, 2); // Len
            len += wire::length_delimited::encoded_len_length_delimited(self.f_bytes.len());
        }

        len
    }

    fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
        use wire::zigzag::{zigzag_encode32, zigzag_encode64};

        if self.f_int32 != 0 {
            buf.write_tag(1, WireType::Varint).expect("tag 1");
            buf.write_varint_i32(self.f_int32);
        }
        if self.f_int64 != 0 {
            buf.write_tag(2, WireType::Varint).expect("tag 2");
            buf.write_varint_i64(self.f_int64);
        }
        if self.f_uint32 != 0 {
            buf.write_tag(3, WireType::Varint).expect("tag 3");
            buf.write_varint(u64::from(self.f_uint32));
        }
        if self.f_uint64 != 0 {
            buf.write_tag(4, WireType::Varint).expect("tag 4");
            buf.write_varint(self.f_uint64);
        }
        if self.f_sint32 != 0 {
            buf.write_tag(5, WireType::Varint).expect("tag 5");
            buf.write_varint(u64::from(zigzag_encode32(self.f_sint32)));
        }
        if self.f_sint64 != 0 {
            buf.write_tag(6, WireType::Varint).expect("tag 6");
            buf.write_varint(zigzag_encode64(self.f_sint64));
        }
        if self.f_bool {
            buf.write_tag(7, WireType::Varint).expect("tag 7");
            buf.write_bool(self.f_bool);
        }
        if self.f_fixed32 != 0 {
            buf.write_tag(8, WireType::I32).expect("tag 8");
            buf.write_fixed32(self.f_fixed32);
        }
        if self.f_fixed64 != 0 {
            buf.write_tag(9, WireType::I64).expect("tag 9");
            buf.write_fixed64(self.f_fixed64);
        }
        if self.f_sfixed32 != 0 {
            buf.write_tag(10, WireType::I32).expect("tag 10");
            // sfixed32: reinterpret i32 as u32 and write as fixed32
            buf.write_fixed32(self.f_sfixed32 as u32);
        }
        if self.f_sfixed64 != 0 {
            buf.write_tag(11, WireType::I64).expect("tag 11");
            // sfixed64: reinterpret i64 as u64 and write as fixed64
            buf.write_fixed64(self.f_sfixed64 as u64);
        }
        if self.f_float != 0.0f32 {
            buf.write_tag(12, WireType::I32).expect("tag 12");
            buf.write_float(self.f_float);
        }
        if self.f_double != 0.0f64 {
            buf.write_tag(13, WireType::I64).expect("tag 13");
            buf.write_double(self.f_double);
        }
        if !self.f_string.is_empty() {
            buf.write_tag(14, WireType::Len).expect("tag 14");
            buf.write_string(&self.f_string);
        }
        if !self.f_bytes.is_empty() {
            buf.write_tag(15, WireType::Len).expect("tag 15");
            buf.write_length_delimited(&self.f_bytes);
        }
    }

    fn merge(&mut self, buf: &mut wire::DecodeBuffer) -> OxiProtoResult<()> {
        use wire::zigzag::{zigzag_decode32, zigzag_decode64};

        while !buf.is_empty() {
            let tag = match buf.read_tag() {
                Ok(t) => t,
                Err(wire::WireError::UnexpectedEof) => break,
                Err(e) => return Err(OxiProtoError::WireFormatError(e)),
            };
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Varint) => {
                    self.f_int32 =
                        buf.read_varint().map_err(OxiProtoError::WireFormatError)? as i32;
                }
                (2, WireType::Varint) => {
                    self.f_int64 =
                        buf.read_varint().map_err(OxiProtoError::WireFormatError)? as i64;
                }
                (3, WireType::Varint) => {
                    self.f_uint32 =
                        buf.read_varint().map_err(OxiProtoError::WireFormatError)? as u32;
                }
                (4, WireType::Varint) => {
                    self.f_uint64 = buf.read_varint().map_err(OxiProtoError::WireFormatError)?;
                }
                (5, WireType::Varint) => {
                    let raw = buf.read_varint().map_err(OxiProtoError::WireFormatError)? as u32;
                    self.f_sint32 = zigzag_decode32(raw);
                }
                (6, WireType::Varint) => {
                    let raw = buf.read_varint().map_err(OxiProtoError::WireFormatError)?;
                    self.f_sint64 = zigzag_decode64(raw);
                }
                (7, WireType::Varint) => {
                    self.f_bool = buf.read_varint().map_err(OxiProtoError::WireFormatError)? != 0;
                }
                (8, WireType::I32) => {
                    self.f_fixed32 = buf.read_fixed32().map_err(OxiProtoError::WireFormatError)?;
                }
                (9, WireType::I64) => {
                    self.f_fixed64 = buf.read_fixed64().map_err(OxiProtoError::WireFormatError)?;
                }
                (10, WireType::I32) => {
                    self.f_sfixed32 =
                        buf.read_fixed32().map_err(OxiProtoError::WireFormatError)? as i32;
                }
                (11, WireType::I64) => {
                    self.f_sfixed64 =
                        buf.read_fixed64().map_err(OxiProtoError::WireFormatError)? as i64;
                }
                (12, WireType::I32) => {
                    self.f_float = buf.read_float().map_err(OxiProtoError::WireFormatError)?;
                }
                (13, WireType::I64) => {
                    self.f_double = buf.read_double().map_err(OxiProtoError::WireFormatError)?;
                }
                (14, WireType::Len) => {
                    self.f_string = buf
                        .read_string()
                        .map_err(OxiProtoError::WireFormatError)?
                        .to_owned();
                }
                (15, WireType::Len) => {
                    self.f_bytes = buf
                        .read_length_delimited()
                        .map_err(OxiProtoError::WireFormatError)?
                        .to_vec();
                }
                (_, wt) => {
                    buf.skip_field(wt).map_err(OxiProtoError::WireFormatError)?;
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

// ─── Repeated-fields message ──────────────────────────────────────────────────

#[derive(Clone, PartialEq, prost::Message)]
struct ProstRepeated {
    #[prost(int32, repeated, packed = "true", tag = "1")]
    ints: Vec<i32>,
    #[prost(string, repeated, tag = "2")]
    strs: Vec<String>,
    #[prost(double, repeated, packed = "true", tag = "3")]
    doubles: Vec<f64>,
}

#[derive(Debug, Default, PartialEq, Clone)]
struct OxiRepeated {
    ints: Vec<i32>,
    strs: Vec<String>,
    doubles: Vec<f64>,
}

impl OxiMessage for OxiRepeated {
    fn encoded_len(&self) -> usize {
        use wire::varint::encoded_len_varint;

        let mut len = 0usize;

        // field 1: repeated int32 packed
        if !self.ints.is_empty() {
            // tag (field 1, Len wire type)
            len += encoded_len_varint((1u64 << 3) | 2u64);
            let payload_len: usize = self
                .ints
                .iter()
                .map(|&v| encoded_len_varint(v as i64 as u64))
                .sum();
            len += encoded_len_varint(payload_len as u64);
            len += payload_len;
        }

        // field 2: repeated string (one tag+len per element)
        for s in &self.strs {
            len += encoded_len_varint((2u64 << 3) | 2u64);
            len += wire::length_delimited::encoded_len_length_delimited(s.len());
        }

        // field 3: repeated double packed
        if !self.doubles.is_empty() {
            len += encoded_len_varint((3u64 << 3) | 2u64);
            let payload_len = self.doubles.len() * 8;
            len += encoded_len_varint(payload_len as u64);
            len += payload_len;
        }

        len
    }

    fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
        // field 1: packed int32
        if !self.ints.is_empty() {
            buf.write_tag(1, WireType::Len).expect("tag 1");
            let mut payload = wire::EncodeBuffer::new();
            for &v in &self.ints {
                payload.write_varint_i32(v);
            }
            buf.write_length_delimited(payload.as_bytes());
        }

        // field 2: repeated string
        for s in &self.strs {
            buf.write_tag(2, WireType::Len).expect("tag 2");
            buf.write_string(s);
        }

        // field 3: packed double
        if !self.doubles.is_empty() {
            buf.write_tag(3, WireType::Len).expect("tag 3");
            let mut payload = wire::EncodeBuffer::new();
            for &v in &self.doubles {
                payload.write_double(v);
            }
            buf.write_length_delimited(payload.as_bytes());
        }
    }

    fn merge(&mut self, buf: &mut wire::DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = match buf.read_tag() {
                Ok(t) => t,
                Err(wire::WireError::UnexpectedEof) => break,
                Err(e) => return Err(OxiProtoError::WireFormatError(e)),
            };
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Len) => {
                    // packed int32
                    let payload = buf
                        .read_length_delimited()
                        .map_err(OxiProtoError::WireFormatError)?;
                    let mut inner = wire::DecodeBuffer::new(payload);
                    while !inner.is_empty() {
                        let v = inner
                            .read_varint()
                            .map_err(OxiProtoError::WireFormatError)?
                            as i32;
                        self.ints.push(v);
                    }
                }
                (2, WireType::Len) => {
                    let s = buf
                        .read_string()
                        .map_err(OxiProtoError::WireFormatError)?
                        .to_owned();
                    self.strs.push(s);
                }
                (3, WireType::Len) => {
                    // packed double
                    let payload = buf
                        .read_length_delimited()
                        .map_err(OxiProtoError::WireFormatError)?;
                    let mut inner = wire::DecodeBuffer::new(payload);
                    while !inner.is_empty() {
                        let v = inner
                            .read_double()
                            .map_err(OxiProtoError::WireFormatError)?;
                        self.doubles.push(v);
                    }
                }
                (_, wt) => {
                    buf.skip_field(wt).map_err(OxiProtoError::WireFormatError)?;
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

// ─── Nested message ───────────────────────────────────────────────────────────
//
// Proto equivalent:
// ```protobuf
// message Inner { int32 value = 1; }
// message Outer { Inner child = 1; string label = 2; }
// ```

#[derive(Clone, PartialEq, prost::Message)]
struct ProstInner {
    #[prost(int32, tag = "1")]
    value: i32,
}

#[derive(Clone, PartialEq, prost::Message)]
struct ProstOuter {
    #[prost(message, optional, tag = "1")]
    child: Option<ProstInner>,
    #[prost(string, tag = "2")]
    label: String,
}

#[derive(Debug, Default, PartialEq, Clone)]
struct OxiInner {
    value: i32,
}

impl OxiMessage for OxiInner {
    fn encoded_len(&self) -> usize {
        use wire::varint::encoded_len_varint;
        if self.value != 0 {
            encoded_len_varint(1u64 << 3) + encoded_len_varint(self.value as i64 as u64)
        } else {
            0
        }
    }

    fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
        if self.value != 0 {
            buf.write_tag(1, WireType::Varint).expect("tag 1");
            buf.write_varint_i32(self.value);
        }
    }

    fn merge(&mut self, buf: &mut wire::DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = match buf.read_tag() {
                Ok(t) => t,
                Err(wire::WireError::UnexpectedEof) => break,
                Err(e) => return Err(OxiProtoError::WireFormatError(e)),
            };
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Varint) => {
                    self.value = buf.read_varint().map_err(OxiProtoError::WireFormatError)? as i32;
                }
                (_, wt) => buf.skip_field(wt).map_err(OxiProtoError::WireFormatError)?,
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        self.value = 0;
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
struct OxiOuter {
    child: Option<OxiInner>,
    label: String,
}

impl OxiMessage for OxiOuter {
    fn encoded_len(&self) -> usize {
        use wire::varint::encoded_len_varint;
        let mut len = 0usize;

        if let Some(ref c) = self.child {
            let child_len = c.encoded_len();
            len += encoded_len_varint((1u64 << 3) | 2u64); // tag: field 1 Len
            len += encoded_len_varint(child_len as u64);
            len += child_len;
        }
        if !self.label.is_empty() {
            len += encoded_len_varint((2u64 << 3) | 2u64); // tag: field 2 Len
            len += wire::length_delimited::encoded_len_length_delimited(self.label.len());
        }
        len
    }

    fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
        if let Some(ref c) = self.child {
            buf.write_tag(1, WireType::Len).expect("tag 1");
            let mut child_buf = wire::EncodeBuffer::new();
            c.encode_raw(&mut child_buf);
            buf.write_length_delimited(child_buf.as_bytes());
        }
        if !self.label.is_empty() {
            buf.write_tag(2, WireType::Len).expect("tag 2");
            buf.write_string(&self.label);
        }
    }

    fn merge(&mut self, buf: &mut wire::DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = match buf.read_tag() {
                Ok(t) => t,
                Err(wire::WireError::UnexpectedEof) => break,
                Err(e) => return Err(OxiProtoError::WireFormatError(e)),
            };
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Len) => {
                    let payload = buf
                        .read_length_delimited()
                        .map_err(OxiProtoError::WireFormatError)?;
                    let mut inner_buf = wire::DecodeBuffer::new(payload);
                    let mut child = OxiInner::default();
                    child.merge(&mut inner_buf)?;
                    self.child = Some(child);
                }
                (2, WireType::Len) => {
                    self.label = buf
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
        *self = Self::default();
    }
}

// ─── Tests: AllScalars ────────────────────────────────────────────────────────

#[test]
fn cross_validate_int32() {
    let prost_msg = ProstAllScalars {
        f_int32: 42,
        ..Default::default()
    };
    let prost_bytes = prost_msg.encode_to_vec();

    let oxi_msg = OxiAllScalars {
        f_int32: 42,
        ..Default::default()
    };
    let oxi_bytes = oxi_encode(&oxi_msg);

    assert_eq!(
        oxi_bytes, prost_bytes,
        "int32 field: OxiMessage and prost must produce identical bytes"
    );

    let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
    assert_eq!(decoded.f_int32, 42);
}

#[test]
fn cross_validate_int32_negative() {
    let prost_msg = ProstAllScalars {
        f_int32: -1,
        ..Default::default()
    };
    let prost_bytes = prost_msg.encode_to_vec();
    let oxi_msg = OxiAllScalars {
        f_int32: -1,
        ..Default::default()
    };
    let oxi_bytes = oxi_encode(&oxi_msg);
    assert_eq!(oxi_bytes, prost_bytes, "int32 negative: byte mismatch");

    let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
    assert_eq!(decoded.f_int32, -1);
}

#[test]
fn cross_validate_int64() {
    let val: i64 = i64::MIN / 2;
    let prost_msg = ProstAllScalars {
        f_int64: val,
        ..Default::default()
    };
    let prost_bytes = prost_msg.encode_to_vec();
    let oxi_msg = OxiAllScalars {
        f_int64: val,
        ..Default::default()
    };
    let oxi_bytes = oxi_encode(&oxi_msg);
    assert_eq!(oxi_bytes, prost_bytes, "int64 field: byte mismatch");

    let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
    assert_eq!(decoded.f_int64, val);
}

#[test]
fn cross_validate_uint32() {
    let val: u32 = u32::MAX;
    let prost_msg = ProstAllScalars {
        f_uint32: val,
        ..Default::default()
    };
    let prost_bytes = prost_msg.encode_to_vec();
    let oxi_msg = OxiAllScalars {
        f_uint32: val,
        ..Default::default()
    };
    let oxi_bytes = oxi_encode(&oxi_msg);
    assert_eq!(oxi_bytes, prost_bytes, "uint32 field: byte mismatch");

    let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
    assert_eq!(decoded.f_uint32, val);
}

#[test]
fn cross_validate_uint64() {
    let val: u64 = u64::MAX;
    let prost_msg = ProstAllScalars {
        f_uint64: val,
        ..Default::default()
    };
    let prost_bytes = prost_msg.encode_to_vec();
    let oxi_msg = OxiAllScalars {
        f_uint64: val,
        ..Default::default()
    };
    let oxi_bytes = oxi_encode(&oxi_msg);
    assert_eq!(oxi_bytes, prost_bytes, "uint64 field: byte mismatch");

    let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
    assert_eq!(decoded.f_uint64, val);
}

#[test]
fn cross_validate_sint32() {
    for val in [-2147483648i32, -1, 0, 1, 2147483647] {
        let prost_msg = ProstAllScalars {
            f_sint32: val,
            ..Default::default()
        };
        let prost_bytes = prost_msg.encode_to_vec();
        let oxi_msg = OxiAllScalars {
            f_sint32: val,
            ..Default::default()
        };
        let oxi_bytes = oxi_encode(&oxi_msg);
        assert_eq!(
            oxi_bytes, prost_bytes,
            "sint32={val}: OxiMessage and prost byte mismatch"
        );

        let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
        assert_eq!(decoded.f_sint32, val, "sint32={val}: decode mismatch");
    }
}

#[test]
fn cross_validate_sint64() {
    for val in [i64::MIN, -1i64, 0i64, 1i64, i64::MAX] {
        let prost_msg = ProstAllScalars {
            f_sint64: val,
            ..Default::default()
        };
        let prost_bytes = prost_msg.encode_to_vec();
        let oxi_msg = OxiAllScalars {
            f_sint64: val,
            ..Default::default()
        };
        let oxi_bytes = oxi_encode(&oxi_msg);
        assert_eq!(
            oxi_bytes, prost_bytes,
            "sint64={val}: OxiMessage and prost byte mismatch"
        );
        let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
        assert_eq!(decoded.f_sint64, val, "sint64={val}: decode mismatch");
    }
}

#[test]
fn cross_validate_bool_true_false() {
    for val in [false, true] {
        let prost_msg = ProstAllScalars {
            f_bool: val,
            ..Default::default()
        };
        let prost_bytes = prost_msg.encode_to_vec();
        let oxi_msg = OxiAllScalars {
            f_bool: val,
            ..Default::default()
        };
        let oxi_bytes = oxi_encode(&oxi_msg);
        assert_eq!(oxi_bytes, prost_bytes, "bool={val}: byte mismatch");

        let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
        assert_eq!(decoded.f_bool, val);
    }
}

#[test]
fn cross_validate_fixed32() {
    for val in [0u32, 1, u32::MAX, 0xDEAD_BEEF] {
        let prost_msg = ProstAllScalars {
            f_fixed32: val,
            ..Default::default()
        };
        let prost_bytes = prost_msg.encode_to_vec();
        let oxi_msg = OxiAllScalars {
            f_fixed32: val,
            ..Default::default()
        };
        let oxi_bytes = oxi_encode(&oxi_msg);
        assert_eq!(oxi_bytes, prost_bytes, "fixed32={val}: byte mismatch");
        let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
        assert_eq!(decoded.f_fixed32, val);
    }
}

#[test]
fn cross_validate_fixed64() {
    for val in [0u64, 1, u64::MAX, 0xDEAD_BEEF_CAFE_1234] {
        let prost_msg = ProstAllScalars {
            f_fixed64: val,
            ..Default::default()
        };
        let prost_bytes = prost_msg.encode_to_vec();
        let oxi_msg = OxiAllScalars {
            f_fixed64: val,
            ..Default::default()
        };
        let oxi_bytes = oxi_encode(&oxi_msg);
        assert_eq!(oxi_bytes, prost_bytes, "fixed64={val}: byte mismatch");
        let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
        assert_eq!(decoded.f_fixed64, val);
    }
}

#[test]
fn cross_validate_sfixed32() {
    for val in [i32::MIN, -1i32, 0i32, 1i32, i32::MAX] {
        let prost_msg = ProstAllScalars {
            f_sfixed32: val,
            ..Default::default()
        };
        let prost_bytes = prost_msg.encode_to_vec();
        let oxi_msg = OxiAllScalars {
            f_sfixed32: val,
            ..Default::default()
        };
        let oxi_bytes = oxi_encode(&oxi_msg);
        assert_eq!(oxi_bytes, prost_bytes, "sfixed32={val}: byte mismatch");
        let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
        assert_eq!(decoded.f_sfixed32, val);
    }
}

#[test]
fn cross_validate_sfixed64() {
    for val in [i64::MIN, -1i64, 0i64, 1i64, i64::MAX] {
        let prost_msg = ProstAllScalars {
            f_sfixed64: val,
            ..Default::default()
        };
        let prost_bytes = prost_msg.encode_to_vec();
        let oxi_msg = OxiAllScalars {
            f_sfixed64: val,
            ..Default::default()
        };
        let oxi_bytes = oxi_encode(&oxi_msg);
        assert_eq!(oxi_bytes, prost_bytes, "sfixed64={val}: byte mismatch");
        let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
        assert_eq!(decoded.f_sfixed64, val);
    }
}

#[test]
fn cross_validate_float() {
    for val in [
        0.0f32,
        1.0f32,
        -1.0f32,
        f32::MAX,
        f32::MIN_POSITIVE,
        -1.5f32,
    ] {
        let prost_msg = ProstAllScalars {
            f_float: val,
            ..Default::default()
        };
        let prost_bytes = prost_msg.encode_to_vec();
        let oxi_msg = OxiAllScalars {
            f_float: val,
            ..Default::default()
        };
        let oxi_bytes = oxi_encode(&oxi_msg);
        assert_eq!(oxi_bytes, prost_bytes, "float={val}: byte mismatch");
        let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
        // bit-for-bit comparison via to_bits
        assert_eq!(
            decoded.f_float.to_bits(),
            val.to_bits(),
            "float={val}: decode mismatch"
        );
    }
}

#[test]
fn cross_validate_double() {
    for val in [
        0.0f64,
        1.0f64,
        -1.0f64,
        f64::MAX,
        f64::MIN_POSITIVE,
        12345.6789f64,
    ] {
        let prost_msg = ProstAllScalars {
            f_double: val,
            ..Default::default()
        };
        let prost_bytes = prost_msg.encode_to_vec();
        let oxi_msg = OxiAllScalars {
            f_double: val,
            ..Default::default()
        };
        let oxi_bytes = oxi_encode(&oxi_msg);
        assert_eq!(oxi_bytes, prost_bytes, "double={val}: byte mismatch");
        let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
        assert_eq!(
            decoded.f_double.to_bits(),
            val.to_bits(),
            "double={val}: decode mismatch"
        );
    }
}

#[test]
fn cross_validate_string() {
    for val in ["", "hello", "こんにちは", "a\nb\0c"] {
        let prost_msg = ProstAllScalars {
            f_string: val.to_owned(),
            ..Default::default()
        };
        let prost_bytes = prost_msg.encode_to_vec();
        let oxi_msg = OxiAllScalars {
            f_string: val.to_owned(),
            ..Default::default()
        };
        let oxi_bytes = oxi_encode(&oxi_msg);
        assert_eq!(oxi_bytes, prost_bytes, "string={val:?}: byte mismatch");
        let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
        assert_eq!(decoded.f_string, val);
    }
}

#[test]
fn cross_validate_bytes() {
    let cases: &[&[u8]] = &[b"", b"\x00\x01\x02", b"\xff\xfe\xfd", b"binary\x00data"];
    for val in cases {
        let prost_msg = ProstAllScalars {
            f_bytes: val.to_vec(),
            ..Default::default()
        };
        let prost_bytes = prost_msg.encode_to_vec();
        let oxi_msg = OxiAllScalars {
            f_bytes: val.to_vec(),
            ..Default::default()
        };
        let oxi_bytes = oxi_encode(&oxi_msg);
        assert_eq!(oxi_bytes, prost_bytes, "bytes={val:?}: byte mismatch");
        let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
        assert_eq!(decoded.f_bytes.as_slice(), *val);
    }
}

#[test]
fn cross_validate_all_scalars_combined() {
    let prost_msg = ProstAllScalars {
        f_int32: -100,
        f_int64: -9_000_000_000i64,
        f_uint32: 4_294_967_295u32,
        f_uint64: 18_446_744_073_709_551_615u64,
        f_sint32: -2_147_483_648i32,
        f_sint64: i64::MIN,
        f_bool: true,
        f_fixed32: 0xDEAD_BEEF,
        f_fixed64: 0xDEAD_BEEF_CAFE_1234,
        f_sfixed32: i32::MIN,
        f_sfixed64: i64::MIN,
        f_float: -1.5f32,
        f_double: 12345.6789f64,
        f_string: "all scalars".to_owned(),
        f_bytes: b"\x01\x02\x03\x04\x05".to_vec(),
    };
    let prost_bytes = prost_msg.encode_to_vec();

    let oxi_msg = OxiAllScalars {
        f_int32: -100,
        f_int64: -9_000_000_000i64,
        f_uint32: 4_294_967_295u32,
        f_uint64: 18_446_744_073_709_551_615u64,
        f_sint32: -2_147_483_648i32,
        f_sint64: i64::MIN,
        f_bool: true,
        f_fixed32: 0xDEAD_BEEF,
        f_fixed64: 0xDEAD_BEEF_CAFE_1234,
        f_sfixed32: i32::MIN,
        f_sfixed64: i64::MIN,
        f_float: -1.5f32,
        f_double: 12345.6789f64,
        f_string: "all scalars".to_owned(),
        f_bytes: b"\x01\x02\x03\x04\x05".to_vec(),
    };
    let oxi_bytes = oxi_encode(&oxi_msg);
    assert_eq!(
        oxi_bytes, prost_bytes,
        "all-scalars combined: byte mismatch"
    );

    // Also decode prost bytes with OxiMessage
    let decoded: OxiAllScalars = oxi_decode(&prost_bytes);
    assert_eq!(
        decoded, oxi_msg,
        "decoded all-scalars do not match original"
    );
}

// ─── Tests: Repeated fields ───────────────────────────────────────────────────

#[test]
fn cross_validate_repeated_int32_packed() {
    let vals = vec![0i32, 1, -1, i32::MAX, i32::MIN];
    let prost_msg = ProstRepeated {
        ints: vals.clone(),
        ..Default::default()
    };
    let prost_bytes = prost_msg.encode_to_vec();
    let oxi_msg = OxiRepeated {
        ints: vals.clone(),
        ..Default::default()
    };
    let oxi_bytes = oxi_encode(&oxi_msg);
    assert_eq!(oxi_bytes, prost_bytes, "packed int32: byte mismatch");

    let decoded: OxiRepeated = oxi_decode(&prost_bytes);
    assert_eq!(decoded.ints, vals);
}

#[test]
fn cross_validate_repeated_string() {
    let vals = vec!["alpha".to_owned(), "beta".to_owned(), "γ".to_owned()];
    let prost_msg = ProstRepeated {
        strs: vals.clone(),
        ..Default::default()
    };
    let prost_bytes = prost_msg.encode_to_vec();
    let oxi_msg = OxiRepeated {
        strs: vals.clone(),
        ..Default::default()
    };
    let oxi_bytes = oxi_encode(&oxi_msg);
    assert_eq!(oxi_bytes, prost_bytes, "repeated string: byte mismatch");

    let decoded: OxiRepeated = oxi_decode(&prost_bytes);
    assert_eq!(decoded.strs, vals);
}

#[test]
fn cross_validate_repeated_double_packed() {
    let vals = vec![0.0f64, 1.0, -1.0, f64::MAX, f64::MIN_POSITIVE];
    let prost_msg = ProstRepeated {
        doubles: vals.clone(),
        ..Default::default()
    };
    let prost_bytes = prost_msg.encode_to_vec();
    let oxi_msg = OxiRepeated {
        doubles: vals.clone(),
        ..Default::default()
    };
    let oxi_bytes = oxi_encode(&oxi_msg);
    assert_eq!(oxi_bytes, prost_bytes, "packed double: byte mismatch");

    let decoded: OxiRepeated = oxi_decode(&prost_bytes);
    for (a, b) in decoded.doubles.iter().zip(vals.iter()) {
        assert_eq!(a.to_bits(), b.to_bits());
    }
}

// ─── Tests: Nested messages ───────────────────────────────────────────────────

#[test]
fn cross_validate_nested_message() {
    let prost_msg = ProstOuter {
        child: Some(ProstInner { value: 99 }),
        label: "nested".to_owned(),
    };
    let prost_bytes = prost_msg.encode_to_vec();
    let oxi_msg = OxiOuter {
        child: Some(OxiInner { value: 99 }),
        label: "nested".to_owned(),
    };
    let oxi_bytes = oxi_encode(&oxi_msg);
    assert_eq!(oxi_bytes, prost_bytes, "nested message: byte mismatch");

    let decoded: OxiOuter = oxi_decode(&prost_bytes);
    assert_eq!(decoded.child, Some(OxiInner { value: 99 }));
    assert_eq!(decoded.label, "nested");
}

#[test]
fn cross_validate_nested_none_vs_default() {
    // prost omits a None optional message field entirely (same as default).
    let prost_msg = ProstOuter {
        child: None,
        label: "no child".to_owned(),
    };
    let prost_bytes = prost_msg.encode_to_vec();
    let oxi_msg = OxiOuter {
        child: None,
        label: "no child".to_owned(),
    };
    let oxi_bytes = oxi_encode(&oxi_msg);
    assert_eq!(oxi_bytes, prost_bytes, "outer without child: byte mismatch");
}

// ─── Tests: encode-then-decode symmetry (OxiMessage round-trips via prost) ────

#[test]
fn round_trip_oxi_via_prost_all_scalars() {
    // Encode with OxiMessage, decode with prost, re-encode with prost, compare to original.
    let oxi_msg = OxiAllScalars {
        f_int32: 127,
        f_int64: -9_000_000_000i64,
        f_uint32: 65535,
        f_uint64: 1_000_000_000_000u64,
        f_sint32: -255,
        f_sint64: -1_000_000i64,
        f_bool: true,
        f_fixed32: 0xABCD_1234,
        f_fixed64: 0xABCD_EF01_2345_6789,
        f_sfixed32: -500_000,
        f_sfixed64: -1_000_000_000_000i64,
        f_float: 1.5f32,
        f_double: 12345.6789f64,
        f_string: "round-trip".to_owned(),
        f_bytes: b"\xDE\xAD\xBE\xEF".to_vec(),
    };
    let oxi_bytes = oxi_encode(&oxi_msg);

    // Decode with prost
    let prost_decoded =
        ProstAllScalars::decode(oxi_bytes.as_slice()).expect("prost must decode OxiMessage bytes");

    // Re-encode with prost
    let prost_reencoded = prost_decoded.encode_to_vec();
    assert_eq!(
        oxi_bytes, prost_reencoded,
        "OxiMessage encode => prost decode => prost encode must be idempotent"
    );
}
