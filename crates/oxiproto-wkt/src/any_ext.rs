//! Extension trait for `google.protobuf.Any`.
//!
//! `Any` wraps an arbitrary protobuf message together with a type URL that
//! identifies the message type.

use prost::Message;
use prost_types::Any;

/// Extension methods for [`prost_types::Any`].
pub trait AnyExt {
    /// Pack a message into an `Any`.
    ///
    /// The `type_url` is set to `"type.googleapis.com/{full_name}"` where
    /// `full_name` is obtained from the [`prost::Name`] trait.
    fn pack<T: Message + prost::Name>(msg: &T) -> Self;

    /// Pack a message with a custom type URL prefix.
    ///
    /// The final type URL will be `"{prefix}/{full_name}"`.
    fn pack_with_prefix<T: Message + prost::Name>(msg: &T, prefix: &str) -> Self;

    /// Attempt to unpack the `Any` into a concrete message type.
    ///
    /// This checks that the `type_url` ends with the expected type name
    /// before decoding.
    ///
    /// # Errors
    ///
    /// Returns `None` if the type URL does not match the expected type, or
    /// if decoding fails.
    fn unpack<T: Message + prost::Name + Default>(&self) -> Option<T>;

    /// Check whether this `Any` contains a message of the given type.
    fn is<T: prost::Name>(&self) -> bool;

    /// Returns the type name portion of the type URL (after the last `/`).
    fn type_name(&self) -> &str;
}

impl AnyExt for Any {
    fn pack<T: Message + prost::Name>(msg: &T) -> Self {
        Self::pack_with_prefix(msg, "type.googleapis.com")
    }

    fn pack_with_prefix<T: Message + prost::Name>(msg: &T, prefix: &str) -> Self {
        let full_name = T::full_name();
        let type_url = format!("{prefix}/{full_name}");
        let value = msg.encode_to_vec();
        Any { type_url, value }
    }

    fn unpack<T: Message + prost::Name + Default>(&self) -> Option<T> {
        if !self.is::<T>() {
            return None;
        }
        T::decode(self.value.as_slice()).ok()
    }

    fn is<T: prost::Name>(&self) -> bool {
        let expected = T::full_name();
        self.type_name() == expected
    }

    fn type_name(&self) -> &str {
        self.type_url.rsplit('/').next().unwrap_or(&self.type_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_name_extraction() {
        let any = Any {
            type_url: "type.googleapis.com/google.protobuf.Timestamp".to_string(),
            value: vec![],
        };
        assert_eq!(any.type_name(), "google.protobuf.Timestamp");
    }

    #[test]
    fn type_name_no_slash() {
        let any = Any {
            type_url: "SomeType".to_string(),
            value: vec![],
        };
        assert_eq!(any.type_name(), "SomeType");
    }

    #[test]
    fn pack_timestamp() {
        use prost_types::Timestamp;
        let ts = Timestamp {
            seconds: 1700000000,
            nanos: 0,
        };
        let any = Any::pack(&ts);
        assert!(any.type_url.ends_with("google.protobuf.Timestamp"));
        assert!(!any.value.is_empty());

        // Unpack should round-trip
        let unpacked: Option<Timestamp> = any.unpack();
        assert_eq!(unpacked, Some(ts));
    }

    #[test]
    fn unpack_wrong_type() {
        use prost_types::{Duration, Timestamp};
        let ts = Timestamp {
            seconds: 100,
            nanos: 0,
        };
        let any = Any::pack(&ts);
        let result: Option<Duration> = any.unpack();
        // type_name mismatch → None
        assert!(result.is_none());
    }

    #[test]
    fn is_check() {
        use prost_types::Timestamp;
        let ts = Timestamp {
            seconds: 0,
            nanos: 0,
        };
        let any = Any::pack(&ts);
        assert!(any.is::<Timestamp>());
    }
}
