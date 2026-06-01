//! Fixed-width 32-bit and 64-bit encoding (little-endian).
//!
//! Used for protobuf `fixed32`, `sfixed32`, `float`, `fixed64`, `sfixed64`,
//! and `double` fields.

use super::WireError;
use prost::alloc::vec::Vec;

/// Encode a 32-bit value as 4 little-endian bytes and append to `buf`.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::encode_fixed32;
///
/// let mut buf = Vec::new();
/// encode_fixed32(0x01020304, &mut buf);
/// assert_eq!(buf, &[0x04, 0x03, 0x02, 0x01]);
/// ```
#[inline]
pub fn encode_fixed32(value: u32, buf: &mut Vec<u8>) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Decode a 32-bit little-endian value from the beginning of `buf`.
///
/// Returns `(value, 4)` on success.
///
/// # Errors
///
/// Returns [`WireError::UnexpectedEof`] if fewer than 4 bytes are available.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::decode_fixed32;
///
/// let buf = [0x04, 0x03, 0x02, 0x01];
/// let (val, consumed) = decode_fixed32(&buf).unwrap();
/// assert_eq!(val, 0x01020304);
/// assert_eq!(consumed, 4);
/// ```
#[inline]
pub fn decode_fixed32(buf: &[u8]) -> Result<(u32, usize), WireError> {
    if buf.len() < 4 {
        return Err(WireError::UnexpectedEof);
    }
    let bytes: [u8; 4] = [buf[0], buf[1], buf[2], buf[3]];
    Ok((u32::from_le_bytes(bytes), 4))
}

/// Encode a 64-bit value as 8 little-endian bytes and append to `buf`.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::encode_fixed64;
///
/// let mut buf = Vec::new();
/// encode_fixed64(1, &mut buf);
/// assert_eq!(buf, &[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
/// ```
#[inline]
pub fn encode_fixed64(value: u64, buf: &mut Vec<u8>) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Decode a 64-bit little-endian value from the beginning of `buf`.
///
/// Returns `(value, 8)` on success.
///
/// # Errors
///
/// Returns [`WireError::UnexpectedEof`] if fewer than 8 bytes are available.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::decode_fixed64;
///
/// let buf = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
/// let (val, consumed) = decode_fixed64(&buf).unwrap();
/// assert_eq!(val, 1);
/// assert_eq!(consumed, 8);
/// ```
#[inline]
pub fn decode_fixed64(buf: &[u8]) -> Result<(u64, usize), WireError> {
    if buf.len() < 8 {
        return Err(WireError::UnexpectedEof);
    }
    let bytes: [u8; 8] = [
        buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
    ];
    Ok((u64::from_le_bytes(bytes), 8))
}

/// Encode an `f32` as 4 little-endian bytes (IEEE 754).
#[inline]
pub fn encode_float(value: f32, buf: &mut Vec<u8>) {
    encode_fixed32(value.to_bits(), buf);
}

/// Decode an `f32` from 4 little-endian bytes (IEEE 754).
///
/// # Errors
///
/// Returns [`WireError::UnexpectedEof`] if fewer than 4 bytes are available.
#[inline]
pub fn decode_float(buf: &[u8]) -> Result<(f32, usize), WireError> {
    let (bits, consumed) = decode_fixed32(buf)?;
    Ok((f32::from_bits(bits), consumed))
}

/// Encode an `f64` as 8 little-endian bytes (IEEE 754).
#[inline]
pub fn encode_double(value: f64, buf: &mut Vec<u8>) {
    encode_fixed64(value.to_bits(), buf);
}

/// Decode an `f64` from 8 little-endian bytes (IEEE 754).
///
/// # Errors
///
/// Returns [`WireError::UnexpectedEof`] if fewer than 8 bytes are available.
#[inline]
pub fn decode_double(buf: &[u8]) -> Result<(f64, usize), WireError> {
    let (bits, consumed) = decode_fixed64(buf)?;
    Ok((f64::from_bits(bits), consumed))
}

/// Encode a signed 32-bit integer as `sfixed32` (little-endian reinterpret).
#[inline]
pub fn encode_sfixed32(value: i32, buf: &mut Vec<u8>) {
    encode_fixed32(value as u32, buf);
}

