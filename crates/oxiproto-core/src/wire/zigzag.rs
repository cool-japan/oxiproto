//! ZigZag encoding for signed integers.
//!
//! ZigZag encoding maps signed integers to unsigned integers so that small
//! absolute values produce small encoded values. This is used by protobuf's
//! `sint32` and `sint64` types.
//!
//! The mapping is:
//! - `0`  → `0`
//! - `-1` → `1`
//! - `1`  → `2`
//! - `-2` → `3`
//! - `2`  → `4`
//! - ...
//!
//! Formula: `encode(n) = (n << 1) ^ (n >> 31)` for 32-bit,
//!          `encode(n) = (n << 1) ^ (n >> 63)` for 64-bit.

/// ZigZag-encode a signed 32-bit integer to an unsigned 32-bit integer.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::zigzag_encode32;
///
/// assert_eq!(zigzag_encode32(0), 0);
/// assert_eq!(zigzag_encode32(-1), 1);
/// assert_eq!(zigzag_encode32(1), 2);
/// assert_eq!(zigzag_encode32(-2), 3);
/// ```
#[inline]
pub fn zigzag_encode32(n: i32) -> u32 {
    ((n << 1) ^ (n >> 31)) as u32
}

/// ZigZag-decode an unsigned 32-bit integer to a signed 32-bit integer.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::zigzag_decode32;
///
/// assert_eq!(zigzag_decode32(0), 0);
/// assert_eq!(zigzag_decode32(1), -1);
/// assert_eq!(zigzag_decode32(2), 1);
/// assert_eq!(zigzag_decode32(3), -2);
/// ```
#[inline]
pub fn zigzag_decode32(n: u32) -> i32 {
    ((n >> 1) as i32) ^ -((n & 1) as i32)
}

/// ZigZag-encode a signed 64-bit integer to an unsigned 64-bit integer.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::zigzag_encode64;
///
/// assert_eq!(zigzag_encode64(0), 0);
/// assert_eq!(zigzag_encode64(-1), 1);
/// assert_eq!(zigzag_encode64(1), 2);
/// ```
#[inline]
pub fn zigzag_encode64(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

/// ZigZag-decode an unsigned 64-bit integer to a signed 64-bit integer.
///
/// # Example
///
/// ```
/// use oxiproto_core::wire::zigzag_decode64;
///
/// assert_eq!(zigzag_decode64(0), 0);
/// assert_eq!(zigzag_decode64(1), -1);
/// assert_eq!(zigzag_decode64(2), 1);
/// ```
#[inline]
pub fn zigzag_decode64(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag32_known_values() {
        // From the protobuf spec table:
        assert_eq!(zigzag_encode32(0), 0);
        assert_eq!(zigzag_encode32(-1), 1);
        assert_eq!(zigzag_encode32(1), 2);
        assert_eq!(zigzag_encode32(-2), 3);
        assert_eq!(zigzag_encode32(2), 4);
        assert_eq!(zigzag_encode32(2147483647), 4294967294); // i32::MAX
        assert_eq!(zigzag_encode32(-2147483648), 4294967295); // i32::MIN
    }

    #[test]
    fn zigzag32_round_trip() {
        let values = [
            0i32,
            1,
            -1,
            2,
            -2,
            127,
            -128,
            255,
            -256,
            i32::MAX,
            i32::MIN,
            i32::MAX - 1,
            i32::MIN + 1,
        ];
        for &v in &values {
            let encoded = zigzag_encode32(v);
            let decoded = zigzag_decode32(encoded);
            assert_eq!(decoded, v, "round-trip failed for {v}");
        }
    }

    #[test]
    fn zigzag64_known_values() {
        assert_eq!(zigzag_encode64(0), 0);
        assert_eq!(zigzag_encode64(-1), 1);
        assert_eq!(zigzag_encode64(1), 2);
        assert_eq!(zigzag_encode64(-2), 3);
        assert_eq!(zigzag_encode64(2147483647), 4294967294);
        assert_eq!(zigzag_encode64(-2147483648), 4294967295);
    }

    #[test]
    fn zigzag64_extremes() {
        assert_eq!(zigzag_encode64(i64::MAX), u64::MAX - 1);
        assert_eq!(zigzag_encode64(i64::MIN), u64::MAX);
    }

    #[test]
    fn zigzag64_round_trip() {
        let values = [
            0i64,
            1,
            -1,
            2,
            -2,
            127,
            -128,
            i32::MAX as i64,
            i32::MIN as i64,
            i64::MAX,
            i64::MIN,
            i64::MAX - 1,
            i64::MIN + 1,
        ];
        for &v in &values {
            let encoded = zigzag_encode64(v);
            let decoded = zigzag_decode64(encoded);
            assert_eq!(decoded, v, "round-trip failed for {v}");
        }
    }

    #[test]
    fn zigzag_small_negatives_produce_small_encoded() {
        // Key property: small absolute values → small encoded values
        for i in 0..100i32 {
            let pos_enc = zigzag_encode32(i);
            let neg_enc = zigzag_encode32(-i);
            // Both should be small (near 2*i)
            assert!(pos_enc <= 200, "positive {i} encoded to {pos_enc}");
            assert!(neg_enc <= 200, "negative {i} encoded to {neg_enc}");
        }
    }
}
