//! Dynamic field value types for native reflection.
//!
//! [`Value`] is the runtime representation of a single protobuf field value,
//! and [`MapKey`] is the restricted set of scalar types that may appear as a
//! `map<K, V>` key. The shapes mirror `prost_reflect::Value` / `MapKey` so
//! callers can move between the two paths with minimal friction.

use std::collections::HashMap;

use super::dynamic::DynamicMessage;

/// A dynamically-typed protobuf field value.
///
/// Each variant corresponds to a protobuf scalar type, a nested message, an
/// enum value (stored by its integer number), a repeated field (`List`), or a
/// map field (`Map`). For repeated and map fields the *element* value is
/// stored using the same `Value` enum.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A `double` value.
    F64(f64),
    /// A `float` value.
    F32(f32),
    /// An `int32`, `sint32`, or `sfixed32` value.
    I32(i32),
    /// An `int64`, `sint64`, or `sfixed64` value.
    I64(i64),
    /// A `uint32` or `fixed32` value.
    U32(u32),
    /// A `uint64` or `fixed64` value.
    U64(u64),
    /// A `bool` value.
    Bool(bool),
    /// A `string` value.
    String(String),
    /// A `bytes` value.
    Bytes(Vec<u8>),
    /// An enum value, stored as its integer number.
    EnumNumber(i32),
    /// A nested message value.
    Message(Box<DynamicMessage>),
    /// A repeated field value.
    List(Vec<Value>),
    /// A map field value.
    Map(HashMap<MapKey, Value>),
}

/// The key of a protobuf `map<K, V>` field.
///
/// Protobuf restricts map keys to integral, boolean, and string scalar types.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MapKey {
    /// A `string` key.
    String(String),
    /// An `int32`, `sint32`, or `sfixed32` key.
    I32(i32),
    /// An `int64`, `sint64`, or `sfixed64` key.
    I64(i64),
    /// A `uint32` or `fixed32` key.
    U32(u32),
    /// A `uint64` or `fixed64` key.
    U64(u64),
    /// A `bool` key.
    Bool(bool),
}

impl Value {
    /// Returns `true` if this value equals the default (zero/empty) value for
    /// its variant.
    ///
    /// This is used to implement proto3 default-value omission on encode: a
    /// singular field whose value is the default is not written to the wire.
    pub fn is_default(&self) -> bool {
        match self {
            Value::F64(v) => *v == 0.0,
            Value::F32(v) => *v == 0.0,
            Value::I32(v) => *v == 0,
            Value::I64(v) => *v == 0,
            Value::U32(v) => *v == 0,
            Value::U64(v) => *v == 0,
            Value::Bool(v) => !*v,
            Value::String(s) => s.is_empty(),
            Value::Bytes(b) => b.is_empty(),
            Value::EnumNumber(n) => *n == 0,
            Value::Message(_) => false,
            Value::List(l) => l.is_empty(),
            Value::Map(m) => m.is_empty(),
        }
    }

    /// Borrow this value as a `f64`, if it is one.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::F64(v) => Some(*v),
            _ => None,
        }
    }

    /// Borrow this value as a `f32`, if it is one.
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Value::F32(v) => Some(*v),
            _ => None,
        }
    }

    /// Borrow this value as an `i32`, if it is one.
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Value::I32(v) => Some(*v),
            _ => None,
        }
    }

    /// Borrow this value as an `i64`, if it is one.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::I64(v) => Some(*v),
            _ => None,
        }
    }

    /// Borrow this value as a `u32`, if it is one.
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Value::U32(v) => Some(*v),
            _ => None,
        }
    }

    /// Borrow this value as a `u64`, if it is one.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::U64(v) => Some(*v),
            _ => None,
        }
    }

    /// Borrow this value as a `bool`, if it is one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Borrow this value as a `&str`, if it is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Borrow this value as a byte slice, if it is `bytes`.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b.as_slice()),
            _ => None,
        }
    }

    /// Borrow this value as an enum number, if it is one.
    pub fn as_enum_number(&self) -> Option<i32> {
        match self {
            Value::EnumNumber(n) => Some(*n),
            _ => None,
        }
    }

    /// Borrow this value as a nested [`DynamicMessage`], if it is a message.
    pub fn as_message(&self) -> Option<&DynamicMessage> {
        match self {
            Value::Message(m) => Some(m.as_ref()),
            _ => None,
        }
    }

    /// Borrow this value as a repeated `List`, if it is one.
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(l) => Some(l.as_slice()),
            _ => None,
        }
    }

    /// Borrow this value as a `Map`, if it is one.
    pub fn as_map(&self) -> Option<&HashMap<MapKey, Value>> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }
}

impl MapKey {
    /// Convert this map key into the equivalent [`Value`].
    pub fn to_value(&self) -> Value {
        match self {
            MapKey::String(s) => Value::String(s.clone()),
            MapKey::I32(v) => Value::I32(*v),
            MapKey::I64(v) => Value::I64(*v),
            MapKey::U32(v) => Value::U32(*v),
            MapKey::U64(v) => Value::U64(*v),
            MapKey::Bool(v) => Value::Bool(*v),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_default_scalars() {
        assert!(Value::I32(0).is_default());
        assert!(!Value::I32(1).is_default());
        assert!(Value::Bool(false).is_default());
        assert!(!Value::Bool(true).is_default());
        assert!(Value::String(String::new()).is_default());
        assert!(!Value::String("x".to_owned()).is_default());
        assert!(Value::Bytes(Vec::new()).is_default());
        assert!(Value::F64(0.0).is_default());
        assert!(Value::EnumNumber(0).is_default());
        assert!(Value::List(Vec::new()).is_default());
    }

    #[test]
    fn accessors() {
        assert_eq!(Value::I32(7).as_i32(), Some(7));
        assert_eq!(Value::I32(7).as_i64(), None);
        assert_eq!(Value::String("hi".to_owned()).as_str(), Some("hi"));
        assert_eq!(Value::Bytes(vec![1, 2]).as_bytes(), Some(&[1u8, 2][..]));
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
        assert_eq!(Value::EnumNumber(3).as_enum_number(), Some(3));
    }

    #[test]
    fn map_key_to_value() {
        assert_eq!(
            MapKey::String("k".to_owned()).to_value(),
            Value::String("k".to_owned())
        );
        assert_eq!(MapKey::I64(5).to_value(), Value::I64(5));
        assert_eq!(MapKey::Bool(true).to_value(), Value::Bool(true));
    }
}
