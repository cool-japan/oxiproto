//! Native protobuf wire format implementation.
//!
//! This module provides the core wire format primitives for encoding and
//! decoding Protocol Buffer messages in pure Rust:
//!
//! - [`WireType`] — the five wire types defined by the protobuf spec.
//! - [`varint`] — LEB128 variable-length integer encoding/decoding.
//! - [`zigzag`] — ZigZag encoding for signed integers.
//! - [`tag`] — Field tag (field_number << 3 | wire_type) encoding/decoding.
//! - [`fixed`] — Fixed-width 32-bit and 64-bit little-endian encoding.
//! - [`length_delimited`] — Length-prefixed byte sequences.
//! - [`WireError`] — Errors produced during wire format operations.
//! - [`DecodeBuffer`] / [`EncodeBuffer`] — Zero-copy read/write abstractions.

mod buf;
mod wire_type;

/// Sub-modules exposed publicly so users can access lower-level helpers
/// (e.g. `wire::varint::encoded_len_varint`).
pub mod alloc_profile;
pub mod fixed;
pub mod length_delimited;
pub mod tag;
pub mod unknown;
pub mod varint;
pub mod zigzag;

pub use buf::{DecodeBuffer, EncodeBuffer};
pub use fixed::{
    decode_double, decode_fixed32, decode_fixed64, decode_float, decode_sfixed32, decode_sfixed64,
    encode_double, encode_fixed32, encode_fixed64, encode_float, encode_sfixed32, encode_sfixed64,
};
pub use length_delimited::{
    decode_length_delimited, decode_string, encode_length_delimited, encode_string,
    encoded_len_length_delimited,
};
pub use tag::{decode_tag, encode_tag, make_tag, Tag, MAX_FIELD_NUMBER};
pub use unknown::{UnknownField, UnknownFields, UnknownValue};
pub use varint::{
    decode_varint, decode_varint32, decode_varint_bool, decode_varint_i32, decode_varint_i64,
    encode_varint, encode_varint32, encode_varint_bool, encode_varint_fixed, encode_varint_i32,
    encode_varint_i64,
};
pub use wire_type::WireType;
pub use zigzag::{zigzag_decode32, zigzag_decode64, zigzag_encode32, zigzag_encode64};

/// Errors produced during wire format encoding or decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// The input ended before a complete value could be read.
    UnexpectedEof,
    /// A varint exceeded the maximum allowed length (10 bytes for u64).
    Overflow,
    /// A field tag contained an invalid wire type value.
    InvalidWireType(u32),
    /// A field tag contained an invalid field number (0 is reserved).
    InvalidFieldNumber(u32),
    /// A length-delimited field's length prefix exceeds available data.
    TruncatedMessage {
        /// The declared length of the field.
        declared: usize,
        /// The actual number of bytes remaining.
        available: usize,
    },
    /// A decoded value is out of range for the target type.
    OutOfRange(prost::alloc::string::String),
    /// A string field contained invalid UTF-8.
    InvalidUtf8(core::str::Utf8Error),
}

impl core::fmt::Display for WireError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WireError::UnexpectedEof => write!(f, "unexpected end of input"),
            WireError::Overflow => write!(f, "varint overflow (>10 bytes)"),
            WireError::InvalidWireType(w) => write!(f, "invalid wire type: {w}"),
            WireError::InvalidFieldNumber(n) => write!(f, "invalid field number: {n}"),
            WireError::TruncatedMessage {
                declared,
                available,
            } => write!(
                f,
                "truncated message: declared {declared} bytes but only {available} available"
            ),
            WireError::OutOfRange(msg) => write!(f, "value out of range: {msg}"),
            WireError::InvalidUtf8(e) => write!(f, "invalid UTF-8: {e}"),
        }
    }
}

impl core::error::Error for WireError {}
