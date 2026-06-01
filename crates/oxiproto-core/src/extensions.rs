#![forbid(unsafe_code)]

//! Proto2 extension field storage.
//!
//! An [`Extensions`] instance stores extension field values as raw
//! wire-format bytes, keyed by field number. Extension values are
//! serialized lazily on write and deserialized on read.

// BTreeMap gives a stable iteration order and is alloc-compatible.
use prost::alloc::{collections::BTreeMap, vec::Vec};

use crate::message::OxiMessage;
use crate::{wire, OxiProtoResult};

/// Proto2 extension field storage.
///
/// An `Extensions` instance stores extension field values as raw
/// wire-format bytes, keyed by field number. Extension values are
/// serialized lazily on write and deserialized on read.
///
/// The typed get/set API (`get_extension`, `set_extension`) is oriented around
/// message-typed extension fields (wire type `Len`), which is the overwhelmingly
/// common case. Raw extension fields with non-Len wire types are read and
/// re-encoded verbatim so the bytes survive a decode/re-encode cycle.
#[derive(Debug, Default, Clone)]
pub struct Extensions {
    raw: BTreeMap<u32, Vec<u8>>,
}

impl Extensions {
    /// Create a new empty `Extensions` instance.
    pub fn new() -> Self {
        Self {
            raw: BTreeMap::new(),
        }
    }

    /// Returns `true` if there are no extension fields.
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Returns the number of stored extension fields.
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Returns `true` if an extension value exists for the given `field_number`.
    pub fn has_extension(&self, field_number: u32) -> bool {
        self.raw.contains_key(&field_number)
    }

    /// Remove the extension value for the given `field_number`, if any.
    pub fn clear_extension(&mut self, field_number: u32) {
        self.raw.remove(&field_number);
    }

    /// Remove all extension field values.
    pub fn clear(&mut self) {
        self.raw.clear();
    }

    /// Retrieve and decode the extension value for `field_number`.
    ///
    /// Returns `Ok(None)` when no value has been stored for `field_number`.
    /// Returns `Ok(Some(value))` when the stored bytes can be decoded as `T`.
    ///
    /// # Errors
    ///
    /// Returns an error if the stored bytes cannot be decoded as a `T`.
    pub fn get_extension<T: OxiMessage>(&self, field_number: u32) -> OxiProtoResult<Option<T>> {
        match self.raw.get(&field_number) {
            None => Ok(None),
            Some(bytes) => {
                let value = T::decode(bytes)?;
                Ok(Some(value))
            }
        }
    }

    /// Encode and store the extension value for `field_number`.
    ///
    /// Overwrites any previously stored value for the same field number.
    ///
    /// # Errors
    ///
    /// Always returns `Ok(())` for well-formed messages. The `OxiProtoResult`
    /// return type is for future extensibility and consistency with the rest of
    /// the API.
    pub fn set_extension<T: OxiMessage>(
        &mut self,
        field_number: u32,
        value: &T,
    ) -> OxiProtoResult<()> {
        let bytes = value.encode_to_vec();
        self.raw.insert(field_number, bytes);
        Ok(())
    }

    /// Merge a raw wire-format field (tag already consumed) from `buf`.
    ///
    /// Used from generated `merge()` impls when the field number is an
    /// extension. The value is stored raw so it can be re-encoded faithfully
    /// by [`encode_raw`](Self::encode_raw).
    ///
    /// For `Len` (length-delimited) fields the raw payload bytes are stored.
    /// For `Varint`, `I32`, and `I64` fields the raw value bytes are re-encoded
    /// so the stored bytes preserve the value for re-encoding. Groups are not
    /// supported as extension fields by the protobuf spec, but if encountered,
    /// are skipped.
    ///
    /// # Errors
    ///
    /// Returns a [`crate::OxiProtoError::WireFormatError`] on malformed input.
    pub fn merge_raw(
        &mut self,
        field_number: u32,
        wire_type: wire::WireType,
        buf: &mut wire::DecodeBuffer,
    ) -> OxiProtoResult<()> {
        let bytes = match wire_type {
            wire::WireType::Len => {
                // Read the payload bytes and store them directly.
                let payload = buf.read_length_delimited()?;
                payload.to_vec()
            }
            wire::WireType::Varint => {
                // Read and re-encode as a varint so the stored bytes are
                // independently decode-able.
                let value = buf.read_varint()?;
                let mut tmp = Vec::new();
                wire::encode_varint(value, &mut tmp);
                tmp
            }
            wire::WireType::I32 => {
                let value = buf.read_fixed32()?;
                let mut tmp = Vec::new();
                wire::encode_fixed32(value, &mut tmp);
                tmp
            }
            wire::WireType::I64 => {
                let value = buf.read_fixed64()?;
                let mut tmp = Vec::new();
                wire::encode_fixed64(value, &mut tmp);
                tmp
            }
            wire::WireType::SGroup | wire::WireType::EGroup => {
                // Groups as extensions are not defined in proto2 spec;
                // skip this field safely.
                buf.skip_field(wire_type)?;
                return Ok(());
            }
        };

        self.raw.insert(field_number, bytes);
        Ok(())
    }

    /// Write all extension fields into `buf` in field-number order.
    ///
    /// Each extension field is written as a `Len`-typed field: tag + varint
    /// length prefix + payload bytes. This encoding is always correct for
    /// message-typed extensions and for raw bytes stored by [`Self::merge_raw`].
    pub fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
        for (&field_number, bytes) in &self.raw {
            // write_tag only fails for field_number == 0 or > MAX_FIELD_NUMBER.
            // We do not store those, so this is infallible in practice.
            let _ = buf.write_tag(field_number, wire::WireType::Len);
            buf.write_length_delimited(bytes);
        }
    }

    /// Total encoded size of all extension fields.
    ///
    /// Each extension field occupies: tag_len + len_prefix_len + payload_len.
    pub fn encoded_len(&self) -> usize {
        self.raw
            .iter()
            .map(|(&field_number, bytes)| {
                // Tag length: (field_number << 3 | 2) as a varint.
                let tag_value = (u64::from(field_number) << 3) | 2u64; // wire type Len = 2
                let tag_len = wire::varint::encoded_len_varint(tag_value);
                tag_len + wire::encoded_len_length_delimited(bytes.len())
            })
            .sum()
    }
}
