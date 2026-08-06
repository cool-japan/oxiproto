//! Protobuf wire-format encode/decode for [`DynamicMessage`].
//!
//! All wire primitives are reused from [`oxiproto_core::wire`] — this module
//! contains no hand-rolled varint logic. It implements the *schema-aware*
//! layer on top: choosing wire types per [`Kind`], packed vs unpacked repeated
//! scalars, `map<K, V>` as repeated synthetic entries, proto3 default
//! omission, and unknown-field preservation.
//!
//! Proto2 groups (wire types 3/4) are supported: a `group` field is encoded as
//! a start-group tag, the body fields inline, then a matching end-group tag,
//! and decoded back into the synthetic message the descriptor pool associates
//! with the field. Unknown groups are preserved verbatim.

use oxiproto_core::wire::{
    zigzag_decode32, zigzag_decode64, zigzag_encode32, zigzag_encode64, DecodeBuffer, EncodeBuffer,
    Tag, UnknownFields, WireType,
};

use super::descriptor::{Cardinality, FieldDescriptor, Kind, MessageDescriptor};
use super::dynamic::{default_scalar_value, should_omit_when_default, DynamicMessage};
use super::value::{MapKey, Value};
use crate::ReflectError;

impl DynamicMessage {
    /// Encode this message to a freshly-allocated byte vector.
    ///
    /// Fields are written in ascending field-number order; unknown fields are
    /// appended afterwards. Proto3 singular fields equal to their type default
    /// are omitted.
    ///
    /// # Errors
    ///
    /// Returns [`ReflectError::Field`] if a field's stored value does not match
    /// its descriptor kind (e.g. a non-message value in a message/group field).
    pub fn encode_to_vec(&self) -> Result<Vec<u8>, ReflectError> {
        let mut buf = EncodeBuffer::new();
        self.encode(&mut buf)?;
        Ok(buf.into_vec())
    }

    /// Encode this message into an existing [`EncodeBuffer`].
    ///
    /// # Errors
    ///
    /// See [`DynamicMessage::encode_to_vec`].
    pub fn encode(&self, buf: &mut EncodeBuffer) -> Result<(), ReflectError> {
        for (field, value) in self.iter_fields() {
            // Skip singular fields whose value equals the default (proto3
            // omission). Repeated/map empties are also skipped.
            if should_omit_when_default(&field, value) {
                continue;
            }
            encode_field(buf, &field, value)?;
        }
        // Re-emit preserved unknown fields so that data added by a newer schema
        // survives a decode → encode round-trip.
        self.unknown.encode_to(buf);
        Ok(())
    }

    /// Decode a message of the given descriptor from `bytes`.
    ///
    /// Repeated scalar fields accept both packed and unpacked encodings.
    /// Fields whose numbers are not in the descriptor are preserved as unknown
    /// fields.
    ///
    /// # Errors
    ///
    /// Returns [`ReflectError::Field`] on malformed wire data (propagated from
    /// the wire layer), including a group that is not closed by a matching
    /// end-group tag.
    pub fn decode(desc: MessageDescriptor, bytes: &[u8]) -> Result<Self, ReflectError> {
        let mut msg = DynamicMessage::new(desc);
        let mut dec = DecodeBuffer::new(bytes);
        decode_into(&mut msg, &mut dec)?;
        Ok(msg)
    }
}

/// Decode the contents of `dec` into `msg` until the buffer is exhausted.
fn decode_into(msg: &mut DynamicMessage, dec: &mut DecodeBuffer<'_>) -> Result<(), ReflectError> {
    while !dec.is_empty() {
        let tag = dec.read_tag().map_err(wire_err)?;
        let desc = msg.descriptor();
        match desc.get_field(tag.field_number) {
            Some(field) => decode_known_field(msg, &field, tag, dec)?,
            None => decode_unknown_field(&mut msg.unknown, tag, dec)?,
        }
    }
    Ok(())
}

