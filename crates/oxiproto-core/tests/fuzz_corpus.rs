//! Deterministic corpus-based fuzz tests for the wire format decoder.
//!
//! Unlike `fuzz_decode.rs` which uses `proptest` to generate random inputs,
//! this file contains hand-crafted and machine-derived **corpus entries**: byte
//! sequences that cover known edge cases, boundary conditions, and real-world
//! protobuf messages from the spec.
//!
//! Goals:
//! 1. Every corpus entry must be processed without panicking (the decoder must
//!    return `Ok` or a well-typed `Err`).
//! 2. Valid corpus entries are decoded correctly and re-encoded identically.
//! 3. Invalid / malformed corpus entries are rejected with a meaningful error.
//!
//! Pure-Rust approach per COOLJAPAN policy — no `cargo-fuzz` / libFuzzer.
//! Uses `proptest` for the randomised part and fixed byte arrays for the corpus.
//!
//! Run with: cargo test -p oxiproto-core --test fuzz_corpus

#![forbid(unsafe_code)]

use oxiproto_core::wire::{
    decode_tag, decode_varint, DecodeBuffer, EncodeBuffer, WireError, WireType,
};
use proptest::prelude::*;

// ─── Corpus entry type ────────────────────────────────────────────────────────

/// Describes one corpus entry: raw bytes, an expected outcome (ok/err), and a
/// brief description for test output.
struct Corpus {
    bytes: &'static [u8],
    /// `true` if the sequence is a valid protobuf encoding; `false` if it must
    /// produce a `WireError`.
    valid: bool,
    description: &'static str,
}

// ─── Corpus: single-byte edge cases ──────────────────────────────────────────
//
// NOTE: These are tested as *messages* (via decode_message_loop which calls
// read_tag + skip_field). A single byte `\x00` encodes tag field_number=0 which
// is reserved and thus invalid. `\x08` encodes tag field_number=1, wire_type=Varint
// but has no value bytes — also invalid. Valid single-byte messages do not
// practically exist since every tag + value requires at least 2 bytes.

static CORPUS_SINGLE_BYTE: &[Corpus] = &[
    // \x00 = field_number 0, reserved → invalid as a field tag
    Corpus {
        bytes: b"\x00",
        valid: false,
        description: "tag field_number=0 is reserved (invalid)",
    },
    // \x01 = field_number 0, wire_type=Varint (0<<3|1 is not valid; field_number=0 still reserved)
    // Actually \x01 = tag value 1 → (0<<3)|1 = field 0, wire I64 → invalid field number
    Corpus {
        bytes: b"\x01",
        valid: false,
        description: "tag decodes to field_number=0 (invalid)",
    },
    // \x08 = tag(field=1, Varint) but no value bytes follow → truncated
    Corpus {
        bytes: b"\x08",
        valid: false,
        description: "tag without value bytes (truncated)",
    },
    // \x80 = continuation byte, varint never terminates
    Corpus {
        bytes: b"\x80",
        valid: false,
        description: "varint: continuation byte, no terminator",
    },
    // \xff = 0xFF continuation, varint never terminates
    Corpus {
        bytes: b"\xff",
        valid: false,
        description: "varint: 0xFF continuation, no terminator",
    },
];

// ─── Corpus: multi-byte varints ───────────────────────────────────────────────

static CORPUS_VARINT: &[Corpus] = &[
    Corpus {
        bytes: &[0x96, 0x01],
        valid: true,
        description: "varint 150",
    },
    Corpus {
        bytes: &[0x80, 0x01],
        valid: true,
        description: "varint 128",
    },
    Corpus {
        bytes: &[0xAC, 0x02],
        valid: true,
        description: "varint 300",
    },
    Corpus {
        bytes: &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01],
        valid: true,
        description: "varint u64::MAX",
    },
    Corpus {
        bytes: &[
            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01,
        ],
        valid: false,
        description: "varint: 11 bytes (overflow)",
    },
    Corpus {
        bytes: &[0x80; 20],
        valid: false,
        description: "varint: 20 continuation bytes",
    },
    // Minimal 10-byte valid varint (1 << 63 in zigzag)
    Corpus {
        bytes: &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01],
        valid: true,
        description: "varint 10-byte minimal",
    },
];

