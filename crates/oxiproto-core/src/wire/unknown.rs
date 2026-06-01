//! Unknown field storage for preserving unrecognized fields during decode.
//!
//! When a protobuf message is decoded and contains fields not recognized by
//! the current schema (e.g., the message definition was updated by a newer
//! version), those fields must be preserved and re-encoded when the message
//! is serialized back. This module provides the data structures for that.

use super::wire_type::WireType;
use prost::alloc::vec::Vec;

/// A single unknown field with its tag information and raw value.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UnknownField {
    /// The field number from the tag.
    pub field_number: u32,
    /// The value, stored by wire type.
    pub value: UnknownValue,
}

/// The raw value of an unknown field, categorized by wire type.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum UnknownValue {
    /// A varint value (wire type 0).
    Varint(u64),
    /// A 64-bit fixed value (wire type 1).
    Fixed64(u64),
    /// A length-delimited value (wire type 2).
    LengthDelimited(Vec<u8>),
    /// A 32-bit fixed value (wire type 5).
    Fixed32(u32),
    /// A group (wire types 3+4) — stored as the raw bytes between start/end
    /// group tags.
    Group(Vec<u8>),
}

impl UnknownValue {
    /// Returns the wire type that corresponds to this value variant.
    pub fn wire_type(&self) -> WireType {
        match self {
            UnknownValue::Varint(_) => WireType::Varint,
            UnknownValue::Fixed64(_) => WireType::I64,
            UnknownValue::LengthDelimited(_) => WireType::Len,
            UnknownValue::Fixed32(_) => WireType::I32,
            UnknownValue::Group(_) => WireType::SGroup,
        }
    }
}

/// A collection of unknown fields encountered during decode.
///
/// Unknown fields are preserved in the order they were encountered so that
/// re-encoding produces byte-identical output (modulo field reordering
/// within the same field number).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UnknownFields {
    fields: Vec<UnknownField>,
}

impl UnknownFields {
    /// Create a new empty collection.
    pub fn new() -> Self {
        Self { fields: Vec::new() }
    }

    /// Add an unknown field to the collection.
    pub fn push(&mut self, field: UnknownField) {
        self.fields.push(field);
    }

    /// Add a varint unknown field.
    pub fn push_varint(&mut self, field_number: u32, value: u64) {
        self.fields.push(UnknownField {
            field_number,
            value: UnknownValue::Varint(value),
        });
    }

    /// Add a fixed64 unknown field.
    pub fn push_fixed64(&mut self, field_number: u32, value: u64) {
        self.fields.push(UnknownField {
            field_number,
            value: UnknownValue::Fixed64(value),
        });
    }

    /// Add a length-delimited unknown field.
    pub fn push_length_delimited(&mut self, field_number: u32, data: Vec<u8>) {
        self.fields.push(UnknownField {
            field_number,
            value: UnknownValue::LengthDelimited(data),
        });
    }

    /// Add a fixed32 unknown field.
    pub fn push_fixed32(&mut self, field_number: u32, value: u32) {
        self.fields.push(UnknownField {
            field_number,
            value: UnknownValue::Fixed32(value),
        });
    }

    /// Returns an iterator over all unknown fields.
    pub fn iter(&self) -> core::slice::Iter<'_, UnknownField> {
        self.fields.iter()
    }

    /// Returns `true` if there are no unknown fields.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Returns the number of unknown fields.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Returns all unknown fields for a given field number.
    pub fn get_field(&self, field_number: u32) -> Vec<&UnknownField> {
        self.fields
            .iter()
            .filter(|f| f.field_number == field_number)
            .collect()
    }

    /// Remove all unknown fields.
    pub fn clear(&mut self) {
        self.fields.clear();
    }

    /// Compute the total encoded length of all unknown fields.
    pub fn encoded_len(&self) -> usize {
        self.fields
            .iter()
            .map(|f| {
                let tag_val =
                    (u64::from(f.field_number) << 3) | u64::from(f.value.wire_type() as u32);
                let tag_len = super::varint::encoded_len_varint(tag_val);
                let val_len = match &f.value {
                    UnknownValue::Varint(v) => super::varint::encoded_len_varint(*v),
                    UnknownValue::Fixed64(_) => 8usize,
                    UnknownValue::LengthDelimited(d) => {
                        super::varint::encoded_len_varint(d.len() as u64) + d.len()
                    }
                    UnknownValue::Fixed32(_) => 4usize,
                    UnknownValue::Group(d) => d.len(),
                };
                tag_len + val_len
            })
            .sum()
    }

    /// Encode all unknown fields to an [`EncodeBuffer`](super::buf::EncodeBuffer).
    ///
    /// Each field is re-encoded with its original tag and value.
    pub fn encode_to(&self, buf: &mut super::buf::EncodeBuffer) {
        for field in &self.fields {
            let wt = field.value.wire_type();
            // Field number 0 should never exist in well-formed data, but
            // we skip rather than panic if encountered.
            if field.field_number == 0 {
                continue;
            }
            if buf.write_tag(field.field_number, wt).is_err() {
                continue;
            }
            match &field.value {
                UnknownValue::Varint(v) => buf.write_varint(*v),
                UnknownValue::Fixed64(v) => buf.write_fixed64(*v),
                UnknownValue::LengthDelimited(data) => buf.write_length_delimited(data),
                UnknownValue::Fixed32(v) => buf.write_fixed32(*v),
                UnknownValue::Group(data) => buf.write_raw(data),
            }
        }
    }
}

