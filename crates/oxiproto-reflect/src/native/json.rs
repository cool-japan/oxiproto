//! Canonical protobuf-JSON encoding/decoding for native [`DynamicMessage`].
//!
//! Implements the [proto3 JSON mapping] as described in the protobuf language
//! specification:
//!
//! - Field names: JSON key is the field's `json_name` (camelCase by default).
//! - Integer types: encoded as JSON numbers; `int64`/`uint64`/`sfixed64`/
//!   `fixed64`/`sint64` encoded as JSON strings (to preserve 64-bit precision).
//! - Float/double: JSON numbers; `NaN`/`Infinity`/`-Infinity` encoded as the
//!   string literals `"NaN"`, `"Infinity"`, `"-Infinity"`.
//! - Bytes: base64-encoded JSON string (standard alphabet, with padding).
//! - Enums: JSON string containing the enum value name; unknown numbers
//!   encoded as the integer.
//! - Repeated fields: JSON array.
//! - Map fields: JSON object with stringified keys.
//! - Nested messages: nested JSON objects.
//! - Proto3 default-valued singular scalar fields are *omitted* from output.
//! - `null` in input is treated as the default value for the field type.
//! - Unknown keys in input are silently skipped.
//!
//! [proto3 JSON mapping]: https://protobuf.dev/programming-guides/proto3/#json

use std::collections::HashMap;
use std::sync::Arc;

use super::descriptor::{Cardinality, FieldDescriptor, Kind, MessageDescriptor};
use super::dynamic::{is_field_value_default, DynamicMessage};
use super::value::{MapKey, Value};
// Note: base64 and serde_json are workspace dependencies available unconditionally.

// ---------------------------------------------------------------------------
// Public error type
// ---------------------------------------------------------------------------

/// Errors produced during protobuf-JSON conversion.
#[derive(Debug)]
pub enum JsonError {
    /// The input is not valid JSON.
    InvalidJson(serde_json::Error),
    /// The JSON structure does not match the message schema.
    Schema(String),
    /// An enum value name could not be resolved.
    UnknownEnumValue(String),
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonError::InvalidJson(e) => write!(f, "invalid JSON: {e}"),
            JsonError::Schema(s) => write!(f, "schema mismatch: {s}"),
            JsonError::UnknownEnumValue(s) => write!(f, "unknown enum value: {s}"),
        }
    }
}

impl std::error::Error for JsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JsonError::InvalidJson(e) => Some(e),
            _ => None,
        }
    }
}

