use base64::{engine::general_purpose::STANDARD, Engine as _};
use oxiproto_wkt::{DurationExt, TimestampExt};
use prost_reflect::{DynamicMessage, FieldDescriptor, Kind, MapKey, ReflectMessage, Value};
use prost_types::{Duration as ProtoDuration, Timestamp};
use serde_json::{Map as JsonMap, Number, Value as JsonValue};

use crate::codec::JsonCodec;

/// Convert a [`DynamicMessage`] to a [`serde_json::Value`] following the
/// canonical Protobuf-JSON mapping.
///
/// # Deferred (not yet implemented)
/// - `google.protobuf.Any` — emitted as an opaque object `{}` with a comment
///   in the source; a future release will handle type-URL resolution.
/// - `google.protobuf.Struct`, `google.protobuf.Value`,
///   `google.protobuf.ListValue` — treated as regular messages for now.
pub fn to_json(msg: &DynamicMessage, codec: &JsonCodec) -> JsonValue {
    to_json_message(msg, codec)
}

fn to_json_message(msg: &DynamicMessage, codec: &JsonCodec) -> JsonValue {
    let desc = msg.descriptor();
    let full_name = desc.full_name();

    // Well-Known Types get special encoding.
    match full_name {
        "google.protobuf.Timestamp" => return timestamp_to_json(msg),
        "google.protobuf.Duration" => return duration_to_json(msg),
        // DEFERRED: google.protobuf.Any — fall through to regular object
        _ => {}
    }

    let mut map = JsonMap::new();

    if codec.always_print() {
        // Emit every field, including defaults.
        for field_desc in desc.fields() {
            let value = msg.get_field(&field_desc);
            let key = json_field_key(&field_desc, codec);
            let json_val = value_to_json(value.as_ref(), &field_desc, codec);
            map.insert(key, json_val);
        }
    } else {
        // Emit only set / non-default fields.
        for (field_desc, value) in msg.fields() {
            let key = json_field_key(&field_desc, codec);
            let json_val = value_to_json(value, &field_desc, codec);
            map.insert(key, json_val);
        }
    }

    JsonValue::Object(map)
}

fn json_field_key(field_desc: &FieldDescriptor, codec: &JsonCodec) -> String {
    if codec.uses_proto_names() {
        field_desc.name().to_owned()
    } else {
        field_desc.json_name().to_owned()
    }
}

fn value_to_json(value: &Value, field_desc: &FieldDescriptor, codec: &JsonCodec) -> JsonValue {
    if field_desc.is_map() {
        return map_value_to_json(value, field_desc, codec);
    }
    if field_desc.is_list() {
        if let Value::List(list) = value {
            let arr: Vec<JsonValue> = list
                .iter()
                .map(|v| scalar_value_to_json(v, &field_desc.kind(), codec))
                .collect();
            return JsonValue::Array(arr);
        }
        return JsonValue::Array(vec![]);
    }
    scalar_value_to_json(value, &field_desc.kind(), codec)
}

fn map_value_to_json(value: &Value, field_desc: &FieldDescriptor, codec: &JsonCodec) -> JsonValue {
    let Kind::Message(entry_desc) = field_desc.kind() else {
        return JsonValue::Object(JsonMap::new());
    };

    let value_field = entry_desc.map_entry_value_field();

    if let Value::Map(map) = value {
        let mut obj = JsonMap::new();
        for (k, v) in map {
            let key_str = map_key_to_string(k);
            let json_val = scalar_value_to_json(v, &value_field.kind(), codec);
            obj.insert(key_str, json_val);
        }
        JsonValue::Object(obj)
    } else {
        JsonValue::Object(JsonMap::new())
    }
}

fn map_key_to_string(key: &MapKey) -> String {
    match key {
        MapKey::Bool(b) => b.to_string(),
        MapKey::I32(n) => n.to_string(),
        MapKey::I64(n) => n.to_string(),
        MapKey::U32(n) => n.to_string(),
        MapKey::U64(n) => n.to_string(),
        MapKey::String(s) => s.clone(),
    }
}

fn scalar_value_to_json(value: &Value, kind: &Kind, codec: &JsonCodec) -> JsonValue {
    match value {
        Value::Bool(b) => JsonValue::Bool(*b),

        // 32-bit integers → JSON number
        Value::I32(n) => JsonValue::Number(Number::from(*n)),
        Value::U32(n) => JsonValue::Number(Number::from(*n)),

        // 64-bit integers → JSON string (to preserve full precision)
        Value::I64(n) => JsonValue::String(n.to_string()),
        Value::U64(n) => JsonValue::String(n.to_string()),

        // Floats: NaN and infinite values are encoded as JSON strings per the
        // proto3 JSON spec; finite values are encoded as JSON numbers.
        Value::F32(f) => {
            if f.is_nan() {
                JsonValue::String("NaN".into())
            } else if f.is_infinite() {
                if *f > 0.0 {
                    JsonValue::String("Infinity".into())
                } else {
                    JsonValue::String("-Infinity".into())
                }
            } else {
                Number::from_f64(f64::from(*f))
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null)
            }
        }
        Value::F64(f) => {
            if f.is_nan() {
                JsonValue::String("NaN".into())
            } else if f.is_infinite() {
                if *f > 0.0 {
                    JsonValue::String("Infinity".into())
                } else {
                    JsonValue::String("-Infinity".into())
                }
            } else {
                Number::from_f64(*f)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null)
            }
        }

        Value::String(s) => JsonValue::String(s.clone()),

        // Bytes → base64 standard alphabet with padding (RFC 4648 §4)
        Value::Bytes(b) => JsonValue::String(STANDARD.encode(b.as_ref())),

        // Enum → name string (or number if configured)
        Value::EnumNumber(n) => {
            if codec.enum_as_number() {
                JsonValue::Number(Number::from(*n))
            } else if let Kind::Enum(enum_desc) = kind {
                if let Some(ev) = enum_desc.get_value(*n) {
                    JsonValue::String(ev.name().to_owned())
                } else {
                    // Unknown enum value: fall back to number
                    JsonValue::Number(Number::from(*n))
                }
            } else {
                JsonValue::Number(Number::from(*n))
            }
        }

        // Nested message: recurse
        Value::Message(nested) => to_json_message(nested, codec),

        // List/Map appearing as scalars — should not happen in normal usage
        Value::List(list) => {
            let arr: Vec<JsonValue> = list
                .iter()
                .map(|v| scalar_value_to_json(v, kind, codec))
                .collect();
            JsonValue::Array(arr)
        }
        Value::Map(_) => JsonValue::Object(JsonMap::new()),
    }
}

