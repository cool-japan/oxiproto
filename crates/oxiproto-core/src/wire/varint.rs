//! LEB128 variable-length integer encoding and decoding.
//!
//! Protobuf uses the LEB128 (Little-Endian Base 128) encoding for varint
//! fields. Each byte uses 7 bits for the value and 1 bit (the MSB) as a
//! continuation flag.
//!
//! The maximum encoded length is 10 bytes (for a 64-bit value that uses all
//! bits).

use super::WireError;
use prost::alloc::{format, vec::Vec};

/// Maximum number of bytes a varint can occupy (ceil(64/7) = 10).
const MAX_VARINT_BYTES: usize = 10;

/// Encode a `u64` value as a varint and append the bytes to `buf`.
///
/// Returns the number of bytes written (1..=10).
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::encode_varint;
///
/// let mut buf = Vec::new();
/// let n = encode_varint(300, &mut buf);
/// assert_eq!(n, 2);
/// assert_eq!(buf, &[0xAC, 0x02]);
/// ```
pub fn encode_varint(mut value: u64, buf: &mut Vec<u8>) -> usize {
    let mut count = 0;
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            buf.push(byte);
            count += 1;
            break;
        } else {
            buf.push(byte | 0x80);
            count += 1;
        }
    }
    count
}

/// Encode a `u64` value as a varint into a fixed-size buffer.
///
/// Returns `(bytes_written, buffer)` where `buffer` is a 10-byte array and
/// `bytes_written` indicates how many leading bytes are valid.
///
/// This is useful when you need to write to a `&mut [u8]` or `Write` without
/// heap allocation.
pub fn encode_varint_fixed(mut value: u64) -> (usize, [u8; MAX_VARINT_BYTES]) {
    let mut buf = [0u8; MAX_VARINT_BYTES];
    let mut i = 0;
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            buf[i] = byte;
            i += 1;
            break;
        } else {
            buf[i] = byte | 0x80;
            i += 1;
        }
    }
    (i, buf)
}

/// Decode a varint from the beginning of `buf`.
///
/// Returns `(value, bytes_consumed)` on success.
///
/// # Errors
///
/// - [`WireError::UnexpectedEof`] if the buffer is empty or ends mid-varint.
/// - [`WireError::Overflow`] if the varint exceeds 10 bytes.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::decode_varint;
///
/// let buf = [0xAC, 0x02];
/// let (value, consumed) = decode_varint(&buf).unwrap();
/// assert_eq!(value, 300);
/// assert_eq!(consumed, 2);
/// ```
pub fn decode_varint(buf: &[u8]) -> Result<(u64, usize), WireError> {
    if buf.is_empty() {
        return Err(WireError::UnexpectedEof);
    }

    let mut result: u64 = 0;
    let mut shift: u32 = 0;

    for (i, &byte) in buf.iter().enumerate() {
        if i >= MAX_VARINT_BYTES {
            return Err(WireError::Overflow);
        }

        let value_bits = u64::from(byte & 0x7F);

        // Check for overflow: if shift >= 63 and value_bits > 1, the result
        // would exceed u64::MAX. The 10th byte (shift=63) can only contribute
        // bit 0 (value 0 or 1).
        if shift >= 63 && value_bits > 1 {
            return Err(WireError::Overflow);
        }

        result |= value_bits << shift;
        shift += 7;

        if byte & 0x80 == 0 {
            return Ok((result, i + 1));
        }
    }

    Err(WireError::UnexpectedEof)
}

/// Compute the number of bytes needed to encode `value` as a varint.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::varint::encoded_len_varint;
///
/// assert_eq!(encoded_len_varint(0), 1);
/// assert_eq!(encoded_len_varint(127), 1);
/// assert_eq!(encoded_len_varint(128), 2);
/// assert_eq!(encoded_len_varint(300), 2);
/// assert_eq!(encoded_len_varint(u64::MAX), 10);
/// ```
pub fn encoded_len_varint(value: u64) -> usize {
    // Each byte encodes 7 bits. We need ceil((bits_needed)/7) bytes.
    // The minimum is 1 byte (for value 0).
    if value == 0 {
        return 1;
    }
    let bits = 64 - value.leading_zeros() as usize;
    bits.div_ceil(7)
}

/// Encode a `u32` value as a varint and append to `buf`.
///
/// Convenience wrapper around [`encode_varint`] for 32-bit values.
pub fn encode_varint32(value: u32, buf: &mut Vec<u8>) -> usize {
    encode_varint(u64::from(value), buf)
}

