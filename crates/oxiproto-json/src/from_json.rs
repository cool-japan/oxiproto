use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use oxiproto_wkt::{DurationExt, TimestampExt};
use prost::bytes::Bytes;
use prost_reflect::{
    DescriptorPool, DynamicMessage, FieldDescriptor, Kind, MapKey, MessageDescriptor, Value,
};
use prost_types::{Duration as ProtoDuration, Timestamp};
use serde_json::Value as JsonValue;

use crate::codec::JsonCodec;

/// Errors produced by [`from_json`].
#[derive(Debug)]
pub enum JsonError {
    /// The JSON value for a field had an incompatible type.
    WrongType {
        /// Name of the field (or context) where the error occurred.
        field: String,
        /// Expected type description.
        expected: &'static str,
        /// Short description of what was actually found.
        got: String,
    },
    /// A field name present in the JSON object was not found in the descriptor.
    UnknownField(String),
    /// A scalar value could not be parsed or decoded.
    MalformedValue(String),
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonError::WrongType {
                field,
                expected,
                got,
            } => {
                write!(f, "field '{field}': expected {expected}, got {got}")
            }
            JsonError::UnknownField(name) => write!(f, "unknown field '{name}'"),
            JsonError::MalformedValue(msg) => write!(f, "malformed value: {msg}"),
        }
    }
}

impl std::error::Error for JsonError {}

impl From<oxiproto_core::OxiProtoError> for JsonError {
    fn from(e: oxiproto_core::OxiProtoError) -> Self {
        JsonError::MalformedValue(e.to_string())
    }
}

impl From<JsonError> for oxiproto_core::OxiProtoError {
    fn from(e: JsonError) -> Self {
        oxiproto_core::OxiProtoError::ParseError(e.to_string())
    }
}

/// Decode a [`serde_json::Value`] into a [`DynamicMessage`] following the
/// canonical Protobuf-JSON mapping.
///
/// The `descriptor` must match the expected message type.  Use
/// [`JsonCodec::default()`] for standard behaviour.
///
/// Well-Known Types receive special decoding:
/// - Timestamp / Duration / FieldMask / Value / ListValue / Struct / Any
///   are all decoded per the proto3 JSON specification.
/// - `NaN`, `"Infinity"`, and `"-Infinity"` strings are accepted for float
///   and double fields.
///
/// # Errors
///
/// Returns [`JsonError`] on type mismatches, unknown fields, or base64 decode
/// failures.
pub fn from_json(
    value: &JsonValue,
    descriptor: &MessageDescriptor,
    codec: &JsonCodec,
) -> Result<DynamicMessage, JsonError> {
    from_json_message(value, descriptor, codec)
}

fn from_json_message(
    value: &JsonValue,
    descriptor: &MessageDescriptor,
    codec: &JsonCodec,
) -> Result<DynamicMessage, JsonError> {
    let full_name = descriptor.full_name();

    // Well-Known Type special decoding
    match full_name {
        "google.protobuf.Timestamp" => return decode_timestamp(value, descriptor),
        "google.protobuf.Duration" => return decode_duration(value, descriptor),
        "google.protobuf.FieldMask" => return decode_field_mask(value, descriptor),
        "google.protobuf.Value" => return decode_proto_value(value, descriptor, codec),
        "google.protobuf.ListValue" => return decode_list_value(value, descriptor, codec),
        "google.protobuf.Struct" => return decode_struct(value, descriptor, codec),
        "google.protobuf.Any" => {
            return decode_any(value, descriptor, codec);
        }
        _ => {}
    }

    let JsonValue::Object(obj) = value else {
        return Err(JsonError::WrongType {
            field: descriptor.full_name().to_owned(),
            expected: "object",
            got: json_type_name(value),
        });
    };

    let mut msg = DynamicMessage::new(descriptor.clone());

    for (json_key, json_val) in obj {
        // Try camelCase name first, then proto name
        let field_desc = descriptor
            .get_field_by_json_name(json_key)
            .or_else(|| descriptor.get_field_by_name(json_key));

        let field_desc = match field_desc {
            Some(f) => f,
            None => {
                return Err(JsonError::UnknownField(json_key.clone()));
            }
        };

        let value = decode_field_value(json_val, &field_desc, descriptor.parent_pool(), codec)?;
        msg.try_set_field(&field_desc, value).map_err(|e| {
            JsonError::MalformedValue(format!(
                "failed to set field '{}': {}",
                field_desc.name(),
                e
            ))
        })?;
    }

    Ok(msg)
}

