#![forbid(unsafe_code)]

//! Proto text-format code generation.
//!
//! When [`CodegenOptions::emit_text_format`] is `true`, each generated message
//! gets a `pub fn to_text_format(&self) -> String` method that serialises the
//! message in the human-readable proto text format (the format used by
//! `protoc --encode` / `--decode`).
//!
//! ## Rules applied
//!
//! - Fields at their proto3 default value are **omitted** (zero, empty, false).
//! - Scalars: `name: value\n`
//! - Strings: `name: "escaped"\n`  — `\`, `"`, `\n`, `\t` are escaped.
//! - Bytes: `name: "\xNN..."\n`   — every byte is hex-escaped.
//! - Booleans: `name: true\n` / `name: false\n`  (omitted when false).
//! - Enums: emit the integer value cast to `i32`; skip when 0.
//! - WKT / wrapper types: emit via `{:?}` Debug form (leaf, no sub-message block).
//! - Singular message (`Option<Box<T>>`): `name {\n  inner\n}\n` if `Some`.
//! - Repeated scalar: one `name: value\n` per element.
//! - Repeated message: one `name {\n  inner\n}\n` per element.
//! - Oneof: match the variant enum, emit `variant_name: value\n` or a block.
//! - Map fields: iterate sorted by key string, emit `name { key: k value: v }\n`.

use prost_types::{
    field_descriptor_proto::{Label, Type},
    DescriptorProto,
};

use crate::{
    emit::{collect_map_entries, field_type_str_with_wkt, to_pascal_case},
    options::{CodegenError, CodegenOptions},
    type_registry::TypeRegistry,
};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Returns `true` if `raw_type_name` (leading-dot form from the descriptor) is
/// a WKT or wrapper type that maps to a leaf Rust type (no `to_text_format()`).
fn is_wkt_leaf(raw_type_name: &str) -> bool {
    let normalized = raw_type_name.trim_start_matches('.');
    let with_dot = format!(".{normalized}");
    crate::wkt_map::wkt_rust_type(&with_dot).is_some()
}

/// Rust expression that is `true` when a scalar value equals proto3 default.
/// `expr` is the Rust expression for the value.
fn is_default_scalar_expr(ftype: i32, expr: &str) -> String {
    match ftype {
        t if t == Type::String as i32 => format!("({expr}).is_empty()"),
        t if t == Type::Bytes as i32 => format!("({expr}).is_empty()"),
        t if t == Type::Bool as i32 => format!("!({expr})"),
        t if t == Type::Float as i32 => format!("({expr}) == 0.0_f32"),
        t if t == Type::Double as i32 => format!("({expr}) == 0.0_f64"),
        // int32/int64/uint32/uint64/sint*/fixed*/sfixed* and enums all default to 0
        _ => format!("({expr}) == 0"),
    }
}

/// Rust expression that formats a scalar value as proto text-format literal.
/// Returns a `String`-producing expression suitable for insertion in a `format!`.
fn scalar_to_text_expr(ftype: i32, expr: &str) -> String {
    match ftype {
        t if t == Type::String as i32 => {
            // Escape: backslash, double-quote, newline, tab
            format!(
                "{{ let _s: &str = &({expr}); \
                 let mut _e = String::with_capacity(_s.len() + 2); \
                 _e.push('\"'); \
                 for _c in _s.chars() {{ match _c {{ \
                   '\\\\' => _e.push_str(\"\\\\\\\\\"), \
                   '\"'  => _e.push_str(\"\\\\\\\"\"), \
                   '\\n' => _e.push_str(\"\\\\n\"), \
                   '\\t' => _e.push_str(\"\\\\t\"), \
                   _oc   => _e.push(_oc), \
                 }} }} \
                 _e.push('\"'); \
                 _e }}"
            )
        }
        t if t == Type::Bytes as i32 => {
            // Hex-escape every byte
            format!(
                "{{ let _b: &[u8] = &({expr}); \
                 let mut _e = String::with_capacity(_b.len() * 4 + 2); \
                 _e.push('\"'); \
                 for _byte in _b.iter() {{ \
                   _e.push_str(&format!(\"\\\\x{{:02x}}\", _byte)); \
                 }} \
                 _e.push('\"'); \
                 _e }}"
            )
        }
        t if t == Type::Bool as i32 => {
            format!("if ({expr}) {{ \"true\".to_string() }} else {{ \"false\".to_string() }}")
        }
        t if t == Type::Float as i32 => format!("format!(\"{{:?}}\", {expr} as f32)"),
        t if t == Type::Double as i32 => format!("format!(\"{{:?}}\", {expr} as f64)"),
        // All integer types: Display
        _ => format!("({expr}).to_string()"),
    }
}

