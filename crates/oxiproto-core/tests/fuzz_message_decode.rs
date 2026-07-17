//! Full-message-level fuzz/property tests for the [`OxiMessage`] decode path.
//!
//! `fuzz_decode.rs` and `fuzz_corpus.rs` already fuzz the low-level
//! [`wire::DecodeBuffer`] primitives (varint/tag/fixed/length-delimited)
//! directly. This file closes the gap one layer up: it fuzzes
//! `SomeMessage::decode(bytes)` for a hand-written message type that mirrors
//! what `oxiproto-codegen` emits (nested messages, repeated fields, unknown
//! field preservation), the same shape real generated code takes in a
//! consuming crate.
//!
//! Goals (Pure Rust, no `cargo-fuzz` / libFuzzer, per COOLJAPAN policy):
//! 1. Decoding **arbitrary** bytes into [`FuzzNode`] never panics -- it always
//!    returns `Ok` or a well-typed `OxiProtoError::WireFormatError`.
//! 2. Encoding a valid, arbitrarily-generated [`FuzzNode`] and decoding it
//!    back round-trips exactly.
//! 3. A seeded-PRNG bit-flip mutation sweep over valid encodings -- a classic
//!    adversarial-input strategy that (unlike pure random bytes) reliably
//!    gets past the first tag and exercises deeper decode states -- never
//!    panics either.
//! 4. Deeply self-nested input is rejected via `WireError::RecursionLimitExceeded`
//!    rather than overflowing the stack (see `oxiproto/tests/recursion_dos.rs`
//!    for the codegen-emitted equivalent of this same regression).
//!
//! Run with: cargo test -p oxiproto-core --test fuzz_message_decode

#![forbid(unsafe_code)]

use oxiproto_core::wire::{self, WireType};
use oxiproto_core::{OxiMessage, OxiProtoError, OxiProtoResult};
use proptest::prelude::*;

// ─── FuzzNode: a small self-referential message ────────────────────────────
//
// Proto3 equivalent:
// ```protobuf
// message FuzzNode {
//   int32           id       = 1;
//   string          label    = 2;
//   repeated FuzzNode children = 3;
//   repeated int32  flags    = 4;
// }
// ```
//
// This mirrors the field shapes `oxiproto-codegen` emits `impl OxiMessage`
// for: a scalar, a string, a repeated nested message (the recursive case that
// exercises `DecodeBuffer::nested`'s depth budget), and a repeated scalar.

#[derive(Debug, Default, Clone, PartialEq)]
struct FuzzNode {
    id: i32,
    label: String,
    children: Vec<FuzzNode>,
    flags: Vec<i32>,
    /// Preserves fields with unrecognized numbers, exactly like generated
    /// code does, so re-encoding a message decoded from a newer schema
    /// doesn't silently drop data.
    unknown: wire::UnknownFields,
}

impl OxiMessage for FuzzNode {
    fn encoded_len(&self) -> usize {
        use wire::varint::encoded_len_varint;

        let mut len = 0usize;
        if self.id != 0 {
            len += encoded_len_varint(1u64 << 3);
            len += encoded_len_varint(self.id as i64 as u64);
        }
        if !self.label.is_empty() {
            len += encoded_len_varint((2u64 << 3) | 2u64);
            len += wire::length_delimited::encoded_len_length_delimited(self.label.len());
        }
        for child in &self.children {
            let child_len = child.encoded_len();
            len += encoded_len_varint((3u64 << 3) | 2u64);
            len += wire::length_delimited::encoded_len_length_delimited(child_len);
        }
        for flag in &self.flags {
            len += encoded_len_varint(4u64 << 3);
            len += encoded_len_varint(*flag as i64 as u64);
        }
        len += self.unknown.encoded_len();
        len
    }

    fn encode_raw(&self, buf: &mut wire::EncodeBuffer) {
        if self.id != 0 {
            let _ = buf.write_tag(1, WireType::Varint);
            buf.write_varint_i32(self.id);
        }
        if !self.label.is_empty() {
            let _ = buf.write_tag(2, WireType::Len);
            buf.write_string(&self.label);
        }
        for child in &self.children {
            let _ = buf.write_tag(3, WireType::Len);
            let child_len = child.encoded_len();
            buf.write_varint(child_len as u64);
            child.encode_raw(buf);
        }
        for flag in &self.flags {
            let _ = buf.write_tag(4, WireType::Varint);
            buf.write_varint_i32(*flag);
        }
        self.unknown.encode_to(buf);
    }