fn decode_field_value(
    json_val: &JsonValue,
    field_desc: &FieldDescriptor,
    pool: &DescriptorPool,
    codec: &JsonCodec,
) -> Result<Value, JsonError> {
    let field_name = field_desc.name().to_owned();

    if field_desc.is_map() {
        return decode_map(json_val, field_desc, pool, codec, &field_name);
    }

    if field_desc.is_list() {
        return decode_list(json_val, field_desc, pool, codec, &field_name);
    }

    decode_scalar(json_val, &field_desc.kind(), pool, codec, &field_name)
}

fn decode_list(
    json_val: &JsonValue,
    field_desc: &FieldDescriptor,
    pool: &DescriptorPool,
    codec: &JsonCodec,
    field_name: &str,
) -> Result<Value, JsonError> {
    let JsonValue::Array(arr) = json_val else {
        return Err(JsonError::WrongType {
            field: field_name.to_owned(),
            expected: "array",
            got: json_type_name(json_val),
        });
    };

    let kind = field_desc.kind();
    let mut items = Vec::with_capacity(arr.len());
    for item in arr {
        items.push(decode_scalar(item, &kind, pool, codec, field_name)?);
    }
    Ok(Value::List(items))
}

fn decode_map(
    json_val: &JsonValue,
    field_desc: &FieldDescriptor,
    pool: &DescriptorPool,
    codec: &JsonCodec,
    field_name: &str,
) -> Result<Value, JsonError> {
    let JsonValue::Object(obj) = json_val else {
        return Err(JsonError::WrongType {
            field: field_name.to_owned(),
            expected: "object",
            got: json_type_name(json_val),
        });
    };

    let Kind::Message(entry_desc) = field_desc.kind() else {
        return Err(JsonError::MalformedValue(format!(
            "map field '{field_name}' has non-message entry descriptor"
        )));
    };

    let key_field = entry_desc.map_entry_key_field();
    let value_field = entry_desc.map_entry_value_field();

    let mut map: HashMap<MapKey, Value> = HashMap::new();
    for (k_str, v_json) in obj {
        let key_val = decode_map_key(k_str, &key_field.kind(), field_name)?;
        let val = decode_scalar(v_json, &value_field.kind(), pool, codec, field_name)?;
        map.insert(key_val, val);
    }
    Ok(Value::Map(map))
}

fn decode_map_key(s: &str, kind: &Kind, field_name: &str) -> Result<MapKey, JsonError> {
    match kind {
        Kind::String => Ok(MapKey::String(s.to_owned())),
        Kind::Bool => match s {
            "true" => Ok(MapKey::Bool(true)),
            "false" => Ok(MapKey::Bool(false)),
            other => Err(JsonError::MalformedValue(format!(
                "field '{field_name}': invalid bool map key '{other}'"
            ))),
        },
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => {
            s.parse::<i32>().map(MapKey::I32).map_err(|_| {
                JsonError::MalformedValue(format!("field '{field_name}': bad i32 key '{s}'"))
            })
        }
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => {
            s.parse::<i64>().map(MapKey::I64).map_err(|_| {
                JsonError::MalformedValue(format!("field '{field_name}': bad i64 key '{s}'"))
            })
        }
        Kind::Uint32 | Kind::Fixed32 => s.parse::<u32>().map(MapKey::U32).map_err(|_| {
            JsonError::MalformedValue(format!("field '{field_name}': bad u32 key '{s}'"))
        }),
        Kind::Uint64 | Kind::Fixed64 => s.parse::<u64>().map(MapKey::U64).map_err(|_| {
            JsonError::MalformedValue(format!("field '{field_name}': bad u64 key '{s}'"))
        }),
        other => Err(JsonError::MalformedValue(format!(
            "field '{field_name}': unsupported map key kind {:?}",
            other
        ))),
    }
}

