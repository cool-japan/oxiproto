#![forbid(unsafe_code)]
//! Free-function convenience helpers for [`DynamicMessage`] field access.
//!
//! Each function looks up a field descriptor by name, then delegates to the
//! corresponding method on [`DynamicMessage`]. All return [`ReflectError::NotFound`]
//! when the given name is not a field of the message's descriptor.

use prost_reflect::{DynamicMessage, ReflectMessage, UnknownField, Value};

use crate::ReflectError;

/// Set a field value by name.
///
/// # Errors
///
/// Returns [`ReflectError::NotFound`] if `name` is not a field of `msg`'s
/// message descriptor.
///
/// Returns [`ReflectError::Field`] if `value` is not type-compatible with the
/// field (e.g. passing a `Value::String` for an `int32` field).
pub fn set_field_by_name(
    msg: &mut DynamicMessage,
    name: &str,
    value: Value,
) -> Result<(), ReflectError> {
    let field = msg
        .descriptor()
        .get_field_by_name(name)
        .ok_or_else(|| ReflectError::NotFound(name.to_owned()))?;
    msg.try_set_field(&field, value)
        .map_err(|e| ReflectError::Field(e.to_string()))
}

/// Get the value of a field by name.
///
/// Returns `Ok(Some(value))` if the field is set to a non-default value.
/// Returns `Ok(None)` if the field exists but is not set (at its default).
///
/// # Errors
///
/// Returns [`ReflectError::NotFound`] if `name` is not a field of `msg`'s
/// message descriptor.
pub fn get_field_by_name(msg: &DynamicMessage, name: &str) -> Result<Option<Value>, ReflectError> {
    let field = msg
        .descriptor()
        .get_field_by_name(name)
        .ok_or_else(|| ReflectError::NotFound(name.to_owned()))?;
    if msg.has_field(&field) {
        Ok(Some(msg.get_field(&field).into_owned()))
    } else {
        Ok(None)
    }
}

/// Check whether a field is set (non-default).
///
/// # Errors
///
/// Returns [`ReflectError::NotFound`] if `name` is not a field of `msg`'s
/// message descriptor.
pub fn has_field(msg: &DynamicMessage, name: &str) -> Result<bool, ReflectError> {
    let field = msg
        .descriptor()
        .get_field_by_name(name)
        .ok_or_else(|| ReflectError::NotFound(name.to_owned()))?;
    Ok(msg.has_field(&field))
}

/// Clear a field, resetting it to its default value.
///
/// After this call, [`has_field`] for the same name will return `false`.
///
/// # Errors
///
/// Returns [`ReflectError::NotFound`] if `name` is not a field of `msg`'s
/// message descriptor.
pub fn clear_field(msg: &mut DynamicMessage, name: &str) -> Result<(), ReflectError> {
    let field = msg
        .descriptor()
        .get_field_by_name(name)
        .ok_or_else(|| ReflectError::NotFound(name.to_owned()))?;
    msg.clear_field(&field);
    Ok(())
}

/// Returns an iterator over the unknown fields preserved on a decoded
/// [`DynamicMessage`].
///
/// Unknown fields arise when a message is decoded from bytes that contain
/// field numbers not present in the descriptor used for decoding.  They are
/// preserved so that a re-encode of the same message does not silently drop
/// data added by a newer schema version.
///
/// For a freshly-constructed or unmodified message the iterator will be empty.
///
/// Note: prost-reflect 0.16 exposes unknown fields only as an iterator over
/// [`UnknownField`] values; there is no publicly-accessible `UnknownFieldSet`
/// type — that struct is `pub(crate)` inside prost-reflect.
///
/// # Examples
///
/// ```rust
/// use oxiproto_reflect::{unknown_fields, DynamicMessage, pool_from_fds};
/// use prost_types::{FileDescriptorSet, FileDescriptorProto, DescriptorProto};
///
/// let fds = FileDescriptorSet {
///     file: vec![FileDescriptorProto {
///         name: Some("test.proto".to_string()),
///         message_type: vec![DescriptorProto {
///             name: Some("Empty".to_string()),
///             ..Default::default()
///         }],
///         ..Default::default()
///     }],
/// };
/// let pool = pool_from_fds(fds).unwrap();
/// let desc = pool.get_message_by_name("Empty").unwrap();
/// let msg = DynamicMessage::new(desc);
/// assert_eq!(unknown_fields(&msg).count(), 0);
/// ```
pub fn unknown_fields(msg: &DynamicMessage) -> impl Iterator<Item = &UnknownField> + '_ {
    msg.unknown_fields()
}
