use base64::{engine::general_purpose::STANDARD, Engine as _};
use oxiproto_wkt::{DurationExt, TimestampExt};
use prost_reflect::{DynamicMessage, FieldDescriptor, Kind, MapKey, ReflectMessage, Value};
use prost_types::{Duration as ProtoDuration, Timestamp};
use serde_json::{Map as JsonMap, Number, Value as JsonValue};

use crate::codec::JsonCodec;

/// Convert a [`DynamicMessage`] to a [`serde_json::Value`] following the
/// canonical Protobuf-JSON mapping.
///
/// Well-Known Types receive special encoding:
/// - Timestamp / Duration / FieldMask / Value / ListValue / Struct / Any
///   are all encoded per the proto3 JSON specification.
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
        "google.protobuf.FieldMask" => return field_mask_to_json(msg),
        "google.protobuf.Value" => return proto_value_to_json(msg, codec),
        "google.protobuf.ListValue" => return list_value_to_json(msg, codec),
        "google.protobuf.Struct" => return struct_to_json(msg, codec),
        "google.protobuf.Any" => return any_to_json(msg, codec),
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

/// Encode a `google.protobuf.FieldMask` as a JSON string.
///
/// Paths are joined with `,`; each dot-separated component is converted from
/// snake_case to camelCase.  Example: `["foo_bar", "baz_qux"]` → `"fooBar,bazQux"`.
fn field_mask_to_json(msg: &DynamicMessage) -> JsonValue {
    let paths_raw = msg
        .get_field_by_name("paths")
        .and_then(|v| match v.as_ref() {
            Value::List(list) => {
                let strings: Vec<String> = list
                    .iter()
                    .filter_map(|item| {
                        if let Value::String(s) = item {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                Some(strings)
            }
            _ => None,
        })
        .unwrap_or_default();

    let encoded: Vec<String> = paths_raw
        .iter()
        .map(|path| {
            // Convert each dot-separated component from snake_case to camelCase.
            path.split('.')
                .map(snake_to_camel)
                .collect::<Vec<_>>()
                .join(".")
        })
        .collect();

    JsonValue::String(encoded.join(","))
}

/// Encode a `google.protobuf.Value` to its JSON representation.
///
/// Unwraps the `kind` oneof:
/// - NullValue → `null`
/// - BoolValue → `bool`
/// - NumberValue → `number`
/// - StringValue → `string`
/// - StructValue → `object` (recurse)
/// - ListValue → `array` (recurse)
fn proto_value_to_json(msg: &DynamicMessage, codec: &JsonCodec) -> JsonValue {
    // Check each oneof arm in field-number order (1..=6).
    // Walk the `kind` oneof fields in field-number order (1..=6).
    // We use `msg.fields()` which only yields fields that are explicitly set
    // (non-default), so whichever oneof arm is set will appear here.
    for (field_desc, val) in msg.fields() {
        match field_desc.name() {
            "null_value" => return JsonValue::Null,
            "bool_value" => {
                if let Value::Bool(b) = val {
                    return JsonValue::Bool(*b);
                }
            }
            "number_value" => {
                if let Value::F64(n) = val {
                    return Number::from_f64(*n)
                        .map(JsonValue::Number)
                        .unwrap_or(JsonValue::Null);
                }
            }
            "string_value" => {
                if let Value::String(s) = val {
                    return JsonValue::String(s.clone());
                }
            }
            "struct_value" => {
                if let Value::Message(nested) = val {
                    return struct_to_json(nested, codec);
                }
            }
            "list_value" => {
                if let Value::Message(nested) = val {
                    return list_value_to_json(nested, codec);
                }
            }
            _ => {}
        }
    }
    // Default: null (no kind set or all defaults)
    JsonValue::Null
}

/// Encode a `google.protobuf.ListValue` to a JSON array.
fn list_value_to_json(msg: &DynamicMessage, codec: &JsonCodec) -> JsonValue {
    let items = msg
        .get_field_by_name("values")
        .and_then(|v| match v.as_ref() {
            Value::List(list) => {
                let arr: Vec<JsonValue> = list
                    .iter()
                    .filter_map(|item| {
                        if let Value::Message(nested) = item {
                            Some(proto_value_to_json(nested, codec))
                        } else {
                            None
                        }
                    })
                    .collect();
                Some(arr)
            }
            _ => None,
        })
        .unwrap_or_default();
    JsonValue::Array(items)
}

/// Encode a `google.protobuf.Struct` to a JSON object.
fn struct_to_json(msg: &DynamicMessage, codec: &JsonCodec) -> JsonValue {
    let mut obj = JsonMap::new();

    if let Some(v) = msg.get_field_by_name("fields") {
        if let Value::Map(map) = v.as_ref() {
            for (key, val) in map {
                let key_str = if let MapKey::String(s) = key {
                    s.clone()
                } else {
                    continue;
                };
                let json_val = if let Value::Message(nested) = val {
                    proto_value_to_json(nested, codec)
                } else {
                    JsonValue::Null
                };
                obj.insert(key_str, json_val);
            }
        }
    }

    JsonValue::Object(obj)
}

/// Encode a `google.protobuf.Any` to a JSON object with `@type`.
///
/// The inner message bytes are decoded using the descriptor found in the pool.
/// If the inner type is a WKT that encodes as a primitive (Timestamp, Duration,
/// FieldMask, Value, ListValue, Struct, and wrapper types), the result is
/// `{"@type": "<url>", "value": <primitive>}`.  Regular messages have their
/// fields inlined with `@type` prepended.
fn any_to_json(msg: &DynamicMessage, codec: &JsonCodec) -> JsonValue {
    let type_url = msg
        .get_field_by_name("type_url")
        .and_then(|v| match v.as_ref() {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let value_bytes = msg
        .get_field_by_name("value")
        .and_then(|v| match v.as_ref() {
            Value::Bytes(b) => Some(b.clone()),
            _ => None,
        })
        .unwrap_or_default();

    if type_url.is_empty() {
        return JsonValue::Object(JsonMap::new());
    }

    let fqn = type_url.rsplit('/').next().unwrap_or(type_url.as_str());

    let desc = msg.descriptor();
    let pool = desc.parent_pool();
    let inner_desc = match pool.get_message_by_name(fqn) {
        Some(d) => d,
        None => {
            // Unknown type — emit raw Any as object
            let mut obj = JsonMap::new();
            obj.insert("@type".to_owned(), JsonValue::String(type_url.clone()));
            obj.insert(
                "value".to_owned(),
                JsonValue::String(STANDARD.encode(value_bytes.as_ref())),
            );
            return JsonValue::Object(obj);
        }
    };

    let inner_msg = match DynamicMessage::decode(inner_desc.clone(), value_bytes.as_ref()) {
        Ok(m) => m,
        Err(_) => return JsonValue::Object(JsonMap::new()),
    };

    // Determine if this WKT encodes as a non-object primitive.
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

    let inner_json = to_json_message(&inner_msg, codec);

    let mut obj = JsonMap::new();
    obj.insert("@type".to_owned(), JsonValue::String(type_url.clone()));

    if is_value_wrapper_wkt {
        // Wrap non-object inner value under "value" key.
        obj.insert("value".to_owned(), inner_json);
    } else {
        // Inline the fields of the inner message.
        if let JsonValue::Object(inner_obj) = inner_json {
            for (k, v) in inner_obj {
                obj.insert(k, v);
            }
        }
    }

    JsonValue::Object(obj)
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