fn decode_scalar(
    json_val: &JsonValue,
    kind: &Kind,
    _pool: &DescriptorPool,
    codec: &JsonCodec,
    field_name: &str,
) -> Result<Value, JsonError> {
    match kind {
        Kind::Bool => match json_val {
            JsonValue::Bool(b) => Ok(Value::Bool(*b)),
            other => Err(JsonError::WrongType {
                field: field_name.to_owned(),
                expected: "bool",
                got: json_type_name(other),
            }),
        },

        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => {
            let n = json_number_as_i32(json_val, field_name)?;
            Ok(Value::I32(n))
        }

        Kind::Uint32 | Kind::Fixed32 => {
            let n = json_number_as_u32(json_val, field_name)?;
            Ok(Value::U32(n))
        }

        // i64/u64 accept either JSON number or JSON string (lenient-in)
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => {
            let n = json_number_or_string_as_i64(json_val, field_name)?;
            Ok(Value::I64(n))
        }

        Kind::Uint64 | Kind::Fixed64 => {
            let n = json_number_or_string_as_u64(json_val, field_name)?;
            Ok(Value::U64(n))
        }

        Kind::Float => match json_val {
            JsonValue::Number(n) => {
                let f = n.as_f64().unwrap_or(0.0) as f32;
                Ok(Value::F32(f))
            }
            // Proto3 JSON spec: NaN/Inf are encoded as strings; accept them on decode.
            JsonValue::String(s) => match s.as_str() {
                "NaN" => Ok(Value::F32(f32::NAN)),
                "Infinity" => Ok(Value::F32(f32::INFINITY)),
                "-Infinity" => Ok(Value::F32(f32::NEG_INFINITY)),
                other => Err(JsonError::MalformedValue(format!(
                    "field '{field_name}': invalid float string '{other}'"
                ))),
            },
            other => Err(JsonError::WrongType {
                field: field_name.to_owned(),
                expected: "number or \"NaN\"/\"Infinity\"/\"-Infinity\"",
                got: json_type_name(other),
            }),
        },

        Kind::Double => match json_val {
            JsonValue::Number(n) => {
                let f = n.as_f64().unwrap_or(0.0);
                Ok(Value::F64(f))
            }
            // Proto3 JSON spec: NaN/Inf are encoded as strings; accept them on decode.
            JsonValue::String(s) => match s.as_str() {
                "NaN" => Ok(Value::F64(f64::NAN)),
                "Infinity" => Ok(Value::F64(f64::INFINITY)),
                "-Infinity" => Ok(Value::F64(f64::NEG_INFINITY)),
                other => Err(JsonError::MalformedValue(format!(
                    "field '{field_name}': invalid double string '{other}'"
                ))),
            },
            other => Err(JsonError::WrongType {
                field: field_name.to_owned(),
                expected: "number or \"NaN\"/\"Infinity\"/\"-Infinity\"",
                got: json_type_name(other),
            }),
        },

        Kind::String => match json_val {
            JsonValue::String(s) => Ok(Value::String(s.clone())),
            other => Err(JsonError::WrongType {
                field: field_name.to_owned(),
                expected: "string",
                got: json_type_name(other),
            }),
        },

        Kind::Bytes => {
            let s = json_val.as_str().ok_or_else(|| JsonError::WrongType {
                field: field_name.to_owned(),
                expected: "base64 string",
                got: json_type_name(json_val),
            })?;
            let bytes = STANDARD.decode(s).map_err(|e| {
                JsonError::MalformedValue(format!("field '{field_name}': base64 decode error: {e}"))
            })?;
            Ok(Value::Bytes(Bytes::from(bytes)))
        }

        Kind::Enum(enum_desc) => {
            // Accept string name or integer
            match json_val {
                JsonValue::String(name) => {
                    if let Some(ev) = enum_desc.get_value_by_name(name) {
                        Ok(Value::EnumNumber(ev.number()))
                    } else {
                        Err(JsonError::MalformedValue(format!(
                            "field '{field_name}': unknown enum value '{name}'"
                        )))
                    }
                }
                JsonValue::Number(n) => {
                    let num = n.as_i64().ok_or_else(|| {
                        JsonError::MalformedValue(format!(
                            "field '{field_name}': enum number out of i64 range"
                        ))
                    })? as i32;
                    Ok(Value::EnumNumber(num))
                }
                other => Err(JsonError::WrongType {
                    field: field_name.to_owned(),
                    expected: "string or number",
                    got: json_type_name(other),
                }),
            }
        }

        Kind::Message(msg_desc) => {
            let nested = from_json_message(json_val, msg_desc, codec)?;
            Ok(Value::Message(nested))
        }
    }
}