/// Decode a field that is present in the descriptor.
fn decode_known_field(
    msg: &mut DynamicMessage,
    field: &FieldDescriptor,
    tag: Tag,
    dec: &mut DecodeBuffer<'_>,
) -> Result<(), ReflectError> {
    if field.is_map() {
        return decode_map_entry(msg, field, tag, dec);
    }

    match field.cardinality() {
        Cardinality::Repeated => decode_repeated(msg, field, tag, dec),
        Cardinality::Optional | Cardinality::Required => {
            match decode_single_value(field, tag, dec)? {
                Decoded::Value(value) => {
                    // For singular fields, last-one-wins (protobuf merge
                    // semantics for scalars); set_field also enforces oneof
                    // exclusivity.
                    msg.set_field(field, value);
                }
                // A closed enum treats an unrecognised number as an unknown
                // field, exactly as proto2 always has: it is preserved raw so a
                // decode → encode round trip is byte-identical, but it is not
                // readable through the field.
                Decoded::ClosedEnumMiss(raw) => {
                    msg.unknown.push_varint(tag.field_number, raw);
                }
            }
            Ok(())
        }
    }
}

/// The outcome of decoding one scalar element.
///
/// Everything except a closed enum's unrecognised number produces a value; that
/// one case carries the raw varint through to the unknown-field set instead, so
/// no bytes are lost and none are re-synthesised.
enum Decoded {
    /// A value for the field.
    Value(Value),
    /// A number rejected by a closed enum, as read from the wire.
    ClosedEnumMiss(u64),
}

/// Decode a repeated field element, appending to (or creating) its list.
fn decode_repeated(
    msg: &mut DynamicMessage,
    field: &FieldDescriptor,
    tag: Tag,
    dec: &mut DecodeBuffer<'_>,
) -> Result<(), ReflectError> {
    // Packed encoding: a single length-delimited blob of back-to-back scalars.
    if tag.wire_type == WireType::Len && field.kind().is_packable() {
        let payload = dec.read_length_delimited().map_err(wire_err)?;
        let mut inner = DecodeBuffer::new(payload);
        let mut decoded = Vec::new();
        let mut rejected = Vec::new();
        while !inner.is_empty() {
            match decode_scalar_from(field, field.kind(), &mut inner)? {
                Decoded::Value(value) => decoded.push(value),
                Decoded::ClosedEnumMiss(raw) => rejected.push(raw),
            }
        }
        append_to_list(msg, field, decoded);
        // Numbers a closed enum rejects leave the packed run and become
        // individual unknown varint entries — the same placement `protoc`
        // produces. A round trip is therefore semantically faithful but not
        // byte-identical for this case, because the surviving elements are
        // re-packed without the rejected ones.
        for raw in rejected {
            msg.unknown.push_varint(tag.field_number, raw);
        }
        return Ok(());
    }

    // Unpacked encoding: one tag+value per element.
    match decode_single_value(field, tag, dec)? {
        Decoded::Value(value) => append_to_list(msg, field, vec![value]),
        Decoded::ClosedEnumMiss(raw) => msg.unknown.push_varint(tag.field_number, raw),
    }
    Ok(())
}

/// Append elements to the field's list value, creating the list if absent.
fn append_to_list(msg: &mut DynamicMessage, field: &FieldDescriptor, mut elems: Vec<Value>) {
    let entry = msg
        .fields
        .entry(field.number())
        .or_insert_with(|| Value::List(Vec::new()));
    match entry {
        Value::List(list) => list.append(&mut elems),
        // If a non-list somehow occupies the slot (e.g. a prior singular
        // decode), replace it with a fresh list.
        other => {
            let mut list = Vec::new();
            list.append(&mut elems);
            *other = Value::List(list);
        }
    }
}

