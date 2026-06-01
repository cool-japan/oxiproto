//! No-panic fuzz tests for the wire-format decoder.
//!
//! These tests feed arbitrary byte sequences (not necessarily valid protobuf)
//! into every decode path and assert the decoder returns `Ok` or `Err`
//! gracefully, never panicking.
//!
//! Uses `proptest` (already a dev-dep) in accordance with the Pure Rust Policy
//! — no `cargo-fuzz` / libFuzzer (C++).

use oxiproto_core::wire::DecodeBuffer;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Single-read no-panic tests
// ---------------------------------------------------------------------------

proptest! {
    /// Varint decode never panics on arbitrary bytes.
    #[test]
    fn fuzz_varint_decode_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..20)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_varint(); // must return Ok or Err, never panic
    }

    /// Varint32 decode never panics on arbitrary bytes.
    #[test]
    fn fuzz_varint32_decode_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..20)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_varint32();
    }

    /// Tag decode never panics on arbitrary bytes.
    #[test]
    fn fuzz_tag_decode_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..20)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_tag();
    }

    /// Length-delimited decode never panics on arbitrary bytes (incl. oversized lengths).
    #[test]
    fn fuzz_length_delimited_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..100)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_length_delimited();
    }

    /// String decode (UTF-8 checked) never panics on arbitrary bytes.
    #[test]
    fn fuzz_string_decode_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..100)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_string();
    }

    /// Fixed32 decode never panics on arbitrary bytes.
    #[test]
    fn fuzz_fixed32_decode_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..20)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_fixed32();
    }

    /// Fixed64 decode never panics on arbitrary bytes.
    #[test]
    fn fuzz_fixed64_decode_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..20)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_fixed64();
    }

    /// Float decode never panics on arbitrary bytes.
    #[test]
    fn fuzz_float_decode_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..20)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_float();
    }

    /// Double decode never panics on arbitrary bytes.
    #[test]
    fn fuzz_double_decode_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..20)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_double();
    }

    /// Bool decode never panics on arbitrary bytes.
    #[test]
    fn fuzz_bool_decode_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..20)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_bool();
    }

    // -----------------------------------------------------------------------
    // Adversarial / edge-case scenarios
    // -----------------------------------------------------------------------

    /// Adversarial: varint-encoded length claiming a huge payload, truncated body.
    ///
    /// The length prefix is valid; the body is too short.  The decoder must
    /// return `Err(WireError::TruncatedMessage)` without panicking.
    #[test]
    fn fuzz_truncated_length_delimited_no_panic(
        claimed_len in 0u64..=(1u64 << 32),
        body in proptest::collection::vec(any::<u8>(), 0..10)
    ) {
        // Varint-encode `claimed_len` as the length prefix.
        let mut bytes: Vec<u8> = Vec::new();
        let mut v = claimed_len;
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
        bytes.extend_from_slice(&body);

        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_length_delimited(); // must not panic even with absurd claimed length
    }

    /// All-0xFF bytes never panic — tests the varint overflow path.
    #[test]
    fn fuzz_all_0xff_no_panic(len in 1usize..50) {
        let bytes = vec![0xFF_u8; len];
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_varint();
    }

    /// All-0x80 bytes (continuation bit always set, no terminator) never panic.
    #[test]
    fn fuzz_all_0x80_no_panic(len in 1usize..50) {
        let bytes = vec![0x80_u8; len];
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_varint();
    }

    /// Empty input never panics for any read method.
    #[test]
    fn fuzz_empty_input_no_panic(_unused in 0u8..1u8) {
        let bytes: &[u8] = &[];
        let mut buf = DecodeBuffer::new(bytes);
        let _ = buf.read_varint();

        let mut buf = DecodeBuffer::new(bytes);
        let _ = buf.read_tag();

        let mut buf = DecodeBuffer::new(bytes);
        let _ = buf.read_length_delimited();

        let mut buf = DecodeBuffer::new(bytes);
        let _ = buf.read_fixed32();

        let mut buf = DecodeBuffer::new(bytes);
        let _ = buf.read_fixed64();
    }

    /// Completely random bytes fed to repeated varint decode calls never panic.
    ///
    /// Simulates a streaming parser consuming as many fields as possible.
    #[test]
    fn fuzz_repeated_varint_decode_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..200)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        for _ in 0..50 {
            match buf.read_varint() {
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }

    /// Repeated tag+field decode over random bytes never panics.
    ///
    /// Simulates a complete message-decode loop: read tag, read value by wire
    /// type. Any error breaks the loop gracefully.
    #[test]
    fn fuzz_message_decode_loop_no_panic(
        bytes in proptest::collection::vec(any::<u8>(), 0..200)
    ) {
        let mut buf = DecodeBuffer::new(&bytes);
        for _ in 0..50 {
            let tag = match buf.read_tag() {
                Ok(t) => t,
                Err(_) => break,
            };
            // skip_field is the canonical "consume without interpreting" path
            if buf.skip_field(tag.wire_type).is_err() {
                break;
            }
        }
    }

    /// Single-byte inputs never panic for varint or tag reads.
    #[test]
    fn fuzz_single_byte_no_panic(byte in any::<u8>()) {
        let bytes = [byte];
        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_varint();

        let mut buf = DecodeBuffer::new(&bytes);
        let _ = buf.read_tag();
    }
}
