#![forbid(unsafe_code)]

//! Emission of `to_json`/`from_json` methods and `impl` blocks for generated message/enum types.
//!
//! This module produces self-contained JSON (de)serialization code that follows
//! the canonical Protobuf-JSON mapping (proto3 JSON).  The generated code calls
//! directly into `::serde_json` and `::base64` — both must be available in the
//! consumer crate (they are NOT re-exported by oxiproto-codegen).
//!
//! # WKT handling
//!
//! `google.protobuf.Timestamp` and `google.protobuf.Duration` are serialised via
//! the `TimestampExt` / `DurationExt` trait methods from `::oxiproto_wkt`.  The
//! consumer crate must also have `oxiproto-wkt` in its dependencies when these WKT
//! fields are present.

use prost_types::{
    field_descriptor_proto::{Label, Type},
    DescriptorProto, EnumDescriptorProto,
};

use crate::options::CodegenError;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Check whether a raw proto type name refers to a Timestamp WKT.
fn is_wkt_timestamp(raw_type_name: &str) -> bool {
    let n = raw_type_name.trim_start_matches('.');
    n == "google.protobuf.Timestamp"
}

/// Check whether a raw proto type name refers to a Duration WKT.
fn is_wkt_duration(raw_type_name: &str) -> bool {
    let n = raw_type_name.trim_start_matches('.');
    n == "google.protobuf.Duration"
}

/// Check whether a raw proto type name refers to a map-entry message.
/// Map entries are silently skipped — they are emitted as `HashMap`/`BTreeMap` fields.
fn is_map_entry_type(raw_type_name: &str) -> bool {
    // Map entry type names end with "Entry" — but the real check is done via
    // `map_field_names` set that is passed in.
    let _ = raw_type_name;
    false
}

/// The Rust `to_pascal_case` helper (mirrors the one in `emit.rs`).
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
            }
        })
        .collect()
}

// ── per-type default-check expressions ───────────────────────────────────────

/// Returns a boolean Rust expression (`value_expr is default`) matching proto3
/// default-value omission rules for scalar fields.
fn is_default_scalar(ftype: i32, value_expr: &str) -> String {
    match ftype {
        t if t == Type::String as i32 => format!("({value_expr}).is_empty()"),
        t if t == Type::Bytes as i32 => format!("({value_expr}).is_empty()"),
        t if t == Type::Bool as i32 => format!("!({value_expr})"),
        t if t == Type::Float as i32 => format!("({value_expr}) == 0.0f32"),
        t if t == Type::Double as i32 => format!("({value_expr}) == 0.0f64"),
        // All integer types, enums: default == 0
        _ => format!("({value_expr}) == 0"),
    }
}

// ── to_json scalar expressions ────────────────────────────────────────────────

/// Returns a Rust expression that converts a scalar value (`value_expr`) to a
/// `::serde_json::Value`.  `ftype` is the proto field type.
fn scalar_to_json_expr(ftype: i32, value_expr: &str) -> String {
    match ftype {
        // 64-bit integers — JSON spec mandates string representation
        t if t == Type::Int64 as i32 || t == Type::Sint64 as i32 || t == Type::Sfixed64 as i32 => {
            format!("::serde_json::Value::String(({value_expr}).to_string())")
        }
        t if t == Type::Uint64 as i32 || t == Type::Fixed64 as i32 => {
            format!("::serde_json::Value::String(({value_expr}).to_string())")
        }
        // float / double — NaN and Inf map to specific strings
        t if t == Type::Float as i32 => {
            format!("{{ let _f = ({value_expr}) as f64; if _f.is_nan() {{ ::serde_json::Value::String(\"NaN\".to_string()) }} else if _f == f64::INFINITY {{ ::serde_json::Value::String(\"Infinity\".to_string()) }} else if _f == f64::NEG_INFINITY {{ ::serde_json::Value::String(\"-Infinity\".to_string()) }} else {{ ::serde_json::Number::from_f64(_f).map(::serde_json::Value::Number).unwrap_or(::serde_json::Value::Null) }} }}")
        }
        t if t == Type::Double as i32 => {
            format!("{{ let _f = ({value_expr}) as f64; if _f.is_nan() {{ ::serde_json::Value::String(\"NaN\".to_string()) }} else if _f == f64::INFINITY {{ ::serde_json::Value::String(\"Infinity\".to_string()) }} else if _f == f64::NEG_INFINITY {{ ::serde_json::Value::String(\"-Infinity\".to_string()) }} else {{ ::serde_json::Number::from_f64(_f).map(::serde_json::Value::Number).unwrap_or(::serde_json::Value::Null) }} }}")
        }
        // bytes — base64 encoded
        t if t == Type::Bytes as i32 => {
            format!("::serde_json::Value::String(::base64::engine::general_purpose::STANDARD.encode(&({value_expr})))")
        }
        // bool
        t if t == Type::Bool as i32 => {
            format!("::serde_json::Value::Bool({value_expr})")
        }
        // string
        t if t == Type::String as i32 => {
            format!("::serde_json::Value::String(({value_expr}).clone())")
        }
        // 32-bit signed integers
        t if t == Type::Int32 as i32 || t == Type::Sint32 as i32 || t == Type::Sfixed32 as i32 => {
            format!("::serde_json::Value::Number(({value_expr}).into())")
        }
        // 32-bit unsigned integers
        t if t == Type::Uint32 as i32 || t == Type::Fixed32 as i32 => {
            format!("::serde_json::Value::Number(({value_expr}).into())")
        }
        // enum / fallback
        _ => {
            format!("::serde_json::Value::Number(({value_expr} as i32).into())")
        }
    }
}