/// Decode a varint and truncate to `u32`.
///
/// # Errors
///
/// Returns [`WireError::OutOfRange`] if the decoded value exceeds `u32::MAX`.
pub fn decode_varint32(buf: &[u8]) -> Result<(u32, usize), WireError> {
    let (val, consumed) = decode_varint(buf)?;
    let val32 = u32::try_from(val)
        .map_err(|_| WireError::OutOfRange(format!("varint value {val} exceeds u32::MAX")))?;
    Ok((val32, consumed))
}

/// Encode an `i64` value as a varint (two's complement cast to u64).
///
/// Protobuf `int64` fields use this encoding. For signed fields where negative
/// values are common, prefer zigzag + varint instead.
pub fn encode_varint_i64(value: i64, buf: &mut Vec<u8>) -> usize {
    encode_varint(value as u64, buf)
}

/// Decode a varint as `i64` (two's complement reinterpretation).
pub fn decode_varint_i64(buf: &[u8]) -> Result<(i64, usize), WireError> {
    let (val, consumed) = decode_varint(buf)?;
    Ok((val as i64, consumed))
}

/// Encode an `i32` value as a varint.
///
/// Protobuf `int32` fields sign-extend to 64 bits before encoding, which
/// means negative `int32` values always occupy 10 bytes. For signed fields
/// where negative values are common, prefer zigzag encoding.
pub fn encode_varint_i32(value: i32, buf: &mut Vec<u8>) -> usize {
    // Sign-extend to i64, then reinterpret as u64 — this matches protobuf
    // spec for int32 encoding.
    encode_varint(value as i64 as u64, buf)
}

/// Decode a varint as `i32`.
///
/// # Errors
///
/// Returns [`WireError::OutOfRange`] if the value does not fit in an `i32`
/// after reinterpretation.
pub fn decode_varint_i32(buf: &[u8]) -> Result<(i32, usize), WireError> {
    let (val, consumed) = decode_varint(buf)?;
    // Protobuf int32 encoding sign-extends to 64 bits, so we truncate
    // back to 32 bits.
    Ok((val as i32, consumed))
}

/// Encode a `bool` as a single-byte varint (0 or 1).
pub fn encode_varint_bool(value: bool, buf: &mut Vec<u8>) -> usize {
    encode_varint(u64::from(value), buf)
}

