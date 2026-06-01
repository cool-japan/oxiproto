//! Integration tests for the `Extensions` struct.
//!
//! Uses a simple hand-implemented `SmallMsg { value: i64 }` as the message
//! type for extension values.

use oxiproto_core::wire::varint::encoded_len_varint;
use oxiproto_core::wire::{self, WireType};
use oxiproto_core::{Extensions, OxiMessage, OxiProtoResult};

// ── Hand-implemented SmallMsg ─────────────────────────────────────────────────

/// Proto equivalent:
/// ```protobuf
/// message SmallMsg {
///   int64 value = 1;
/// }
/// ```
#[derive(Debug, Default, PartialEq, Clone)]
struct SmallMsg {
    value: i64,
}

impl OxiMessage for SmallMsg {
    fn encoded_len(&self) -> usize {
        if self.value == 0 {
            return 0;
        }
        // tag = (1 << 3) | 0 = 8
        let tag_value = (1u64 << 3) | u64::from(WireType::Varint.value());
        encoded_len_varint(tag_value) + encoded_len_varint(self.value as u64)
    }

    fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
        if self.value != 0 {
            let _ = buf.write_tag(1, WireType::Varint);
            buf.write_varint_i64(self.value);
        }
    }

    fn merge(&mut self, buf: &mut wire::DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = buf.read_tag()?;
            match tag.field_number {
                1 => {
                    self.value = buf.read_varint_i64()?;
                }
                _ => {
                    buf.skip_field(tag.wire_type)?;
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        self.value = 0;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn extensions_set_and_get_round_trip() {
    let mut ext = Extensions::new();
    let msg = SmallMsg { value: 42 };

    ext.set_extension::<SmallMsg>(100, &msg)
        .expect("set_extension must succeed");

    let retrieved: SmallMsg = ext
        .get_extension::<SmallMsg>(100)
        .expect("get_extension must succeed")
        .expect("extension must be present");

    assert_eq!(retrieved, msg, "round-tripped message must equal original");
}

#[test]
fn extensions_has_extension_true_after_set() {
    let mut ext = Extensions::new();
    assert!(!ext.has_extension(200), "no extension yet");

    ext.set_extension::<SmallMsg>(200, &SmallMsg { value: 1 })
        .expect("set");

    assert!(
        ext.has_extension(200),
        "extension should be present after set"
    );
}

#[test]
fn extensions_has_extension_false_for_unset_field() {
    let ext = Extensions::new();
    assert!(
        !ext.has_extension(999),
        "extension for unknown field must be absent"
    );
}

#[test]
fn extensions_clear_extension_removes_it() {
    let mut ext = Extensions::new();
    ext.set_extension::<SmallMsg>(50, &SmallMsg { value: 5 })
        .expect("set");
    assert!(ext.has_extension(50));

    ext.clear_extension(50);

    assert!(
        !ext.has_extension(50),
        "extension must be absent after clear_extension"
    );
    let result: Option<SmallMsg> = ext
        .get_extension::<SmallMsg>(50)
        .expect("get after clear must not error");
    assert!(
        result.is_none(),
        "get_extension must return None after clear"
    );
}

#[test]
fn extensions_is_empty_and_len() {
    let mut ext = Extensions::new();
    assert!(ext.is_empty(), "new Extensions must be empty");
    assert_eq!(ext.len(), 0, "new Extensions len must be 0");

    ext.set_extension::<SmallMsg>(1, &SmallMsg { value: 1 })
        .expect("set 1");
    assert!(!ext.is_empty());
    assert_eq!(ext.len(), 1);

    ext.set_extension::<SmallMsg>(2, &SmallMsg { value: 2 })
        .expect("set 2");
    assert_eq!(ext.len(), 2);

    ext.clear();
    assert!(ext.is_empty());
    assert_eq!(ext.len(), 0);
}

#[test]
fn extensions_overwrite_stores_latest_value() {
    let mut ext = Extensions::new();

    ext.set_extension::<SmallMsg>(10, &SmallMsg { value: 100 })
        .expect("set first");
    ext.set_extension::<SmallMsg>(10, &SmallMsg { value: 999 })
        .expect("set second (overwrite)");

    let retrieved: SmallMsg = ext
        .get_extension::<SmallMsg>(10)
        .expect("get")
        .expect("present");

    assert_eq!(retrieved.value, 999, "second set must overwrite the first");
}

#[test]
fn extensions_encode_raw_and_merge_raw_round_trip() {
    let mut ext = Extensions::new();
    let msg_a = SmallMsg { value: 1234 };
    let msg_b = SmallMsg { value: -5678 };

    ext.set_extension::<SmallMsg>(10, &msg_a).expect("set 10");
    ext.set_extension::<SmallMsg>(20, &msg_b).expect("set 20");

    // Encode to wire bytes.
    let mut enc = wire::EncodeBuffer::new();
    ext.encode_raw(&mut enc);
    let wire_bytes = enc.into_vec();

    // Decode back via merge_raw.
    let mut ext2 = Extensions::new();
    let mut dec = wire::DecodeBuffer::new(&wire_bytes);
    while !dec.is_empty() {
        let tag = dec.read_tag().expect("read tag");
        ext2.merge_raw(tag.field_number, tag.wire_type, &mut dec)
            .expect("merge_raw");
    }

    // Verify both extensions are present in ext2.
    let retrieved_a: SmallMsg = ext2
        .get_extension::<SmallMsg>(10)
        .expect("get 10")
        .expect("present 10");
    let retrieved_b: SmallMsg = ext2
        .get_extension::<SmallMsg>(20)
        .expect("get 20")
        .expect("present 20");

    assert_eq!(
        retrieved_a, msg_a,
        "field 10 must round-trip via encode/merge_raw"
    );
    assert_eq!(
        retrieved_b, msg_b,
        "field 20 must round-trip via encode/merge_raw"
    );
}

#[test]
fn extensions_encoded_len_matches_actual() {
    let mut ext = Extensions::new();
    ext.set_extension::<SmallMsg>(5, &SmallMsg { value: 42 })
        .expect("set 5");
    ext.set_extension::<SmallMsg>(100, &SmallMsg { value: 0 })
        .expect("set 100");

    let declared = ext.encoded_len();

    let mut enc = wire::EncodeBuffer::new();
    ext.encode_raw(&mut enc);
    let actual = enc.len();

    assert_eq!(
        declared, actual,
        "encoded_len must match actual encoded byte count"
    );
}

#[test]
fn extensions_get_extension_absent_returns_none() {
    let ext = Extensions::new();
    let result: Option<SmallMsg> = ext.get_extension(42).expect("get must not error");
    assert!(result.is_none(), "absent extension must return None");
}

#[test]
fn extensions_merge_raw_len_type_stores_correct_bytes() {
    // Build raw wire bytes: tag(7, Len) + len-prefixed SmallMsg bytes.
    let inner_msg = SmallMsg { value: 77 };
    let inner_bytes = inner_msg.encode_to_vec();

    let mut enc = wire::EncodeBuffer::new();
    enc.write_tag(7, WireType::Len).expect("tag");
    enc.write_length_delimited(&inner_bytes);
    let wire_bytes = enc.into_vec();

    let mut ext = Extensions::new();
    let mut dec = wire::DecodeBuffer::new(&wire_bytes);
    let tag = dec.read_tag().expect("read tag");
    ext.merge_raw(tag.field_number, tag.wire_type, &mut dec)
        .expect("merge_raw");

    let retrieved: SmallMsg = ext
        .get_extension::<SmallMsg>(7)
        .expect("get")
        .expect("present");
    assert_eq!(retrieved, inner_msg);
}
