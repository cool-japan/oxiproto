//! Protobuf field tag encoding and decoding.
//!
//! A field tag is a varint that encodes both the field number and the wire
//! type:
//!
//! ```text
//! tag = (field_number << 3) | wire_type
//! ```
//!
//! Field numbers must be in the range `1..=536_870_911` (29 bits). Wire type
//! values are in `0..=5`.

use super::varint::{decode_varint, encode_varint};
use super::wire_type::WireType;
use super::WireError;
use prost::alloc::vec::Vec;

/// A decoded field tag containing the field number and wire type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    /// The field number (1..=536_870_911).
    pub field_number: u32,
    /// The wire type for this field.
    pub wire_type: WireType,
}

/// Maximum valid field number (2^29 - 1 = 536_870_911).
pub const MAX_FIELD_NUMBER: u32 = (1 << 29) - 1;

/// Field numbers 19000..=19999 are reserved by the protobuf spec for the
/// implementation. We do not reject them at the wire level (only at the
/// schema level), but this constant is provided for schema-level checks.
pub const RESERVED_RANGE_START: u32 = 19000;
/// End of the reserved field number range (inclusive).
pub const RESERVED_RANGE_END: u32 = 19999;

/// Encode a field tag as a varint and append it to `buf`.
///
/// # Errors
///
/// Returns [`WireError::InvalidFieldNumber`] if `field_number` is 0 or
/// exceeds [`MAX_FIELD_NUMBER`].
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::{encode_tag, WireType};
///
/// let mut buf = Vec::new();
/// encode_tag(1, WireType::Varint, &mut buf).unwrap();
/// assert_eq!(buf, &[0x08]); // (1 << 3) | 0 = 8
/// ```
pub fn encode_tag(
    field_number: u32,
    wire_type: WireType,
    buf: &mut Vec<u8>,
) -> Result<usize, WireError> {
    if field_number == 0 || field_number > MAX_FIELD_NUMBER {
        return Err(WireError::InvalidFieldNumber(field_number));
    }
    let tag_value = (u64::from(field_number) << 3) | u64::from(wire_type.value());
    Ok(encode_varint(tag_value, buf))
}

/// Decode a field tag from the beginning of `buf`.
///
/// Returns the [`Tag`] and the number of bytes consumed.
///
/// # Errors
///
/// - [`WireError::UnexpectedEof`] if the buffer is empty.
/// - [`WireError::InvalidWireType`] if the wire type bits are invalid.
/// - [`WireError::InvalidFieldNumber`] if the field number is 0 or exceeds
///   [`MAX_FIELD_NUMBER`].
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::{decode_tag, WireType};
///
/// let buf = [0x08]; // (1 << 3) | 0
/// let (tag, consumed) = decode_tag(&buf).unwrap();
/// assert_eq!(tag.field_number, 1);
/// assert_eq!(tag.wire_type, WireType::Varint);
/// assert_eq!(consumed, 1);
/// ```
pub fn decode_tag(buf: &[u8]) -> Result<(Tag, usize), WireError> {
    let (raw, consumed) = decode_varint(buf)?;

    let wire_type_bits = (raw & 0x07) as u32;
    let wire_type = WireType::from_u32(wire_type_bits)?;

    let field_number = (raw >> 3) as u32;
    if field_number == 0 || field_number > MAX_FIELD_NUMBER {
        return Err(WireError::InvalidFieldNumber(field_number));
    }

    Ok((
        Tag {
            field_number,
            wire_type,
        },
        consumed,
    ))
}

/// Compute the tag value as a `u64` without encoding it.
///
/// # Errors
///
/// Returns [`WireError::InvalidFieldNumber`] if `field_number` is 0 or
/// exceeds [`MAX_FIELD_NUMBER`].
pub fn make_tag(field_number: u32, wire_type: WireType) -> Result<u64, WireError> {
    if field_number == 0 || field_number > MAX_FIELD_NUMBER {
        return Err(WireError::InvalidFieldNumber(field_number));
    }
    Ok((u64::from(field_number) << 3) | u64::from(wire_type.value()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_field1_varint() {
        let mut buf = Vec::new();
        encode_tag(1, WireType::Varint, &mut buf).expect("encode");
        // (1 << 3) | 0 = 8
        assert_eq!(buf, &[0x08]);
        let (tag, consumed) = decode_tag(&buf).expect("decode");
        assert_eq!(tag.field_number, 1);
        assert_eq!(tag.wire_type, WireType::Varint);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn encode_decode_field2_len() {
        let mut buf = Vec::new();
        encode_tag(2, WireType::Len, &mut buf).expect("encode");
        // (2 << 3) | 2 = 18
        assert_eq!(buf, &[0x12]);
        let (tag, consumed) = decode_tag(&buf).expect("decode");
        assert_eq!(tag.field_number, 2);
        assert_eq!(tag.wire_type, WireType::Len);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn encode_decode_field16_i32() {
        let mut buf = Vec::new();
        encode_tag(16, WireType::I32, &mut buf).expect("encode");
        // (16 << 3) | 5 = 133 → varint: 0x85 0x01
        let (tag, consumed) = decode_tag(&buf).expect("decode");
        assert_eq!(tag.field_number, 16);
        assert_eq!(tag.wire_type, WireType::I32);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn round_trip_all_wire_types() {
        for wire_val in 0..=5u32 {
            let wt = WireType::from_u32(wire_val).expect("valid wire type");
            let mut buf = Vec::new();
            encode_tag(1, wt, &mut buf).expect("encode");
            let (tag, _) = decode_tag(&buf).expect("decode");
            assert_eq!(tag.field_number, 1);
            assert_eq!(tag.wire_type, wt);
        }
    }

    #[test]
    fn round_trip_large_field_number() {
        let mut buf = Vec::new();
        encode_tag(MAX_FIELD_NUMBER, WireType::Len, &mut buf).expect("encode");
        let (tag, _) = decode_tag(&buf).expect("decode");
        assert_eq!(tag.field_number, MAX_FIELD_NUMBER);
        assert_eq!(tag.wire_type, WireType::Len);
    }

    #[test]
    fn field_number_zero_rejected() {
        let mut buf = Vec::new();
        assert!(matches!(
            encode_tag(0, WireType::Varint, &mut buf),
            Err(WireError::InvalidFieldNumber(0))
        ));
    }

    #[test]
    fn field_number_too_large_rejected() {
        let mut buf = Vec::new();
        assert!(matches!(
            encode_tag(MAX_FIELD_NUMBER + 1, WireType::Varint, &mut buf),
            Err(WireError::InvalidFieldNumber(_))
        ));
    }

    #[test]
    fn decode_field_number_zero_in_wire_rejected() {
        // Manually encode a tag with field_number=0, wire_type=0
        // tag_value = (0 << 3) | 0 = 0 → varint [0x00]
        assert!(matches!(
            decode_tag(&[0x00]),
            Err(WireError::InvalidFieldNumber(0))
        ));
    }

    #[test]
    fn make_tag_values() {
        assert_eq!(make_tag(1, WireType::Varint).expect("ok"), 0x08);
        assert_eq!(make_tag(2, WireType::Len).expect("ok"), 0x12);
    }
}