/// Decode a single scalar/message value for a field given its tag.
fn decode_single_value(
    field: &FieldDescriptor,
    tag: Tag,
    dec: &mut DecodeBuffer<'_>,
) -> Result<Decoded, ReflectError> {
    match field.kind() {
        Kind::Group(idx) => {
            if tag.wire_type != WireType::SGroup {
                return Err(ReflectError::Field(format!(
                    "group field '{}' expected start-group wire type, got {}",
                    field.name(),
                    tag.wire_type
                )));
            }
            let nested_desc = MessageDescriptor {
                pool: field.pool.clone(),
                index: idx,
            };
            // A group is a message whose body is delimited by start/end-group
            // tags rather than a length prefix. `read_group_body` returns the
            // body as a self-contained field stream (consuming the matching
            // end-group tag); `nested` then decodes it under the shared
            // recursion budget, so group-in-group nesting is bounded exactly
            // like message-in-message.
            let body = dec.read_group_body(tag.field_number).map_err(wire_err)?;
            let mut nested_dec = dec.nested(body).map_err(wire_err)?;
            let mut nested = DynamicMessage::new(nested_desc);
            decode_into(&mut nested, &mut nested_dec)?;
            Ok(Decoded::Value(Value::Message(Box::new(nested))))
        }
        Kind::Message(idx) => {
            if tag.wire_type != WireType::Len {
                return Err(ReflectError::Field(format!(
                    "message field '{}' expected length-delimited wire type, got {}",
                    field.name(),
                    tag.wire_type
                )));
            }
            let payload = dec.read_length_delimited().map_err(wire_err)?;
            let nested_desc = MessageDescriptor {
                pool: field.pool.clone(),
                index: idx,
            };
            // Descend through the shared recursion budget: `nested` returns a
            // depth-incremented buffer and rejects input that nests deeper than
            // `MAX_DECODE_DEPTH`, guarding against stack-overflow DoS.
            let mut nested_dec = dec.nested(payload).map_err(wire_err)?;
            let mut nested = DynamicMessage::new(nested_desc);
            decode_into(&mut nested, &mut nested_dec)?;
            Ok(Decoded::Value(Value::Message(Box::new(nested))))
        }
        kind => decode_scalar_with_tag(kind, tag, dec, field),
    }
}

/// Decode a scalar value, validating the tag's wire type.
fn decode_scalar_with_tag(
    kind: Kind,
    tag: Tag,
    dec: &mut DecodeBuffer<'_>,
    field: &FieldDescriptor,
) -> Result<Decoded, ReflectError> {
    let expected = scalar_wire_type(kind)?;
    if tag.wire_type != expected {
        return Err(ReflectError::Field(format!(
            "field '{}' expected wire type {expected}, got {}",
            field.name(),
            tag.wire_type
        )));
    }
    decode_scalar_from(field, kind, dec)
}

/// Decode a scalar value of `kind` from the buffer (wire type already known to
/// match). Used for both single values and packed elements.
fn decode_scalar_from(
    field: &FieldDescriptor,
    kind: Kind,
    dec: &mut DecodeBuffer<'_>,
) -> Result<Decoded, ReflectError> {
    let value = match kind {
        Kind::Double => Value::F64(dec.read_double().map_err(wire_err)?),
        Kind::Float => Value::F32(dec.read_float().map_err(wire_err)?),
        Kind::Int32 => Value::I32(dec.read_varint().map_err(wire_err)? as i32),
        Kind::Int64 => Value::I64(dec.read_varint().map_err(wire_err)? as i64),
        Kind::Uint32 => {
            let v = dec.read_varint().map_err(wire_err)?;
            Value::U32(v as u32)
        }
        Kind::Uint64 => Value::U64(dec.read_varint().map_err(wire_err)?),
        Kind::Sint32 => {
            let raw = dec.read_varint().map_err(wire_err)? as u32;
            Value::I32(zigzag_decode32(raw))
        }
        Kind::Sint64 => {
            let raw = dec.read_varint().map_err(wire_err)?;
            Value::I64(zigzag_decode64(raw))
        }
        Kind::Fixed32 => Value::U32(dec.read_fixed32().map_err(wire_err)?),
        Kind::Fixed64 => Value::U64(dec.read_fixed64().map_err(wire_err)?),
        Kind::Sfixed32 => Value::I32(dec.read_fixed32().map_err(wire_err)? as i32),
        Kind::Sfixed64 => Value::I64(dec.read_fixed64().map_err(wire_err)? as i64),
        Kind::Bool => Value::Bool(dec.read_varint().map_err(wire_err)? != 0),
        Kind::String => decode_string(field, dec)?,
        Kind::Bytes => Value::Bytes(dec.read_length_delimited().map_err(wire_err)?.to_vec()),
        Kind::Enum(_) => {
            let raw = dec.read_varint().map_err(wire_err)?;
            let number = raw as i32;
            // `features.enum_type = CLOSED` (the proto2 baseline) means a
            // number outside the declared set is not a legal value for the
            // field; it belongs in the unknown-field set. An OPEN enum keeps
            // the raw number so a value added by a newer schema survives.
            let closed = field.enum_type().is_some_and(|enum_desc| {
                enum_desc.is_closed() && enum_desc.get_value(number).is_none()
            });
            if closed {
                return Ok(Decoded::ClosedEnumMiss(raw));
            }
            Value::EnumNumber(number)
        }
        Kind::Message(_) | Kind::Group(_) => {
            return Err(ReflectError::Field(
                "message/group kind is not a scalar".to_owned(),
            ))
        }
    };
    Ok(Decoded::Value(value))
}