// ─── Corpus: field tags ───────────────────────────────────────────────────────

static CORPUS_TAGS: &[Corpus] = &[
    // Valid tags from the encoding spec
    Corpus {
        bytes: b"\x08",
        valid: true,
        description: "tag: field 1, Varint (= 8)",
    },
    Corpus {
        bytes: b"\x12",
        valid: true,
        description: "tag: field 2, Len (= 18)",
    },
    Corpus {
        bytes: b"\x09",
        valid: true,
        description: "tag: field 1, I64 (= 9)",
    },
    Corpus {
        bytes: b"\x0d",
        valid: true,
        description: "tag: field 1, I32 (= 13)",
    },
    Corpus {
        bytes: b"\x0a",
        valid: true,
        description: "tag: field 1, Len (= 10)",
    },
    // Invalid: wire type 6 and 7 are unassigned
    Corpus {
        bytes: b"\x0e",
        valid: false,
        description: "tag: wire type 6 (unassigned)",
    },
    Corpus {
        bytes: b"\x0f",
        valid: false,
        description: "tag: wire type 7 (unassigned)",
    },
    // Invalid: field number 0 is reserved
    Corpus {
        bytes: b"\x00",
        valid: false,
        description: "tag: field_number=0 is reserved",
    },
    // High field number (2047 << 3) | 0 = 16376 = \xF8\x7F
    Corpus {
        bytes: &[0xF8, 0x7F],
        valid: true,
        description: "tag: field 2047, Varint",
    },
    // Max field number: (1<<29 - 1) = 536870911
    Corpus {
        bytes: &[0xf8, 0xff, 0xff, 0xff, 0x07], // (536870911 << 3) | 0 = varint of that
        valid: true,
        description: "tag: max valid field number",
    },
];

// ─── Corpus: length-delimited fields ─────────────────────────────────────────

static CORPUS_LEN_DELIMITED: &[Corpus] = &[
    Corpus {
        bytes: b"\x00",
        valid: true,
        description: "length-delimited: empty payload",
    },
    Corpus {
        bytes: b"\x07testing",
        valid: true,
        description: "length-delimited: 'testing'",
    },
    Corpus {
        bytes: b"\x03\x01\x02\x03",
        valid: true,
        description: "length-delimited: 3-byte payload",
    },
    // Claimed length bigger than remaining bytes
    Corpus {
        bytes: b"\x10\x01\x02",
        valid: false,
        description: "length-delimited: claimed 16, only 2 bytes",
    },
    // Zero-length is valid
    Corpus {
        bytes: b"\x00",
        valid: true,
        description: "length-delimited: zero-length payload",
    },
    // Max-range varint as length (way too large)
    Corpus {
        bytes: &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01],
        valid: false,
        description: "length-delimited: u64::MAX as claimed length",
    },
];

// ─── Corpus: complete messages (valid) ────────────────────────────────────────

