//! Integration tests for the `OxiMessage` trait.
//!
//! Tests a hand-implemented `TestFoo` message against both its own round-trip
//! and byte-level cross-validation against a prost-derived equivalent.
//!
//! Proto equivalent:
//! ```protobuf
//! message TestFoo {
//!   int32  id   = 1;
//!   string name = 2;
//!   repeated string tags = 3;
//! }
//! ```

use oxiproto_core::wire::varint::encoded_len_varint;
use oxiproto_core::wire::{self, encoded_len_length_delimited, WireType};
use oxiproto_core::{OxiMessage, OxiProtoResult};

// ── Hand-implemented TestFoo ──────────────────────────────────────────────────

#[derive(Debug, Default, PartialEq, Clone)]
struct TestFoo {
    id: i32,
    name: String,
    tags: Vec<String>,
}

impl OxiMessage for TestFoo {
    fn encoded_len(&self) -> usize {
        let mut len = 0usize;

        // Field 1: int32 id (omit if default 0)
        if self.id != 0 {
            // tag = (1 << 3) | 0 = 8
            let tag_value = (1u64 << 3) | u64::from(WireType::Varint.value());
            len += encoded_len_varint(tag_value);
            // int32 is sign-extended to i64 then encoded as u64
            len += encoded_len_varint(self.id as i64 as u64);
        }

        // Field 2: string name (omit if empty)
        if !self.name.is_empty() {
            // tag = (2 << 3) | 2 = 18
            let tag_value = (2u64 << 3) | u64::from(WireType::Len.value());
            len += encoded_len_varint(tag_value);
            len += encoded_len_length_delimited(self.name.len());
        }

        // Field 3: repeated string tags (each element gets its own tag)
        for tag_str in &self.tags {
            // tag = (3 << 3) | 2 = 26
            let tag_value = (3u64 << 3) | u64::from(WireType::Len.value());
            len += encoded_len_varint(tag_value);
            len += encoded_len_length_delimited(tag_str.len());
        }

        len
    }

    fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
        // Field 1: int32 id (omit if default 0)
        if self.id != 0 {
            let _ = buf.write_tag(1, WireType::Varint);
            buf.write_varint_i32(self.id);
        }

        // Field 2: string name (omit if empty)
        if !self.name.is_empty() {
            let _ = buf.write_tag(2, WireType::Len);
            buf.write_string(&self.name);
        }