/// Decode a `string` field's payload, honouring `features.utf8_validation`.
///
/// `VERIFY` (proto3 and the Editions default) rejects a payload that is not
/// valid UTF-8. `NONE` (the proto2 baseline, and whatever an edition scope asks
/// for) accepts it: valid text still becomes a [`Value::String`], and only a
/// genuinely invalid payload falls through to
/// [`Value::UnvalidatedString`], which preserves the bytes verbatim. Keeping
/// the common case on `Value::String` means enabling NONE does not change the
/// JSON or text representation of well-formed data.
fn decode_string(
    field: &FieldDescriptor,
    dec: &mut DecodeBuffer<'_>,
) -> Result<Value, ReflectError> {
    if field.validates_utf8() {
        return Ok(Value::String(
            dec.read_string().map_err(wire_err)?.to_owned(),
        ));
    }
    let bytes = dec.read_length_delimited().map_err(wire_err)?;
    match core::str::from_utf8(bytes) {
        Ok(text) => Ok(Value::String(text.to_owned())),
        Err(_) => Ok(Value::UnvalidatedString(bytes.to_vec())),
    }
}

/// Decode one `map<K, V>` synthetic entry message and merge it into the map.
fn decode_map_entry(
    msg: &mut DynamicMessage,
    field: &FieldDescriptor,
    tag: Tag,
    dec: &mut DecodeBuffer<'_>,
) -> Result<(), ReflectError> {
    if tag.wire_type != WireType::Len {
        return Err(ReflectError::Field(format!(
            "map field '{}' expected length-delimited entries, got {}",
            field.name(),
            tag.wire_type
        )));
    }
    let payload = dec.read_length_delimited().map_err(wire_err)?;

    let key_field = field
        .map_entry_key_field()
        .ok_or_else(|| ReflectError::Field("map field missing entry key field".to_owned()))?;
    let value_field = field
        .map_entry_value_field()
        .ok_or_else(|| ReflectError::Field("map field missing entry value field".to_owned()))?;

    // A map entry omits key/value when they equal the default; supply defaults.
    let mut key_val = default_scalar_value(key_field.kind());
    let mut val_val = match value_field.kind() {
        Kind::Message(idx) => {
            let nested_desc = MessageDescriptor {
                pool: value_field.pool.clone(),
                index: idx,
            };
            Value::Message(Box::new(DynamicMessage::new(nested_desc)))
        }
        other => default_scalar_value(other),
    };

    // A map entry is itself a nested message level; descend through the shared
    // recursion budget so deeply nested map values cannot overflow the stack.
    let mut entry_dec = dec.nested(payload).map_err(wire_err)?;
    while !entry_dec.is_empty() {
        let entry_tag = entry_dec.read_tag().map_err(wire_err)?;
        let (target, target_field) = match entry_tag.field_number {
            1 => (&mut key_val, &key_field),
            2 => (&mut val_val, &value_field),
            _ => {
                entry_dec
                    .skip_field(entry_tag.wire_type)
                    .map_err(wire_err)?;
                continue;
            }
        };
        match decode_single_value(target_field, entry_tag, &mut entry_dec)? {
            Decoded::Value(value) => *target = value,
            // A closed enum rejected the entry's value. `protoc` moves the
            // whole entry into the unknown-field set rather than inserting a
            // half-decoded pair, so the map keeps only entries it fully
            // understands and the raw bytes still survive a re-encode.
            Decoded::ClosedEnumMiss(_) => {
                msg.unknown
                    .push_length_delimited(field.number(), payload.to_vec());
                return Ok(());
            }
        }
    }

    let map_key = value_to_map_key(&key_val).ok_or_else(|| {
        if matches!(key_val, Value::UnvalidatedString(_)) {
            // Only reachable with features.utf8_validation = NONE on the entry's
            // key field. A map key has to be hashable *and* comparable as text,
            // so an unvalidated payload cannot be admitted.
            ReflectError::Field(format!(
                "map field '{}' has a key that is not valid UTF-8",
                field.name()
            ))
        } else {
            ReflectError::Field(format!(
                "map field '{}' has an unsupported key type",
                field.name()
            ))
        }
    })?;

    let entry = msg
        .fields
        .entry(field.number())
        .or_insert_with(|| Value::Map(std::collections::HashMap::new()));
    match entry {
        Value::Map(map) => {
            map.insert(map_key, val_val);
        }
        other => {
            let mut map = std::collections::HashMap::new();
            map.insert(map_key, val_val);
            *other = Value::Map(map);
        }
    }
    Ok(())
}