// ---------------------------------------------------------------------------
// Number helpers
// ---------------------------------------------------------------------------

fn json_number_as_i32(v: &JsonValue, field: &str) -> Result<i32, JsonError> {
    match v {
        JsonValue::Number(n) => n
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| {
                JsonError::MalformedValue(format!("field '{field}': value out of i32 range"))
            }),
        other => Err(JsonError::WrongType {
            field: field.to_owned(),
            expected: "number",
            got: json_type_name(other),
        }),
    }
}

fn json_number_as_u32(v: &JsonValue, field: &str) -> Result<u32, JsonError> {
    match v {
        JsonValue::Number(n) => n
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| {
                JsonError::MalformedValue(format!("field '{field}': value out of u32 range"))
            }),
        other => Err(JsonError::WrongType {
            field: field.to_owned(),
            expected: "number",
            got: json_type_name(other),
        }),
    }
}

fn json_number_or_string_as_i64(v: &JsonValue, field: &str) -> Result<i64, JsonError> {
    match v {
        JsonValue::Number(n) => n.as_i64().ok_or_else(|| {
            JsonError::MalformedValue(format!("field '{field}': value out of i64 range"))
        }),
        JsonValue::String(s) => s.parse::<i64>().map_err(|_| {
            JsonError::MalformedValue(format!("field '{field}': cannot parse '{s}' as i64"))
        }),
        other => Err(JsonError::WrongType {
            field: field.to_owned(),
            expected: "number or string",
            got: json_type_name(other),
        }),
    }
}

fn json_number_or_string_as_u64(v: &JsonValue, field: &str) -> Result<u64, JsonError> {
    match v {
        JsonValue::Number(n) => n.as_u64().ok_or_else(|| {
            JsonError::MalformedValue(format!("field '{field}': value out of u64 range"))
        }),
        JsonValue::String(s) => s.parse::<u64>().map_err(|_| {
            JsonError::MalformedValue(format!("field '{field}': cannot parse '{s}' as u64"))
        }),
        other => Err(JsonError::WrongType {
            field: field.to_owned(),
            expected: "number or string",
            got: json_type_name(other),
        }),
    }
}

// ---------------------------------------------------------------------------
// WKT decoders
// ---------------------------------------------------------------------------

fn decode_timestamp(
    value: &JsonValue,
    descriptor: &MessageDescriptor,
) -> Result<DynamicMessage, JsonError> {
    let s = value.as_str().ok_or_else(|| JsonError::WrongType {
        field: "Timestamp".to_owned(),
        expected: "RFC3339 string",
        got: json_type_name(value),
    })?;

    // Use oxiproto-wkt's pure-Rust RFC 3339 parser (no chrono dep needed).
    let ts = Timestamp::from_rfc3339(s)
        .map_err(|e| JsonError::MalformedValue(format!("invalid RFC3339 timestamp '{s}': {e}")))?;

    let mut msg = DynamicMessage::new(descriptor.clone());

    let secs_field = descriptor
        .get_field_by_name("seconds")
        .ok_or_else(|| JsonError::MalformedValue("Timestamp missing 'seconds' field".to_owned()))?;
    let nanos_field = descriptor
        .get_field_by_name("nanos")
        .ok_or_else(|| JsonError::MalformedValue("Timestamp missing 'nanos' field".to_owned()))?;

    msg.try_set_field(&secs_field, Value::I64(ts.seconds))
        .map_err(|e| JsonError::MalformedValue(e.to_string()))?;
    msg.try_set_field(&nanos_field, Value::I32(ts.nanos))
        .map_err(|e| JsonError::MalformedValue(e.to_string()))?;

    Ok(msg)
}