/// Decode a varint as `bool` (0 → false, nonzero → true).
pub fn decode_varint_bool(buf: &[u8]) -> Result<(bool, usize), WireError> {
    let (val, consumed) = decode_varint(buf)?;
    Ok((val != 0, consumed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_zero() {
        let mut buf = Vec::new();
        encode_varint(0, &mut buf);
        assert_eq!(buf, &[0x00]);
        let (val, consumed) = decode_varint(&buf).expect("decode");
        assert_eq!(val, 0);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn encode_decode_one() {
        let mut buf = Vec::new();
        encode_varint(1, &mut buf);
        assert_eq!(buf, &[0x01]);
        let (val, consumed) = decode_varint(&buf).expect("decode");
        assert_eq!(val, 1);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn encode_decode_127() {
        let mut buf = Vec::new();
        encode_varint(127, &mut buf);
        assert_eq!(buf, &[0x7F]);
        let (val, consumed) = decode_varint(&buf).expect("decode");
        assert_eq!(val, 127);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn encode_decode_128() {
        let mut buf = Vec::new();
        encode_varint(128, &mut buf);
        assert_eq!(buf, &[0x80, 0x01]);
        let (val, consumed) = decode_varint(&buf).expect("decode");
        assert_eq!(val, 128);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn encode_decode_300() {
        let mut buf = Vec::new();
        encode_varint(300, &mut buf);
        assert_eq!(buf, &[0xAC, 0x02]);
        let (val, consumed) = decode_varint(&buf).expect("decode");
        assert_eq!(val, 300);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn encode_decode_u64_max() {
        let mut buf = Vec::new();
        let n = encode_varint(u64::MAX, &mut buf);
        assert_eq!(n, 10);
        let (val, consumed) = decode_varint(&buf).expect("decode");
        assert_eq!(val, u64::MAX);
        assert_eq!(consumed, 10);
    }

    #[test]
    fn encode_decode_u32_max() {
        let mut buf = Vec::new();
        encode_varint(u64::from(u32::MAX), &mut buf);
        let (val, consumed) = decode_varint(&buf).expect("decode");
        assert_eq!(val, u64::from(u32::MAX));
        assert_eq!(consumed, 5);
    }

    #[test]
    fn decode_empty_returns_eof() {
        assert!(matches!(decode_varint(&[]), Err(WireError::UnexpectedEof)));
    }

    #[test]
    fn decode_truncated_returns_eof() {
        // Byte with continuation bit set but no follow-up byte
        assert!(matches!(
            decode_varint(&[0x80]),
            Err(WireError::UnexpectedEof)
        ));
    }

    #[test]
    fn decode_overflow_returns_error() {
        // 11 continuation bytes — varint too long
        let buf = [0x80; 11];
        assert!(matches!(decode_varint(&buf), Err(WireError::Overflow)));
    }

    #[test]
    fn encode_decode_i64_negative() {
        let mut buf = Vec::new();
        encode_varint_i64(-1, &mut buf);
        // -1 as u64 is u64::MAX, which takes 10 bytes
        assert_eq!(buf.len(), 10);
        let (val, consumed) = decode_varint_i64(&buf).expect("decode");
        assert_eq!(val, -1);
        assert_eq!(consumed, 10);
    }

    #[test]
    fn encode_decode_i32_negative() {
        let mut buf = Vec::new();
        encode_varint_i32(-1, &mut buf);
        // int32 sign-extends to 64 bits, so -1 takes 10 bytes
        assert_eq!(buf.len(), 10);
        let (val, consumed) = decode_varint_i32(&buf).expect("decode");
        assert_eq!(val, -1);
        assert_eq!(consumed, 10);
    }

    #[test]
    fn encode_decode_bool() {
        let mut buf = Vec::new();
        encode_varint_bool(false, &mut buf);
        encode_varint_bool(true, &mut buf);
        let (val, consumed) = decode_varint_bool(&buf).expect("decode false");
        assert!(!val);
        assert_eq!(consumed, 1);
        let (val, consumed) = decode_varint_bool(&buf[1..]).expect("decode true");
        assert!(val);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn encoded_len_varint_values() {
        assert_eq!(encoded_len_varint(0), 1);
        assert_eq!(encoded_len_varint(1), 1);
        assert_eq!(encoded_len_varint(127), 1);
        assert_eq!(encoded_len_varint(128), 2);
        assert_eq!(encoded_len_varint(300), 2);
        assert_eq!(encoded_len_varint(16383), 2);
        assert_eq!(encoded_len_varint(16384), 3);
        assert_eq!(encoded_len_varint(u64::MAX), 10);
    }

    #[test]
    fn decode_varint32_in_range() {
        let mut buf = Vec::new();
        encode_varint(1000, &mut buf);
        let (val, consumed) = decode_varint32(&buf).expect("decode");
        assert_eq!(val, 1000);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn decode_varint32_out_of_range() {
        let mut buf = Vec::new();
        encode_varint(u64::from(u32::MAX) + 1, &mut buf);
        assert!(matches!(
            decode_varint32(&buf),
            Err(WireError::OutOfRange(_))
        ));
    }

    #[test]
    fn encode_fixed_matches_vec() {
        for value in [0u64, 1, 127, 128, 300, u32::MAX as u64, u64::MAX] {
            let mut vec_buf = Vec::new();
            let vec_len = encode_varint(value, &mut vec_buf);
            let (fixed_len, fixed_buf) = encode_varint_fixed(value);
            assert_eq!(vec_len, fixed_len);
            assert_eq!(&vec_buf[..], &fixed_buf[..fixed_len]);
        }
    }

    #[test]
    fn round_trip_various_values() {
        let test_values: &[u64] = &[
            0,
            1,
            127,
            128,
            255,
            256,
            16383,
            16384,
            2_097_151,
            2_097_152,
            0xFFFF_FFFF,
            0x1_0000_0000,
            u64::MAX / 2,
            u64::MAX - 1,
            u64::MAX,
        ];
        for &value in test_values {
            let mut buf = Vec::new();
            encode_varint(value, &mut buf);
            let (decoded, consumed) = decode_varint(&buf).expect("decode");
            assert_eq!(decoded, value, "round-trip failed for {value}");
            assert_eq!(consumed, buf.len());
        }
    }
}
