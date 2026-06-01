//! Length-delimited field encoding and decoding.
//!
//! Length-delimited fields use a varint length prefix followed by that many
//! bytes of payload. This encoding is used for `string`, `bytes`, embedded
//! messages, and packed repeated fields.

use super::varint::{decode_varint, encode_varint};
use super::WireError;
use prost::alloc::vec::Vec;

/// Encode a length-delimited value: varint length prefix + raw bytes.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::encode_length_delimited;
///
/// let mut buf = Vec::new();
/// encode_length_delimited(b"hello", &mut buf);
/// assert_eq!(buf, &[5, b'h', b'e', b'l', b'l', b'o']);
/// ```
pub fn encode_length_delimited(data: &[u8], buf: &mut Vec<u8>) {
    encode_varint(data.len() as u64, buf);
    buf.extend_from_slice(data);
}

/// Decode a length-delimited value from the beginning of `buf`.
///
/// Returns a slice of the payload and the total number of bytes consumed
/// (length prefix + payload).
///
/// # Errors
///
/// - [`WireError::UnexpectedEof`] if the buffer is empty or the length prefix
///   is truncated.
/// - [`WireError::TruncatedMessage`] if the declared length exceeds the
///   remaining bytes.
/// - [`WireError::Overflow`] if the length prefix is too large.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::decode_length_delimited;
///
/// let buf = [5, b'h', b'e', b'l', b'l', b'o'];
/// let (payload, consumed) = decode_length_delimited(&buf).unwrap();
/// assert_eq!(payload, b"hello");
/// assert_eq!(consumed, 6);
/// ```
pub fn decode_length_delimited(buf: &[u8]) -> Result<(&[u8], usize), WireError> {
    let (length, prefix_len) = decode_varint(buf)?;
    let length = length as usize;
    let remaining = &buf[prefix_len..];

    if remaining.len() < length {
        return Err(WireError::TruncatedMessage {
            declared: length,
            available: remaining.len(),
        });
    }

    Ok((&remaining[..length], prefix_len + length))
}

/// Encode a string as a length-delimited field.
///
/// This is a convenience wrapper that encodes a UTF-8 string as
/// length-delimited bytes.
pub fn encode_string(s: &str, buf: &mut Vec<u8>) {
    encode_length_delimited(s.as_bytes(), buf);
}

/// Decode a length-delimited field as a UTF-8 string.
///
/// # Errors
///
/// Returns [`WireError::InvalidUtf8`] if the bytes are not valid UTF-8.
pub fn decode_string(buf: &[u8]) -> Result<(&str, usize), WireError> {
    let (payload, consumed) = decode_length_delimited(buf)?;
    let s = core::str::from_utf8(payload).map_err(WireError::InvalidUtf8)?;
    Ok((s, consumed))
}

/// Compute the total encoded length of a length-delimited field.
///
/// This includes the varint length prefix plus the payload size.
pub fn encoded_len_length_delimited(data_len: usize) -> usize {
    super::varint::encoded_len_varint(data_len as u64) + data_len
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::alloc::vec;

    #[test]
    fn encode_decode_empty() {
        let mut buf = Vec::new();
        encode_length_delimited(b"", &mut buf);
        assert_eq!(buf, &[0]); // length=0
        let (payload, consumed) = decode_length_delimited(&buf).expect("decode");
        assert!(payload.is_empty());
        assert_eq!(consumed, 1);
    }

    #[test]
    fn encode_decode_hello() {
        let mut buf = Vec::new();
        encode_length_delimited(b"hello", &mut buf);
        assert_eq!(buf.len(), 6); // 1 byte length + 5 bytes payload
        let (payload, consumed) = decode_length_delimited(&buf).expect("decode");
        assert_eq!(payload, b"hello");
        assert_eq!(consumed, 6);
    }

    #[test]
    fn encode_decode_large() {
        let data = vec![0xABu8; 300];
        let mut buf = Vec::new();
        encode_length_delimited(&data, &mut buf);
        // 300 requires 2-byte varint
        assert_eq!(buf.len(), 2 + 300);
        let (payload, consumed) = decode_length_delimited(&buf).expect("decode");
        assert_eq!(payload, &data[..]);
        assert_eq!(consumed, 302);
    }

    #[test]
    fn decode_truncated_returns_error() {
        // Declare 10 bytes but only provide 3
        let buf = [10, 0x01, 0x02, 0x03];
        assert!(matches!(
            decode_length_delimited(&buf),
            Err(WireError::TruncatedMessage {
                declared: 10,
                available: 3,
            })
        ));
    }

    #[test]
    fn decode_empty_buf_returns_eof() {
        assert!(matches!(
            decode_length_delimited(&[]),
            Err(WireError::UnexpectedEof)
        ));
    }

    #[test]
    fn encode_decode_string() {
        let mut buf = Vec::new();
        encode_string("world", &mut buf);
        let (s, consumed) = decode_string(&buf).expect("decode");
        assert_eq!(s, "world");
        assert_eq!(consumed, 6);
    }

    #[test]
    fn decode_string_invalid_utf8() {
        let mut buf = Vec::new();
        encode_length_delimited(&[0xFF, 0xFE], &mut buf);
        assert!(matches!(
            decode_string(&buf),
            Err(WireError::InvalidUtf8(_))
        ));
    }

    #[test]
    fn encoded_len_calculation() {
        assert_eq!(encoded_len_length_delimited(0), 1); // 1 byte prefix + 0
        assert_eq!(encoded_len_length_delimited(5), 6); // 1 byte prefix + 5
        assert_eq!(encoded_len_length_delimited(127), 128); // 1 byte prefix + 127
        assert_eq!(encoded_len_length_delimited(128), 130); // 2 byte prefix + 128
    }
}
