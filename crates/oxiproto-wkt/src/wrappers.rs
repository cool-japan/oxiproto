//! Extension traits for wrapper-like patterns and Value/Struct helpers.
//!
//! The protobuf well-known wrapper types (`google.protobuf.DoubleValue`, etc.)
//! are simple single-field messages. Since `prost_types` does not export them
//! directly, we define our own pure-Rust equivalents and provide convenience
//! constructors.

use prost_types::value::Kind;

// ---------------------------------------------------------------------------
// Native wrapper structs (matching google.protobuf.wrappers.proto)
// ---------------------------------------------------------------------------

/// Wrapper for a `double` value.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DoubleValue {
    /// The wrapped value.
    pub value: f64,
}

/// Wrapper for a `float` value.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FloatValue {
    /// The wrapped value.
    pub value: f32,
}

/// Wrapper for an `int64` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Int64Value {
    /// The wrapped value.
    pub value: i64,
}

/// Wrapper for a `uint64` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UInt64Value {
    /// The wrapped value.
    pub value: u64,
}

/// Wrapper for an `int32` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Int32Value {
    /// The wrapped value.
    pub value: i32,
}

/// Wrapper for a `uint32` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UInt32Value {
    /// The wrapped value.
    pub value: u32,
}

/// Wrapper for a `bool` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BoolValue {
    /// The wrapped value.
    pub value: bool,
}

/// Wrapper for a `string` value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StringValue {
    /// The wrapped value.
    pub value: String,
}

/// Wrapper for a `bytes` value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BytesValue {
    /// The wrapped value.
    pub value: Vec<u8>,
}

// ---------------------------------------------------------------------------
// WrapperExt trait
// ---------------------------------------------------------------------------

/// Convenience methods for protobuf wrapper types.
pub trait WrapperExt<T> {
    /// Wrap a value into the corresponding wrapper type.
    fn wrap(value: T) -> Self;
    /// Extract the wrapped value.
    fn unwrap_value(&self) -> T;
}

impl WrapperExt<f64> for DoubleValue {
    fn wrap(value: f64) -> Self {
        DoubleValue { value }
    }
    fn unwrap_value(&self) -> f64 {
        self.value
    }
}

impl WrapperExt<f32> for FloatValue {
    fn wrap(value: f32) -> Self {
        FloatValue { value }
    }
    fn unwrap_value(&self) -> f32 {
        self.value
    }
}

impl WrapperExt<i64> for Int64Value {
    fn wrap(value: i64) -> Self {
        Int64Value { value }
    }
    fn unwrap_value(&self) -> i64 {
        self.value
    }
}

impl WrapperExt<u64> for UInt64Value {
    fn wrap(value: u64) -> Self {
        UInt64Value { value }
    }
    fn unwrap_value(&self) -> u64 {
        self.value
    }
}

impl WrapperExt<i32> for Int32Value {
    fn wrap(value: i32) -> Self {
        Int32Value { value }
    }
    fn unwrap_value(&self) -> i32 {
        self.value
    }
}

impl WrapperExt<u32> for UInt32Value {
    fn wrap(value: u32) -> Self {
        UInt32Value { value }
    }
    fn unwrap_value(&self) -> u32 {
        self.value
    }
}

impl WrapperExt<bool> for BoolValue {
    fn wrap(value: bool) -> Self {
        BoolValue { value }
    }
    fn unwrap_value(&self) -> bool {
        self.value
    }
}

impl WrapperExt<String> for StringValue {
    fn wrap(value: String) -> Self {
        StringValue { value }
    }
    fn unwrap_value(&self) -> String {
        self.value.clone()
    }
}

impl WrapperExt<Vec<u8>> for BytesValue {
    fn wrap(value: Vec<u8>) -> Self {
        BytesValue { value }
    }
    fn unwrap_value(&self) -> Vec<u8> {
        self.value.clone()
    }
}

// ---------------------------------------------------------------------------
// ValueExt trait for prost_types::Value
// ---------------------------------------------------------------------------

/// Extension methods for [`prost_types::Value`] (part of `google.protobuf.Struct`).
pub trait ValueExt {
    /// Create a null value.
    fn null() -> Self;
    /// Create a boolean value.
    fn from_bool(b: bool) -> Self;
    /// Create a number value.
    fn from_f64(n: f64) -> Self;
    /// Create a string value.
    fn from_string(s: impl Into<String>) -> Self;

    /// Extract as a boolean, if the value is a boolean.
    fn as_bool(&self) -> Option<bool>;
    /// Extract as a number, if the value is a number.
    fn as_number(&self) -> Option<f64>;
    /// Extract as a string reference, if the value is a string.
    fn as_string(&self) -> Option<&str>;
    /// Returns `true` if the value is null.
    fn is_null(&self) -> bool;
}

