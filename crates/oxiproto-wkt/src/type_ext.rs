#![forbid(unsafe_code)]
//! Extension traits for `prost_types::Type` and `prost_types::Enum`.
//!
//! Provides ergonomic construction and access methods for protobuf message
//! type descriptors and enum type descriptors.

use prost_types::{Enum, EnumValue, Field, Type};

/// Extension methods for [`prost_types::Type`] (message type descriptor).
pub trait TypeExt {
    /// Create a `Type` with the given fully-qualified name and no fields.
    #[allow(clippy::new_ret_no_self)]
    fn new(name: impl Into<String>) -> Type;

    /// Return the fully-qualified name of this message type.
    fn name(&self) -> &str;

    /// Return the fields defined in this message type.
    fn fields(&self) -> &[Field];

    /// Return the names of `oneof` groups defined in this message type.
    fn oneofs(&self) -> &[String];
}

impl TypeExt for Type {
    fn new(name: impl Into<String>) -> Type {
        Type {
            name: name.into(),
            fields: Vec::new(),
            oneofs: Vec::new(),
            options: Vec::new(),
            source_context: None,
            syntax: 0,
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn fields(&self) -> &[Field] {
        &self.fields
    }

    fn oneofs(&self) -> &[String] {
        &self.oneofs
    }
}

/// Extension methods for [`prost_types::Enum`] (enum type descriptor).
pub trait EnumTypeExt {
    /// Create an `Enum` with the given fully-qualified name and no values.
    #[allow(clippy::new_ret_no_self)]
    fn new(name: impl Into<String>) -> Enum;

    /// Return the fully-qualified name of this enum type.
    fn name(&self) -> &str;

    /// Return the enum values defined in this type.
    fn values(&self) -> &[EnumValue];
}

impl EnumTypeExt for Enum {
    fn new(name: impl Into<String>) -> Enum {
        Enum {
            name: name.into(),
            enumvalue: Vec::new(),
            options: Vec::new(),
            source_context: None,
            syntax: 0,
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn values(&self) -> &[EnumValue] {
        &self.enumvalue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_new_and_name() {
        let t = Type::new("google.protobuf.Timestamp");
        assert_eq!(t.name(), "google.protobuf.Timestamp");
        assert!(t.fields().is_empty());
        assert!(t.oneofs().is_empty());
    }

    #[test]
    fn enum_new_and_name() {
        let e = Enum::new("google.protobuf.NullValue");
        assert_eq!(e.name(), "google.protobuf.NullValue");
        assert!(e.values().is_empty());
    }
}