fn decode_duration(
    value: &JsonValue,
    descriptor: &MessageDescriptor,
) -> Result<DynamicMessage, JsonError> {
    let s = value.as_str().ok_or_else(|| JsonError::WrongType {
        field: "Duration".to_owned(),
        expected: "duration string",
        got: json_type_name(value),
    })?;

    // Use oxiproto-wkt's duration string parser.
    let dur = ProtoDuration::from_duration_string(s)
        .map_err(|e| JsonError::MalformedValue(format!("invalid duration '{s}': {e}")))?;

    let mut msg = DynamicMessage::new(descriptor.clone());
    let secs_field = descriptor
        .get_field_by_name("seconds")
        .ok_or_else(|| JsonError::MalformedValue("Duration missing 'seconds' field".to_owned()))?;
    let nanos_field = descriptor
        .get_field_by_name("nanos")
        .ok_or_else(|| JsonError::MalformedValue("Duration missing 'nanos' field".to_owned()))?;

    msg.try_set_field(&secs_field, Value::I64(dur.seconds))
        .map_err(|e| JsonError::MalformedValue(e.to_string()))?;
    msg.try_set_field(&nanos_field, Value::I32(dur.nanos))
        .map_err(|e| JsonError::MalformedValue(e.to_string()))?;

    Ok(msg)
}

// ---------------------------------------------------------------------------
// Additional WKT decoders
// ---------------------------------------------------------------------------

/// Decode `google.protobuf.FieldMask` from a JSON string like `"fooBar,bazQux"`.
///
/// The JSON string is a comma-separated list of camelCase paths; each path
/// component is converted back to snake_case to match the proto field names.
fn decode_field_mask(
    value: &JsonValue,
    descriptor: &MessageDescriptor,
) -> Result<DynamicMessage, JsonError> {
    let s = value.as_str().ok_or_else(|| JsonError::WrongType {
        field: "FieldMask".to_owned(),
        expected: "string",
        got: json_type_name(value),
    })?;

    let paths: Vec<prost_reflect::Value> = if s.is_empty() {
        vec![]
    } else {
        s.split(',')
            .map(|token| {
                // Each token is a dot-separated list of camelCase components;
                // convert each component individually to snake_case.
                let snake = token
                    .split('.')
                    .map(camel_to_snake)
                    .collect::<Vec<_>>()
                    .join(".");
                prost_reflect::Value::String(snake)
            })
            .collect()
    };

    let mut msg = DynamicMessage::new(descriptor.clone());
    let paths_field = descriptor
        .get_field_by_name("paths")
        .ok_or_else(|| JsonError::MalformedValue("FieldMask missing 'paths' field".to_owned()))?;
    msg.try_set_field(&paths_field, prost_reflect::Value::List(paths))
        .map_err(|e| JsonError::MalformedValue(e.to_string()))?;
    Ok(msg)
}