/// Returns a Rust expression that converts a map-key to a JSON object key string.
fn map_key_to_string_expr(ftype: i32, value_expr: &str) -> String {
    match ftype {
        t if t == Type::Bool as i32 => {
            format!("(if {value_expr} {{ \"true\".to_string() }} else {{ \"false\".to_string() }})")
        }
        t if t == Type::String as i32 => format!("({value_expr}).clone()"),
        // All integer types
        _ => format!("({value_expr}).to_string()"),
    }
}

// ── from_json scalar decoding ─────────────────────────────────────────────────

/// Returns a Rust expression that decodes a `&::serde_json::Value` reference
/// (`value_expr`) into the Rust scalar type for `ftype`.  The expression returns
/// `Result<T, JsonError>`.
fn scalar_from_json_expr(ftype: i32, value_expr: &str, field_name: &str) -> String {
    match ftype {
        // String
        t if t == Type::String as i32 => format!(
            "match {value_expr} {{ ::serde_json::Value::String(_s) => Ok(_s.clone()), ::serde_json::Value::Null => Ok(String::new()), _other => Err(JsonError::WrongType {{ field: \"{field_name}\".to_string(), expected: \"string\", got: _json_type(_other) }}) }}"
        ),
        // Bytes (base64)
        t if t == Type::Bytes as i32 => format!(
            "match {value_expr} {{ ::serde_json::Value::String(_s) => ::base64::engine::general_purpose::STANDARD.decode(_s.as_bytes()).map_err(|_e| JsonError::MalformedValue(format!(\"base64 decode failed for field '{field_name}': {{_e}}\"))), ::serde_json::Value::Null => Ok(Vec::new()), _other => Err(JsonError::WrongType {{ field: \"{field_name}\".to_string(), expected: \"base64 string\", got: _json_type(_other) }}) }}"
        ),
        // Bool
        t if t == Type::Bool as i32 => format!(
            "match {value_expr} {{ ::serde_json::Value::Bool(_b) => Ok(*_b), ::serde_json::Value::String(_s) if _s == \"true\" => Ok(true), ::serde_json::Value::String(_s) if _s == \"false\" => Ok(false), ::serde_json::Value::Null => Ok(false), _other => Err(JsonError::WrongType {{ field: \"{field_name}\".to_string(), expected: \"bool\", got: _json_type(_other) }}) }}"
        ),
        // 32-bit signed int
        t if t == Type::Int32 as i32
            || t == Type::Sint32 as i32
            || t == Type::Sfixed32 as i32 =>
        {
            format!("match {value_expr} {{ ::serde_json::Value::Number(_n) => _n.as_i64().map(|_v| _v as i32).ok_or_else(|| JsonError::MalformedValue(format!(\"cannot parse field '{field_name}' as i32\"))), ::serde_json::Value::Null => Ok(0i32), _other => Err(JsonError::WrongType {{ field: \"{field_name}\".to_string(), expected: \"number\", got: _json_type(_other) }}) }}")
        }
        // 32-bit unsigned int
        t if t == Type::Uint32 as i32 || t == Type::Fixed32 as i32 => {
            format!("match {value_expr} {{ ::serde_json::Value::Number(_n) => _n.as_u64().map(|_v| _v as u32).ok_or_else(|| JsonError::MalformedValue(format!(\"cannot parse field '{field_name}' as u32\"))), ::serde_json::Value::Null => Ok(0u32), _other => Err(JsonError::WrongType {{ field: \"{field_name}\".to_string(), expected: \"number\", got: _json_type(_other) }}) }}")
        }
        // 64-bit signed int — accepts Number OR String
        t if t == Type::Int64 as i32
            || t == Type::Sint64 as i32
            || t == Type::Sfixed64 as i32 =>
        {
            format!("match {value_expr} {{ ::serde_json::Value::Number(_n) => _n.as_i64().ok_or_else(|| JsonError::MalformedValue(format!(\"cannot parse field '{field_name}' as i64\"))), ::serde_json::Value::String(_s) => _s.parse::<i64>().map_err(|_e| JsonError::MalformedValue(format!(\"cannot parse field '{field_name}' as i64: {{_e}}\"))), ::serde_json::Value::Null => Ok(0i64), _other => Err(JsonError::WrongType {{ field: \"{field_name}\".to_string(), expected: \"number or string\", got: _json_type(_other) }}) }}")
        }
        // 64-bit unsigned int — accepts Number OR String
        t if t == Type::Uint64 as i32 || t == Type::Fixed64 as i32 => {
            format!("match {value_expr} {{ ::serde_json::Value::Number(_n) => _n.as_u64().ok_or_else(|| JsonError::MalformedValue(format!(\"cannot parse field '{field_name}' as u64\"))), ::serde_json::Value::String(_s) => _s.parse::<u64>().map_err(|_e| JsonError::MalformedValue(format!(\"cannot parse field '{field_name}' as u64: {{_e}}\"))), ::serde_json::Value::Null => Ok(0u64), _other => Err(JsonError::WrongType {{ field: \"{field_name}\".to_string(), expected: \"number or string\", got: _json_type(_other) }}) }}")
        }
        // float
        t if t == Type::Float as i32 => {
            format!("match {value_expr} {{ ::serde_json::Value::Number(_n) => Ok(_n.as_f64().unwrap_or(0.0) as f32), ::serde_json::Value::String(_s) => match _s.as_str() {{ \"NaN\" => Ok(f32::NAN), \"Infinity\" => Ok(f32::INFINITY), \"-Infinity\" => Ok(f32::NEG_INFINITY), _other => _other.parse::<f32>().map_err(|_e| JsonError::MalformedValue(format!(\"cannot parse field '{field_name}' as f32: {{_e}}\"))) }}, ::serde_json::Value::Null => Ok(0.0f32), _other => Err(JsonError::WrongType {{ field: \"{field_name}\".to_string(), expected: \"number or float-string\", got: _json_type(_other) }}) }}")
        }
        // double
        t if t == Type::Double as i32 => {
            format!("match {value_expr} {{ ::serde_json::Value::Number(_n) => Ok(_n.as_f64().unwrap_or(0.0)), ::serde_json::Value::String(_s) => match _s.as_str() {{ \"NaN\" => Ok(f64::NAN), \"Infinity\" => Ok(f64::INFINITY), \"-Infinity\" => Ok(f64::NEG_INFINITY), _other => _other.parse::<f64>().map_err(|_e| JsonError::MalformedValue(format!(\"cannot parse field '{field_name}' as f64: {{_e}}\"))) }}, ::serde_json::Value::Null => Ok(0.0f64), _other => Err(JsonError::WrongType {{ field: \"{field_name}\".to_string(), expected: \"number or float-string\", got: _json_type(_other) }}) }}")
        }
        // Enum fallback — callers should use `EnumType::from_json_value` instead;
        // this branch handles "enum as integer number" for map-value use
        _ => {
            format!("match {value_expr} {{ ::serde_json::Value::Number(_n) => Ok(_n.as_i64().unwrap_or(0) as i32), ::serde_json::Value::Null => Ok(0i32), _other => Err(JsonError::WrongType {{ field: \"{field_name}\".to_string(), expected: \"number\", got: _json_type(_other) }}) }}")
        }
    }
}