impl From<JsonError> for crate::ReflectError {
    fn from(e: JsonError) -> Self {
        crate::ReflectError::Field(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Public API on DynamicMessage
// ---------------------------------------------------------------------------

impl DynamicMessage {
    /// Encode this message to canonical protobuf-JSON, returning a
    /// [`serde_json::Value`] tree.
    ///
    /// Proto3 default-valued singular scalar fields are omitted.
    ///
    /// # Errors
    ///
    /// Returns [`JsonError`] if the message contains an unsupported feature
    /// (e.g. a group-kind field).
    pub fn to_json(&self) -> Result<serde_json::Value, JsonError> {
        encode_message(self)
    }

    /// Encode this message to a canonical protobuf-JSON string.
    ///
    /// # Errors
    ///
    /// See [`DynamicMessage::to_json`].
    pub fn to_json_string(&self) -> Result<String, JsonError> {
        let v = self.to_json()?;
        serde_json::to_string(&v).map_err(JsonError::InvalidJson)
    }

    /// Decode a protobuf-JSON [`serde_json::Value`] into a new
    /// [`DynamicMessage`] of the given descriptor.
    ///
    /// Unknown JSON keys are silently skipped. `null` values are treated as
    /// the type default (clearing the field).
    ///
    /// # Errors
    ///
    /// Returns [`JsonError`] if the JSON does not match the schema.
    pub fn from_json(desc: MessageDescriptor, json: &serde_json::Value) -> Result<Self, JsonError> {
        decode_message(desc, json)
    }

    /// Decode a protobuf-JSON string into a new [`DynamicMessage`] of the
    /// given descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`JsonError`] if the string is not valid JSON or does not match
    /// the schema.
    pub fn from_json_str(desc: MessageDescriptor, s: &str) -> Result<Self, JsonError> {
        let json: serde_json::Value = serde_json::from_str(s).map_err(JsonError::InvalidJson)?;
        decode_message(desc, &json)
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

fn encode_message(msg: &DynamicMessage) -> Result<serde_json::Value, JsonError> {
    let mut map = serde_json::Map::new();
    let desc = msg.descriptor();

    for field in desc.fields() {
        let value = msg.get_field(&field);
        // Omit proto3 default-valued singular fields.
        if is_field_value_default(&field, &value) {
            continue;
        }
        let json_value = encode_field_value(&value, &field)?;
        map.insert(field.json_name().to_owned(), json_value);
    }

    Ok(serde_json::Value::Object(map))
}

fn encode_field_value(
    value: &Value,
    field: &FieldDescriptor,
) -> Result<serde_json::Value, JsonError> {
    if field.is_map() {
        return encode_map(value, field);
    }
    if matches!(field.cardinality(), Cardinality::Repeated) {
        return encode_list(value, field);
    }
    encode_singular(value, field)
}

fn encode_list(value: &Value, field: &FieldDescriptor) -> Result<serde_json::Value, JsonError> {
    match value {
        Value::List(items) => {
            let mut arr = Vec::with_capacity(items.len());
            for item in items {
                arr.push(encode_singular(item, field)?);
            }
            Ok(serde_json::Value::Array(arr))
        }
        other => Err(JsonError::Schema(format!(
            "expected list for repeated field '{}', got {:?}",
            field.name(),
            other
        ))),
    }
}

fn encode_map(value: &Value, field: &FieldDescriptor) -> Result<serde_json::Value, JsonError> {
    match value {
        Value::Map(entries) => {
            let val_field = field.map_entry_value_field().ok_or_else(|| {
                JsonError::Schema(format!(
                    "map field '{}' missing value field descriptor",
                    field.name()
                ))
            })?;
            let mut obj = serde_json::Map::new();
            // Sort by string key for deterministic output.
            let mut sorted: Vec<_> = entries.iter().collect();
            sorted.sort_by_key(|(k, _)| map_key_to_string(k));
            for (k, v) in sorted {
                obj.insert(map_key_to_string(k), encode_singular(v, &val_field)?);
            }
            Ok(serde_json::Value::Object(obj))
        }
        other => Err(JsonError::Schema(format!(
            "expected map for map field '{}', got {:?}",
            field.name(),
            other
        ))),
    }
}

/// Encode a single (non-repeated, non-map) field value.
fn encode_singular(value: &Value, field: &FieldDescriptor) -> Result<serde_json::Value, JsonError> {
    match value {
        Value::F64(v) => encode_f64(*v),
        Value::F32(v) => encode_f64(f64::from(*v)),
        Value::I32(v) => Ok(serde_json::json!(*v)),
        Value::U32(v) => Ok(serde_json::json!(*v)),
        // 64-bit integers: JSON string to preserve full precision.
        Value::I64(v) => Ok(serde_json::Value::String(v.to_string())),
        Value::U64(v) => Ok(serde_json::Value::String(v.to_string())),
        Value::Bool(v) => Ok(serde_json::Value::Bool(*v)),
        Value::String(s) => Ok(serde_json::Value::String(s.clone())),
        Value::Bytes(b) => encode_bytes(b),
        Value::EnumNumber(n) => encode_enum_number(*n, field),
        Value::Message(m) => encode_message(m),
        Value::List(_) | Value::Map(_) => Err(JsonError::Schema(format!(
            "unexpected list/map in singular context for field '{}'",
            field.name()
        ))),
    }
}

fn encode_f64(v: f64) -> Result<serde_json::Value, JsonError> {
    if v.is_nan() {
        return Ok(serde_json::Value::String("NaN".to_owned()));
    }
    if v.is_infinite() {
        let s = if v > 0.0 { "Infinity" } else { "-Infinity" };
        return Ok(serde_json::Value::String(s.to_owned()));
    }
    serde_json::Number::from_f64(v)
        .map(serde_json::Value::Number)
        .ok_or_else(|| JsonError::Schema(format!("cannot represent f64 as JSON number: {v}")))
}

fn encode_bytes(b: &[u8]) -> Result<serde_json::Value, JsonError> {
    use base64::Engine as _;
    Ok(serde_json::Value::String(
        base64::engine::general_purpose::STANDARD.encode(b),
    ))
}

fn encode_enum_number(n: i32, field: &FieldDescriptor) -> Result<serde_json::Value, JsonError> {
    // Emit the enum value name if we can resolve it.
    if let Some(enum_desc) = field.enum_type() {
        if let Some(val_desc) = enum_desc.get_value(n) {
            return Ok(serde_json::Value::String(val_desc.name().to_owned()));
        }
    }
    // Unknown enum number: emit as integer (proto3 JSON rule).
    Ok(serde_json::json!(n))
}

fn map_key_to_string(key: &MapKey) -> String {
    match key {
        MapKey::String(s) => s.clone(),
        MapKey::I32(v) => v.to_string(),
        MapKey::I64(v) => v.to_string(),
        MapKey::U32(v) => v.to_string(),
        MapKey::U64(v) => v.to_string(),
        MapKey::Bool(v) => if *v { "true" } else { "false" }.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

fn decode_message(
    desc: MessageDescriptor,
    json: &serde_json::Value,
) -> Result<DynamicMessage, JsonError> {
    let obj = match json {
        serde_json::Value::Object(m) => m,
        // null at message level → empty message.
        serde_json::Value::Null => return Ok(DynamicMessage::new(desc)),
        other => {
            return Err(JsonError::Schema(format!(
                "expected JSON object for message, got {}",
                json_type_name(other)
            )));
        }
    };

    let mut msg = DynamicMessage::new(desc.clone());

    for (json_key, json_val) in obj {
        // Accept both json_name (camelCase) and snake_case field names.
        let field = desc
            .get_field_by_json_name(json_key)
            .or_else(|| desc.get_field_by_name(json_key));

        let field = match field {
            Some(f) => f,
            // Unknown key — silently skip (proto3 JSON interop rule).
            None => continue,
        };

        // null → treat as default (clear the field).
        if json_val.is_null() {
            continue;
        }

        let value = decode_field_value(json_val, &field)?;
        msg.set_field(&field, value);
    }

    Ok(msg)
}

fn decode_field_value(
    json: &serde_json::Value,
    field: &FieldDescriptor,
) -> Result<Value, JsonError> {
    if field.is_map() {
        return decode_map(json, field);
    }
    if matches!(field.cardinality(), Cardinality::Repeated) {
        return decode_list(json, field);
    }
    decode_singular(json, field)
}

fn decode_list(json: &serde_json::Value, field: &FieldDescriptor) -> Result<Value, JsonError> {
    let arr = match json {
        serde_json::Value::Array(a) => a,
        other => {
            return Err(JsonError::Schema(format!(
                "expected JSON array for repeated field '{}', got {}",
                field.name(),
                json_type_name(other)
            )));
        }
    };
    let mut items = Vec::with_capacity(arr.len());
    for item in arr {
        items.push(decode_singular(item, field)?);
    }
    Ok(Value::List(items))
}

fn decode_map(json: &serde_json::Value, field: &FieldDescriptor) -> Result<Value, JsonError> {
    let obj = match json {
        serde_json::Value::Object(o) => o,
        other => {
            return Err(JsonError::Schema(format!(
                "expected JSON object for map field '{}', got {}",
                field.name(),
                json_type_name(other)
            )));
        }
    };

    let key_field = field.map_entry_key_field().ok_or_else(|| {
        JsonError::Schema(format!(
            "map field '{}' missing key field descriptor",
            field.name()
        ))
    })?;
    let val_field = field.map_entry_value_field().ok_or_else(|| {
        JsonError::Schema(format!(
            "map field '{}' missing value field descriptor",
            field.name()
        ))
    })?;

    let mut map = HashMap::new();
    for (k_str, v_json) in obj {
        let map_key = parse_map_key(k_str, key_field.kind())?;
        let map_val = decode_singular(v_json, &val_field)?;
        map.insert(map_key, map_val);
    }
    Ok(Value::Map(map))
}

fn parse_map_key(s: &str, kind: Kind) -> Result<MapKey, JsonError> {
    match kind {
        Kind::String => Ok(MapKey::String(s.to_owned())),
        Kind::Bool => match s {
            "true" => Ok(MapKey::Bool(true)),
            "false" => Ok(MapKey::Bool(false)),
            other => Err(JsonError::Schema(format!("invalid bool map key: {other}"))),
        },
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => s
            .parse::<i32>()
            .map(MapKey::I32)
            .map_err(|_| JsonError::Schema(format!("invalid int32 map key: {s}"))),
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => s
            .parse::<i64>()
            .map(MapKey::I64)
            .map_err(|_| JsonError::Schema(format!("invalid int64 map key: {s}"))),
        Kind::Uint32 | Kind::Fixed32 => s
            .parse::<u32>()
            .map(MapKey::U32)
            .map_err(|_| JsonError::Schema(format!("invalid uint32 map key: {s}"))),
        Kind::Uint64 | Kind::Fixed64 => s
            .parse::<u64>()
            .map(MapKey::U64)
            .map_err(|_| JsonError::Schema(format!("invalid uint64 map key: {s}"))),
        other => Err(JsonError::Schema(format!(
            "unsupported map key kind: {other:?}"
        ))),
    }
}

/// Decode a single (non-repeated, non-map) JSON value using the full field
/// descriptor for enum-name lookup and nested-message construction.
fn decode_singular(json: &serde_json::Value, field: &FieldDescriptor) -> Result<Value, JsonError> {
    match field.kind() {
        Kind::Double => decode_f64(json),
        Kind::Float => decode_f32(json),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => decode_i32(json),
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => decode_i64(json),
        Kind::Uint32 | Kind::Fixed32 => decode_u32(json),
        Kind::Uint64 | Kind::Fixed64 => decode_u64(json),
        Kind::Bool => decode_bool(json),
        Kind::String => decode_string_val(json),
        Kind::Bytes => decode_bytes_val(json),
        Kind::Enum(_) => decode_enum(json, field),
        Kind::Message(msg_index) => {
            if json.is_null() {
                let msg_desc = MessageDescriptor {
                    pool: Arc::clone(&field.pool),
                    index: msg_index,
                };
                return Ok(Value::Message(Box::new(DynamicMessage::new(msg_desc))));
            }
            let msg_desc = MessageDescriptor {
                pool: Arc::clone(&field.pool),
                index: msg_index,
            };
            Ok(Value::Message(Box::new(decode_message(msg_desc, json)?)))
        }
        Kind::Group(_) => Err(JsonError::Schema(
            "group fields are not supported in JSON".to_owned(),
        )),
    }
}

fn decode_f64(json: &serde_json::Value) -> Result<Value, JsonError> {
    match json {
        serde_json::Value::Number(n) => {
            Ok(Value::F64(n.as_f64().ok_or_else(|| {
                JsonError::Schema("number out of f64 range".to_owned())
            })?))
        }
        serde_json::Value::String(s) => match s.as_str() {
            "NaN" => Ok(Value::F64(f64::NAN)),
            "Infinity" => Ok(Value::F64(f64::INFINITY)),
            "-Infinity" => Ok(Value::F64(f64::NEG_INFINITY)),
            other => other
                .parse::<f64>()
                .map(Value::F64)
                .map_err(|_| JsonError::Schema(format!("invalid f64 string: {other}"))),
        },
        other => Err(type_mismatch("f64", other)),
    }
}

fn decode_f32(json: &serde_json::Value) -> Result<Value, JsonError> {
    match json {
        serde_json::Value::Number(n) => {
            Ok(Value::F32(n.as_f64().map(|v| v as f32).ok_or_else(
                || JsonError::Schema("number out of range for f32".to_owned()),
            )?))
        }
        serde_json::Value::String(s) => match s.as_str() {
            "NaN" => Ok(Value::F32(f32::NAN)),
            "Infinity" => Ok(Value::F32(f32::INFINITY)),
            "-Infinity" => Ok(Value::F32(f32::NEG_INFINITY)),
            other => other
                .parse::<f32>()
                .map(Value::F32)
                .map_err(|_| JsonError::Schema(format!("invalid f32 string: {other}"))),
        },
        other => Err(type_mismatch("f32", other)),
    }
}

fn decode_i32(json: &serde_json::Value) -> Result<Value, JsonError> {
    match json {
        serde_json::Value::Number(n) => n
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .map(Value::I32)
            .ok_or_else(|| JsonError::Schema(format!("value out of i32 range: {n}"))),
        serde_json::Value::String(s) => s
            .parse::<i32>()
            .map(Value::I32)
            .map_err(|_| JsonError::Schema(format!("invalid i32 string: {s}"))),
        other => Err(type_mismatch("i32", other)),
    }
}

fn decode_i64(json: &serde_json::Value) -> Result<Value, JsonError> {
    match json {
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Value::I64)
            .ok_or_else(|| JsonError::Schema(format!("value out of i64 range: {n}"))),
        serde_json::Value::String(s) => s
            .parse::<i64>()
            .map(Value::I64)
            .map_err(|_| JsonError::Schema(format!("invalid i64 string: {s}"))),
        other => Err(type_mismatch("i64", other)),
    }
}

fn decode_u32(json: &serde_json::Value) -> Result<Value, JsonError> {
    match json {
        serde_json::Value::Number(n) => n
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .map(Value::U32)
            .ok_or_else(|| JsonError::Schema(format!("value out of u32 range: {n}"))),
        serde_json::Value::String(s) => s
            .parse::<u32>()
            .map(Value::U32)
            .map_err(|_| JsonError::Schema(format!("invalid u32 string: {s}"))),
        other => Err(type_mismatch("u32", other)),
    }
}

fn decode_u64(json: &serde_json::Value) -> Result<Value, JsonError> {
    match json {
        serde_json::Value::Number(n) => n
            .as_u64()
            .map(Value::U64)
            .ok_or_else(|| JsonError::Schema(format!("value out of u64 range: {n}"))),
        serde_json::Value::String(s) => s
            .parse::<u64>()
            .map(Value::U64)
            .map_err(|_| JsonError::Schema(format!("invalid u64 string: {s}"))),
        other => Err(type_mismatch("u64", other)),
    }
}

fn decode_bool(json: &serde_json::Value) -> Result<Value, JsonError> {
    match json {
        serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
        serde_json::Value::String(s) => match s.as_str() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            other => Err(JsonError::Schema(format!("invalid bool string: {other}"))),
        },
        other => Err(type_mismatch("bool", other)),
    }
}

fn decode_string_val(json: &serde_json::Value) -> Result<Value, JsonError> {
    match json {
        serde_json::Value::String(s) => Ok(Value::String(s.clone())),
        other => Err(type_mismatch("string", other)),
    }
}

fn decode_bytes_val(json: &serde_json::Value) -> Result<Value, JsonError> {
    match json {
        serde_json::Value::String(s) => {
            use base64::Engine as _;
            // Accept standard alphabet with or without padding, and URL-safe.
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(s)
                .or_else(|_| {
                    base64::engine::general_purpose::STANDARD_NO_PAD.decode(s.trim_end_matches('='))
                })
                .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s))
                .map_err(|e| JsonError::Schema(format!("invalid base64 for bytes field: {e}")))?;
            Ok(Value::Bytes(bytes))
        }
        other => Err(type_mismatch("bytes (base64 string)", other)),
    }
}

fn decode_enum(json: &serde_json::Value, field: &FieldDescriptor) -> Result<Value, JsonError> {
    let enum_desc = field.enum_type().ok_or_else(|| {
        JsonError::Schema(format!(
            "field '{}' has enum kind but no enum descriptor",
            field.name()
        ))
    })?;

    match json {
        serde_json::Value::Number(n) => {
            let num = n
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| JsonError::Schema(format!("enum number out of i32 range: {n}")))?;
            Ok(Value::EnumNumber(num))
        }
        serde_json::Value::String(s) => enum_desc
            .get_value_by_name(s)
            .map(|v| Value::EnumNumber(v.number()))
            .ok_or_else(|| JsonError::UnknownEnumValue(s.clone())),
        other => Err(type_mismatch("enum (number or name string)", other)),
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn json_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

fn type_mismatch(expected: &str, got: &serde_json::Value) -> JsonError {
    JsonError::Schema(format!("expected {expected}, got {}", json_type_name(got)))
}