/// Decode an unknown field (one whose number is absent from the descriptor),
/// preserving its raw bytes.
fn decode_unknown_field(
    unknown: &mut UnknownFields,
    tag: Tag,
    dec: &mut DecodeBuffer<'_>,
) -> Result<(), ReflectError> {
    match tag.wire_type {
        WireType::Varint => {
            let v = dec.read_varint().map_err(wire_err)?;
            unknown.push_varint(tag.field_number, v);
        }
        WireType::I64 => {
            let v = dec.read_fixed64().map_err(wire_err)?;
            unknown.push_fixed64(tag.field_number, v);
        }
        WireType::I32 => {
            let v = dec.read_fixed32().map_err(wire_err)?;
            unknown.push_fixed32(tag.field_number, v);
        }
        WireType::Len => {
            let payload = dec.read_length_delimited().map_err(wire_err)?;
            unknown.push_length_delimited(tag.field_number, payload.to_vec());
        }
        WireType::SGroup => {
            // Preserve an unknown group verbatim so a decode → encode round-trip
            // stays byte-identical. The stored bytes are the body plus the
            // re-encoded end-group tag, matching what `UnknownFields::encode_to`
            // writes after re-emitting the start-group tag.
            let body = dec.read_group_body(tag.field_number).map_err(wire_err)?;
            let mut raw = body.to_vec();
            let mut end = EncodeBuffer::new();
            end.write_tag(tag.field_number, WireType::EGroup)
                .map_err(wire_err)?;
            raw.extend_from_slice(end.as_bytes());
            unknown.push_group(tag.field_number, raw);
        }
        WireType::EGroup => {
            // A bare end-group with no matching start-group is malformed.
            return Err(ReflectError::Field(format!(
                "unexpected end-group tag for field {}",
                tag.field_number
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// Encode a single (already non-default) field.
fn encode_field(
    buf: &mut EncodeBuffer,
    field: &FieldDescriptor,
    value: &Value,
) -> Result<(), ReflectError> {
    if field.is_map() {
        return encode_map(buf, field, value);
    }
    match field.cardinality() {
        Cardinality::Repeated => encode_repeated(buf, field, value),
        Cardinality::Optional | Cardinality::Required => {
            encode_single(buf, field, value, field.number())
        }
    }
}

/// Encode a repeated field (packed for packable scalars when the field's
/// `packed` flag is set, otherwise unpacked).
fn encode_repeated(
    buf: &mut EncodeBuffer,
    field: &FieldDescriptor,
    value: &Value,
) -> Result<(), ReflectError> {
    let list = match value {
        Value::List(l) => l,
        _ => {
            return Err(ReflectError::Field(format!(
                "repeated field '{}' holds a non-list value",
                field.name()
            )))
        }
    };
    if list.is_empty() {
        return Ok(());
    }

    if field.is_packed() && field.kind().is_packable() {
        // Packed: a single length-delimited payload of back-to-back scalars.
        let mut payload = EncodeBuffer::new();
        for elem in list {
            encode_scalar_payload(&mut payload, field.kind(), elem, field)?;
        }
        buf.write_tag(field.number(), WireType::Len)
            .map_err(wire_err)?;
        buf.write_length_delimited(payload.as_bytes());
    } else {
        for elem in list {
            encode_single(buf, field, elem, field.number())?;
        }
    }
    Ok(())
}

/// Encode a `map<K, V>` field as a series of synthetic entry messages.
fn encode_map(
    buf: &mut EncodeBuffer,
    field: &FieldDescriptor,
    value: &Value,
) -> Result<(), ReflectError> {
    let map = match value {
        Value::Map(m) => m,
        _ => {
            return Err(ReflectError::Field(format!(
                "map field '{}' holds a non-map value",
                field.name()
            )))
        }
    };
    let key_field = field
        .map_entry_key_field()
        .ok_or_else(|| ReflectError::Field("map field missing entry key field".to_owned()))?;
    let value_field = field
        .map_entry_value_field()
        .ok_or_else(|| ReflectError::Field("map field missing entry value field".to_owned()))?;

    for (k, v) in map {
        let key_value = k.to_value();
        let mut entry = EncodeBuffer::new();
        // Map entries always write key (field 1) and value (field 2), even at
        // default, to match the canonical encoding produced by protoc/prost.
        encode_single(&mut entry, &key_field, &key_value, 1)?;
        encode_single(&mut entry, &value_field, v, 2)?;
        buf.write_tag(field.number(), WireType::Len)
            .map_err(wire_err)?;
        buf.write_length_delimited(entry.as_bytes());
    }
    Ok(())
}

/// Encode a single value (scalar or message) with the given field number.
fn encode_single(
    buf: &mut EncodeBuffer,
    field: &FieldDescriptor,
    value: &Value,
    field_number: u32,
) -> Result<(), ReflectError> {
    match field.kind() {
        Kind::Group(_) => {
            let nested = match value {
                Value::Message(m) => m,
                _ => {
                    return Err(ReflectError::Field(format!(
                        "group field '{}' holds a non-message value",
                        field.name()
                    )))
                }
            };
            // A group is written as a start-group tag, the body fields inline
            // (no length prefix), then an end-group tag with the same number.
            buf.write_tag(field_number, WireType::SGroup)
                .map_err(wire_err)?;
            nested.encode(buf)?;
            buf.write_tag(field_number, WireType::EGroup)
                .map_err(wire_err)?;
            Ok(())
        }
        Kind::Message(_) => {
            let nested = match value {
                Value::Message(m) => m,
                _ => {
                    return Err(ReflectError::Field(format!(
                        "message field '{}' holds a non-message value",
                        field.name()
                    )))
                }
            };
            let payload = nested.encode_to_vec()?;
            buf.write_tag(field_number, WireType::Len)
                .map_err(wire_err)?;
            buf.write_length_delimited(&payload);
            Ok(())
        }
        kind => {
            let wt = scalar_wire_type(kind)?;
            buf.write_tag(field_number, wt).map_err(wire_err)?;
            encode_scalar_payload(buf, kind, value, field)
        }
    }
}

/// Encode just the payload of a scalar (no tag), used for both singular and
/// packed-repeated elements.
fn encode_scalar_payload(
    buf: &mut EncodeBuffer,
    kind: Kind,
    value: &Value,
    field: &FieldDescriptor,
) -> Result<(), ReflectError> {
    match kind {
        Kind::Double => buf.write_double(expect_f64(value, field)?),
        Kind::Float => buf.write_float(expect_f32(value, field)?),
        Kind::Int32 => buf.write_varint_i32(expect_i32(value, field)?),
        Kind::Int64 => buf.write_varint_i64(expect_i64(value, field)?),
        Kind::Uint32 => buf.write_varint32(expect_u32(value, field)?),
        Kind::Uint64 => buf.write_varint(expect_u64(value, field)?),
        Kind::Sint32 => buf.write_varint32(zigzag_encode32(expect_i32(value, field)?)),
        Kind::Sint64 => buf.write_varint(zigzag_encode64(expect_i64(value, field)?)),
        Kind::Fixed32 => buf.write_fixed32(expect_u32(value, field)?),
        Kind::Fixed64 => buf.write_fixed64(expect_u64(value, field)?),
        Kind::Sfixed32 => buf.write_fixed32(expect_i32(value, field)? as u32),
        Kind::Sfixed64 => buf.write_fixed64(expect_i64(value, field)? as u64),
        Kind::Bool => buf.write_bool(expect_bool(value, field)?),
        // Both `Value::String` and `Value::UnvalidatedString` are `string`
        // payloads; writing the raw bytes keeps a NONE-validated field
        // byte-identical across a decode → encode round trip.
        Kind::String => buf.write_length_delimited(expect_string_bytes(value, field)?),
        Kind::Bytes => buf.write_length_delimited(expect_bytes(value, field)?),
        Kind::Enum(_) => buf.write_varint_i32(expect_enum(value, field)?),
        Kind::Message(_) | Kind::Group(_) => {
            return Err(ReflectError::Field(
                "message/group kind has no scalar payload".to_owned(),
            ))
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The wire type used to encode a scalar of `kind`.
fn scalar_wire_type(kind: Kind) -> Result<WireType, ReflectError> {
    let wt = match kind {
        Kind::Int32
        | Kind::Int64
        | Kind::Uint32
        | Kind::Uint64
        | Kind::Sint32
        | Kind::Sint64
        | Kind::Bool
        | Kind::Enum(_) => WireType::Varint,
        Kind::Fixed64 | Kind::Sfixed64 | Kind::Double => WireType::I64,
        Kind::Fixed32 | Kind::Sfixed32 | Kind::Float => WireType::I32,
        Kind::String | Kind::Bytes => WireType::Len,
        Kind::Message(_) | Kind::Group(_) => {
            return Err(ReflectError::Field(
                "message/group kind has no scalar wire type".to_owned(),
            ))
        }
    };
    Ok(wt)
}

/// Convert a decoded scalar [`Value`] into a [`MapKey`], if it is a valid key
/// type.
fn value_to_map_key(value: &Value) -> Option<MapKey> {
    match value {
        Value::String(s) => Some(MapKey::String(s.clone())),
        Value::I32(v) => Some(MapKey::I32(*v)),
        Value::I64(v) => Some(MapKey::I64(*v)),
        Value::U32(v) => Some(MapKey::U32(*v)),
        Value::U64(v) => Some(MapKey::U64(*v)),
        Value::Bool(v) => Some(MapKey::Bool(*v)),
        _ => None,
    }
}

/// Map a [`oxiproto_core::wire::WireError`] to a [`ReflectError`].
fn wire_err(e: oxiproto_core::wire::WireError) -> ReflectError {
    ReflectError::Field(format!("wire format error: {e}"))
}

// Typed accessors used during encode, producing a descriptive error on a type
// mismatch rather than panicking.

fn type_mismatch(field: &FieldDescriptor, expected: &str) -> ReflectError {
    ReflectError::Field(format!(
        "field '{}' expected a {expected} value",
        field.name()
    ))
}

fn expect_f64(value: &Value, field: &FieldDescriptor) -> Result<f64, ReflectError> {
    value.as_f64().ok_or_else(|| type_mismatch(field, "f64"))
}
fn expect_f32(value: &Value, field: &FieldDescriptor) -> Result<f32, ReflectError> {
    value.as_f32().ok_or_else(|| type_mismatch(field, "f32"))
}
fn expect_i32(value: &Value, field: &FieldDescriptor) -> Result<i32, ReflectError> {
    value.as_i32().ok_or_else(|| type_mismatch(field, "i32"))
}
fn expect_i64(value: &Value, field: &FieldDescriptor) -> Result<i64, ReflectError> {
    value.as_i64().ok_or_else(|| type_mismatch(field, "i64"))
}
fn expect_u32(value: &Value, field: &FieldDescriptor) -> Result<u32, ReflectError> {
    value.as_u32().ok_or_else(|| type_mismatch(field, "u32"))
}
fn expect_u64(value: &Value, field: &FieldDescriptor) -> Result<u64, ReflectError> {
    value.as_u64().ok_or_else(|| type_mismatch(field, "u64"))
}
fn expect_bool(value: &Value, field: &FieldDescriptor) -> Result<bool, ReflectError> {
    value.as_bool().ok_or_else(|| type_mismatch(field, "bool"))
}
fn expect_string_bytes<'a>(
    value: &'a Value,
    field: &FieldDescriptor,
) -> Result<&'a [u8], ReflectError> {
    value
        .as_string_bytes()
        .ok_or_else(|| type_mismatch(field, "string"))
}
fn expect_bytes<'a>(value: &'a Value, field: &FieldDescriptor) -> Result<&'a [u8], ReflectError> {
    value
        .as_bytes()
        .ok_or_else(|| type_mismatch(field, "bytes"))
}
fn expect_enum(value: &Value, field: &FieldDescriptor) -> Result<i32, ReflectError> {
    value
        .as_enum_number()
        .or_else(|| value.as_i32())
        .ok_or_else(|| type_mismatch(field, "enum number"))
}
