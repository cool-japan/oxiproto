//! Smoke tests for the no_std + alloc feature gate.
//!
//! This file exercises wire encode/decode and the `OxiMessage` trait
//! without relying on std-only APIs. It compiles cleanly under both
//! `--features std` (default) and `--no-default-features --features alloc`.

use oxiproto_core::wire::{
    encoded_len_length_delimited, varint::encoded_len_varint, DecodeBuffer, EncodeBuffer, WireType,
};
use oxiproto_core::{OxiMessage, OxiProtoError, OxiProtoResult};

// ── Minimal hand-implemented message ─────────────────────────────────────────

/// A minimal two-field test message: `id` (varint u32, field 1) and
/// `name` (length-delimited string, field 2).
#[derive(Debug, Default, PartialEq, Clone)]
struct SmallMsg {
    id: u32,
    name: String,
}

impl OxiMessage for SmallMsg {
    fn encoded_len(&self) -> usize {
        let mut len = 0usize;

        // Field 1: uint32 (varint, wire type 0)
        if self.id != 0 {
            let tag_val = (1u64 << 3) | u64::from(WireType::Varint.value());
            len += encoded_len_varint(tag_val);
            len += encoded_len_varint(u64::from(self.id));
        }

        // Field 2: string (length-delimited, wire type 2)
        if !self.name.is_empty() {
            let tag_val = (2u64 << 3) | u64::from(WireType::Len.value());
            len += encoded_len_varint(tag_val);
            len += encoded_len_length_delimited(self.name.len());
        }

        len
    }

    fn encode_raw(&self, buf: &mut EncodeBuffer) {
        if self.id != 0 {
            let _ = buf.write_tag(1, WireType::Varint);
            buf.write_varint32(self.id);
        }
        if !self.name.is_empty() {
            let _ = buf.write_tag(2, WireType::Len);
            buf.write_string(&self.name);
        }
    }

    fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = buf.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Varint) => {
                    self.id = buf.read_varint32()?;
                }
                (2, WireType::Len) => {
                    let s = buf.read_string()?;
                    self.name = s.to_owned();
                }
                (_, wt) => {
                    buf.skip_field(wt)?;
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        self.id = 0;
        self.name.clear();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn round_trip_small_msg() {
    let original = SmallMsg {
        id: 42,
        name: "hello".to_owned(),
    };
    let bytes = original.encode_to_vec();
    assert!(!bytes.is_empty(), "encoded bytes should be non-empty");
    let decoded = SmallMsg::decode(&bytes).expect("must decode");
    assert_eq!(original, decoded);
}

#[test]
fn empty_encodes_to_zero_bytes() {
    let msg = SmallMsg::default();
    let bytes = msg.encode_to_vec();
    assert!(
        bytes.is_empty(),
        "all-default message must encode to 0 bytes"
    );
}

#[test]
fn encoded_len_matches_actual_len() {
    let msg = SmallMsg {
        id: 12345,
        name: "oxiproto no_std".to_owned(),
    };
    let declared = msg.encoded_len();
    let actual = msg.encode_to_vec();
    assert_eq!(
        declared,
        actual.len(),
        "encoded_len() must match actual byte count"
    );
}

#[test]
fn id_only_round_trip() {
    let original = SmallMsg {
        id: u32::MAX,
        name: String::new(),
    };
    let bytes = original.encode_to_vec();
    let decoded = SmallMsg::decode(&bytes).expect("must decode");
    assert_eq!(decoded.id, u32::MAX);
    assert!(decoded.name.is_empty());
}

#[test]
fn name_only_round_trip() {
    let original = SmallMsg {
        id: 0,
        name: "no_std_works".to_owned(),
    };
    let bytes = original.encode_to_vec();
    let decoded = SmallMsg::decode(&bytes).expect("must decode");
    assert_eq!(decoded.id, 0);
    assert_eq!(decoded.name, "no_std_works");
}

#[test]
fn clear_resets_to_default() {
    let mut msg = SmallMsg {
        id: 99,
        name: "test".to_owned(),
    };
    msg.clear();
    assert_eq!(msg, SmallMsg::default());
}

#[test]
fn unknown_field_is_skipped() {
    // Encode two fields: known (field 1) plus an unknown (field 99).
    let mut enc = EncodeBuffer::new();
    let _ = enc.write_tag(1, WireType::Varint);
    enc.write_varint32(7);
    let _ = enc.write_tag(99, WireType::Varint); // unknown — must be skipped
    enc.write_varint(1);
    let _ = enc.write_tag(2, WireType::Len);
    enc.write_string("hi");

    let msg = SmallMsg::decode(enc.as_bytes()).expect("decode with unknown field");
    assert_eq!(msg.id, 7);
    assert_eq!(msg.name, "hi");
}

#[test]
fn wire_error_propagated_as_oxi_proto_error() {
    // Empty buffer should trigger an error during decode when data is expected.
    let truncated: &[u8] = &[0x08]; // tag present but no varint payload
    let result = SmallMsg::decode(truncated);
    assert!(
        matches!(result, Err(OxiProtoError::WireFormatError(_))),
        "truncated buffer must return WireFormatError, got {:?}",
        result
    );
}