/// Encode a `google.protobuf.Timestamp` as an RFC 3339 string.
///
/// Delegates to [`oxiproto_wkt::TimestampExt::to_rfc3339`] for canonical
/// pure-Rust formatting.  Falls back to `"0001-01-01T00:00:00Z"` if the
/// seconds value is out of range.
fn timestamp_to_json(msg: &DynamicMessage) -> JsonValue {
    let seconds = msg
        .get_field_by_name("seconds")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let nanos = msg
        .get_field_by_name("nanos")
        .and_then(|v| v.as_i32())
        .unwrap_or(0);

    let ts = Timestamp { seconds, nanos };
    let s = ts
        .to_rfc3339()
        .unwrap_or_else(|_| String::from("0001-01-01T00:00:00Z"));
    JsonValue::String(s)
}

/// Encode a `google.protobuf.Duration` as a string like `"1.5s"` or `"-1s"`.
///
/// Delegates to [`oxiproto_wkt::DurationExt::to_duration_string`].
fn duration_to_json(msg: &DynamicMessage) -> JsonValue {
    let seconds = msg
        .get_field_by_name("seconds")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let nanos = msg
        .get_field_by_name("nanos")
        .and_then(|v| v.as_i32())
        .unwrap_or(0);

    let dur = ProtoDuration { seconds, nanos };
    JsonValue::String(dur.to_duration_string())
}

/// Convert snake_case field name to camelCase.
///
/// Per Protobuf spec, each `_`-separated segment after the first has its
/// leading character uppercased.  Leading and trailing underscores are passed
/// through unchanged.
#[allow(dead_code)] // referenced indirectly via json_name() from prost-reflect
pub(crate) fn snake_to_camel(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;
    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use prost_reflect::Value;
    use serde_json::Value as JsonValue;

    use crate::codec::JsonCodec;

    use super::scalar_value_to_json;

    fn dummy_kind() -> prost_reflect::Kind {
        // Bool kind is a convenient stand-in when the kind isn't used by the arm
        // under test (float arms ignore the `kind` parameter).
        prost_reflect::Kind::Bool
    }

    fn codec() -> JsonCodec {
        JsonCodec::default()
    }

    // --- F32 special values ---

    #[test]
    fn f32_nan_encodes_as_string() {
        let result = scalar_value_to_json(&Value::F32(f32::NAN), &dummy_kind(), &codec());
        assert_eq!(result, JsonValue::String("NaN".into()));
    }

    #[test]
    fn f32_positive_infinity_encodes_as_string() {
        let result = scalar_value_to_json(&Value::F32(f32::INFINITY), &dummy_kind(), &codec());
        assert_eq!(result, JsonValue::String("Infinity".into()));
    }

    #[test]
    fn f32_negative_infinity_encodes_as_string() {
        let result = scalar_value_to_json(&Value::F32(f32::NEG_INFINITY), &dummy_kind(), &codec());
        assert_eq!(result, JsonValue::String("-Infinity".into()));
    }

    #[test]
    fn f32_finite_encodes_as_number() {
        let result = scalar_value_to_json(&Value::F32(1.5_f32), &dummy_kind(), &codec());
        // serde_json represents 1.5 as a Number, not a String
        assert!(matches!(result, JsonValue::Number(_)));
    }

    // --- F64 special values ---

    #[test]
    fn f64_nan_encodes_as_string() {
        let result = scalar_value_to_json(&Value::F64(f64::NAN), &dummy_kind(), &codec());
        assert_eq!(result, JsonValue::String("NaN".into()));
    }

    #[test]
    fn f64_positive_infinity_encodes_as_string() {
        let result = scalar_value_to_json(&Value::F64(f64::INFINITY), &dummy_kind(), &codec());
        assert_eq!(result, JsonValue::String("Infinity".into()));
    }

    #[test]
    fn f64_negative_infinity_encodes_as_string() {
        let result = scalar_value_to_json(&Value::F64(f64::NEG_INFINITY), &dummy_kind(), &codec());
        assert_eq!(result, JsonValue::String("-Infinity".into()));
    }

    #[test]
    fn f64_finite_encodes_as_number() {
        let result = scalar_value_to_json(&Value::F64(2.5_f64), &dummy_kind(), &codec());
        assert!(matches!(result, JsonValue::Number(_)));
    }
}