impl ValueExt for prost_types::Value {
    fn null() -> Self {
        prost_types::Value {
            kind: Some(Kind::NullValue(0)),
        }
    }

    fn from_bool(b: bool) -> Self {
        prost_types::Value {
            kind: Some(Kind::BoolValue(b)),
        }
    }

    fn from_f64(n: f64) -> Self {
        prost_types::Value {
            kind: Some(Kind::NumberValue(n)),
        }
    }

    fn from_string(s: impl Into<String>) -> Self {
        prost_types::Value {
            kind: Some(Kind::StringValue(s.into())),
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match &self.kind {
            Some(Kind::BoolValue(b)) => Some(*b),
            _ => None,
        }
    }

    fn as_number(&self) -> Option<f64> {
        match &self.kind {
            Some(Kind::NumberValue(n)) => Some(*n),
            _ => None,
        }
    }

    fn as_string(&self) -> Option<&str> {
        match &self.kind {
            Some(Kind::StringValue(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    fn is_null(&self) -> bool {
        matches!(&self.kind, Some(Kind::NullValue(_)) | None)
    }
}

// ---------------------------------------------------------------------------
// StructExt trait for prost_types::Struct
// ---------------------------------------------------------------------------

/// Extension methods for [`prost_types::Struct`].
pub trait StructExt {
    /// Create an empty Struct.
    fn empty() -> Self;
    /// Insert a key-value pair into the struct.
    fn insert(&mut self, key: impl Into<String>, value: prost_types::Value);
    /// Get a value by key.
    fn get(&self, key: &str) -> Option<&prost_types::Value>;
    /// Returns the number of fields.
    fn len(&self) -> usize;
    /// Returns `true` if the struct has no fields.
    fn is_empty(&self) -> bool;
}

impl StructExt for prost_types::Struct {
    fn empty() -> Self {
        prost_types::Struct {
            fields: std::collections::BTreeMap::new(),
        }
    }

    fn insert(&mut self, key: impl Into<String>, value: prost_types::Value) {
        self.fields.insert(key.into(), value);
    }

    fn get(&self, key: &str) -> Option<&prost_types::Value> {
        self.fields.get(key)
    }

    fn len(&self) -> usize {
        self.fields.len()
    }

    fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_value_wrap_unwrap() {
        let wrapped = DoubleValue::wrap(12.5);
        assert!((wrapped.unwrap_value() - 12.5).abs() < f64::EPSILON);
    }

    #[test]
    fn int64_value_wrap_unwrap() {
        let wrapped = Int64Value::wrap(-42);
        assert_eq!(wrapped.unwrap_value(), -42);
    }

    #[test]
    fn bool_value_wrap_unwrap() {
        assert!(BoolValue::wrap(true).unwrap_value());
        assert!(!BoolValue::wrap(false).unwrap_value());
    }

    #[test]
    fn string_value_wrap_unwrap() {
        let wrapped = StringValue::wrap("hello".to_string());
        assert_eq!(wrapped.unwrap_value(), "hello");
    }

    #[test]
    fn bytes_value_wrap_unwrap() {
        let wrapped = BytesValue::wrap(vec![1, 2, 3]);
        assert_eq!(wrapped.unwrap_value(), vec![1, 2, 3]);
    }

    #[test]
    fn value_null() {
        let v = prost_types::Value::null();
        assert!(v.is_null());
        assert!(v.as_bool().is_none());
    }

    #[test]
    fn value_bool() {
        let v = prost_types::Value::from_bool(true);
        assert_eq!(v.as_bool(), Some(true));
        assert!(!v.is_null());
    }

    #[test]
    fn value_number() {
        let v = prost_types::Value::from_f64(42.0);
        assert_eq!(v.as_number(), Some(42.0));
    }

    #[test]
    fn value_string() {
        let v = prost_types::Value::from_string("test");
        assert_eq!(v.as_string(), Some("test"));
    }

    #[test]
    fn struct_operations() {
        let mut s = prost_types::Struct::empty();
        assert!(s.is_empty());

        s.insert("name", prost_types::Value::from_string("Alice"));
        s.insert("age", prost_types::Value::from_f64(30.0));

        assert_eq!(s.len(), 2);
        assert!(!s.is_empty());

        let name = s.get("name");
        assert!(name.is_some());
        assert_eq!(name.and_then(|v| v.as_string()), Some("Alice"));

        assert!(s.get("missing").is_none());
    }
}
