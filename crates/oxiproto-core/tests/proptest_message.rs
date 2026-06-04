//! Property-based round-trip tests for [`OxiMessage`] encode/decode.
//!
//! These tests use `proptest` to generate arbitrary field values and verify that
//! encoding followed by decoding produces the original value (and the same bytes
//! as prost where applicable).
//!
//! Complements `proptest_wire.rs` (which targets the low-level wire primitives)
//! by working at the message-trait level.
//!
//! Run with: cargo test -p oxiproto-core --test proptest_message

#![forbid(unsafe_code)]

use oxiproto_core::wire::{self, WireType};
use oxiproto_core::{OxiMessage, OxiProtoError, OxiProtoResult};
use proptest::prelude::*;

// ─── A generic all-field-types message for proptest ───────────────────────────
//
// Proto3 equivalent:
// ```protobuf
// message PropMsg {
//   int32  i32_val   = 1;
//   int64  i64_val   = 2;
//   uint32 u32_val   = 3;
//   uint64 u64_val   = 4;
//   sint32 s32_val   = 5;
//   sint64 s64_val   = 6;
//   bool   bool_val  = 7;
//   float  f32_val   = 8;
//   double f64_val   = 9;
//   string str_val   = 10;
//   bytes  bytes_val = 11;
// }
// ```

#[derive(Debug, Default, PartialEq, Clone)]
struct PropMsg {
    i32_val: i32,
    i64_val: i64,
    u32_val: u32,
    u64_val: u64,
    s32_val: i32,
    s64_val: i64,
    bool_val: bool,
    f32_val: f32,
    f64_val: f64,
    str_val: String,
    bytes_val: Vec<u8>,
}

impl OxiMessage for PropMsg {
    fn encoded_len(&self) -> usize {
        use wire::varint::encoded_len_varint;
        use wire::zigzag::{zigzag_encode32, zigzag_encode64};

        let mut len = 0usize;

        if self.i32_val != 0 {
            len += encoded_len_varint(1u64 << 3);
            len += encoded_len_varint(self.i32_val as i64 as u64);
        }
        if self.i64_val != 0 {
            len += encoded_len_varint(2u64 << 3);
            len += encoded_len_varint(self.i64_val as u64);
        }
        if self.u32_val != 0 {
            len += encoded_len_varint(3u64 << 3);
            len += encoded_len_varint(u64::from(self.u32_val));
        }
        if self.u64_val != 0 {
            len += encoded_len_varint(4u64 << 3);
            len += encoded_len_varint(self.u64_val);
        }
        if self.s32_val != 0 {
            len += encoded_len_varint(5u64 << 3);
            len += encoded_len_varint(u64::from(zigzag_encode32(self.s32_val)));
        }
        if self.s64_val != 0 {
            len += encoded_len_varint(6u64 << 3);
            len += encoded_len_varint(zigzag_encode64(self.s64_val));
        }
        if self.bool_val {
            len += encoded_len_varint(7u64 << 3);
            len += 1;
        }
        if self.f32_val != 0.0f32 {
            len += encoded_len_varint((8u64 << 3) | 5u64); // I32
            len += 4;
        }
        if self.f64_val != 0.0f64 {
            len += encoded_len_varint((9u64 << 3) | 1u64); // I64
            len += 8;
        }
        if !self.str_val.is_empty() {
            len += encoded_len_varint((10u64 << 3) | 2u64); // Len
            len += wire::length_delimited::encoded_len_length_delimited(self.str_val.len());
        }
        if !self.bytes_val.is_empty() {
            len += encoded_len_varint((11u64 << 3) | 2u64); // Len
            len += wire::length_delimited::encoded_len_length_delimited(self.bytes_val.len());
        }

        len
    }

    fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
        use wire::zigzag::{zigzag_encode32, zigzag_encode64};