        // Field 3: repeated string tags
        for tag_str in &self.tags {
            let _ = buf.write_tag(3, WireType::Len);
            buf.write_string(tag_str);
        }
    }

    fn merge(&mut self, buf: &mut wire::DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = buf.read_tag()?;
            match tag.field_number {
                1 => {
                    self.id = buf.read_varint_i32()?;
                }
                2 => {
                    let s = buf.read_string()?;
                    self.name = s.to_owned();
                }
                3 => {
                    let s = buf.read_string()?;
                    self.tags.push(s.to_owned());
                }
                _ => {
                    buf.skip_field(tag.wire_type)?;
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        self.id = 0;
        self.name.clear();
        self.tags.clear();
    }
}

// ── Prost-derived equivalent for byte cross-validation ───────────────────────

/// Prost-derived mirror of TestFoo for byte-identical wire format cross-check.
#[derive(Clone, PartialEq, prost::Message)]
struct ProstFoo {
    #[prost(int32, tag = "1")]
    id: i32,
    #[prost(string, tag = "2")]
    name: String,
    #[prost(string, repeated, tag = "3")]
    tags: Vec<String>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn message_trait_round_trip_non_default() {
    let original = TestFoo {
        id: 42,
        name: "hello".to_owned(),
        tags: vec!["a".to_owned(), "b".to_owned()],
    };

    let encoded = original.encode_to_vec();
    let decoded = TestFoo::decode(&encoded).expect("decode must succeed");

    assert_eq!(decoded.id, original.id, "id field must round-trip");
    assert_eq!(decoded.name, original.name, "name field must round-trip");
    assert_eq!(decoded.tags, original.tags, "tags field must round-trip");
}

#[test]
fn message_trait_byte_cross_validation() {
    // Encode via OxiMessage
    let oxi_msg = TestFoo {
        id: 42,
        name: "hello".to_owned(),
        tags: vec!["a".to_owned(), "b".to_owned()],
    };
    let oxi_bytes = oxi_msg.encode_to_vec();

    // Encode via prost::Message
    let prost_msg = ProstFoo {
        id: 42,
        name: "hello".to_owned(),
        tags: vec!["a".to_owned(), "b".to_owned()],
    };
    use prost::Message as ProstMessageTrait;
    let prost_bytes = prost_msg.encode_to_vec();

    assert_eq!(
        oxi_bytes, prost_bytes,
        "OxiMessage-encoded bytes must be identical to prost-encoded bytes.\n\
         oxi  = {:?}\n\
         prost= {:?}",
        oxi_bytes, prost_bytes
    );
}

#[test]
fn message_trait_decode_empty_gives_defaults() {
    let msg = TestFoo::decode(&[]).expect("decode from empty must succeed");
    assert_eq!(msg.id, 0, "default id is 0");
    assert_eq!(msg.name, "", "default name is empty");
    assert!(msg.tags.is_empty(), "default tags is empty");
}

#[test]
fn message_trait_encode_empty_gives_zero_bytes() {
    let msg = TestFoo::default();
    let bytes = msg.encode_to_vec();
    assert!(
        bytes.is_empty(),
        "encoding all-default proto3 message must produce 0 bytes, got {:?}",
        bytes
    );
}

#[test]
fn message_trait_encoded_len_matches_actual() {
    let msg = TestFoo {
        id: 100,
        name: "oxiproto".to_owned(),
        tags: vec!["tag1".to_owned(), "tag2".to_owned(), "tag3".to_owned()],
    };
    let declared_len = msg.encoded_len();
    let actual_bytes = msg.encode_to_vec();
    assert_eq!(
        declared_len,
        actual_bytes.len(),
        "encoded_len() must equal actual encoded byte count"
    );
}

#[test]
fn message_trait_encoded_len_empty_is_zero() {
    let msg = TestFoo::default();
    assert_eq!(msg.encoded_len(), 0, "empty message encoded_len must be 0");
}

#[test]
fn message_trait_clear_resets_fields() {
    let mut msg = TestFoo {
        id: 7,
        name: "test".to_owned(),
        tags: vec!["x".to_owned()],
    };
    msg.clear();
    assert_eq!(msg.id, 0);
    assert_eq!(msg.name, "");
    assert!(msg.tags.is_empty());
}

#[test]
fn message_trait_only_id_set() {
    let original = TestFoo {
        id: -1,
        name: String::new(),
        tags: vec![],
    };
    let bytes = original.encode_to_vec();
    let decoded = TestFoo::decode(&bytes).expect("decode");
    assert_eq!(decoded.id, -1);
    assert_eq!(decoded.name, "");
    assert!(decoded.tags.is_empty());
}

#[test]
fn message_trait_negative_id_matches_prost() {
    let oxi_msg = TestFoo {
        id: -1,
        name: String::new(),
        tags: vec![],
    };
    let oxi_bytes = oxi_msg.encode_to_vec();

    let prost_msg = ProstFoo {
        id: -1,
        name: String::new(),
        tags: vec![],
    };
    use prost::Message as ProstMessageTrait;
    let prost_bytes = prost_msg.encode_to_vec();

    assert_eq!(
        oxi_bytes, prost_bytes,
        "Negative int32 encoding must match prost.\noxi={:?}\nprost={:?}",
        oxi_bytes, prost_bytes
    );
}

#[test]
fn message_trait_decode_unknown_field_is_skipped() {
    // Encode a message with an extra field (field 99, varint 1) not known to TestFoo.
    let mut enc = wire::EncodeBuffer::new();
    let _ = enc.write_tag(1, WireType::Varint);
    enc.write_varint_i32(5);
    let _ = enc.write_tag(99, WireType::Varint); // unknown field
    enc.write_varint(1);
    let _ = enc.write_tag(2, WireType::Len);
    enc.write_string("decoded");

    let msg = TestFoo::decode(enc.as_bytes()).expect("decode with unknown field");
    assert_eq!(msg.id, 5);
    assert_eq!(msg.name, "decoded");
}