/// Parse a map key from a JSON object key string back to the Rust key type.
fn parse_map_key_expr(ftype: i32, key_str_expr: &str, field_name: &str) -> String {
    match ftype {
        t if t == Type::Bool as i32 => {
            format!("match {key_str_expr} {{ \"true\" => Ok(true), \"false\" => Ok(false), _s => Err(JsonError::MalformedValue(format!(\"cannot parse map key for '{field_name}' as bool: {{_s}}\"))) }}")
        }
        t if t == Type::String as i32 => {
            format!("Ok::<String, JsonError>({key_str_expr}.to_string())")
        }
        t if t == Type::Int32 as i32 || t == Type::Sint32 as i32 || t == Type::Sfixed32 as i32 => {
            format!("{key_str_expr}.parse::<i32>().map_err(|_e| JsonError::MalformedValue(format!(\"cannot parse map key for '{field_name}' as i32: {{_e}}\"))) ")
        }
        t if t == Type::Uint32 as i32 || t == Type::Fixed32 as i32 => {
            format!("{key_str_expr}.parse::<u32>().map_err(|_e| JsonError::MalformedValue(format!(\"cannot parse map key for '{field_name}' as u32: {{_e}}\"))) ")
        }
        t if t == Type::Int64 as i32 || t == Type::Sint64 as i32 || t == Type::Sfixed64 as i32 => {
            format!("{key_str_expr}.parse::<i64>().map_err(|_e| JsonError::MalformedValue(format!(\"cannot parse map key for '{field_name}' as i64: {{_e}}\"))) ")
        }
        t if t == Type::Uint64 as i32 || t == Type::Fixed64 as i32 => {
            format!("{key_str_expr}.parse::<u64>().map_err(|_e| JsonError::MalformedValue(format!(\"cannot parse map key for '{field_name}' as u64: {{_e}}\"))) ")
        }
        _ => format!("Ok::<String, JsonError>({key_str_expr}.to_string())"),
    }
}

// ── file-level prelude ────────────────────────────────────────────────────────