        if self.i32_val != 0 {
            buf.write_tag(1, WireType::Varint).expect("tag 1");
            buf.write_varint_i32(self.i32_val);
        }
        if self.i64_val != 0 {
            buf.write_tag(2, WireType::Varint).expect("tag 2");
            buf.write_varint_i64(self.i64_val);
        }
        if self.u32_val != 0 {
            buf.write_tag(3, WireType::Varint).expect("tag 3");
            buf.write_varint(u64::from(self.u32_val));
        }
        if self.u64_val != 0 {
            buf.write_tag(4, WireType::Varint).expect("tag 4");
            buf.write_varint(self.u64_val);
        }
        if self.s32_val != 0 {
            buf.write_tag(5, WireType::Varint).expect("tag 5");
            buf.write_varint(u64::from(zigzag_encode32(self.s32_val)));
        }
        if self.s64_val != 0 {
            buf.write_tag(6, WireType::Varint).expect("tag 6");
            buf.write_varint(zigzag_encode64(self.s64_val));
        }
        if self.bool_val {
            buf.write_tag(7, WireType::Varint).expect("tag 7");
            buf.write_bool(self.bool_val);
        }
        if self.f32_val != 0.0f32 {
            buf.write_tag(8, WireType::I32).expect("tag 8");
            buf.write_float(self.f32_val);
        }
        if self.f64_val != 0.0f64 {
            buf.write_tag(9, WireType::I64).expect("tag 9");
            buf.write_double(self.f64_val);
        }
        if !self.str_val.is_empty() {
            buf.write_tag(10, WireType::Len).expect("tag 10");
            buf.write_string(&self.str_val);
        }
        if !self.bytes_val.is_empty() {
            buf.write_tag(11, WireType::Len).expect("tag 11");
            buf.write_length_delimited(&self.bytes_val);
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
                    self.i32_val =
                        buf.read_varint().map_err(OxiProtoError::WireFormatError)? as i32;
                }
                (2, WireType::Varint) => {
                    self.i64_val =
                        buf.read_varint().map_err(OxiProtoError::WireFormatError)? as i64;
                }
                (3, WireType::Varint) => {
                    self.u32_val =
                        buf.read_varint().map_err(OxiProtoError::WireFormatError)? as u32;
                }
                (4, WireType::Varint) => {
                    self.u64_val = buf.read_varint().map_err(OxiProtoError::WireFormatError)?;
                }
                (5, WireType::Varint) => {
                    let raw = buf.read_varint().map_err(OxiProtoError::WireFormatError)? as u32;
                    self.s32_val = zigzag_decode32(raw);
                }
                (6, WireType::Varint) => {
                    let raw = buf.read_varint().map_err(OxiProtoError::WireFormatError)?;
                    self.s64_val = zigzag_decode64(raw);
                }
                (7, WireType::Varint) => {
                    self.bool_val = buf.read_varint().map_err(OxiProtoError::WireFormatError)? != 0;
                }
                (8, WireType::I32) => {
                    self.f32_val = buf.read_float().map_err(OxiProtoError::WireFormatError)?;
                }
                (9, WireType::I64) => {
                    self.f64_val = buf.read_double().map_err(OxiProtoError::WireFormatError)?;
                }
                (10, WireType::Len) => {
                    self.str_val = buf
                        .read_string()
                        .map_err(OxiProtoError::WireFormatError)?
                        .to_owned();
                }
                (11, WireType::Len) => {
                    self.bytes_val = buf
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

// ─── Repeated-fields message for proptest ────────────────────────────────────

#[derive(Debug, Default, PartialEq, Clone)]
struct PropRepeated {
    ints: Vec<i32>,
    strs: Vec<String>,
}

impl OxiMessage for PropRepeated {
    fn encoded_len(&self) -> usize {
        use wire::varint::encoded_len_varint;

        let mut len = 0usize;

        if !self.ints.is_empty() {
            len += encoded_len_varint((1u64 << 3) | 2u64);
            let payload: usize = self
                .ints
                .iter()
                .map(|&v| encoded_len_varint(v as i64 as u64))
                .sum();
            len += encoded_len_varint(payload as u64);
            len += payload;
        }
        for s in &self.strs {
            len += encoded_len_varint((2u64 << 3) | 2u64);
            len += wire::length_delimited::encoded_len_length_delimited(s.len());
        }

        len
    }

    fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
        if !self.ints.is_empty() {
            buf.write_tag(1, WireType::Len).expect("tag 1");
            let mut payload = wire::EncodeBuffer::new();
            for &v in &self.ints {
                payload.write_varint_i32(v);
            }
            buf.write_length_delimited(payload.as_bytes());
        }
        for s in &self.strs {
            buf.write_tag(2, WireType::Len).expect("tag 2");
            buf.write_string(s);
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

// ─── Proptest strategy builders ───────────────────────────────────────────────

/// Strategy that generates finite (non-NaN, non-infinite) f32 values.
fn arb_finite_f32() -> impl Strategy<Value = f32> {
    // Use prop_oneof! which handles different Strategy types via boxing.
    prop_oneof![
        proptest::num::f32::NORMAL,
        proptest::num::f32::ZERO,
        proptest::num::f32::SUBNORMAL,
    ]
}

/// Strategy that generates finite (non-NaN, non-infinite) f64 values.
fn arb_finite_f64() -> impl Strategy<Value = f64> {
    prop_oneof![
        proptest::num::f64::NORMAL,
        proptest::num::f64::ZERO,
        proptest::num::f64::SUBNORMAL,
    ]
}

fn arb_prop_msg() -> impl Strategy<Value = PropMsg> {
    // We avoid NaN values for float/double because NaN != NaN breaks PartialEq.
    // proto3 default for float/double is 0.0; we generate finite values only.
    (
        any::<i32>(),
        any::<i64>(),
        any::<u32>(),
        any::<u64>(),
        any::<i32>(),
        any::<i64>(),
        any::<bool>(),
        arb_finite_f32(),
        arb_finite_f64(),
        ".*",                                           // str_val
        proptest::collection::vec(any::<u8>(), 0..128), // bytes_val
    )
        .prop_map(
            |(
                i32_val,
                i64_val,
                u32_val,
                u64_val,
                s32_val,
                s64_val,
                bool_val,
                f32_val,
                f64_val,
                str_val,
                bytes_val,
            )| {
                PropMsg {
                    i32_val,
                    i64_val,
                    u32_val,
                    u64_val,
                    s32_val,
                    s64_val,
                    bool_val,
                    f32_val,
                    f64_val,
                    str_val,
                    bytes_val,
                }
            },
        )
}

fn arb_prop_repeated() -> impl Strategy<Value = PropRepeated> {
    (
        proptest::collection::vec(any::<i32>(), 0..32),
        proptest::collection::vec(".*", 0..16),
    )
        .prop_map(|(ints, strs)| PropRepeated { ints, strs })
}

// ─── Proptest tests ───────────────────────────────────────────────────────────

proptest! {
    /// `PropMsg` encode/decode round-trip: for any combination of scalar field values,
    /// decoding the encoded bytes must return an equal message.
    ///
    /// float/double NaN is excluded because NaN != NaN breaks structural equality.
    #[test]
    fn prop_msg_round_trip(msg in arb_prop_msg()) {
        let bytes = msg.encode_to_vec();
        let decoded = PropMsg::decode(&bytes)
            .expect("PropMsg::decode must succeed for self-encoded bytes");

        // Compare integer and boolean fields directly.
        prop_assert_eq!(msg.i32_val, decoded.i32_val, "i32_val mismatch");
        prop_assert_eq!(msg.i64_val, decoded.i64_val, "i64_val mismatch");
        prop_assert_eq!(msg.u32_val, decoded.u32_val, "u32_val mismatch");
        prop_assert_eq!(msg.u64_val, decoded.u64_val, "u64_val mismatch");
        prop_assert_eq!(msg.s32_val, decoded.s32_val, "s32_val mismatch");
        prop_assert_eq!(msg.s64_val, decoded.s64_val, "s64_val mismatch");
        prop_assert_eq!(msg.bool_val, decoded.bool_val, "bool_val mismatch");

        // Compare float/double via bit pattern to avoid floating-point trap.
        prop_assert_eq!(
            msg.f32_val.to_bits(),
            decoded.f32_val.to_bits(),
            "f32_val bit mismatch"
        );
        prop_assert_eq!(
            msg.f64_val.to_bits(),
            decoded.f64_val.to_bits(),
            "f64_val bit mismatch"
        );

        // String and bytes.
        prop_assert_eq!(&msg.str_val, &decoded.str_val, "str_val mismatch");
        prop_assert_eq!(&msg.bytes_val, &decoded.bytes_val, "bytes_val mismatch");
    }

    /// `encoded_len` reports the exact byte count that `encode_to_vec` produces.
    #[test]
    fn prop_msg_encoded_len_matches_actual(msg in arb_prop_msg()) {
        let bytes = msg.encode_to_vec();
        prop_assert_eq!(
            msg.encoded_len(),
            bytes.len(),
            "encoded_len() must match actual byte count"
        );
    }

    /// Re-encoding a decoded message produces the same bytes (idempotent serialization).
    #[test]
    fn prop_msg_encode_is_idempotent(msg in arb_prop_msg()) {
        let bytes1 = msg.encode_to_vec();
        let decoded = PropMsg::decode(&bytes1)
            .expect("decode must succeed for self-encoded bytes");
        let bytes2 = decoded.encode_to_vec();
        prop_assert_eq!(
            bytes1,
            bytes2,
            "re-encoding a decoded message must produce identical bytes"
        );
    }

    /// Repeated-fields round-trip.
    #[test]
    fn prop_repeated_round_trip(msg in arb_prop_repeated()) {
        let bytes = msg.encode_to_vec();
        let decoded = PropRepeated::decode(&bytes)
            .expect("PropRepeated::decode must succeed for self-encoded bytes");
        prop_assert_eq!(&msg.ints, &decoded.ints, "ints mismatch");
        prop_assert_eq!(&msg.strs, &decoded.strs, "strs mismatch");
    }

    /// `encoded_len` is correct for repeated-fields message.
    #[test]
    fn prop_repeated_encoded_len_matches_actual(msg in arb_prop_repeated()) {
        let bytes = msg.encode_to_vec();
        prop_assert_eq!(
            msg.encoded_len(),
            bytes.len(),
            "encoded_len() for repeated fields must match actual byte count"
        );
    }

    /// Encoding the default (all-zero/empty) message always produces zero bytes.
    ///
    /// Proto3 mandates that default values are not serialized.
    #[test]
    fn prop_default_msg_encodes_to_zero_bytes(_unused in 0u8..1u8) {
        let msg = PropMsg::default();
        let bytes = msg.encode_to_vec();
        prop_assert!(
            bytes.is_empty(),
            "all-default PropMsg must encode to empty bytes, got {:?}",
            bytes
        );
    }

    /// OxiMessage bytes for integer-only content are identical to prost bytes.
    ///
    /// The strategy avoids float fields (NaN handling differences) and generates
    /// only integer fields, comparing against prost's derive codec.
    #[test]
    fn prop_oxi_vs_prost_int_fields(
        i32_val in any::<i32>(),
        i64_val in any::<i64>(),
        u32_val in any::<u32>(),
        u64_val in any::<u64>(),
    ) {
        // prost reference message: { int32=1, int64=2, uint32=3, uint64=4 }
        #[derive(Clone, prost::Message)]
        struct ProstInts {
            #[prost(int32, tag = "1")]
            a: i32,
            #[prost(int64, tag = "2")]
            b: i64,
            #[prost(uint32, tag = "3")]
            c: u32,
            #[prost(uint64, tag = "4")]
            d: u64,
        }

        use prost::Message as _;

        #[derive(Debug, Default, PartialEq)]
        struct OxiInts {
            a: i32,
            b: i64,
            c: u32,
            d: u64,
        }

        impl OxiMessage for OxiInts {
            fn encoded_len(&self) -> usize {
                use wire::varint::encoded_len_varint;
                let mut n = 0usize;
                if self.a != 0 { n += encoded_len_varint(1u64<<3) + encoded_len_varint(self.a as i64 as u64); }
                if self.b != 0 { n += encoded_len_varint(2u64<<3) + encoded_len_varint(self.b as u64); }
                if self.c != 0 { n += encoded_len_varint(3u64<<3) + encoded_len_varint(u64::from(self.c)); }
                if self.d != 0 { n += encoded_len_varint(4u64<<3) + encoded_len_varint(self.d); }
                n
            }
            fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
                if self.a != 0 { buf.write_tag(1, WireType::Varint).ok(); buf.write_varint_i32(self.a); }
                if self.b != 0 { buf.write_tag(2, WireType::Varint).ok(); buf.write_varint_i64(self.b); }
                if self.c != 0 { buf.write_tag(3, WireType::Varint).ok(); buf.write_varint(u64::from(self.c)); }
                if self.d != 0 { buf.write_tag(4, WireType::Varint).ok(); buf.write_varint(self.d); }
            }
            fn merge(&mut self, buf: &mut wire::DecodeBuffer) -> OxiProtoResult<()> {
                while !buf.is_empty() {
                    let tag = match buf.read_tag() {
                        Ok(t) => t,
                        Err(wire::WireError::UnexpectedEof) => break,
                        Err(e) => return Err(OxiProtoError::WireFormatError(e)),
                    };
                    match (tag.field_number, tag.wire_type) {
                        (1, WireType::Varint) => { self.a = buf.read_varint().map_err(OxiProtoError::WireFormatError)? as i32; }
                        (2, WireType::Varint) => { self.b = buf.read_varint().map_err(OxiProtoError::WireFormatError)? as i64; }
                        (3, WireType::Varint) => { self.c = buf.read_varint().map_err(OxiProtoError::WireFormatError)? as u32; }
                        (4, WireType::Varint) => { self.d = buf.read_varint().map_err(OxiProtoError::WireFormatError)?; }
                        (_, wt) => { buf.skip_field(wt).map_err(OxiProtoError::WireFormatError)?; }
                    }
                }
                Ok(())
            }
            fn clear(&mut self) { *self = Self::default(); }
        }

        let prost_bytes = ProstInts { a: i32_val, b: i64_val, c: u32_val, d: u64_val }.encode_to_vec();
        let oxi_bytes = OxiInts { a: i32_val, b: i64_val, c: u32_val, d: u64_val }.encode_to_vec();

        prop_assert_eq!(
            &oxi_bytes, &prost_bytes,
            "int fields: OxiMessage bytes must equal prost bytes"
        );
    }

    /// Clearing then re-encoding produces an empty byte sequence.
    #[test]
    fn prop_clear_then_encode_is_empty(msg in arb_prop_msg()) {
        let mut m = msg;
        m.clear();
        let bytes = m.encode_to_vec();
        prop_assert!(
            bytes.is_empty(),
            "cleared PropMsg must encode to empty bytes, got {:?}",
            bytes
        );
    }

    /// Merging additional fields into an existing message (last-write-wins for scalars).
    #[test]
    fn prop_merge_overwrites_scalar(
        first in any::<i32>().prop_filter("non-zero", |&v| v != 0),
        second_val in any::<i32>().prop_filter("non-zero", |&v| v != 0),
    ) {
        let first_bytes = PropMsg { i32_val: first, ..Default::default() }.encode_to_vec();
        let second_bytes = PropMsg { i32_val: second_val, ..Default::default() }.encode_to_vec();

        let mut msg = PropMsg::decode(&first_bytes)
            .expect("decode first");
        prop_assert_eq!(msg.i32_val, first);

        // Merge the second message on top.
        let mut buf = wire::DecodeBuffer::new(&second_bytes);
        msg.merge(&mut buf).expect("merge second");
        // Last write wins: the merged value should be `second_val`.
        prop_assert_eq!(
            msg.i32_val,
            second_val,
            "last-write-wins: merged int32 should equal the second value"
        );
    }
}