/// Decode a signed 32-bit integer from `sfixed32` encoding.
///
/// # Errors
///
/// Returns [`WireError::UnexpectedEof`] if fewer than 4 bytes are available.
#[inline]
pub fn decode_sfixed32(buf: &[u8]) -> Result<(i32, usize), WireError> {
    let (bits, consumed) = decode_fixed32(buf)?;
    Ok((bits as i32, consumed))
}

/// Encode a signed 64-bit integer as `sfixed64` (little-endian reinterpret).
#[inline]
pub fn encode_sfixed64(value: i64, buf: &mut Vec<u8>) {
    encode_fixed64(value as u64, buf);
}

/// Decode a signed 64-bit integer from `sfixed64` encoding.
///
/// # Errors
///
/// Returns [`WireError::UnexpectedEof`] if fewer than 8 bytes are available.
#[inline]
pub fn decode_sfixed64(buf: &[u8]) -> Result<(i64, usize), WireError> {
    let (bits, consumed) = decode_fixed64(buf)?;
    Ok((bits as i64, consumed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed32_round_trip() {
        let values = [0u32, 1, 255, 256, u32::MAX, 0xDEAD_BEEF];
        for &v in &values {
            let mut buf = Vec::new();
            encode_fixed32(v, &mut buf);
            assert_eq!(buf.len(), 4);
            let (decoded, consumed) = decode_fixed32(&buf).expect("decode");
            assert_eq!(decoded, v);
            assert_eq!(consumed, 4);
        }
    }

    #[test]
    fn fixed64_round_trip() {
        let values = [0u64, 1, u32::MAX as u64, u64::MAX, 0xDEAD_BEEF_CAFE_BABEu64];
        for &v in &values {
            let mut buf = Vec::new();
            encode_fixed64(v, &mut buf);
            assert_eq!(buf.len(), 8);
            let (decoded, consumed) = decode_fixed64(&buf).expect("decode");
            assert_eq!(decoded, v);
            assert_eq!(consumed, 8);
        }
    }

    #[test]
    fn float_round_trip() {
        let values = [0.0f32, 1.0, -1.0, f32::MIN, f32::MAX, f32::EPSILON];
        for &v in &values {
            let mut buf = Vec::new();
            encode_float(v, &mut buf);
            let (decoded, _) = decode_float(&buf).expect("decode");
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn float_nan_preserved() {
        let mut buf = Vec::new();
        encode_float(f32::NAN, &mut buf);
        let (decoded, _) = decode_float(&buf).expect("decode");
        assert!(decoded.is_nan());
    }

    #[test]
    fn double_round_trip() {
        let values = [
            0.0f64,
            1.0,
            -1.0,
            f64::MIN,
            f64::MAX,
            f64::EPSILON,
            core::f64::consts::PI,
        ];
        for &v in &values {
            let mut buf = Vec::new();
            encode_double(v, &mut buf);
            let (decoded, _) = decode_double(&buf).expect("decode");
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn sfixed32_round_trip() {
        let values = [0i32, 1, -1, i32::MIN, i32::MAX];
        for &v in &values {
            let mut buf = Vec::new();
            encode_sfixed32(v, &mut buf);
            let (decoded, consumed) = decode_sfixed32(&buf).expect("decode");
            assert_eq!(decoded, v);
            assert_eq!(consumed, 4);
        }
    }

    #[test]
    fn sfixed64_round_trip() {
        let values = [0i64, 1, -1, i64::MIN, i64::MAX];
        for &v in &values {
            let mut buf = Vec::new();
            encode_sfixed64(v, &mut buf);
            let (decoded, consumed) = decode_sfixed64(&buf).expect("decode");
            assert_eq!(decoded, v);
            assert_eq!(consumed, 8);
        }
    }

    #[test]
    fn decode_fixed32_eof() {
        assert!(matches!(
            decode_fixed32(&[0x01, 0x02]),
            Err(WireError::UnexpectedEof)
        ));
    }

    #[test]
    fn decode_fixed64_eof() {
        assert!(matches!(
            decode_fixed64(&[0x01, 0x02, 0x03, 0x04]),
            Err(WireError::UnexpectedEof)
        ));
    }

    #[test]
    fn little_endian_order() {
        let mut buf = Vec::new();
        encode_fixed32(0x01020304, &mut buf);
        assert_eq!(buf, &[0x04, 0x03, 0x02, 0x01]);
    }
}