/// Emit the per-file prelude: `JsonError` type, `_json_type` helper, and the
/// `use ::base64::Engine as _` import required by generated `to_json` code.
/// This is emitted **once per generated file**, before any message impls.
pub(crate) fn emit_json_file_prelude() -> String {
    let mut out = String::new();

    out.push_str("use ::base64::Engine as _;\n\n");

    // JsonError enum
    out.push_str("/// Error type for Protobuf-JSON decoding (generated).\n");
    out.push_str("#[derive(Debug)]\n");
    out.push_str("pub enum JsonError {\n");
    out.push_str("    /// The JSON value had an unexpected type for a field.\n");
    out.push_str("    WrongType {\n");
    out.push_str("        /// Field context.\n");
    out.push_str("        field: String,\n");
    out.push_str("        /// Expected type.\n");
    out.push_str("        expected: &'static str,\n");
    out.push_str("        /// Actual type.\n");
    out.push_str("        got: &'static str,\n");
    out.push_str("    },\n");
    out.push_str("    /// A value could not be decoded.\n");
    out.push_str("    MalformedValue(String),\n");
    out.push_str("}\n\n");

    out.push_str("impl ::core::fmt::Display for JsonError {\n");
    out.push_str(
        "    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {\n",
    );
    out.push_str("        match self {\n");
    out.push_str("            JsonError::WrongType { field, expected, got } =>\n");
    out.push_str(
        "                write!(f, \"field '{field}': expected {expected}, got {got}\"),\n",
    );
    out.push_str("            JsonError::MalformedValue(msg) =>\n");
    out.push_str("                write!(f, \"malformed value: {msg}\"),\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    out.push_str("impl ::std::error::Error for JsonError {}\n\n");

    // _json_type helper
    out.push_str("#[allow(dead_code)]\n");
    out.push_str("fn _json_type(v: &::serde_json::Value) -> &'static str {\n");
    out.push_str("    match v {\n");
    out.push_str("        ::serde_json::Value::Null => \"null\",\n");
    out.push_str("        ::serde_json::Value::Bool(_) => \"bool\",\n");
    out.push_str("        ::serde_json::Value::Number(_) => \"number\",\n");
    out.push_str("        ::serde_json::Value::String(_) => \"string\",\n");
    out.push_str("        ::serde_json::Value::Array(_) => \"array\",\n");
    out.push_str("        ::serde_json::Value::Object(_) => \"object\",\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    out
}

// ── enum JSON impl ────────────────────────────────────────────────────────────

/// Emit `to_json_str` and `from_json_value` methods on a proto enum type.
pub(crate) fn emit_enum_json_impl(
    en: &EnumDescriptorProto,
    name: &str,
) -> Result<String, CodegenError> {
    let mut out = String::new();

    // Suppress clippy::wrong_self_convention: to_json_str takes &self but returns str,
    // which is intentional for the proto-JSON API — not a bug in convention.
    out.push_str("#[allow(clippy::all, clippy::wrong_self_convention)]\n");
    out.push_str(&format!("impl {name} {{\n"));
    out.push_str("    /// Canonical proto-JSON name for this enum variant.\n");
    out.push_str("    pub fn to_json_str(&self) -> &'static str {\n");
    out.push_str("        match self {\n");
    for val in &en.value {
        let vname = val
            .name
            .as_deref()
            .ok_or_else(|| CodegenError::InvalidDescriptor("enum value missing name".into()))?;
        let variant = to_pascal_case(vname);
        out.push_str(&format!("            {name}::{variant} => \"{vname}\",\n"));
    }
    out.push_str("        }\n");
    out.push_str("    }\n\n");

    out.push_str("    /// Decode a `serde_json::Value` into this enum type.\n");
    out.push_str("    pub fn from_json_value(v: &::serde_json::Value) -> ::core::result::Result<Self, JsonError> {\n");
    out.push_str("        match v {\n");
    out.push_str("            ::serde_json::Value::String(_s) => match _s.as_str() {\n");
    for val in &en.value {
        let vname = val
            .name
            .as_deref()
            .ok_or_else(|| CodegenError::InvalidDescriptor("enum value missing name".into()))?;
        let variant = to_pascal_case(vname);
        out.push_str(&format!(
            "                \"{vname}\" => Ok({name}::{variant}),\n"
        ));
    }
    out.push_str(&format!(
        "                _unknown => Err(JsonError::MalformedValue(format!(\"unknown {name} variant: {{_unknown}}\"))),\n"
    ));
    out.push_str("            },\n");
    out.push_str("            ::serde_json::Value::Number(_n) => {\n");
    out.push_str("                let _i = _n.as_i64().unwrap_or(0) as i32;\n");
    out.push_str(&format!(
        "                {name}::from_i32(_i).ok_or_else(|| JsonError::MalformedValue(format!(\"unknown {name} discriminant: {{_i}}\")))\n"
    ));
    out.push_str("            },\n");
    out.push_str(&format!(
        "            _other => Err(JsonError::WrongType {{ field: \"{name}\".to_string(), expected: \"string or number\", got: _json_type(_other) }}),\n"
    ));
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    Ok(out)
}

// ── message JSON impls ────────────────────────────────────────────────────────

/// Collect information about map-entry nested types for a message.
struct MapEntryInfo {
    field_name: String,
    key_ftype: i32,
    val_ftype: i32,
    val_type_name: String, // For message value types
}

fn collect_map_entry_info(msg: &DescriptorProto) -> Vec<MapEntryInfo> {
    let mut result = Vec::new();
    for nested in &msg.nested_type {
        let is_map_entry = nested.options.as_ref().is_some_and(|o| o.map_entry());
        if !is_map_entry {
            continue;
        }
        let entry_name = nested.name.as_deref().unwrap_or("");
        for field in &msg.field {
            let type_name = field.type_name.as_deref().unwrap_or("");
            let type_last = type_name.split('.').next_back().unwrap_or("");
            if type_last != entry_name {
                continue;
            }
            let field_name = match field.name.as_deref() {
                Some(n) if !n.is_empty() => n.to_string(),
                _ => continue,
            };
            let key_field = nested
                .field
                .iter()
                .find(|f| f.name.as_deref() == Some("key"));
            let val_field = nested
                .field
                .iter()
                .find(|f| f.name.as_deref() == Some("value"));
            if let (Some(kf), Some(vf)) = (key_field, val_field) {
                let key_ftype = kf.r#type.unwrap_or(Type::String as i32);
                let val_ftype = vf.r#type.unwrap_or(Type::String as i32);
                let val_type_name = vf.type_name.as_deref().unwrap_or("").to_string();
                result.push(MapEntryInfo {
                    field_name,
                    key_ftype,
                    val_ftype,
                    val_type_name,
                });
            }
        }
    }
    result
}

/// Emit `impl Foo { pub fn to_json(&self) -> ::serde_json::Value { ... }
///                   pub fn from_json(value: &::serde_json::Value) -> Result<Self, JsonError> { ... } }`
/// for a message type.
///
/// Parameters match those available at the call site in `emit.rs`.
pub(crate) fn emit_json_impls(
    msg: &DescriptorProto,
    full_name: &str,
    file_package: &str,
    map_field_names: &std::collections::HashSet<String>,
    registry: &crate::type_registry::TypeRegistry,
) -> Result<String, CodegenError> {
    let map_entries = collect_map_entry_info(msg);
    let map_entry_map: std::collections::HashMap<String, &MapEntryInfo> = map_entries
        .iter()
        .map(|e| (e.field_name.clone(), e))
        .collect();

    let mut out = String::new();

    // Determine which oneof indices exist and collect their info
    let oneof_count = msg.oneof_decl.len();
    let mut emitted_oneofs = vec![false; oneof_count];

    // Suppress all clippy and style lints for the generated impl block.
    out.push_str("#[allow(clippy::all, clippy::wrong_self_convention, non_camel_case_types, clippy::enum_variant_names, clippy::needless_match, clippy::unnecessary_cast)]\n");
    out.push_str(&format!("impl {full_name} {{\n"));
    out.push_str(
        "    /// Serialise this message to a canonical Protobuf-JSON `serde_json::Value`.\n",
    );
    out.push_str("    pub fn to_json(&self) -> ::serde_json::Value {\n");
    out.push_str("        let mut _map = ::serde_json::Map::new();\n");

    // Emit each field
    for field in &msg.field {
        let fname = field
            .name
            .as_deref()
            .ok_or_else(|| CodegenError::InvalidDescriptor("field missing name".into()))?;

        // JSON key: use json_name if present (proto3 uses camelCase), else fname
        let json_key = field
            .json_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(fname);

        let ftype = field.r#type.unwrap_or(Type::String as i32);
        let label = field.label.unwrap_or(Label::Optional as i32);
        let is_repeated = label == Label::Repeated as i32;
        let raw_type_name = field.type_name.as_deref().unwrap_or("");

        // Handle oneof fields
        if let Some(oneof_idx) = field.oneof_index {
            let oi = oneof_idx as usize;
            if oi < oneof_count && !emitted_oneofs[oi] {
                emitted_oneofs[oi] = true;
                let oneof_name = msg
                    .oneof_decl
                    .get(oi)
                    .and_then(|o| o.name.as_deref())
                    .unwrap_or("unknown");
                let oneof_type = format!("{full_name}_{}", to_pascal_case(oneof_name));
                out.push_str(&format!(
                    "        if let Some(ref _ov) = self.{oneof_name} {{\n"
                ));
                out.push_str("            match _ov {\n");
                // Emit all variants for this oneof
                for of in &msg.field {
                    if of.oneof_index != Some(oneof_idx) {
                        continue;
                    }
                    let vname = of.name.as_deref().unwrap_or("unknown");
                    let variant = to_pascal_case(vname);
                    let vjson_key = of
                        .json_name
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(vname);
                    let vtype = of.r#type.unwrap_or(Type::String as i32);
                    let vraw_type = of.type_name.as_deref().unwrap_or("");
                    let val_expr = if vtype == Type::Message as i32 {
                        "_inner.to_json()".to_string()
                    } else if vtype == Type::Enum as i32 {
                        let enum_type_name = registry.resolve(file_package, vraw_type);
                        format!("::serde_json::Value::String({enum_type_name}::to_json_str(_val).to_string())")
                    } else {
                        scalar_to_json_expr(vtype, "*_val")
                    };
                    if vtype == Type::Message as i32 {
                        out.push_str(&format!(
                            "                {oneof_type}::{variant}(_inner) => {{ _map.insert(\"{vjson_key}\".to_string(), {val_expr}); }}\n"
                        ));
                    } else {
                        out.push_str(&format!(
                            "                {oneof_type}::{variant}(_val) => {{ _map.insert(\"{vjson_key}\".to_string(), {val_expr}); }}\n"
                        ));
                    }
                }
                out.push_str("            }\n");
                out.push_str("        }\n");
            }
            continue;
        }

        // Map fields
        if map_field_names.contains(fname) {
            let mei = match map_entry_map.get(fname) {
                Some(m) => m,
                None => continue,
            };
            out.push_str(&format!("        if !self.{fname}.is_empty() {{\n"));
            out.push_str("            let mut _jmap = ::serde_json::Map::new();\n");
            out.push_str(&format!("            for (_mk, _mv) in &self.{fname} {{\n"));
            let key_str = map_key_to_string_expr(mei.key_ftype, "_mk");
            let val_expr = if mei.val_ftype == Type::Message as i32 {
                "_mv.to_json()".to_string()
            } else if mei.val_ftype == Type::Enum as i32 {
                let enum_type_name = registry.resolve(file_package, &mei.val_type_name);
                format!(
                    "::serde_json::Value::String({enum_type_name}::to_json_str(_mv).to_string())"
                )
            } else {
                scalar_to_json_expr(mei.val_ftype, "*_mv")
            };
            out.push_str(&format!("                let _ks = {key_str};\n"));
            out.push_str(&format!("                _jmap.insert(_ks, {val_expr});\n"));
            out.push_str("            }\n");
            out.push_str(&format!(
                "            _map.insert(\"{json_key}\".to_string(), ::serde_json::Value::Object(_jmap));\n"
            ));
            out.push_str("        }\n");
            continue;
        }

        // WKT Timestamp
        if ftype == Type::Message as i32 && is_wkt_timestamp(raw_type_name) {
            out.push_str(&format!(
                "        if self.{fname}.seconds != 0 || self.{fname}.nanos != 0 {{\n"
            ));
            out.push_str(&format!(
                "            match ::oxiproto_wkt::TimestampExt::to_rfc3339(&self.{fname}) {{\n"
            ));
            out.push_str(&format!(
                "                Ok(_s) => {{ _map.insert(\"{json_key}\".to_string(), ::serde_json::Value::String(_s)); }}\n"
            ));
            out.push_str("                Err(_) => {}\n");
            out.push_str("            }\n");
            out.push_str("        }\n");
            continue;
        }

        // WKT Duration
        if ftype == Type::Message as i32 && is_wkt_duration(raw_type_name) {
            out.push_str(&format!(
                "        if self.{fname}.seconds != 0 || self.{fname}.nanos != 0 {{\n"
            ));
            out.push_str(&format!(
                "            let _ds = ::oxiproto_wkt::DurationExt::to_duration_string(&self.{fname});\n"
            ));
            out.push_str(&format!(
                "            _map.insert(\"{json_key}\".to_string(), ::serde_json::Value::String(_ds));\n"
            ));
            out.push_str("        }\n");
            continue;
        }

        // Repeated fields
        if is_repeated {
            if ftype == Type::Message as i32 {
                out.push_str(&format!("        if !self.{fname}.is_empty() {{\n"));
                out.push_str(&format!(
                    "            let _arr: ::serde_json::Value = ::serde_json::Value::Array(self.{fname}.iter().map(|_item| _item.to_json()).collect());\n"
                ));
                out.push_str(&format!(
                    "            _map.insert(\"{json_key}\".to_string(), _arr);\n"
                ));
                out.push_str("        }\n");
            } else if ftype == Type::Enum as i32 {
                let enum_type_name = registry.resolve(file_package, raw_type_name);
                out.push_str(&format!("        if !self.{fname}.is_empty() {{\n"));
                out.push_str(&format!(
                    "            let _arr: ::serde_json::Value = ::serde_json::Value::Array(self.{fname}.iter().map(|_item| ::serde_json::Value::String({enum_type_name}::to_json_str(_item).to_string())).collect());\n"
                ));
                out.push_str(&format!(
                    "            _map.insert(\"{json_key}\".to_string(), _arr);\n"
                ));
                out.push_str("        }\n");
            } else {
                out.push_str(&format!("        if !self.{fname}.is_empty() {{\n"));
                let elem_expr = scalar_to_json_expr(ftype, "_item");
                // For repeated scalars that are Copy (integers, bool, floats), deref the item
                let iter_pattern = if matches!(ftype,
                    t if t == Type::String as i32 || t == Type::Bytes as i32
                ) {
                    // String/Bytes: pass by ref
                    format!("self.{fname}.iter().map(|_item| {elem_expr})")
                } else {
                    // Scalars: deref copy
                    let deref_expr = scalar_to_json_expr(ftype, "*_item");
                    format!("self.{fname}.iter().map(|_item| {deref_expr})")
                };
                out.push_str(&format!(
                    "            let _arr: ::serde_json::Value = ::serde_json::Value::Array({iter_pattern}.collect());\n"
                ));
                out.push_str(&format!(
                    "            _map.insert(\"{json_key}\".to_string(), _arr);\n"
                ));
                out.push_str("        }\n");
            }
            continue;
        }

        // Singular message (Option<Box<T>>)
        if ftype == Type::Message as i32 {
            out.push_str(&format!("        if let Some(ref _v) = self.{fname} {{\n"));
            out.push_str(&format!(
                "            _map.insert(\"{json_key}\".to_string(), _v.to_json());\n"
            ));
            out.push_str("        }\n");
            continue;
        }

        // Singular enum
        if ftype == Type::Enum as i32 {
            let enum_type_name = registry.resolve(file_package, raw_type_name);
            // Enum default is 0 (first variant) — omit when default
            out.push_str(&format!("        if (self.{fname} as i32) != 0 {{\n"));
            out.push_str(&format!(
                "            _map.insert(\"{json_key}\".to_string(), ::serde_json::Value::String({enum_type_name}::to_json_str(&self.{fname}).to_string()));\n"
            ));
            out.push_str("        }\n");
            continue;
        }

        // Singular scalar — omit when default
        let value_expr = format!("self.{fname}");
        let default_check = is_default_scalar(ftype, &value_expr);
        let json_val = scalar_to_json_expr(ftype, &value_expr);
        out.push_str(&format!("        if !({default_check}) {{\n"));
        out.push_str(&format!(
            "            _map.insert(\"{json_key}\".to_string(), {json_val});\n"
        ));
        out.push_str("        }\n");
    }

    out.push_str("        ::serde_json::Value::Object(_map)\n");
    out.push_str("    }\n\n");

    // ── from_json ──────────────────────────────────────────────────────────────

    out.push_str(
        "    /// Deserialise this message from a canonical Protobuf-JSON `serde_json::Value`.\n",
    );
    out.push_str("    pub fn from_json(value: &::serde_json::Value) -> ::core::result::Result<Self, JsonError> {\n");
    out.push_str("        let _obj = match value {\n");
    out.push_str("            ::serde_json::Value::Object(_o) => _o,\n");
    out.push_str(&format!(
        "            _other => return Err(JsonError::WrongType {{ field: \"{full_name}\".to_string(), expected: \"object\", got: _json_type(_other) }}),\n"
    ));
    out.push_str("        };\n");
    out.push_str("        let mut _out = Self::default();\n");
    out.push_str("        for (_k, _v) in _obj {\n");
    out.push_str("            match _k.as_str() {\n");

    // Build match arms — collect all field keys first to dedup camelCase==snake_case
    let mut arm_added_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Reset oneof tracking for from_json
    let mut from_json_oneof_emitted = vec![false; oneof_count];

    for field in &msg.field {
        let fname = field
            .name
            .as_deref()
            .ok_or_else(|| CodegenError::InvalidDescriptor("field missing name".into()))?;

        let json_key = field
            .json_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(fname);

        let ftype = field.r#type.unwrap_or(Type::String as i32);
        let label = field.label.unwrap_or(Label::Optional as i32);
        let is_repeated = label == Label::Repeated as i32;
        let raw_type_name = field.type_name.as_deref().unwrap_or("");

        // Handle oneof fields
        if let Some(oneof_idx) = field.oneof_index {
            let oi = oneof_idx as usize;
            if oi < oneof_count && !from_json_oneof_emitted[oi] {
                from_json_oneof_emitted[oi] = true;
                let oneof_name = msg
                    .oneof_decl
                    .get(oi)
                    .and_then(|o| o.name.as_deref())
                    .unwrap_or("unknown");
                // Full qualified oneof enum type (e.g. "OneofMsg_Value")
                let oneof_type = format!("{full_name}_{}", to_pascal_case(oneof_name));
                // Emit one arm per variant
                for of in &msg.field {
                    if of.oneof_index != Some(oneof_idx) {
                        continue;
                    }
                    let vname = of.name.as_deref().unwrap_or("unknown");
                    let vjson_key = of
                        .json_name
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(vname);
                    let variant = to_pascal_case(vname);
                    let vtype = of.r#type.unwrap_or(Type::String as i32);
                    let vraw_type = of.type_name.as_deref().unwrap_or("");

                    // Determine match pattern (dedup camelCase == snake_case)
                    let pattern = build_match_pattern(vjson_key, vname, &mut arm_added_keys);
                    if let Some(pat) = pattern {
                        out.push_str(&format!("                {pat} => {{\n"));
                        out.push_str(
                            "                    if !matches!(_v, ::serde_json::Value::Null) {\n",
                        );
                        if vtype == Type::Message as i32 {
                            let inner_type = registry.resolve(file_package, vraw_type);
                            out.push_str(&format!(
                                "                        let _decoded = {inner_type}::from_json(_v)?;\n"
                            ));
                            out.push_str(&format!(
                                "                        _out.{oneof_name} = Some({oneof_type}::{variant}(Box::new(_decoded)));\n"
                            ));
                        } else if vtype == Type::Enum as i32 {
                            let enum_type = registry.resolve(file_package, vraw_type);
                            out.push_str(&format!(
                                "                        let _decoded = {enum_type}::from_json_value(_v)?;\n"
                            ));
                            out.push_str(&format!(
                                "                        _out.{oneof_name} = Some({oneof_type}::{variant}(_decoded));\n"
                            ));
                        } else {
                            let from_expr = scalar_from_json_expr(vtype, "_v", vname);
                            out.push_str(&format!(
                                "                        let _decoded = {from_expr}?;\n"
                            ));
                            out.push_str(&format!(
                                "                        _out.{oneof_name} = Some({oneof_type}::{variant}(_decoded));\n"
                            ));
                        }
                        out.push_str("                    }\n");
                        out.push_str("                }\n");
                    }
                }
            }
            continue;
        }

        // Map fields
        if map_field_names.contains(fname) {
            let mei = match map_entry_map.get(fname) {
                Some(m) => m,
                None => continue,
            };
            let pattern = build_match_pattern(json_key, fname, &mut arm_added_keys);
            if let Some(pat) = pattern {
                out.push_str(&format!("                {pat} => {{\n"));
                out.push_str(
                    "                    if let ::serde_json::Value::Object(_mo) = _v {\n",
                );
                out.push_str("                        for (_mk_str, _mv_val) in _mo {\n");
                let key_parse = parse_map_key_expr(mei.key_ftype, "_mk_str.as_str()", fname);
                out.push_str(&format!(
                    "                            let _parsed_key = {key_parse}?;\n"
                ));
                if mei.val_ftype == Type::Message as i32 {
                    let inner_type = registry.resolve(file_package, &mei.val_type_name);
                    out.push_str(&format!(
                        "                            let _parsed_val = {inner_type}::from_json(_mv_val)?;\n"
                    ));
                } else if mei.val_ftype == Type::Enum as i32 {
                    let enum_type = registry.resolve(file_package, &mei.val_type_name);
                    out.push_str(&format!(
                        "                            let _parsed_val = {enum_type}::from_json_value(_mv_val)?;\n"
                    ));
                } else {
                    let val_expr = scalar_from_json_expr(mei.val_ftype, "_mv_val", fname);
                    out.push_str(&format!(
                        "                            let _parsed_val = {val_expr}?;\n"
                    ));
                }
                out.push_str(&format!(
                    "                            _out.{fname}.insert(_parsed_key, _parsed_val);\n"
                ));
                out.push_str("                        }\n");
                out.push_str("                    }\n");
                out.push_str("                }\n");
            }
            continue;
        }

        // WKT Timestamp
        if ftype == Type::Message as i32 && is_wkt_timestamp(raw_type_name) {
            let pattern = build_match_pattern(json_key, fname, &mut arm_added_keys);
            if let Some(pat) = pattern {
                out.push_str(&format!("                {pat} => {{\n"));
                out.push_str("                    if let ::serde_json::Value::String(_s) = _v {\n");
                out.push_str(&format!(
                    "                        _out.{fname} = ::oxiproto_wkt::TimestampExt::from_rfc3339(_s).map_err(|_e| JsonError::MalformedValue(format!(\"invalid Timestamp for field '{fname}': {{_e}}\")))?;\n"
                ));
                out.push_str("                    }\n");
                out.push_str("                }\n");
            }
            continue;
        }

        // WKT Duration
        if ftype == Type::Message as i32 && is_wkt_duration(raw_type_name) {
            let pattern = build_match_pattern(json_key, fname, &mut arm_added_keys);
            if let Some(pat) = pattern {
                out.push_str(&format!("                {pat} => {{\n"));
                out.push_str("                    if let ::serde_json::Value::String(_s) = _v {\n");
                out.push_str(&format!(
                    "                        _out.{fname} = ::oxiproto_wkt::DurationExt::from_duration_string(_s).map_err(|_e| JsonError::MalformedValue(format!(\"invalid Duration for field '{fname}': {{_e}}\")))?;\n"
                ));
                out.push_str("                    }\n");
                out.push_str("                }\n");
            }
            continue;
        }

        // Repeated fields
        if is_repeated {
            let pattern = build_match_pattern(json_key, fname, &mut arm_added_keys);
            if let Some(pat) = pattern {
                out.push_str(&format!("                {pat} => {{\n"));
                out.push_str(
                    "                    if let ::serde_json::Value::Array(_arr) = _v {\n",
                );
                out.push_str("                        for _item in _arr {\n");
                if ftype == Type::Message as i32 {
                    let inner_type = registry.resolve(file_package, raw_type_name);
                    out.push_str(&format!(
                        "                            _out.{fname}.push({inner_type}::from_json(_item)?);\n"
                    ));
                } else if ftype == Type::Enum as i32 {
                    let enum_type = registry.resolve(file_package, raw_type_name);
                    out.push_str(&format!(
                        "                            _out.{fname}.push({enum_type}::from_json_value(_item)?);\n"
                    ));
                } else {
                    let elem_expr = scalar_from_json_expr(ftype, "_item", fname);
                    out.push_str(&format!(
                        "                            _out.{fname}.push({elem_expr}?);\n"
                    ));
                }
                out.push_str("                        }\n");
                out.push_str("                    }\n");
                out.push_str("                }\n");
            }
            continue;
        }

        // Singular message (Option<Box<T>>)
        if ftype == Type::Message as i32 {
            let pattern = build_match_pattern(json_key, fname, &mut arm_added_keys);
            if let Some(pat) = pattern {
                let inner_type = registry.resolve(file_package, raw_type_name);
                out.push_str(&format!("                {pat} => {{\n"));
                out.push_str("                    if !matches!(_v, ::serde_json::Value::Null) {\n");
                out.push_str(&format!(
                    "                        _out.{fname} = Some(Box::new({inner_type}::from_json(_v)?));\n"
                ));
                out.push_str("                    }\n");
                out.push_str("                }\n");
            }
            continue;
        }

        // Singular enum
        if ftype == Type::Enum as i32 {
            let pattern = build_match_pattern(json_key, fname, &mut arm_added_keys);
            if let Some(pat) = pattern {
                let enum_type = registry.resolve(file_package, raw_type_name);
                out.push_str(&format!("                {pat} => {{\n"));
                out.push_str("                    if !matches!(_v, ::serde_json::Value::Null) {\n");
                out.push_str(&format!(
                    "                        _out.{fname} = {enum_type}::from_json_value(_v)?;\n"
                ));
                out.push_str("                    }\n");
                out.push_str("                }\n");
            }
            continue;
        }

        // Singular scalar
        let pattern = build_match_pattern(json_key, fname, &mut arm_added_keys);
        if let Some(pat) = pattern {
            out.push_str(&format!("                {pat} => {{\n"));
            let from_expr = scalar_from_json_expr(ftype, "_v", fname);
            out.push_str(&format!(
                "                    _out.{fname} = {from_expr}?;\n"
            ));
            out.push_str("                }\n");
        }
    }

    // Catch-all: unknown fields are silently skipped
    out.push_str("                _ => {}\n");
    out.push_str("            }\n");
    out.push_str("        }\n");
    out.push_str("        Ok(_out)\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    Ok(out)
}

/// Build a match arm pattern string for a field, deduplicating when
/// `json_key == snake_name` (to avoid unreachable-patterns lint).
///
/// Returns `None` if both keys have already been registered (shouldn't happen
/// in practice — each field appears exactly once).
fn build_match_pattern(
    json_key: &str,
    snake_name: &str,
    seen: &mut std::collections::HashSet<String>,
) -> Option<String> {
    let already_json = seen.contains(json_key);
    let already_snake = seen.contains(snake_name);

    if already_json && already_snake {
        return None; // both already covered
    }

    seen.insert(json_key.to_string());
    seen.insert(snake_name.to_string());

    if json_key == snake_name || already_json {
        // Only emit the snake_case key (or the one not yet registered)
        if already_json {
            Some(format!("\"{snake_name}\""))
        } else {
            Some(format!("\"{json_key}\""))
        }
    } else if already_snake {
        Some(format!("\"{json_key}\""))
    } else {
        Some(format!("\"{json_key}\" | \"{snake_name}\""))
    }
}

// ── suppress unused import warning for is_map_entry_type ─────────────────────
const _: fn() = || {
    let _ = is_map_entry_type;
};