    fn merge(&mut self, buf: &mut wire::DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = match buf.read_tag() {
                Ok(t) => t,
                Err(wire::WireError::UnexpectedEof) => break,
                Err(e) => return Err(OxiProtoError::WireFormatError(e)),
            };
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Varint) => {
                    self.id = buf
                        .read_varint_i32()
                        .map_err(OxiProtoError::WireFormatError)?;
                }
                (2, WireType::Len) => {
                    self.label = buf
                        .read_string()
                        .map_err(OxiProtoError::WireFormatError)?
                        .to_owned();
                }
                (3, WireType::Len) => {
                    let bytes = buf
                        .read_length_delimited()
                        .map_err(OxiProtoError::WireFormatError)?;
                    let mut inner = buf.nested(bytes).map_err(OxiProtoError::WireFormatError)?;
                    let mut child = FuzzNode::default();
                    child.merge(&mut inner)?;
                    self.children.push(child);
                }
                (4, WireType::Varint) => {
                    self.flags.push(
                        buf.read_varint_i32()
                            .map_err(OxiProtoError::WireFormatError)?,
                    );
                }
                (_, wt) => {
                    // Preserve, don't just skip -- matches generated code's
                    // unknown-field handling and lets the round-trip test
                    // below assert byte-for-byte re-encoding.
                    match wt {
                        WireType::Varint => {
                            let v = buf.read_varint().map_err(OxiProtoError::WireFormatError)?;
                            self.unknown.push_varint(tag.field_number, v);
                        }
                        WireType::I64 => {
                            let v = buf.read_fixed64().map_err(OxiProtoError::WireFormatError)?;
                            self.unknown.push_fixed64(tag.field_number, v);
                        }
                        WireType::Len => {
                            let v = buf
                                .read_length_delimited()
                                .map_err(OxiProtoError::WireFormatError)?
                                .to_vec();
                            self.unknown.push_length_delimited(tag.field_number, v);
                        }
                        WireType::I32 => {
                            let v = buf.read_fixed32().map_err(OxiProtoError::WireFormatError)?;
                            self.unknown.push_fixed32(tag.field_number, v);
                        }
                        _ => {
                            buf.skip_field(wt).map_err(OxiProtoError::WireFormatError)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

// ─── Arbitrary-value strategy (bounded depth/breadth to keep cases fast) ──────

fn arb_fuzz_node() -> impl Strategy<Value = FuzzNode> {
    let leaf = (
        any::<i32>(),
        ".{0,24}",
        proptest::collection::vec(any::<i32>(), 0..6),
    )
        .prop_map(|(id, label, flags)| FuzzNode {
            id,
            label,
            children: Vec::new(),
            flags,
            unknown: wire::UnknownFields::new(),
        });

    leaf.prop_recursive(
        3,  // max recursion depth
        16, // max total nodes (approx size budget)
        3,  // max children per node
        |inner| {
            (
                any::<i32>(),
                ".{0,24}",
                proptest::collection::vec(inner, 0..3),
                proptest::collection::vec(any::<i32>(), 0..6),
            )
                .prop_map(|(id, label, children, flags)| FuzzNode {
                    id,
                    label,
                    children,
                    flags,
                    unknown: wire::UnknownFields::new(),
                })
        },
    )
}

// ─── 1. Arbitrary bytes never panic, always Ok or typed WireFormatError ────────

proptest! {
    #[test]
    fn fuzz_decode_arbitrary_bytes_never_panics(
        bytes in proptest::collection::vec(any::<u8>(), 0..512)
    ) {
        match FuzzNode::decode(&bytes) {
            Ok(_) => {}
            Err(OxiProtoError::WireFormatError(_)) => {}
            // Any other error variant would mean merge() is producing an
            // error class the wire decoder was never supposed to surface.
            Err(other) => prop_assert!(
                false,
                "decode of arbitrary bytes returned a non-wire error: {other}"
            ),
        }
    }

    /// Same property, but the byte buffer is seeded with a plausible tag
    /// prefix so more inputs make it past the first `read_tag` call and
    /// into the nested-message / repeated-field decode paths.
    #[test]
    fn fuzz_decode_tag_prefixed_bytes_never_panics(
        field_number in 1u32..16,
        wire_type_raw in 0u32..8, // includes the 6,7 "invalid wire type" cases
        rest in proptest::collection::vec(any::<u8>(), 0..256)
    ) {
        let mut bytes = Vec::new();
        let tag_value = (u64::from(field_number) << 3) | u64::from(wire_type_raw);
        let mut v = tag_value;
        loop {
            let byte = (v & 0x7F) as u8;
            v >>= 7;
            if v != 0 {
                bytes.push(byte | 0x80);
            } else {
                bytes.push(byte);
                break;
            }
        }
        bytes.extend_from_slice(&rest);

        match FuzzNode::decode(&bytes) {
            Ok(_) => {}
            Err(OxiProtoError::WireFormatError(_)) => {}
            Err(other) => prop_assert!(
                false,
                "decode of tag-prefixed arbitrary bytes returned a non-wire error: {other}"
            ),
        }
    }

    // ─── 2. Encode -> decode round-trips for arbitrarily-generated valid messages ──

    #[test]
    fn fuzz_encode_decode_round_trip(msg in arb_fuzz_node()) {
        let bytes = msg.encode_to_vec();
        match FuzzNode::decode(&bytes) {
            Ok(decoded) => prop_assert_eq!(&msg, &decoded, "round-trip mismatch"),
            Err(e) => prop_assert!(
                false,
                "decode of self-encoded valid bytes must succeed, got {e}"
            ),
        }
    }

    #[test]
    fn fuzz_encoded_len_matches_actual(msg in arb_fuzz_node()) {
        let bytes = msg.encode_to_vec();
        prop_assert_eq!(msg.encoded_len(), bytes.len());
    }

    // ─── 3. Seeded bit-flip mutation of valid encodings never panics ──────────────
    //
    // Flipping a handful of bits in an otherwise-valid encoding is a stronger
    // adversarial strategy than pure random bytes: the prefix often still
    // parses as plausible tags, so mutation reaches deeper into the repeated-
    // field and nested-message decode logic than fresh random bytes usually
    // do before hitting an early error.

    #[test]
    fn fuzz_bit_flip_mutation_never_panics(
        msg in arb_fuzz_node(),
        flip_positions in proptest::collection::vec(any::<usize>(), 1..8),
        flip_masks in proptest::collection::vec(any::<u8>(), 1..8),
    ) {
        let mut bytes = msg.encode_to_vec();
        if !bytes.is_empty() {
            for (pos, mask) in flip_positions.iter().zip(flip_masks.iter()) {
                let idx = pos % bytes.len();
                bytes[idx] ^= mask;
            }
        }

        match FuzzNode::decode(&bytes) {
            Ok(_) => {}
            Err(OxiProtoError::WireFormatError(_)) => {}
            Err(other) => prop_assert!(
                false,
                "decode of bit-flipped bytes returned a non-wire error: {other}"
            ),
        }
    }
}

// ─── 4. Deterministic seeded-PRNG adversarial sweep (no proptest) ─────────────
//
// A small xorshift64 PRNG seeded with a fixed constant, run for a large fixed
// number of iterations. Deterministic across runs (same seed -> same
// sequence), and doesn't depend on proptest's case-count configuration.
// This directly satisfies the "seeded-PRNG adversarial-input loop" fallback
// called for when a property-test dependency isn't wanted -- here it's used
// in addition to proptest, not instead of it, to widen the exploration budget
// beyond proptest's default 256 cases per property.

struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        // xorshift64 requires a non-zero seed.
        Self(if seed == 0 {
            0xDEAD_BEEF_CAFE_F00D
        } else {
            seed
        })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_u8(&mut self) -> u8 {
        (self.next_u64() & 0xFF) as u8
    }
}

#[test]
fn seeded_adversarial_decode_sweep_never_panics() {
    let mut rng = XorShift64::new(0x0BAD_C0DE_1234_5678);
    let mut ok_count = 0usize;
    let mut err_count = 0usize;

    for _ in 0..20_000 {
        let len = (rng.next_u64() % 96) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| rng.next_u8()).collect();

        match FuzzNode::decode(&bytes) {
            Ok(_) => ok_count += 1,
            Err(OxiProtoError::WireFormatError(_)) => err_count += 1,
            Err(other) => panic!("unexpected non-wire error for bytes {bytes:02x?}: {other}"),
        }
    }

    // Sanity: the sweep should exercise both outcomes, not degenerate into
    // "everything errors" (which would mean the corpus never gets past the
    // first tag) or "everything succeeds" (which would mean errors are being
    // swallowed somewhere).
    assert!(
        ok_count > 0,
        "expected at least one successful decode in the sweep"
    );
    assert!(
        err_count > 0,
        "expected at least one rejected decode in the sweep"
    );
}

/// Deeply self-nested input must be rejected via the shared recursion-depth
/// budget rather than overflowing the stack. Complements
/// `oxiproto/tests/recursion_dos.rs`, which exercises the same regression
/// through codegen-emitted code instead of a hand-written `OxiMessage` impl.
#[test]
fn deeply_nested_children_rejected_not_overflowed() {
    fn push_varint(mut value: u64, out: &mut Vec<u8>) {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                break;
            }
        }
    }

    // Build `depth` levels of `FuzzNode { children: [FuzzNode { ... }] }`,
    // inside-out. Field 3 (children, repeated message) has LEN tag
    // `(3 << 3) | 2 == 0x1A`.
    let mut bytes: Vec<u8> = Vec::new();
    for _ in 0..5000 {
        let mut next = Vec::new();
        next.push(0x1A);
        push_varint(bytes.len() as u64, &mut next);
        next.extend_from_slice(&bytes);
        bytes = next;
    }

    match FuzzNode::decode(&bytes) {
        Err(OxiProtoError::WireFormatError(wire::WireError::RecursionLimitExceeded)) => {}
        other => panic!("expected RecursionLimitExceeded, got {other:?}"),
    }
}