/// Valid complete protobuf messages derived from the encoding guide.
static CORPUS_VALID_MESSAGES: &[Corpus] = &[
    // message Test1 { int32 a = 1; }  a = 150  ⇒  \x08\x96\x01
    Corpus {
        bytes: &[0x08, 0x96, 0x01],
        valid: true,
        description: "Test1 { a = 150 }",
    },
    // message Test2 { string b = 2; }  b = "testing"  ⇒  \x12\x07testing
    Corpus {
        bytes: &[0x12, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g'],
        valid: true,
        description: "Test2 { b = \"testing\" }",
    },
    // Packed repeated: field 4, payload [3, 270, 86942]
    Corpus {
        bytes: &[0x22, 0x06, 0x03, 0x8E, 0x02, 0x9E, 0xA7, 0x05],
        valid: true,
        description: "packed repeated int32 [3, 270, 86942]",
    },
    // Multi-field: { a:42 (field1 varint), b:"hi" (field2 len), c:true (field3 varint) }
    Corpus {
        bytes: &[0x08, 0x2A, 0x12, 0x02, b'h', b'i', 0x18, 0x01],
        valid: true,
        description: "multi-field message",
    },
    // Nested message: outer.field3 = inner{ a=150 }
    Corpus {
        bytes: &[0x1A, 0x03, 0x08, 0x96, 0x01],
        valid: true,
        description: "Test3 { c = Test1 { a = 150 } }",
    },
    // Empty message (zero bytes) is valid in proto3.
    Corpus {
        bytes: &[],
        valid: true,
        description: "empty message (all defaults)",
    },
    // float 1.0 as field 5: \x2d\x00\x00\x80\x3f
    Corpus {
        bytes: &[0x2D, 0x00, 0x00, 0x80, 0x3F],
        valid: true,
        description: "float field 1.0",
    },
    // double 1.0 as field 1: \x09 + IEEE754 LE
    Corpus {
        bytes: &[0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F],
        valid: true,
        description: "double field 1.0",
    },
    // bool true as field 1: \x08\x01
    Corpus {
        bytes: &[0x08, 0x01],
        valid: true,
        description: "bool true",
    },
    // sfixed32 -1 as field 1: \x0d\xff\xff\xff\xff
    Corpus {
        bytes: &[0x0D, 0xFF, 0xFF, 0xFF, 0xFF],
        valid: true,
        description: "sfixed32 -1",
    },
    // sfixed64 -1 as field 1: \x09 + 8× \xff
    Corpus {
        bytes: &[0x09, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        valid: true,
        description: "sfixed64 -1",
    },
    // int32 -1 as field 1: \x08 + 10 bytes of varint(u64::MAX)
    Corpus {
        bytes: &[
            0x08, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01,
        ],
        valid: true,
        description: "int32 -1 (sign-extended 10-byte varint)",
    },
    // sint32 -1 as field 1: tag\x08 + zigzag(1) = \x01
    Corpus {
        bytes: &[0x08, 0x01],
        valid: true,
        description: "sint32 / zigzag: -1 encodes as varint 1",
    },
    // bytes field: \x0a\x03\x01\x02\x03
    Corpus {
        bytes: &[0x0A, 0x03, 0x01, 0x02, 0x03],
        valid: true,
        description: "bytes field {1: [1,2,3]}",
    },
];

// ─── Corpus: adversarial / malformed ─────────────────────────────────────────

static CORPUS_MALFORMED: &[Corpus] = &[
    // Truncated fixed32 (only 3 bytes instead of 4)
    Corpus {
        bytes: &[0x0D, 0x01, 0x02, 0x03],
        valid: false,
        description: "truncated sfixed32",
    },
    // Truncated fixed64 (only 5 bytes instead of 8)
    Corpus {
        bytes: &[0x09, 0x01, 0x02, 0x03, 0x04, 0x05],
        valid: false,
        description: "truncated sfixed64 (5 bytes)",
    },
    // String with claimed length > actual bytes
    Corpus {
        bytes: &[0x0A, 0x10, b'a', b'b', b'c'],
        valid: false,
        description: "string: claimed 16 chars, got 3",
    },
    // Nested message: inner claimed 100 bytes, only 2 provided
    Corpus {
        bytes: &[0x12, 0x64, 0x08, 0x01],
        valid: false,
        description: "nested: claimed 100-byte inner, only 2 bytes",
    },
    // Varint-only truncation at message boundary
    Corpus {
        bytes: &[0x08],
        valid: false,
        description: "tag followed by no value bytes",
    },
    // Field tag then truncated string length
    Corpus {
        bytes: &[0x12, 0x80],
        valid: false,
        description: "tag then incomplete varint length",
    },
];

// ─── Corpus runner helpers ────────────────────────────────────────────────────

/// Run a message-level decode loop over the corpus bytes.
///
/// Returns `Ok(true)` if at least one field was successfully read, `Ok(false)`
/// for valid empty messages, or `Err(WireError)` for malformed input.
fn decode_message_loop(bytes: &[u8]) -> Result<(), WireError> {
    let mut buf = DecodeBuffer::new(bytes);
    while !buf.is_empty() {
        let tag = buf.read_tag()?;
        buf.skip_field(tag.wire_type)?;
    }
    Ok(())
}

/// Run the varint decoder on the full byte slice.
fn decode_varint_all(bytes: &[u8]) -> Result<u64, WireError> {
    decode_varint(bytes).map(|(v, _)| v)
}

// ─── Tests: single-byte corpus ────────────────────────────────────────────────

#[test]
fn corpus_single_byte_no_panic() {
    for entry in CORPUS_SINGLE_BYTE {
        // The decoder must never panic — only Ok or Err.
        // Single-byte entries are tested as messages (tag + value decode loop).
        // All entries here are expected to be invalid (field_number=0 is reserved,
        // incomplete varints, or invalid wire types).
        let result = decode_message_loop(entry.bytes);
        if entry.valid {
            result.unwrap_or_else(|e| {
                panic!("Expected Ok for {:?} but got Err({e:?})", entry.description)
            });
        } else {
            // Must either return Err or succeed (empty message is fine too).
            // The key requirement is: NO PANIC.
            // We still assert it's Err for entries we expect to be invalid.
            assert!(
                result.is_err(),
                "Expected Err for {:?} but got Ok (unexpected success)",
                entry.description
            );
        }
    }
}

// ─── Tests: varint corpus ─────────────────────────────────────────────────────

#[test]
fn corpus_varint_decode() {
    for entry in CORPUS_VARINT {
        let result = decode_varint_all(entry.bytes);
        if entry.valid {
            result.unwrap_or_else(|e| {
                panic!("Expected Ok for {:?} but got Err({e:?})", entry.description)
            });
        } else {
            assert!(
                result.is_err(),
                "Expected Err for {:?} but got Ok",
                entry.description
            );
        }
    }
}

// ─── Tests: tag corpus ────────────────────────────────────────────────────────

#[test]
fn corpus_tag_decode() {
    for entry in CORPUS_TAGS {
        let result = decode_tag(entry.bytes);
        if entry.valid {
            result.unwrap_or_else(|e| {
                panic!("Expected Ok for {:?} but got Err({e:?})", entry.description)
            });
        } else {
            assert!(
                result.is_err(),
                "Expected Err for {:?} but got Ok",
                entry.description
            );
        }
    }
}

// ─── Tests: length-delimited corpus ──────────────────────────────────────────

#[test]
fn corpus_length_delimited_decode() {
    for entry in CORPUS_LEN_DELIMITED {
        let mut buf = DecodeBuffer::new(entry.bytes);
        let result = buf.read_length_delimited();
        if entry.valid {
            result.unwrap_or_else(|e| {
                panic!("Expected Ok for {:?} but got Err({e:?})", entry.description)
            });
        } else {
            assert!(
                result.is_err(),
                "Expected Err for {:?} but got Ok",
                entry.description
            );
        }
    }
}

// ─── Tests: valid messages corpus ────────────────────────────────────────────

#[test]
fn corpus_valid_messages_decode_no_error() {
    for entry in CORPUS_VALID_MESSAGES {
        assert!(
            entry.valid,
            "Test data error: {:?} should be marked valid",
            entry.description
        );
        decode_message_loop(entry.bytes).unwrap_or_else(|e| {
            panic!(
                "Valid corpus entry {:?} produced unexpected error: {e:?}",
                entry.description
            )
        });
    }
}

#[test]
fn corpus_valid_messages_round_trip() {
    // Each valid corpus message can be decoded field-by-field, then all fields
    // re-encoded, and the round-tripped bytes equal the original.
    for entry in CORPUS_VALID_MESSAGES {
        // Collect field tags and raw payloads
        let mut buf = DecodeBuffer::new(entry.bytes);
        let mut enc = EncodeBuffer::new();
        while !buf.is_empty() {
            let tag = match buf.read_tag() {
                Ok(t) => t,
                Err(WireError::UnexpectedEof) => break,
                Err(e) => panic!("read_tag in {:?}: {e:?}", entry.description),
            };
            // Re-emit tag
            enc.write_tag(tag.field_number, tag.wire_type)
                .unwrap_or_else(|e| panic!("write_tag in {:?}: {e:?}", entry.description));
            // Read + re-emit value
            match tag.wire_type {
                WireType::Varint => {
                    let v = buf.read_varint().unwrap_or_else(|e| {
                        panic!("read_varint in {:?}: {e:?}", entry.description)
                    });
                    enc.write_varint(v);
                }
                WireType::I64 => {
                    let v = buf.read_fixed64().unwrap_or_else(|e| {
                        panic!("read_fixed64 in {:?}: {e:?}", entry.description)
                    });
                    enc.write_fixed64(v);
                }
                WireType::Len => {
                    let payload = buf.read_length_delimited().unwrap_or_else(|e| {
                        panic!("read_length_delimited in {:?}: {e:?}", entry.description)
                    });
                    enc.write_length_delimited(payload);
                }
                WireType::I32 => {
                    let v = buf.read_fixed32().unwrap_or_else(|e| {
                        panic!("read_fixed32 in {:?}: {e:?}", entry.description)
                    });
                    enc.write_fixed32(v);
                }
                WireType::SGroup | WireType::EGroup => {
                    // Groups are deprecated but must not panic.
                    // Skip by advancing past the group.
                    buf.skip_field(tag.wire_type).unwrap_or_else(|e| {
                        panic!("skip_field SGroup/EGroup in {:?}: {e:?}", entry.description)
                    });
                }
            }
        }
        assert_eq!(
            enc.as_bytes(),
            entry.bytes,
            "round-trip mismatch for corpus entry {:?}",
            entry.description
        );
    }
}

// ─── Tests: malformed messages corpus ────────────────────────────────────────

#[test]
fn corpus_malformed_messages_return_err() {
    for entry in CORPUS_MALFORMED {
        assert!(
            !entry.valid,
            "Test data error: {:?} should be marked invalid",
            entry.description
        );
        let result = decode_message_loop(entry.bytes);
        assert!(
            result.is_err(),
            "Malformed corpus entry {:?} should have returned Err, got Ok",
            entry.description
        );
    }
}

// ─── Proptest: corpus-guided fuzzing with known seed patterns ─────────────────
//
// These tests mutate known-valid corpus entries by flipping bits, truncating, or
// prepending/appending random bytes, and verify no panic results.

proptest! {
    /// Mutate a known-valid corpus entry by prepending random bytes; must not panic.
    #[test]
    fn fuzz_corpus_prepend_mutation(
        seed_idx in 0usize..15usize,
        prefix in proptest::collection::vec(any::<u8>(), 0..8),
    ) {
        let seeds = CORPUS_VALID_MESSAGES;
        let entry = &seeds[seed_idx % seeds.len()];
        let mut mutated = prefix;
        mutated.extend_from_slice(entry.bytes);
        // Must not panic; may succeed or fail.
        let _ = decode_message_loop(&mutated);
    }

    /// Mutate a known-valid corpus entry by appending random bytes; must not panic.
    #[test]
    fn fuzz_corpus_append_mutation(
        seed_idx in 0usize..15usize,
        suffix in proptest::collection::vec(any::<u8>(), 0..8),
    ) {
        let seeds = CORPUS_VALID_MESSAGES;
        let entry = &seeds[seed_idx % seeds.len()];
        let mut mutated = entry.bytes.to_vec();
        mutated.extend_from_slice(&suffix);
        let _ = decode_message_loop(&mutated);
    }

    /// Bit-flip fuzzing on valid corpus entries; must not panic.
    ///
    /// Flips a single bit in the first (up to 8) bytes of a known-valid entry.
    /// The mutated bytes may or may not be valid; either outcome is acceptable
    /// as long as no panic occurs.
    #[test]
    fn fuzz_corpus_bit_flip(
        seed_idx in 0usize..15usize,
        byte_offset in 0usize..8usize,
        bit_offset in 0u8..8u8,
    ) {
        let seeds = CORPUS_VALID_MESSAGES;
        let entry = &seeds[seed_idx % seeds.len()];
        let mut mutated = entry.bytes.to_vec();
        if !mutated.is_empty() {
            let pos = byte_offset % mutated.len();
            mutated[pos] ^= 1u8 << bit_offset;
        }
        let _ = decode_message_loop(&mutated);
    }

    /// Truncation fuzzing: trim a known-valid corpus entry to a random prefix.
    #[test]
    fn fuzz_corpus_truncation(
        seed_idx in 0usize..15usize,
        keep in 0usize..20usize,
    ) {
        let seeds = CORPUS_VALID_MESSAGES;
        let entry = &seeds[seed_idx % seeds.len()];
        let len = keep.min(entry.bytes.len());
        let truncated = &entry.bytes[..len];
        let _ = decode_message_loop(truncated);
    }

    /// Randomly generated byte sequences of length 0–100: must not panic.
    #[test]
    fn fuzz_corpus_random_bytes(
        bytes in proptest::collection::vec(any::<u8>(), 0..100)
    ) {
        let _ = decode_message_loop(&bytes);
    }
}

// ─── Additional deterministic edge-case tests ─────────────────────────────────

/// The decoder must accept messages with consecutive unknown fields.
#[test]
fn corpus_multiple_unknown_fields_skipped_without_error() {
    // Build a message: field 100 (varint 1), field 200 (len "x"), field 300 (fixed64 0).
    let mut enc = EncodeBuffer::new();
    enc.write_tag(100, WireType::Varint).expect("tag100");
    enc.write_varint(1);
    enc.write_tag(200, WireType::Len).expect("tag200");
    enc.write_string("x");
    enc.write_tag(300, WireType::I64).expect("tag300");
    enc.write_fixed64(0);
    let bytes = enc.into_vec();
    decode_message_loop(&bytes).expect("multiple unknown fields must be skippable");
}

/// A message with a mix of high and low field numbers must decode without error.
#[test]
fn corpus_high_and_low_field_numbers() {
    let mut enc = EncodeBuffer::new();
    // field 1
    enc.write_tag(1, WireType::Varint).expect("tag1");
    enc.write_varint(42);
    // field 536870911 (max)
    enc.write_tag(536_870_911, WireType::Varint)
        .expect("tag max");
    enc.write_varint(7);
    let bytes = enc.into_vec();
    decode_message_loop(&bytes).expect("high and low field numbers must decode cleanly");
}

/// Repeated decode on the same buffer slice produces consistent results.
#[test]
fn corpus_idempotent_varint_decode() {
    let golden = &[0x96_u8, 0x01]; // varint 150
    for _ in 0..100 {
        let (v, c) = decode_varint(golden).expect("must decode");
        assert_eq!(v, 150);
        assert_eq!(c, 2);
    }
}

/// All valid wire types can be skipped by `skip_field`.
#[test]
fn corpus_skip_field_all_wire_types() {
    use WireType::*;
    for wt in [Varint, I64, Len, I32] {
        let mut enc = EncodeBuffer::new();
        enc.write_tag(1, wt).expect("tag");
        match wt {
            Varint => enc.write_varint(9999),
            I64 => enc.write_fixed64(0xDEAD_CAFE),
            Len => enc.write_string("skip me"),
            I32 => enc.write_fixed32(0xABCD),
            SGroup | EGroup => unreachable!(),
        }
        let bytes = enc.into_vec();
        let mut dec = DecodeBuffer::new(&bytes);
        let tag = dec.read_tag().expect("tag");
        dec.skip_field(tag.wire_type)
            .unwrap_or_else(|e| panic!("skip_field for {wt:?} must not fail: {e:?}"));
        assert!(
            dec.is_empty(),
            "buffer must be consumed after skip_field({wt:?})"
        );
    }
}