/// Convert a camelCase identifier to snake_case.
///
/// Each uppercase letter is replaced by `_<lower>`.  This is the inverse of
/// `snake_to_camel` defined in `to_json.rs`.
fn camel_to_snake(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        if c.is_uppercase() {
            result.push('_');
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Decode `google.protobuf.Value` from any JSON value.
///
/// The JSON form maps directly:
/// - `null` → NullValue
/// - `bool` → BoolValue
/// - `number` → NumberValue
/// - `string` → StringValue
/// - `object` → StructValue
/// - `array` → ListValue
fn decode_proto_value(
    value: &JsonValue,
    descriptor: &MessageDescriptor,
    codec: &JsonCodec,
) -> Result<DynamicMessage, JsonError> {
    let mut msg = DynamicMessage::new(descriptor.clone());

    let (field_name, field_val) = match value {
        JsonValue::Null => {
            let f = descriptor.get_field_by_name("null_value").ok_or_else(|| {
                JsonError::MalformedValue("Value missing 'null_value' field".to_owned())
            })?;
            // NullValue enum value 0
            (f, prost_reflect::Value::EnumNumber(0))
        }
        JsonValue::Bool(b) => {
            let f = descriptor.get_field_by_name("bool_value").ok_or_else(|| {
                JsonError::MalformedValue("Value missing 'bool_value' field".to_owned())
            })?;
            (f, prost_reflect::Value::Bool(*b))
        }
        JsonValue::Number(n) => {
            let f = descriptor
                .get_field_by_name("number_value")
                .ok_or_else(|| {
                    JsonError::MalformedValue("Value missing 'number_value' field".to_owned())
                })?;
            let num = n.as_f64().ok_or_else(|| {
                JsonError::MalformedValue("number_value out of f64 range".to_owned())
            })?;
            (f, prost_reflect::Value::F64(num))
        }
        JsonValue::String(s) => {
            let f = descriptor
                .get_field_by_name("string_value")
                .ok_or_else(|| {
                    JsonError::MalformedValue("Value missing 'string_value' field".to_owned())
                })?;
            (f, prost_reflect::Value::String(s.clone()))
        }
        JsonValue::Object(_) => {
            let struct_desc = descriptor
                .parent_pool()
                .get_message_by_name("google.protobuf.Struct")
                .ok_or_else(|| {
                    JsonError::MalformedValue(
                        "google.protobuf.Struct not in pool for Value decode".to_owned(),
                    )
                })?;
            let nested = decode_struct(value, &struct_desc, codec)?;
            let f = descriptor
                .get_field_by_name("struct_value")
                .ok_or_else(|| {
                    JsonError::MalformedValue("Value missing 'struct_value' field".to_owned())
                })?;
            (f, prost_reflect::Value::Message(nested))
        }
        JsonValue::Array(_) => {
            let list_desc = descriptor
                .parent_pool()
                .get_message_by_name("google.protobuf.ListValue")
                .ok_or_else(|| {
                    JsonError::MalformedValue(
                        "google.protobuf.ListValue not in pool for Value decode".to_owned(),
                    )
                })?;
            let nested = decode_list_value(value, &list_desc, codec)?;
            let f = descriptor.get_field_by_name("list_value").ok_or_else(|| {
                JsonError::MalformedValue("Value missing 'list_value' field".to_owned())
            })?;
            (f, prost_reflect::Value::Message(nested))
        }
    };

    msg.try_set_field(&field_name, field_val)
        .map_err(|e| JsonError::MalformedValue(e.to_string()))?;
    Ok(msg)
}

/// Decode `google.protobuf.ListValue` from a JSON array.
fn decode_list_value(
    value: &JsonValue,
    descriptor: &MessageDescriptor,
    codec: &JsonCodec,
) -> Result<DynamicMessage, JsonError> {
    let JsonValue::Array(arr) = value else {
        return Err(JsonError::WrongType {
            field: "ListValue".to_owned(),
            expected: "array",
            got: json_type_name(value),
        });
    };

    let value_desc = descriptor
        .parent_pool()
        .get_message_by_name("google.protobuf.Value")
        .ok_or_else(|| {
            JsonError::MalformedValue(
                "google.protobuf.Value not in pool for ListValue decode".to_owned(),
            )
        })?;

    let items: Result<Vec<prost_reflect::Value>, JsonError> = arr
        .iter()
        .map(|item| {
            let nested = decode_proto_value(item, &value_desc, codec)?;
            Ok(prost_reflect::Value::Message(nested))
        })
        .collect();

    let values_field = descriptor
        .get_field_by_name("values")
        .ok_or_else(|| JsonError::MalformedValue("ListValue missing 'values' field".to_owned()))?;

    let mut msg = DynamicMessage::new(descriptor.clone());
    msg.try_set_field(&values_field, prost_reflect::Value::List(items?))
        .map_err(|e| JsonError::MalformedValue(e.to_string()))?;
    Ok(msg)
}

/// Decode `google.protobuf.Struct` from a JSON object.
fn decode_struct(
    value: &JsonValue,
    descriptor: &MessageDescriptor,
    codec: &JsonCodec,
) -> Result<DynamicMessage, JsonError> {
    let JsonValue::Object(obj) = value else {
        return Err(JsonError::WrongType {
            field: "Struct".to_owned(),
            expected: "object",
            got: json_type_name(value),
        });
    };

    let value_desc = descriptor
        .parent_pool()
        .get_message_by_name("google.protobuf.Value")
        .ok_or_else(|| {
            JsonError::MalformedValue(
                "google.protobuf.Value not in pool for Struct decode".to_owned(),
            )
        })?;

    // Struct.fields is a map<string, Value>
    let fields_field = descriptor
        .get_field_by_name("fields")
        .ok_or_else(|| JsonError::MalformedValue("Struct missing 'fields' field".to_owned()))?;

    let mut map: HashMap<prost_reflect::MapKey, prost_reflect::Value> = HashMap::new();
    for (k, v) in obj {
        let nested = decode_proto_value(v, &value_desc, codec)?;
        map.insert(
            prost_reflect::MapKey::String(k.clone()),
            prost_reflect::Value::Message(nested),
        );
    }

    let mut msg = DynamicMessage::new(descriptor.clone());
    msg.try_set_field(&fields_field, prost_reflect::Value::Map(map))
        .map_err(|e| JsonError::MalformedValue(e.to_string()))?;
    Ok(msg)
}

/// Decode `google.protobuf.Any` from a JSON object with an `@type` field.
///
/// The JSON representation is:
/// - For WKT scalars (Timestamp, Duration, FieldMask, Struct, Value, ListValue):
///   `{"@type": "<url>", "value": <primitive>}`
/// - For regular messages:
///   `{"@type": "<url>", ...fields...}`
///
/// In both cases `@type` drives descriptor lookup; the inner message is decoded
/// recursively and re-encoded to binary to populate `Any.value`.
fn decode_any(
    value: &JsonValue,
    descriptor: &MessageDescriptor,
    codec: &JsonCodec,
) -> Result<DynamicMessage, JsonError> {
    use prost::Message as _;

    let JsonValue::Object(obj) = value else {
        return Err(JsonError::WrongType {
            field: "Any".to_owned(),
            expected: "object with @type",
            got: json_type_name(value),
        });
    };

    let type_url = obj
        .get("@type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonError::MalformedValue("Any missing '@type' field".to_owned()))?;

    // Strip prefix to get the fully-qualified message name.
    let fqn = type_url.rsplit('/').next().unwrap_or(type_url);

    let pool = descriptor.parent_pool();
    let inner_desc = pool.get_message_by_name(fqn).ok_or_else(|| {
        JsonError::MalformedValue(format!("Any: type '{fqn}' not found in descriptor pool"))
    })?;

    // Determine if this is a WKT that uses the "value" key wrapper form.
    let is_value_wrapper_wkt = matches!(
        inner_desc.full_name(),
        "google.protobuf.Timestamp"
            | "google.protobuf.Duration"
            | "google.protobuf.FieldMask"
            | "google.protobuf.Value"
            | "google.protobuf.ListValue"
            | "google.protobuf.Struct"
            | "google.protobuf.BoolValue"
            | "google.protobuf.Int32Value"
            | "google.protobuf.Int64Value"
            | "google.protobuf.UInt32Value"
            | "google.protobuf.UInt64Value"
            | "google.protobuf.FloatValue"
            | "google.protobuf.DoubleValue"
            | "google.protobuf.StringValue"
            | "google.protobuf.BytesValue"
    );

    let inner_json = if is_value_wrapper_wkt {
        // The actual value lives under the "value" key.
        obj.get("value").unwrap_or(&JsonValue::Null)
    } else {
        // Build a new object excluding the @type key for regular messages.
        value
    };

    let inner_msg = if is_value_wrapper_wkt {
        from_json_message(inner_json, &inner_desc, codec)?
    } else {
        // Pass the full object but we need to strip @type so unknown-field
        // check doesn't fire. Build a filtered object.
        let filtered: serde_json::Map<String, JsonValue> = obj
            .iter()
            .filter(|(k, _)| k.as_str() != "@type")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let filtered_val = JsonValue::Object(filtered);
        from_json_message(&filtered_val, &inner_desc, codec)?
    };

    // Re-encode the inner message to bytes.
    let encoded_bytes = inner_msg.encode_to_vec();

    // Build the Any DynamicMessage.
    let type_url_field = descriptor
        .get_field_by_name("type_url")
        .ok_or_else(|| JsonError::MalformedValue("Any missing 'type_url' field".to_owned()))?;
    let value_field = descriptor
        .get_field_by_name("value")
        .ok_or_else(|| JsonError::MalformedValue("Any missing 'value' field".to_owned()))?;

    let mut msg = DynamicMessage::new(descriptor.clone());
    msg.try_set_field(
        &type_url_field,
        prost_reflect::Value::String(type_url.to_owned()),
    )
    .map_err(|e| JsonError::MalformedValue(e.to_string()))?;
    msg.try_set_field(
        &value_field,
        prost_reflect::Value::Bytes(prost::bytes::Bytes::from(encoded_bytes)),
    )
    .map_err(|e| JsonError::MalformedValue(e.to_string()))?;
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn json_type_name(v: &JsonValue) -> String {
    match v {
        JsonValue::Null => "null".to_owned(),
        JsonValue::Bool(_) => "bool".to_owned(),
        JsonValue::Number(_) => "number".to_owned(),
        JsonValue::String(_) => "string".to_owned(),
        JsonValue::Array(_) => "array".to_owned(),
        JsonValue::Object(_) => "object".to_owned(),
    }
}
