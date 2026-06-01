//! A runtime-typed protobuf message: [`DynamicMessage`].
//!
//! A `DynamicMessage` pairs a [`MessageDescriptor`] with a sparse map from
//! field number to [`Value`]. Only explicitly-set fields are stored; reading
//! an unset field yields the field's default value (proto3 semantics).
//!
//! Wire encoding/decoding lives in [`super::wire_codec`]; this module owns the
//! in-memory representation and field access semantics (including oneof
//! exclusivity and unknown-field preservation).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::HashMap;

use oxiproto_core::wire::UnknownFields;

use super::descriptor::{Cardinality, FieldDescriptor, Kind, MessageDescriptor};
use super::value::Value;

/// A dynamically-typed protobuf message instance.
///
/// Construct one with [`DynamicMessage::new`], populate it with
/// [`DynamicMessage::set_field`], and serialise with
/// [`DynamicMessage::encode_to_vec`]. Decode bytes with
/// [`DynamicMessage::decode`].
///
/// Two `DynamicMessage`s compare equal when they share the same descriptor and
/// carry equal field values and unknown fields.
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicMessage {
    pub(crate) desc: MessageDescriptor,
    /// Sparse field storage keyed by field number. A `BTreeMap` keeps fields
    /// ordered by number on encode, which is the conventional (though not
    /// required) protobuf serialisation order and makes output deterministic.
    pub(crate) fields: BTreeMap<u32, Value>,
    /// Unknown fields preserved across a decode → encode round-trip.
    pub(crate) unknown: UnknownFields,
}

impl DynamicMessage {
    /// Create an empty message for the given descriptor.
    pub fn new(desc: MessageDescriptor) -> Self {
        Self {
            desc,
            fields: BTreeMap::new(),
            unknown: UnknownFields::new(),
        }
    }

    /// The descriptor describing this message's schema.
    pub fn descriptor(&self) -> MessageDescriptor {
        self.desc.clone()
    }

    /// Borrow the preserved unknown fields.
    pub fn unknown_fields(&self) -> &UnknownFields {
        &self.unknown
    }

    /// Mutably borrow the preserved unknown fields.
    pub fn unknown_fields_mut(&mut self) -> &mut UnknownFields {
        &mut self.unknown
    }

    /// Returns `true` if the field is explicitly set to a non-default value.
    ///
    /// For proto3 singular scalar fields, a value equal to the type default is
    /// considered *not* present. For message, repeated, and map fields,
    /// presence means the entry exists and is non-empty.
    pub fn has_field(&self, field: &FieldDescriptor) -> bool {
        match self.fields.get(&field.number()) {
            None => false,
            Some(v) => !is_field_value_default(field, v),
        }
    }

    /// Get the value of a field, returning the field's default (as an owned
    /// value) if it is not set.
    ///
    /// The returned [`Cow`] borrows the stored value when present and owns a
    /// freshly-constructed default otherwise.
    pub fn get_field(&self, field: &FieldDescriptor) -> Cow<'_, Value> {
        match self.fields.get(&field.number()) {
            Some(v) => Cow::Borrowed(v),
            None => Cow::Owned(default_value_for(field)),
        }
    }

    /// Set the value of a field.
    ///
    /// If the field is a member of a (real, non-synthetic) oneof, all sibling
    /// arms of that oneof are cleared first, enforcing oneof exclusivity.
    pub fn set_field(&mut self, field: &FieldDescriptor, value: Value) {
        if let Some(oneof) = field.containing_oneof() {
            // Clear every sibling arm (including synthetic proto3-optional
            // oneofs, which have exactly one member — clearing it is a no-op
            // for the field being set).
            for sibling in oneof.fields() {
                if sibling.number() != field.number() {
                    self.fields.remove(&sibling.number());
                }
            }
        }
        self.fields.insert(field.number(), value);
    }

    /// Clear a field, removing any stored value.
    pub fn clear_field(&mut self, field: &FieldDescriptor) {
        self.fields.remove(&field.number());
    }

    /// Get a field by name. Returns `None` if the name is not a field of this
    /// message's descriptor.
    pub fn get_field_by_name(&self, name: &str) -> Option<Cow<'_, Value>> {
        let field = self.desc.get_field_by_name(name)?;
        Some(self.get_field(&field))
    }

    /// Set a field by name. Returns `false` (and does nothing) if the name is
    /// not a field of this message's descriptor.
    pub fn set_field_by_name(&mut self, name: &str, value: Value) -> bool {
        match self.desc.get_field_by_name(name) {
            Some(field) => {
                self.set_field(&field, value);
                true
            }
            None => false,
        }
    }

    /// Returns the [`FieldDescriptor`] of whichever arm of `oneof` is set, if
    /// any. `oneof` is identified by name.
    pub fn which_oneof(&self, oneof_name: &str) -> Option<FieldDescriptor> {
        let oneof = self.desc.oneofs().find(|o| o.name() == oneof_name)?;
        let set_field = oneof
            .fields()
            .find(|f| self.fields.contains_key(&f.number()));
        set_field
    }

    /// Iterate over the explicitly-set fields as `(descriptor, value)` pairs,
    /// ordered by field number.
    pub fn iter_fields(&self) -> impl Iterator<Item = (FieldDescriptor, &Value)> + '_ {
        self.fields
            .iter()
            .filter_map(move |(&number, value)| self.desc.get_field(number).map(|f| (f, value)))
    }
}

/// Construct the default [`Value`] for a field based on its kind and
/// cardinality.
pub(crate) fn default_value_for(field: &FieldDescriptor) -> Value {
    if field.is_map() {
        return Value::Map(HashMap::new());
    }
    if matches!(field.cardinality(), Cardinality::Repeated) {
        return Value::List(Vec::new());
    }
    default_scalar_value(field.kind())
}

/// The default [`Value`] for a singular field of the given kind.
pub(crate) fn default_scalar_value(kind: Kind) -> Value {
    match kind {
        Kind::Double => Value::F64(0.0),
        Kind::Float => Value::F32(0.0),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => Value::I32(0),
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => Value::I64(0),
        Kind::Uint32 | Kind::Fixed32 => Value::U32(0),
        Kind::Uint64 | Kind::Fixed64 => Value::U64(0),
        Kind::Bool => Value::Bool(false),
        Kind::String => Value::String(String::new()),
        Kind::Bytes => Value::Bytes(Vec::new()),
        Kind::Enum(_) => Value::EnumNumber(0),
        // For message/group kinds the singular default is "absent"; we model
        // that with an empty list-less placeholder that `is_default` treats as
        // present-but-empty. Callers normally branch on cardinality before
        // reaching here, so this is only hit for an unset singular message.
        Kind::Message(_) | Kind::Group(_) => Value::List(Vec::new()),
    }
}

/// Returns `true` if `value` is the default for the given field (used by
/// `has_field` and proto3 default-omission on encode).
pub(crate) fn is_field_value_default(field: &FieldDescriptor, value: &Value) -> bool {
    match field.cardinality() {
        Cardinality::Repeated => match value {
            Value::List(l) => l.is_empty(),
            Value::Map(m) => m.is_empty(),
            _ => false,
        },
        Cardinality::Optional | Cardinality::Required => {
            // A singular message that is present is never "default".
            if matches!(field.kind(), Kind::Message(_) | Kind::Group(_)) {
                return !matches!(value, Value::Message(_));
            }
            value.is_default()
        }
    }
}
