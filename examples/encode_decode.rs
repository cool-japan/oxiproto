//! Encode/decode a message using OxiProto's native wire format.
//!
//! This is the low-level API that `oxiproto-codegen` emits `impl OxiMessage`
//! blocks for automatically (see the `codegen_usage` example for that). It
//! is shown here by hand so the wire-format contract is fully visible: which
//! bytes get written for which field, how proto3 "omit default values" works,
//! and how unknown/malformed input is rejected without ever panicking.
//!
//! Run with:
//! ```text
//! cargo run --example encode_decode -p oxiproto-examples
//! ```

use oxiproto_core::wire::{self, WireType};
use oxiproto_core::{OxiMessage, OxiProtoError, OxiProtoResult};

/// Mirrors the following proto3 definition:
///
/// ```protobuf
/// syntax = "proto3";
/// message ContactCard {
///   string name   = 1;
///   int32  age    = 2;
///   bool   active = 3;
/// }
/// ```
#[derive(Debug, Default, Clone, PartialEq)]
struct ContactCard {
    name: String,
    age: i32,
    active: bool,
}

impl OxiMessage for ContactCard {
    fn encoded_len(&self) -> usize {
        use wire::varint::encoded_len_varint;

        let mut len = 0usize;
        // proto3 semantics: fields at their default value are never written.
        if !self.name.is_empty() {
            len += encoded_len_varint((1u64 << 3) | 2u64); // tag: field 1, Len
            len += wire::length_delimited::encoded_len_length_delimited(self.name.len());
        }
        if self.age != 0 {
            len += encoded_len_varint(2u64 << 3); // tag: field 2, Varint
            len += encoded_len_varint(self.age as i64 as u64);
        }
        if self.active {
            len += encoded_len_varint(3u64 << 3); // tag: field 3, Varint
            len += 1;
        }
        len
    }

    fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
        if !self.name.is_empty() {
            let _ = buf.write_tag(1, WireType::Len);
            buf.write_string(&self.name);
        }
        if self.age != 0 {
            let _ = buf.write_tag(2, WireType::Varint);
            buf.write_varint_i32(self.age);
        }
        if self.active {
            let _ = buf.write_tag(3, WireType::Varint);
            buf.write_bool(self.active);
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
                    self.name = buf
                        .read_string()
                        .map_err(OxiProtoError::WireFormatError)?
                        .to_owned();
                }
                (2, WireType::Varint) => {
                    self.age = buf
                        .read_varint_i32()
                        .map_err(OxiProtoError::WireFormatError)?;
                }
                (3, WireType::Varint) => {
                    self.active = buf.read_bool().map_err(OxiProtoError::WireFormatError)?;
                }
                // Any field number/wire-type we don't recognize is skipped, not
                // rejected -- this is what lets old readers tolerate messages
                // written by newer schema versions.
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let card = ContactCard {
        name: "Ada Lovelace".to_owned(),
        age: 36,
        active: true,
    };

    // encode_to_vec() / decode() come from the default OxiMessage methods --
    // only encoded_len/encode_raw/merge/clear need to be implemented above.
    let bytes = card.encode_to_vec();
    println!("encoded {} bytes: {bytes:02x?}", bytes.len());

    let decoded = ContactCard::decode(&bytes)?;
    assert_eq!(decoded, card);
    println!("round-trip ok: {decoded:?}");

    // A default-valued message encodes to zero bytes (proto3 omits defaults).
    let empty = ContactCard::default();
    assert!(empty.encode_to_vec().is_empty());
    println!("default message encodes to 0 bytes, as expected");

    // Decoding never panics on malformed input -- it returns a typed error.
    // `[0x0A, 0x01, 0xFF]` is: tag for field 1 (Len wire type), a
    // length-delimited payload of 1 byte, and that byte (0xFF) is not valid
    // UTF-8 -- so decoding the `name` string field must fail cleanly.
    match ContactCard::decode(&[0x0A, 0x01, 0xFF]) {
        Ok(msg) => return Err(format!("expected a decode error, got {msg:?}").into()),
        Err(OxiProtoError::WireFormatError(e)) => {
            println!("malformed input rejected as expected: {e}");
        }
        Err(other) => return Err(format!("unexpected error variant: {other}").into()),
    }

    Ok(())
}