// ── collection helpers ────────────────────────────────────────────────────────

/// A oneof group extracted from the descriptor.
struct OneofGroup {
    /// Snake-case oneof field name on the struct.
    field_name: String,
    /// Rust enum type name: `{type_name}_{PascalOneof}`.
    enum_type: String,
    /// Each member field with proto field name and its descriptor type info.
    members: Vec<OneofMember>,
}

struct OneofMember {
    proto_name: String,
    variant_name: String,
    ftype: i32,
    raw_type_name: String,
}

fn collect_oneof_groups(
    msg: &DescriptorProto,
    type_name: &str,
    file_package: &str,
    registry: &TypeRegistry,
) -> Result<Vec<OneofGroup>, CodegenError> {
    let mut groups: Vec<OneofGroup> = msg
        .oneof_decl
        .iter()
        .map(|od| {
            let field_name = od
                .name
                .as_deref()
                .ok_or_else(|| CodegenError::InvalidDescriptor("oneof missing name".into()))?;
            let enum_type = format!("{}_{}", type_name, to_pascal_case(field_name));
            Ok(OneofGroup {
                field_name: field_name.to_string(),
                enum_type,
                members: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;

    for field in &msg.field {
        let Some(oi) = field.oneof_index else {
            continue;
        };
        let oi = oi as usize;
        if oi >= groups.len() {
            continue;
        }
        let proto_name = field
            .name
            .as_deref()
            .ok_or_else(|| CodegenError::InvalidDescriptor("oneof field missing name".into()))?;
        let variant_name = to_pascal_case(proto_name);
        let ftype = field.r#type.unwrap_or(Type::String as i32);
        let raw_type_name = field.type_name.as_deref().unwrap_or("").to_string();

        // Validate field type resolves without error.
        let _ = field_type_str_with_wkt(field, type_name, file_package, registry)?;

        groups[oi].members.push(OneofMember {
            proto_name: proto_name.to_string(),
            variant_name,
            ftype,
            raw_type_name,
        });
    }

    Ok(groups)
}

// ── emission ──────────────────────────────────────────────────────────────────

/// Emit a `pub fn to_text_format(&self) -> String { … }` block for a message.
pub(crate) fn emit_text_format_impl(
    msg: &DescriptorProto,
    type_name: &str,
    _opts: &CodegenOptions,
    file_package: &str,
    registry: &TypeRegistry,
) -> Result<String, CodegenError> {
    // Skip synthetic map-entry types (never top-level structs).
    if msg.options.as_ref().is_some_and(|o| o.map_entry()) {
        return Ok(String::new());
    }

    let map_entries = collect_map_entries(msg, file_package, registry);
    let oneof_groups = collect_oneof_groups(msg, type_name, file_package, registry)?;

    // Build set of field names that belong to a oneof.
    let oneof_field_names: std::collections::HashSet<String> = msg
        .field
        .iter()
        .filter(|f| f.oneof_index.is_some())
        .filter_map(|f| f.name.as_deref().map(str::to_string))
        .collect();

    let mut out = String::new();
    out.push_str(&format!("impl {type_name} {{\n"));
    out.push_str("    /// Render this message as proto text format.\n");
    out.push_str("    pub fn to_text_format(&self) -> String {\n");
    out.push_str("        let mut _out = String::new();\n");

    // ── regular (non-oneof, non-map) fields ───────────────────────────────────
    for field in &msg.field {
        let fname = field
            .name
            .as_deref()
            .ok_or_else(|| CodegenError::InvalidDescriptor("field missing name".into()))?;

        // Skip oneof members — handled separately.
        if oneof_field_names.contains(fname) {
            continue;
        }

        // Skip map fields — handled separately.
        if map_entries.contains_key(fname) {
            continue;
        }

        let label = field.label.unwrap_or(Label::Optional as i32);
        let ftype = field.r#type.unwrap_or(Type::String as i32);
        let raw_type_name = field.type_name.as_deref().unwrap_or("");
        let repeated = label == Label::Repeated as i32;

        emit_regular_field(&mut out, fname, ftype, raw_type_name, repeated)?;
    }

    // ── map fields ────────────────────────────────────────────────────────────
    for (fname, map_info) in &map_entries {
        out.push_str("        {\n");
        out.push_str(&format!(
            "            let mut _keys: Vec<String> = self.{fname}.keys().map(|_k| _k.to_string()).collect();\n"
        ));
        out.push_str("            _keys.sort();\n");
        out.push_str("            for _ks in &_keys {\n");

        let key_type = &map_info.key_type;

        // Produce the key expression used to look up in the map.
        let key_parse = if key_type == "String" {
            "_ks.clone()".to_string()
        } else if key_type == "bool" {
            "_ks.parse::<bool>().unwrap_or(false)".to_string()
        } else if key_type.starts_with('i') || key_type.starts_with('u') {
            format!("_ks.parse::<{key_type}>().unwrap_or(0)")
        } else {
            "_ks.clone()".to_string()
        };

        out.push_str(&format!(
            "            if let Some(_val) = self.{fname}.get(&{key_parse}) {{\n"
        ));
        out.push_str(&format!(
            "                _out.push_str(\"{fname} {{ key: \");\n"
        ));
        out.push_str("                _out.push_str(_ks);\n");
        out.push_str("                _out.push_str(\" value: \");\n");
        out.push_str("                _out.push_str(&format!(\"{:?}\", _val));\n");
        out.push_str("                _out.push_str(\" }\\n\");\n");
        out.push_str("            }\n");
        out.push_str("        }\n");
        out.push_str("        }\n");
    }

    // ── oneof fields ──────────────────────────────────────────────────────────
    for group in &oneof_groups {
        if group.members.is_empty() {
            continue;
        }
        let field_name = &group.field_name;
        let enum_type = &group.enum_type;

        out.push_str(&format!(
            "        if let Some(_oneof) = &self.{field_name} {{\n"
        ));
        out.push_str("            match _oneof {\n");

        for member in &group.members {
            let variant = &member.variant_name;
            let proto_name = &member.proto_name;
            let ftype = member.ftype;
            let raw_type_name = &member.raw_type_name;

            let is_message = ftype == Type::Message as i32;
            let is_wkt = !raw_type_name.is_empty() && is_wkt_leaf(raw_type_name);

            if is_message && !is_wkt {
                // Oneof message variant: inner is Box<T>
                out.push_str(&format!(
                    "                {enum_type}::{variant}(_inner) => {{\n"
                ));
                out.push_str(&format!(
                    "                    _out.push_str(\"{proto_name} {{\\n\");\n"
                ));
                out.push_str(
                    "                    for _line in _inner.to_text_format().lines() {\n",
                );
                out.push_str("                        _out.push_str(\"  \");\n");
                out.push_str("                        _out.push_str(_line);\n");
                out.push_str("                        _out.push('\\n');\n");
                out.push_str("                    }\n");
                out.push_str("                    _out.push_str(\"}\\n\");\n");
                out.push_str("                }\n");
            } else {
                // Scalar / enum / WKT variant
                out.push_str(&format!(
                    "                {enum_type}::{variant}(_v) => {{\n"
                ));
                // Determine how to format the value in the generated code
                let value_str = if is_wkt {
                    "format!(\"{:?}\", _v)".to_string()
                } else if ftype == Type::String as i32 || ftype == Type::Bytes as i32 {
                    scalar_to_text_expr(ftype, "_v")
                } else if ftype == Type::Bool as i32 {
                    "if *_v { \"true\".to_string() } else { \"false\".to_string() }".to_string()
                } else if ftype == Type::Enum as i32 {
                    "(*_v as i32).to_string()".to_string()
                } else {
                    "_v.to_string()".to_string()
                };
                out.push_str(&format!(
                    "                    _out.push_str(&format!(\"{proto_name}: {{}}\\n\", {value_str}));\n"
                ));
                out.push_str("                }\n");
            }
        }

        out.push_str("            }\n");
        out.push_str("        }\n");
    }

    out.push_str("        _out\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    Ok(out)
}

/// Emit the push-statements for one regular (non-oneof, non-map) field.
fn emit_regular_field(
    out: &mut String,
    fname: &str,
    ftype: i32,
    raw_type_name: &str,
    repeated: bool,
) -> Result<(), CodegenError> {
    let is_message = ftype == Type::Message as i32;
    let is_wkt = !raw_type_name.is_empty() && is_wkt_leaf(raw_type_name);

    if repeated {
        emit_repeated_field(out, fname, ftype, is_wkt, is_message && !is_wkt)?;
    } else if is_message && !is_wkt {
        // Singular real message: Option<Box<T>>
        out.push_str(&format!("        if let Some(_inner) = &self.{fname} {{\n"));
        out.push_str(&format!("            _out.push_str(\"{fname} {{\\n\");\n"));
        out.push_str("            for _line in _inner.to_text_format().lines() {\n");
        out.push_str("                _out.push_str(\"  \");\n");
        out.push_str("                _out.push_str(_line);\n");
        out.push_str("                _out.push('\\n');\n");
        out.push_str("            }\n");
        out.push_str("            _out.push_str(\"}\\n\");\n");
        out.push_str("        }\n");
    } else if is_wkt {
        // WKT / wrapper type — emit via Debug (always present when non-default).
        out.push_str(&format!(
            "        _out.push_str(&format!(\"{fname}: {{:?}}\\n\", self.{fname}));\n"
        ));
    } else if ftype == Type::Bool as i32 {
        // Only emit if true (false is proto3 default).
        out.push_str(&format!("        if self.{fname} {{\n"));
        out.push_str(&format!(
            "            _out.push_str(\"{fname}: true\\n\");\n"
        ));
        out.push_str("        }\n");
    } else if ftype == Type::Enum as i32 {
        // Enum: cast to i32, skip if 0.
        out.push_str(&format!("        if (self.{fname} as i32) != 0 {{\n"));
        out.push_str(&format!(
            "            _out.push_str(&format!(\"{fname}: {{}}\\n\", self.{fname} as i32));\n"
        ));
        out.push_str("        }\n");
    } else if ftype == Type::String as i32 || ftype == Type::Bytes as i32 {
        // String and bytes both use is_empty() guard and scalar_to_text_expr.
        out.push_str(&format!("        if !self.{fname}.is_empty() {{\n"));
        let text_expr = scalar_to_text_expr(ftype, &format!("self.{fname}"));
        out.push_str(&format!(
            "            _out.push_str(&format!(\"{fname}: {{}}\\n\", {text_expr}));\n"
        ));
        out.push_str("        }\n");
    } else {
        // Numeric scalars (int/float): skip if zero/default.
        let default_check = is_default_scalar_expr(ftype, &format!("self.{fname}"));
        out.push_str(&format!("        if !({default_check}) {{\n"));
        out.push_str(&format!(
            "            _out.push_str(&format!(\"{fname}: {{}}\\n\", self.{fname}));\n"
        ));
        out.push_str("        }\n");
    }

    Ok(())
}

/// Emit the push-statements for a repeated field.
fn emit_repeated_field(
    out: &mut String,
    fname: &str,
    ftype: i32,
    is_wkt: bool,
    is_real_message: bool,
) -> Result<(), CodegenError> {
    if is_wkt {
        // Repeated WKT / wrapper types: emit via Debug (no to_text_format()).
        out.push_str(&format!("        for _elem in &self.{fname} {{\n"));
        out.push_str(&format!(
            "            _out.push_str(&format!(\"{fname}: {{:?}}\\n\", _elem));\n"
        ));
        out.push_str("        }\n");
    } else if is_real_message {
        out.push_str(&format!("        for _elem in &self.{fname} {{\n"));
        out.push_str(&format!("            _out.push_str(\"{fname} {{\\n\");\n"));
        out.push_str("            for _line in _elem.to_text_format().lines() {\n");
        out.push_str("                _out.push_str(\"  \");\n");
        out.push_str("                _out.push_str(_line);\n");
        out.push_str("                _out.push('\\n');\n");
        out.push_str("            }\n");
        out.push_str("            _out.push_str(\"}\\n\");\n");
        out.push_str("        }\n");
    } else if ftype == Type::String as i32 || ftype == Type::Bytes as i32 {
        out.push_str(&format!("        for _elem in &self.{fname} {{\n"));
        let text_expr = scalar_to_text_expr(ftype, "_elem");
        out.push_str(&format!(
            "            _out.push_str(&format!(\"{fname}: {{}}\\n\", {text_expr}));\n"
        ));
        out.push_str("        }\n");
    } else if ftype == Type::Bool as i32 {
        out.push_str(&format!("        for _elem in &self.{fname} {{\n"));
        out.push_str(&format!(
            "            _out.push_str(&format!(\"{fname}: {{}}\\n\", if *_elem {{ \"true\" }} else {{ \"false\" }}));\n"
        ));
        out.push_str("        }\n");
    } else if ftype == Type::Enum as i32 {
        out.push_str(&format!("        for _elem in &self.{fname} {{\n"));
        out.push_str(&format!(
            "            _out.push_str(&format!(\"{fname}: {{}}\\n\", *_elem as i32));\n"
        ));
        out.push_str("        }\n");
    } else {
        out.push_str(&format!("        for _elem in &self.{fname} {{\n"));
        out.push_str(&format!(
            "            _out.push_str(&format!(\"{fname}: {{}}\\n\", _elem));\n"
        ));
        out.push_str("        }\n");
    }

    Ok(())
}
