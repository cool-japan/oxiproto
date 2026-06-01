//! Protobuf wire types.

use super::WireError;

/// The five wire types defined by the Protocol Buffers encoding specification.
///
/// Each field in a protobuf message is encoded with a tag that includes the
/// field number and one of these wire types, telling the decoder how to read
/// the value.
///
/// Reference: <https://protobuf.dev/programming-guides/encoding/#structure>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u32)]
pub enum WireType {
    /// Varint: variable-length integer.
    ///
    /// Used for `int32`, `int64`, `uint32`, `uint64`, `sint32`, `sint64`,
    /// `bool`, and `enum` fields.
    Varint = 0,
    /// 64-bit: fixed 8-byte value (little-endian).
    ///
    /// Used for `fixed64`, `sfixed64`, and `double`.
    I64 = 1,
    /// Length-delimited: a varint length prefix followed by that many bytes.
    ///
    /// Used for `string`, `bytes`, embedded messages, and packed repeated
    /// fields.
    Len = 2,
    /// Start group (deprecated in proto3).
    ///
    /// Groups are a legacy proto2 feature. The wire format uses a start-group
    /// tag and an end-group tag to delimit the group's fields.
    SGroup = 3,
    /// End group (deprecated in proto3).
    EGroup = 4,
    /// 32-bit: fixed 4-byte value (little-endian).
    ///
    /// Used for `fixed32`, `sfixed32`, and `float`.
    I32 = 5,
}

impl WireType {
    /// Convert a raw `u32` wire type value to a [`WireType`].
    ///
    /// # Errors
    ///
    /// Returns [`WireError::InvalidWireType`] if the value is not in `0..=5`.
    pub fn from_u32(value: u32) -> Result<Self, WireError> {
        match value {
            0 => Ok(WireType::Varint),
            1 => Ok(WireType::I64),
            2 => Ok(WireType::Len),
            3 => Ok(WireType::SGroup),
            4 => Ok(WireType::EGroup),
            5 => Ok(WireType::I32),
            other => Err(WireError::InvalidWireType(other)),
        }
    }

    /// Return the `u32` representation of this wire type.
    #[inline]
    pub fn value(self) -> u32 {
        self as u32
    }
}

impl core::fmt::Display for WireType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WireType::Varint => write!(f, "Varint(0)"),
            WireType::I64 => write!(f, "64-bit(1)"),
            WireType::Len => write!(f, "Len(2)"),
            WireType::SGroup => write!(f, "SGroup(3)"),
            WireType::EGroup => write!(f, "EGroup(4)"),
            WireType::I32 => write!(f, "32-bit(5)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::alloc::string::ToString;

    #[test]
    fn wire_type_round_trip_all() {
        for val in 0..=5u32 {
            let wt = WireType::from_u32(val).expect("valid wire type");
            assert_eq!(wt.value(), val);
        }
    }

    #[test]
    fn wire_type_invalid_6() {
        assert!(matches!(
            WireType::from_u32(6),
            Err(WireError::InvalidWireType(6))
        ));
    }

    #[test]
    fn wire_type_invalid_large() {
        assert!(WireType::from_u32(255).is_err());
    }

    #[test]
    fn wire_type_display() {
        assert_eq!(WireType::Varint.to_string(), "Varint(0)");
        assert_eq!(WireType::Len.to_string(), "Len(2)");
    }
}