impl<'a> IntoIterator for &'a UnknownFields {
    type Item = &'a UnknownField;
    type IntoIter = core::slice::Iter<'a, UnknownField>;

    fn into_iter(self) -> Self::IntoIter {
        self.fields.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::alloc::vec;

    #[test]
    fn unknown_fields_push_and_iter() {
        let mut fields = UnknownFields::new();
        assert!(fields.is_empty());

        fields.push_varint(100, 42);
        fields.push_fixed32(101, 0xDEAD);
        fields.push_length_delimited(102, vec![1, 2, 3]);
        fields.push_fixed64(103, 0xCAFE_BABE);

        assert_eq!(fields.len(), 4);
        assert!(!fields.is_empty());

        let f100 = fields.get_field(100);
        assert_eq!(f100.len(), 1);
        assert_eq!(f100[0].value, UnknownValue::Varint(42));
    }

    #[test]
    fn unknown_fields_multiple_same_number() {
        let mut fields = UnknownFields::new();
        fields.push_varint(1, 10);
        fields.push_varint(1, 20);
        fields.push_varint(1, 30);

        let all_1 = fields.get_field(1);
        assert_eq!(all_1.len(), 3);
    }

    #[test]
    fn unknown_fields_clear() {
        let mut fields = UnknownFields::new();
        fields.push_varint(1, 42);
        fields.clear();
        assert!(fields.is_empty());
    }

    #[test]
    fn unknown_value_wire_types() {
        assert_eq!(UnknownValue::Varint(0).wire_type(), WireType::Varint);
        assert_eq!(UnknownValue::Fixed64(0).wire_type(), WireType::I64);
        assert_eq!(
            UnknownValue::LengthDelimited(vec![]).wire_type(),
            WireType::Len
        );
        assert_eq!(UnknownValue::Fixed32(0).wire_type(), WireType::I32);
        assert_eq!(UnknownValue::Group(vec![]).wire_type(), WireType::SGroup);
    }

    #[test]
    fn encode_round_trip() {
        let mut fields = UnknownFields::new();
        fields.push_varint(1, 42);
        fields.push_fixed32(2, 100);
        fields.push_length_delimited(3, vec![10, 20, 30]);

        let mut enc = super::super::buf::EncodeBuffer::new();
        fields.encode_to(&mut enc);
        assert!(!enc.is_empty());

        // Decode the encoded bytes and verify
        let mut dec = super::super::buf::DecodeBuffer::new(enc.as_bytes());

        let tag1 = dec.read_tag().expect("tag1");
        assert_eq!(tag1.field_number, 1);
        assert_eq!(tag1.wire_type, WireType::Varint);
        assert_eq!(dec.read_varint().expect("val1"), 42);

        let tag2 = dec.read_tag().expect("tag2");
        assert_eq!(tag2.field_number, 2);
        assert_eq!(tag2.wire_type, WireType::I32);
        assert_eq!(dec.read_fixed32().expect("val2"), 100);

        let tag3 = dec.read_tag().expect("tag3");
        assert_eq!(tag3.field_number, 3);
        assert_eq!(tag3.wire_type, WireType::Len);
        assert_eq!(dec.read_length_delimited().expect("val3"), &[10, 20, 30]);

        assert!(dec.is_empty());
    }
}
