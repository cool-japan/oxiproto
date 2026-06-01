//! Property-based round-trip tests for the wire format codec.
//!
//! Each test encodes a value and decodes it, asserting that the decoded value
//! matches the original.  This complements the deterministic unit tests in the
//! source modules by covering arbitrary inputs.

use oxiproto_core::wire;
use proptest::prelude::*;

proptest! {
    /// Varint encoding round-trips for arbitrary u64 values.
    #[test]
    fn varint_round_trip(v in 0u64..=u64::MAX) {
        let mut buf: Vec<u8> = Vec::new();
        wire::varint::encode_varint(v, &mut buf);
        let (decoded, consumed) = wire::varint::decode_varint(&buf)
            .expect("decode_varint should succeed for valid encoding");
        prop_assert_eq!(v, decoded);
        prop_assert_eq!(buf.len(), consumed);
    }

    /// Encoded length of varint matches the actual number of bytes written.
    #[test]
    fn varint_encoded_len_matches_actual(v in 0u64..=u64::MAX) {
        let mut buf: Vec<u8> = Vec::new();
        wire::varint::encode_varint(v, &mut buf);
        let expected_len = wire::varint::encoded_len_varint(v);
        prop_assert_eq!(buf.len(), expected_len);
    }

    /// Zigzag encoding round-trips for arbitrary i32 values.
    #[test]
    fn zigzag32_round_trip(v in i32::MIN..=i32::MAX) {
        let encoded = wire::zigzag::zigzag_encode32(v);
        let decoded = wire::zigzag::zigzag_decode32(encoded);
        prop_assert_eq!(v, decoded);
    }

    /// Zigzag encoding round-trips for arbitrary i64 values.
    #[test]
    fn zigzag64_round_trip(v in i64::MIN..=i64::MAX) {
        let encoded = wire::zigzag::zigzag_encode64(v);
        let decoded = wire::zigzag::zigzag_decode64(encoded);
        prop_assert_eq!(v, decoded);
    }

    /// Zigzag-encoded values for small absolute values are compact.
    ///
    /// Key property of zigzag: `|n|` small → encoded value small.
    #[test]
    fn zigzag32_small_abs_stays_small(v in -256i32..=256i32) {
        let encoded = wire::zigzag::zigzag_encode32(v);
        // |v| ≤ 256, so encoded ≤ 512 (= 2 * |v| or 2 * |v| + 1).
        prop_assert!(encoded <= 512, "encoded {v} → {encoded}, expected ≤ 512");
    }

    /// Length-delimited byte sequences round-trip correctly.
    #[test]
    fn length_delimited_round_trip(
        bytes in proptest::collection::vec(0u8..=255u8, 0..4096)
    ) {
        let mut buf: Vec<u8> = Vec::new();
        wire::length_delimited::encode_length_delimited(&bytes, &mut buf);
        let (payload, consumed) = wire::length_delimited::decode_length_delimited(&buf)
            .expect("decode_length_delimited should succeed for valid encoding");
        prop_assert_eq!(bytes.as_slice(), payload);
        prop_assert_eq!(buf.len(), consumed);
    }

    /// Encoded length of length-delimited fields matches actual bytes written.
    #[test]
    fn length_delimited_encoded_len_matches_actual(
        bytes in proptest::collection::vec(0u8..=255u8, 0..4096)
    ) {
        let mut buf: Vec<u8> = Vec::new();
        wire::length_delimited::encode_length_delimited(&bytes, &mut buf);
        let expected_len = wire::length_delimited::encoded_len_length_delimited(bytes.len());
        prop_assert_eq!(buf.len(), expected_len);
    }

    /// Field tags round-trip for all valid field numbers and wire types.
    #[test]
    fn tag_round_trip(
        field_number in 1u32..=wire::MAX_FIELD_NUMBER,
        wire_type_byte in 0u32..=5u32,
    ) {
        let wire_type = match wire_type_byte {
            0 => wire::WireType::Varint,
            1 => wire::WireType::I64,
            2 => wire::WireType::Len,
            3 => wire::WireType::SGroup,
            4 => wire::WireType::EGroup,
            5 => wire::WireType::I32,
            _ => unreachable!(),
        };
        let mut buf: Vec<u8> = Vec::new();
        wire::tag::encode_tag(field_number, wire_type, &mut buf)
            .expect("encode_tag should succeed for valid field_number");
        let (tag, consumed) = wire::tag::decode_tag(&buf)
            .expect("decode_tag should succeed for valid encoding");
        prop_assert_eq!(tag.field_number, field_number);
        prop_assert_eq!(tag.wire_type, wire_type);
        prop_assert_eq!(buf.len(), consumed);
    }

    /// EncodeBuffer/DecodeBuffer varint round-trip using the buffer API.
    #[test]
    fn buf_varint_round_trip(v in 0u64..=u64::MAX) {
        let mut enc = wire::EncodeBuffer::new();
        enc.write_varint(v);
        let bytes = enc.into_vec();
        let mut dec = wire::DecodeBuffer::new(&bytes);
        let decoded = dec.read_varint()
            .expect("read_varint should succeed for valid encoding");
        prop_assert_eq!(v, decoded);
        prop_assert!(dec.is_empty());
    }

    /// EncodeBuffer/DecodeBuffer length-delimited round-trip using buffer API.
    #[test]
    fn buf_length_delimited_round_trip(
        bytes in proptest::collection::vec(0u8..=255u8, 0..1024)
    ) {
        let mut enc = wire::EncodeBuffer::new();
        enc.write_length_delimited(&bytes);
        let buf_bytes = enc.into_vec();
        let mut dec = wire::DecodeBuffer::new(&buf_bytes);
        let decoded = dec.read_length_delimited()
            .expect("read_length_delimited should succeed for valid encoding");
        prop_assert_eq!(bytes.as_slice(), decoded);
        prop_assert!(dec.is_empty());
    }
}
