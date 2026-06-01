#![forbid(unsafe_code)]

//! Emission of `impl OxiMessage for T` and `impl OxiName for T` blocks.
//!
//! This module produces the native wire-format encode/decode implementation
//! for every generated struct. The emitted code calls directly into
//! `::oxiproto_core::wire` primitives with no prost dependency.

use prost_types::{
    field_descriptor_proto::{Label, Type},
    DescriptorProto, FieldDescriptorProto,
};

use crate::options::CodegenError;

// ── wire-type constants (proto encoding spec) ────────────────────────────────

/// Returns the protobuf wire type value for a given field type.
fn wire_type_for_field(ftype: i32) -> u32 {
    if ftype == Type::Fixed32 as i32
        || ftype == Type::Sfixed32 as i32
        || ftype == Type::Float as i32
    {
        5 // I32
    } else if ftype == Type::Fixed64 as i32
        || ftype == Type::Sfixed64 as i32
        || ftype == Type::Double as i32
    {
        1 // I64
    } else if ftype == Type::Message as i32
        || ftype == Type::Bytes as i32
        || ftype == Type::String as i32
    {
        2 // Len
    } else {
        0 // Varint (int32, int64, uint32, uint64, sint32, sint64, bool, enum)
    }
}

/// Whether a scalar type is packed when repeated (proto3 default for numerics).
fn is_packable(ftype: i32) -> bool {
    !matches!(
        ftype,
        t if t == Type::String as i32 || t == Type::Bytes as i32 || t == Type::Message as i32
    )
}

// ── tag-len helper ────────────────────────────────────────────────────────────

/// Emit code that evaluates to the encoded byte length of the field tag.
fn tag_len_expr(field_number: u32, wire_type: u32) -> String {
    let tag_value = (u64::from(field_number) << 3) | u64::from(wire_type);
    format!("::oxiproto_core::wire::varint::encoded_len_varint({tag_value}u64)")
}

// ── encoded_len helpers ───────────────────────────────────────────────────────

/// Returns an expression (String) that computes the contribution of a single
/// non-repeated field to `encoded_len`.  The expression evaluates to `usize`.
/// `value_expr` is the Rust expression for the field value (e.g. `self.x`).
fn scalar_encoded_len_expr(field_number: u32, ftype: i32, value_expr: &str) -> String {
    let wire_type = wire_type_for_field(ftype);
    let tag = tag_len_expr(field_number, wire_type);
    match ftype {
        t if t == Type::Fixed32 as i32
            || t == Type::Sfixed32 as i32
            || t == Type::Float as i32 =>
        {
            format!("{tag} + 4usize")
        }
        t if t == Type::Fixed64 as i32
            || t == Type::Sfixed64 as i32
            || t == Type::Double as i32 =>
        {
            format!("{tag} + 8usize")
        }
        t if t == Type::Sint32 as i32 => format!(
            "{tag} + ::oxiproto_core::wire::varint::encoded_len_varint(::oxiproto_core::wire::zigzag::zigzag_encode32({value_expr}) as u64)"
        ),
        t if t == Type::Sint64 as i32 => format!(
            "{tag} + ::oxiproto_core::wire::varint::encoded_len_varint(::oxiproto_core::wire::zigzag::zigzag_encode64({value_expr}) as u64)"
        ),
        t if t == Type::String as i32 => format!(
            "{tag} + ::oxiproto_core::wire::varint::encoded_len_varint(({value_expr}).len() as u64) + ({value_expr}).len()"
        ),
        t if t == Type::Bytes as i32 => format!(
            "{tag} + ::oxiproto_core::wire::varint::encoded_len_varint(({value_expr}).len() as u64) + ({value_expr}).len()"
        ),
        t if t == Type::Int32 as i32 => format!(
            "{tag} + ::oxiproto_core::wire::varint::encoded_len_varint(({value_expr}) as i64 as u64)"
        ),
        t if t == Type::Int64 as i32 => format!(
            "{tag} + ::oxiproto_core::wire::varint::encoded_len_varint(({value_expr}) as u64)"
        ),
        t if t == Type::Uint32 as i32 => format!(
            "{tag} + ::oxiproto_core::wire::varint::encoded_len_varint(u64::from({value_expr}))"
        ),
        t if t == Type::Uint64 as i32 => format!(
            "{tag} + ::oxiproto_core::wire::varint::encoded_len_varint({value_expr})"
        ),
        t if t == Type::Bool as i32 => format!("{tag} + 1usize"),
        // Enum (i32 varint)
        _ => format!(
            "{tag} + ::oxiproto_core::wire::varint::encoded_len_varint(({value_expr}) as i64 as u64)"
        ),
    }
}

// ── encode_raw helpers ────────────────────────────────────────────────────────

/// Returns statements (String) that encode a single scalar value.
/// The statements are placed inside `fn encode_raw`.
fn scalar_encode_stmt(field_number: u32, ftype: i32, value_expr: &str, indent: &str) -> String {
    let wire_type = wire_type_for_field(ftype);
    let wire_type_path = wire_type_path(wire_type);
    let mut s = format!("{indent}let _ = buf.write_tag({field_number}u32, {wire_type_path});\n");
    match ftype {
        t if t == Type::Fixed32 as i32 || t == Type::Sfixed32 as i32 => {
            s.push_str(&format!(
                "{indent}buf.write_fixed32(({value_expr}) as u32);\n"
            ));
        }
        t if t == Type::Float as i32 => {
            s.push_str(&format!("{indent}buf.write_float({value_expr});\n"));
        }
        t if t == Type::Fixed64 as i32 || t == Type::Sfixed64 as i32 => {
            s.push_str(&format!(
                "{indent}buf.write_fixed64(({value_expr}) as u64);\n"
            ));
        }
        t if t == Type::Double as i32 => {
            s.push_str(&format!("{indent}buf.write_double({value_expr});\n"));
        }
        t if t == Type::Sint32 as i32 => {
            s.push_str(&format!(
                "{indent}buf.write_varint(::oxiproto_core::wire::zigzag::zigzag_encode32({value_expr}) as u64);\n"
            ));
        }
        t if t == Type::Sint64 as i32 => {
            s.push_str(&format!(
                "{indent}buf.write_varint(::oxiproto_core::wire::zigzag::zigzag_encode64({value_expr}) as u64);\n"
            ));
        }
        t if t == Type::String as i32 => {
            s.push_str(&format!("{indent}buf.write_string(&{value_expr});\n"));
        }
        t if t == Type::Bytes as i32 => {
            s.push_str(&format!(
                "{indent}buf.write_length_delimited(&{value_expr});\n"
            ));
        }
        t if t == Type::Int32 as i32 => {
            s.push_str(&format!("{indent}buf.write_varint_i32({value_expr});\n"));
        }
        t if t == Type::Int64 as i32 => {
            s.push_str(&format!("{indent}buf.write_varint_i64({value_expr});\n"));
        }
        t if t == Type::Uint32 as i32 => {
            s.push_str(&format!("{indent}buf.write_varint32({value_expr});\n"));
        }
        t if t == Type::Uint64 as i32 => {
            s.push_str(&format!("{indent}buf.write_varint({value_expr});\n"));
        }
        t if t == Type::Bool as i32 => {
            s.push_str(&format!("{indent}buf.write_bool({value_expr});\n"));
        }
        // Enum: stored as i32, encode as varint_i32
        _ => {
            s.push_str(&format!(
                "{indent}buf.write_varint_i32({value_expr} as i32);\n"
            ));
        }
    }
    s
}

/// Returns decode code (Rust expression String) to read a scalar from DecodeBuffer.
/// Returns `(assign_expr, read_stmts)` where `assign_stmts` is the code to populate
/// the field.
fn scalar_decode_stmts(ftype: i32, field_access: &str, indent: &str) -> String {
    match ftype {
        t if t == Type::Fixed32 as i32 => format!(
            "{indent}{field_access} = buf.read_fixed32().map_err(::oxiproto_core::OxiProtoError::WireFormatError)? as i32;\n"
        ),
        t if t == Type::Sfixed32 as i32 => format!(
            "{indent}{field_access} = buf.read_fixed32().map_err(::oxiproto_core::OxiProtoError::WireFormatError)? as i32;\n"
        ),
        t if t == Type::Float as i32 => format!(
            "{indent}{field_access} = buf.read_float().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n"
        ),
        t if t == Type::Fixed64 as i32 => format!(
            "{indent}{field_access} = buf.read_fixed64().map_err(::oxiproto_core::OxiProtoError::WireFormatError)? as u64;\n"
        ),
        t if t == Type::Sfixed64 as i32 => format!(
            "{indent}{field_access} = buf.read_fixed64().map_err(::oxiproto_core::OxiProtoError::WireFormatError)? as i64;\n"
        ),
        t if t == Type::Double as i32 => format!(
            "{indent}{field_access} = buf.read_double().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n"
        ),
        t if t == Type::Sint32 as i32 => format!(
            "{indent}{field_access} = ::oxiproto_core::wire::zigzag::zigzag_decode32(buf.read_varint32().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?);\n"
        ),
        t if t == Type::Sint64 as i32 => format!(
            "{indent}{field_access} = ::oxiproto_core::wire::zigzag::zigzag_decode64(buf.read_varint().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?);\n"
        ),
        t if t == Type::String as i32 => format!(
            "{indent}{field_access} = buf.read_string().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?.to_owned();\n"
        ),
        t if t == Type::Bytes as i32 => format!(
            "{indent}{field_access} = buf.read_length_delimited().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?.to_vec();\n"
        ),
        t if t == Type::Int32 as i32 => format!(
            "{indent}{field_access} = buf.read_varint_i32().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n"
        ),
        t if t == Type::Int64 as i32 => format!(
            "{indent}{field_access} = buf.read_varint_i64().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n"
        ),
        t if t == Type::Uint32 as i32 => format!(
            "{indent}{field_access} = buf.read_varint32().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n"
        ),
        t if t == Type::Uint64 as i32 => format!(
            "{indent}{field_access} = buf.read_varint().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n"
        ),
        t if t == Type::Bool as i32 => format!(
            "{indent}{field_access} = buf.read_bool().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n"
        ),
        // Enum: decode as i32
        _ => format!(
            "{indent}{field_access} = buf.read_varint_i32().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n"
        ),
    }
}

fn wire_type_path(wire_type: u32) -> &'static str {
    match wire_type {
        0 => "::oxiproto_core::wire::WireType::Varint",
        1 => "::oxiproto_core::wire::WireType::I64",
        2 => "::oxiproto_core::wire::WireType::Len",
        5 => "::oxiproto_core::wire::WireType::I32",
        _ => "::oxiproto_core::wire::WireType::Varint",
    }
}

// ── proto3 default check ──────────────────────────────────────────────────────

/// Returns a Rust condition expression that is `true` when the field value is
/// the proto3 default (i.e., should be omitted on encode).
fn is_default_expr(ftype: i32, value_expr: &str) -> String {
    match ftype {
        t if t == Type::String as i32 => format!("({value_expr}).is_empty()"),
        t if t == Type::Bytes as i32 => format!("({value_expr}).is_empty()"),
        t if t == Type::Bool as i32 => format!("!({value_expr})"),
        t if t == Type::Float as i32 => format!("({value_expr}) == 0.0f32"),
        t if t == Type::Double as i32 => format!("({value_expr}) == 0.0f64"),
        // All integer types default == 0
        _ => format!("({value_expr}) == 0"),
    }
}

// ── Field info collection ─────────────────────────────────────────────────────

/// Simple wrapper to pass field classification data through.
struct FieldInfo<'a> {
    field: &'a FieldDescriptorProto,
    name: &'a str,
    number: u32,
    ftype: i32,
    is_repeated: bool,
    in_oneof: bool,
}

fn collect_fields(msg: &DescriptorProto) -> Result<Vec<FieldInfo<'_>>, CodegenError> {
    let mut result = Vec::new();
    for field in &msg.field {
        let name = field
            .name
            .as_deref()
            .ok_or_else(|| CodegenError::InvalidDescriptor("field missing name".into()))?;
        let number = field
            .number
            .ok_or_else(|| CodegenError::InvalidDescriptor("field missing number".into()))?
            as u32;
        let ftype = field.r#type.unwrap_or(Type::String as i32);
        let label = field.label.unwrap_or(Label::Optional as i32);
        let is_repeated = label == Label::Repeated as i32;
        let in_oneof = field.oneof_index.is_some();
        result.push(FieldInfo {
            field,
            name,
            number,
            ftype,
            is_repeated,
            in_oneof,
        });
    }
    Ok(result)
}

// ── encoded_len emission ──────────────────────────────────────────────────────

#[allow(dead_code)]
fn emit_encoded_len_body(
    msg: &DescriptorProto,
    struct_name: &str,
    map_field_names: &std::collections::HashSet<String>,
    _oneof_names: &[String],
) -> Result<String, CodegenError> {
    let mut body = String::new();
    body.push_str("        let mut len = 0usize;\n");

    let fields = collect_fields(msg)?;
    let mut emitted_oneofs: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for fi in &fields {
        // Skip map fields — handled separately
        if map_field_names.contains(fi.name) {
            let tag_len = tag_len_expr(fi.number, 2); // map entry is Len
                                                      // Each map entry contributes tag + len-prefix + entry_len
                                                      // We encode entry length conservatively — emit a loop
            let map_type_name = format!(
                "{struct_name}_{}",
                to_pascal_case(&format!("{}_entry", fi.name))
            );
            let _ = map_type_name; // unused for now — we'll size each entry directly
            body.push_str(&format!("        for (_k, _v) in &self.{} {{\n", fi.name));
            // We can't precisely compute entry size without knowing key/value types
            // at this point, so we use a simpler approach: encode to a temp buffer
            body.push_str(&format!(
                "            let _ = {tag_len}; // map entries: use encode_to_vec for sizing\n"
            ));
            body.push_str("        }\n");
            // For simplicity, use actual encoding to measure: call encode_raw and check len
            // Actually the cleanest approach: we skip map entries in encoded_len and
            // compute during encode_raw by encoding and measuring.
            // But that's wrong. Instead, for map fields we embed encode_map_entry_len helper.
            // Since we don't know K/V Rust types at this level, let's use the key/value
            // types from the nested map entry descriptor.
            continue;
        }

        if fi.in_oneof {
            let oi = fi.field.oneof_index.unwrap_or(0) as usize;
            if emitted_oneofs.contains(&oi) {
                continue;
            }
            emitted_oneofs.insert(oi);
            // Oneof: use the oneof field name from msg.oneof_decl
            if let Some(oneof) = msg.oneof_decl.get(oi) {
                let oname = oneof.name.as_deref().unwrap_or("unknown");
                body.push_str(&format!("        if let Some(ref _v) = self.{oname} {{\n"));
                // Delegate to the oneof enum's encoded_len
                // We need to collect the oneof enum's contribution
                body.push_str(&format!(
                    "            len += _oxi_oneof_encoded_len_{oname}(self);\n"
                ));
                body.push_str("        }\n");
            }
            continue;
        }

        if fi.is_repeated {
            if fi.ftype == Type::Message as i32 {
                // Repeated message: each element is tag + varint_len + encoded_len
                let tag = tag_len_expr(fi.number, 2);
                body.push_str(&format!("        for _item in &self.{} {{\n", fi.name));
                body.push_str("            let _item_len = _item.encoded_len();\n");
                body.push_str(&format!(
                    "            len += {tag} + ::oxiproto_core::wire::varint::encoded_len_varint(_item_len as u64) + _item_len;\n"
                ));
                body.push_str("        }\n");
            } else if fi.ftype == Type::String as i32 || fi.ftype == Type::Bytes as i32 {
                // Repeated string/bytes: unpacked
                let tag = tag_len_expr(fi.number, 2);
                body.push_str(&format!("        for _item in &self.{} {{\n", fi.name));
                body.push_str(&format!(
                    "            len += {tag} + ::oxiproto_core::wire::varint::encoded_len_varint(_item.len() as u64) + _item.len();\n"
                ));
                body.push_str("        }\n");
            } else if is_packable(fi.ftype) {
                // Packed repeated scalar
                let tag = tag_len_expr(fi.number, 2); // packed = Len
                body.push_str(&format!("        if !self.{}.is_empty() {{\n", fi.name));
                body.push_str(&format!(
                    "            let _payload_len: usize = self.{}.iter().map(|_v| {}).sum();\n",
                    fi.name,
                    packed_elem_len_expr(fi.ftype, "*_v")
                ));
                body.push_str(&format!(
                    "            len += {tag} + ::oxiproto_core::wire::varint::encoded_len_varint(_payload_len as u64) + _payload_len;\n"
                ));
                body.push_str("        }\n");
            }
        } else if fi.ftype == Type::Message as i32 {
            // Singular message: Option<Box<T>>
            body.push_str(&format!(
                "        if let Some(ref _msg) = self.{} {{\n",
                fi.name
            ));
            let tag = tag_len_expr(fi.number, 2);
            body.push_str("            let _msg_len = _msg.encoded_len();\n");
            body.push_str(&format!(
                "            len += {tag} + ::oxiproto_core::wire::varint::encoded_len_varint(_msg_len as u64) + _msg_len;\n"
            ));
            body.push_str("        }\n");
        } else {
            // Singular scalar: only emit if non-default (proto3)
            let value_expr = format!("self.{}", fi.name);
            let default_check = is_default_expr(fi.ftype, &value_expr);
            body.push_str(&format!("        if !({default_check}) {{\n"));
            let len_expr = scalar_encoded_len_expr(fi.number, fi.ftype, &value_expr);
            body.push_str(&format!("            len += {len_expr};\n"));
            body.push_str("        }\n");
        }
    }

    // Map fields: size them properly using a second pass
    emit_map_encoded_len(&mut body, msg, map_field_names)?;

    body.push_str("        len += self._unknown.encoded_len();\n");
    body.push_str("        len\n");

    // Emit oneof helper closures/functions inline (as nested fns won't work in impl)
    // Instead we'll use inline match arms — but we need to compute the oneof encoded_len
    // We need to restructure: oneof encoded_len must inline the match
    // Let's refactor: replace the _oxi_oneof_encoded_len_ references with inline match
    // This requires a different approach — let's redo the oneof part

    Ok(body)
}

/// Returns the encoded length expression for a single packed element.
fn packed_elem_len_expr(ftype: i32, value_expr: &str) -> String {
    match ftype {
        t if t == Type::Fixed32 as i32
            || t == Type::Sfixed32 as i32
            || t == Type::Float as i32 =>
        {
            "4usize".to_string()
        }
        t if t == Type::Fixed64 as i32
            || t == Type::Sfixed64 as i32
            || t == Type::Double as i32 =>
        {
            "8usize".to_string()
        }
        t if t == Type::Sint32 as i32 => format!(
            "::oxiproto_core::wire::varint::encoded_len_varint(::oxiproto_core::wire::zigzag::zigzag_encode32({value_expr}) as u64)"
        ),
        t if t == Type::Sint64 as i32 => format!(
            "::oxiproto_core::wire::varint::encoded_len_varint(::oxiproto_core::wire::zigzag::zigzag_encode64({value_expr}) as u64)"
        ),
        t if t == Type::Int32 as i32 => format!(
            "::oxiproto_core::wire::varint::encoded_len_varint(({value_expr}) as i64 as u64)"
        ),
        t if t == Type::Int64 as i32 => format!(
            "::oxiproto_core::wire::varint::encoded_len_varint(({value_expr}) as u64)"
        ),
        t if t == Type::Uint32 as i32 => format!(
            "::oxiproto_core::wire::varint::encoded_len_varint(u64::from({value_expr}))"
        ),
        t if t == Type::Uint64 as i32 => format!(
            "::oxiproto_core::wire::varint::encoded_len_varint({value_expr})"
        ),
        t if t == Type::Bool as i32 => "1usize".to_string(),
        _ => format!(
            "::oxiproto_core::wire::varint::encoded_len_varint(({value_expr}) as u64)"
        ),
    }
}

fn emit_map_encoded_len(
    body: &mut String,
    msg: &DescriptorProto,
    map_field_names: &std::collections::HashSet<String>,
) -> Result<(), CodegenError> {
    // For each map field, find the nested entry type and compute entry sizes
    for nested in &msg.nested_type {
        let is_map_entry = nested.options.as_ref().is_some_and(|o| o.map_entry());
        if !is_map_entry {
            continue;
        }
        let entry_name = nested.name.as_deref().unwrap_or("");
        // Find the field referencing this entry
        for field in &msg.field {
            let type_name = field.type_name.as_deref().unwrap_or("");
            let type_last = type_name.split('.').next_back().unwrap_or("");
            if type_last != entry_name {
                continue;
            }
            let field_name = field.name.as_deref().unwrap_or("");
            if !map_field_names.contains(field_name) {
                continue;
            }
            let field_number = field.number.unwrap_or(0) as u32;
            let tag = tag_len_expr(field_number, 2);

            let key_field = nested
                .field
                .iter()
                .find(|f| f.name.as_deref() == Some("key"));
            let val_field = nested
                .field
                .iter()
                .find(|f| f.name.as_deref() == Some("value"));

            if let (Some(kf), Some(vf)) = (key_field, val_field) {
                let ktype = kf.r#type.unwrap_or(Type::String as i32);
                let vtype = vf.r#type.unwrap_or(Type::String as i32);
                let k_tag = tag_len_expr(1, wire_type_for_field(ktype));
                let v_tag = tag_len_expr(2, wire_type_for_field(vtype));

                body.push_str(&format!(
                    "        for (_mk, _mv) in &self.{field_name} {{\n"
                ));
                let k_len = key_field_len_expr(ktype, "_mk", &k_tag);
                let v_len = val_field_len_expr(vtype, "_mv", &v_tag);
                body.push_str(&format!(
                    "            let _entry_len: usize = {k_len} + {v_len};\n"
                ));
                body.push_str(&format!(
                    "            len += {tag} + ::oxiproto_core::wire::varint::encoded_len_varint(_entry_len as u64) + _entry_len;\n"
                ));
                body.push_str("        }\n");
            }
        }
    }
    Ok(())
}

fn key_field_len_expr(ftype: i32, value_expr: &str, tag_expr: &str) -> String {
    if ftype == Type::String as i32 {
        format!("{tag_expr} + ::oxiproto_core::wire::varint::encoded_len_varint(({value_expr}).len() as u64) + ({value_expr}).len()")
    } else {
        format!("{tag_expr} + {}", packed_elem_len_expr(ftype, value_expr))
    }
}

fn val_field_len_expr(ftype: i32, value_expr: &str, tag_expr: &str) -> String {
    if ftype == Type::Message as i32 {
        format!("{tag_expr} + ::oxiproto_core::wire::varint::encoded_len_varint(({value_expr}).encoded_len() as u64) + ({value_expr}).encoded_len()")
    } else if ftype == Type::String as i32 || ftype == Type::Bytes as i32 {
        format!("{tag_expr} + ::oxiproto_core::wire::varint::encoded_len_varint(({value_expr}).len() as u64) + ({value_expr}).len()")
    } else {
        format!("{tag_expr} + {}", packed_elem_len_expr(ftype, value_expr))
    }
}

// ── encode_raw emission ───────────────────────────────────────────────────────

fn emit_encode_raw_body(
    msg: &DescriptorProto,
    map_field_names: &std::collections::HashSet<String>,
) -> Result<String, CodegenError> {
    let mut body = String::new();
    let fields = collect_fields(msg)?;
    let mut emitted_oneofs: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for fi in &fields {
        if map_field_names.contains(fi.name) {
            // Handled below
            continue;
        }

        if fi.in_oneof {
            let oi = fi.field.oneof_index.unwrap_or(0) as usize;
            if emitted_oneofs.contains(&oi) {
                continue;
            }
            emitted_oneofs.insert(oi);
            if let Some(oneof) = msg.oneof_decl.get(oi) {
                let oname = oneof.name.as_deref().unwrap_or("unknown");
                // Collect all fields for this oneof
                let oneof_fields: Vec<&FieldDescriptorProto> = msg
                    .field
                    .iter()
                    .filter(|f| f.oneof_index == Some(oi as i32))
                    .collect();
                let oneof_type_name = format!("{}_", oneof.name.as_deref().unwrap_or(""));
                body.push_str(&format!("        if let Some(ref _ov) = self.{oname} {{\n"));
                body.push_str("            match _ov {\n");
                for of in &oneof_fields {
                    let vname = of.name.as_deref().unwrap_or("unknown");
                    let variant = crate::emit::to_pascal_case_pub(vname);
                    let vtype = of.r#type.unwrap_or(Type::String as i32);
                    let field_num = of.number.unwrap_or(0) as u32;
                    let _ = oneof_type_name.as_str();
                    if vtype == Type::Message as i32 {
                        let vtag = tag_len_expr(field_num, 2);
                        let _ = vtag;
                        body.push_str(&format!("                {variant}(_inner) => {{\n"));
                        body.push_str(&format!(
                            "                    let _ = buf.write_tag({field_num}u32, ::oxiproto_core::wire::WireType::Len);\n"
                        ));
                        body.push_str(
                            "                    let _inner_len = _inner.encoded_len();\n",
                        );
                        body.push_str("                    buf.write_varint(_inner_len as u64);\n");
                        body.push_str("                    _inner.encode_raw(buf);\n");
                        body.push_str("                }\n");
                    } else {
                        let wire_type = wire_type_for_field(vtype);
                        let wtp = wire_type_path(wire_type);
                        body.push_str(&format!("                {variant}(_val) => {{\n"));
                        body.push_str(&format!(
                            "                    let _ = buf.write_tag({field_num}u32, {wtp});\n"
                        ));
                        let encode = encode_val_expr(vtype, "*_val");
                        body.push_str(&format!("                    {encode};\n"));
                        body.push_str("                }\n");
                    }
                }
                body.push_str("            }\n");
                body.push_str("        }\n");
            }
            continue;
        }

        if fi.is_repeated {
            if fi.ftype == Type::Message as i32 {
                body.push_str(&format!("        for _item in &self.{} {{\n", fi.name));
                body.push_str(&format!(
                    "            let _ = buf.write_tag({}u32, ::oxiproto_core::wire::WireType::Len);\n",
                    fi.number
                ));
                body.push_str("            let _item_len = _item.encoded_len();\n");
                body.push_str("            buf.write_varint(_item_len as u64);\n");
                body.push_str("            _item.encode_raw(buf);\n");
                body.push_str("        }\n");
            } else if fi.ftype == Type::String as i32 {
                body.push_str(&format!("        for _item in &self.{} {{\n", fi.name));
                body.push_str(&format!(
                    "            let _ = buf.write_tag({}u32, ::oxiproto_core::wire::WireType::Len);\n",
                    fi.number
                ));
                body.push_str("            buf.write_string(_item);\n");
                body.push_str("        }\n");
            } else if fi.ftype == Type::Bytes as i32 {
                body.push_str(&format!("        for _item in &self.{} {{\n", fi.name));
                body.push_str(&format!(
                    "            let _ = buf.write_tag({}u32, ::oxiproto_core::wire::WireType::Len);\n",
                    fi.number
                ));
                body.push_str("            buf.write_length_delimited(_item);\n");
                body.push_str("        }\n");
            } else if is_packable(fi.ftype) {
                // Packed repeated
                body.push_str(&format!("        if !self.{}.is_empty() {{\n", fi.name));
                body.push_str(&format!(
                    "            let _ = buf.write_tag({}u32, ::oxiproto_core::wire::WireType::Len);\n",
                    fi.number
                ));
                body.push_str(&format!(
                    "            let _payload_len: usize = self.{}.iter().map(|_v| {}).sum();\n",
                    fi.name,
                    packed_elem_len_expr(fi.ftype, "*_v")
                ));
                body.push_str("            buf.write_varint(_payload_len as u64);\n");
                body.push_str(&format!("            for _v in &self.{} {{\n", fi.name));
                let encode = encode_val_expr(fi.ftype, "*_v");
                body.push_str(&format!("                {encode};\n"));
                body.push_str("            }\n");
                body.push_str("        }\n");
            }
        } else if fi.ftype == Type::Message as i32 {
            body.push_str(&format!(
                "        if let Some(ref _msg) = self.{} {{\n",
                fi.name
            ));
            body.push_str(&format!(
                "            let _ = buf.write_tag({}u32, ::oxiproto_core::wire::WireType::Len);\n",
                fi.number
            ));
            body.push_str("            let _msg_len = _msg.encoded_len();\n");
            body.push_str("            buf.write_varint(_msg_len as u64);\n");
            body.push_str("            _msg.encode_raw(buf);\n");
            body.push_str("        }\n");
        } else {
            // Singular scalar: skip default values
            let value_expr = format!("self.{}", fi.name);
            let default_check = is_default_expr(fi.ftype, &value_expr);
            body.push_str(&format!("        if !({default_check}) {{\n"));
            let stmt = scalar_encode_stmt(fi.number, fi.ftype, &value_expr, "            ");
            body.push_str(&stmt);
            body.push_str("        }\n");
        }
    }

    // Map fields
    emit_map_encode_raw(&mut body, msg, map_field_names)?;

    body.push_str("        self._unknown.encode_to(buf);\n");
    Ok(body)
}

fn encode_val_expr(ftype: i32, value_expr: &str) -> String {
    match ftype {
        t if t == Type::Fixed32 as i32 || t == Type::Sfixed32 as i32 => {
            format!("buf.write_fixed32(({value_expr}) as u32)")
        }
        t if t == Type::Float as i32 => format!("buf.write_float({value_expr})"),
        t if t == Type::Fixed64 as i32 || t == Type::Sfixed64 as i32 => {
            format!("buf.write_fixed64(({value_expr}) as u64)")
        }
        t if t == Type::Double as i32 => format!("buf.write_double({value_expr})"),
        t if t == Type::Sint32 as i32 => format!(
            "buf.write_varint(::oxiproto_core::wire::zigzag::zigzag_encode32({value_expr}) as u64)"
        ),
        t if t == Type::Sint64 as i32 => format!(
            "buf.write_varint(::oxiproto_core::wire::zigzag::zigzag_encode64({value_expr}) as u64)"
        ),
        t if t == Type::Int32 as i32 => format!("buf.write_varint_i32({value_expr})"),
        t if t == Type::Int64 as i32 => format!("buf.write_varint_i64({value_expr})"),
        t if t == Type::Uint32 as i32 => format!("buf.write_varint32({value_expr})"),
        t if t == Type::Uint64 as i32 => format!("buf.write_varint({value_expr})"),
        t if t == Type::Bool as i32 => format!("buf.write_bool({value_expr})"),
        _ => format!("buf.write_varint_i32({value_expr} as i32)"),
    }
}

fn emit_map_encode_raw(
    body: &mut String,
    msg: &DescriptorProto,
    map_field_names: &std::collections::HashSet<String>,
) -> Result<(), CodegenError> {
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
            let field_name = field.name.as_deref().unwrap_or("");
            if !map_field_names.contains(field_name) {
                continue;
            }
            let field_number = field.number.unwrap_or(0) as u32;

            let key_field = nested
                .field
                .iter()
                .find(|f| f.name.as_deref() == Some("key"));
            let val_field = nested
                .field
                .iter()
                .find(|f| f.name.as_deref() == Some("value"));

            if let (Some(kf), Some(vf)) = (key_field, val_field) {
                let ktype = kf.r#type.unwrap_or(Type::String as i32);
                let vtype = vf.r#type.unwrap_or(Type::String as i32);
                let k_wire = wire_type_for_field(ktype);
                let v_wire = wire_type_for_field(vtype);
                let k_wtp = wire_type_path(k_wire);
                let v_wtp = wire_type_path(v_wire);

                body.push_str(&format!(
                    "        for (_mk, _mv) in &self.{field_name} {{\n"
                ));
                // Compute entry size
                let k_tag = tag_len_expr(1, k_wire);
                let v_tag = tag_len_expr(2, v_wire);
                let k_len = key_field_len_expr(ktype, "_mk", &k_tag);
                let v_len = val_field_len_expr(vtype, "_mv", &v_tag);
                body.push_str(&format!(
                    "            let _entry_len: usize = {k_len} + {v_len};\n"
                ));
                body.push_str(&format!(
                    "            let _ = buf.write_tag({field_number}u32, ::oxiproto_core::wire::WireType::Len);\n"
                ));
                body.push_str("            buf.write_varint(_entry_len as u64);\n");
                // Key
                body.push_str(&format!(
                    "            let _ = buf.write_tag(1u32, {k_wtp});\n"
                ));
                let k_enc = encode_map_key_expr(ktype, "_mk");
                body.push_str(&format!("            {k_enc};\n"));
                // Value
                body.push_str(&format!(
                    "            let _ = buf.write_tag(2u32, {v_wtp});\n"
                ));
                let v_enc = encode_map_val_expr(vtype, "_mv");
                body.push_str(&format!("            {v_enc};\n"));
                body.push_str("        }\n");
            }
        }
    }
    Ok(())
}

fn encode_map_key_expr(ftype: i32, expr: &str) -> String {
    if ftype == Type::String as i32 {
        format!("buf.write_string({expr})")
    } else if ftype == Type::Bytes as i32 {
        format!("buf.write_length_delimited({expr})")
    } else {
        encode_val_expr(ftype, &format!("*{expr}"))
    }
}

fn encode_map_val_expr(ftype: i32, expr: &str) -> String {
    if ftype == Type::Message as i32 {
        format!("{{ let _ml = {expr}.encoded_len(); buf.write_varint(_ml as u64); {expr}.encode_raw(buf); }}")
    } else if ftype == Type::String as i32 {
        format!("buf.write_string({expr})")
    } else if ftype == Type::Bytes as i32 {
        format!("buf.write_length_delimited({expr})")
    } else {
        encode_val_expr(ftype, &format!("*{expr}"))
    }
}

// ── merge emission ────────────────────────────────────────────────────────────

fn emit_merge_body(
    msg: &DescriptorProto,
    map_field_names: &std::collections::HashSet<String>,
) -> Result<String, CodegenError> {
    let mut body = String::new();
    body.push_str("        loop {\n");
    body.push_str("            if buf.is_empty() { break; }\n");
    body.push_str("            let _tag = match buf.read_tag() {\n");
    body.push_str("                Ok(t) => t,\n");
    body.push_str(
        "                Err(::oxiproto_core::wire::WireError::UnexpectedEof) => break,\n",
    );
    body.push_str(
        "                Err(e) => return Err(::oxiproto_core::OxiProtoError::WireFormatError(e)),\n",
    );
    body.push_str("            };\n");
    body.push_str("            match _tag.field_number {\n");

    let fields = collect_fields(msg)?;

    // Collect oneof groups
    let mut oneof_field_sets: Vec<Vec<&FieldDescriptorProto>> =
        vec![Vec::new(); msg.oneof_decl.len()];
    for fi in &fields {
        if let Some(oi) = fi.field.oneof_index {
            if let Some(set) = oneof_field_sets.get_mut(oi as usize) {
                set.push(fi.field);
            }
        }
    }

    for fi in &fields {
        if fi.in_oneof {
            // Handled by the oneof block below
            continue;
        }
        if map_field_names.contains(fi.name) {
            // Map fields handled separately
            continue;
        }

        body.push_str(&format!("                {} => {{\n", fi.number));
        if fi.is_repeated {
            if fi.ftype == Type::Message as i32 {
                body.push_str("                    let _bytes = buf.read_length_delimited().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n");
                body.push_str("                    let mut _inner_buf = ::oxiproto_core::wire::DecodeBuffer::new(_bytes);\n");
                // We can't call T::decode_raw here because we don't know the type name
                // Instead, emit a placeholder that will require OxiMessage bound
                body.push_str("                    let mut _new_item = Default::default();\n");
                body.push_str("                    ::oxiproto_core::OxiMessage::merge(&mut _new_item, &mut _inner_buf)?;\n");
                body.push_str(&format!(
                    "                    self.{}.push(_new_item);\n",
                    fi.name
                ));
            } else if fi.ftype == Type::String as i32 {
                body.push_str("                    let _s = buf.read_string().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?.to_owned();\n");
                body.push_str(&format!("                    self.{}.push(_s);\n", fi.name));
            } else if fi.ftype == Type::Bytes as i32 {
                body.push_str("                    let _b = buf.read_length_delimited().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?.to_vec();\n");
                body.push_str(&format!("                    self.{}.push(_b);\n", fi.name));
            } else if is_packable(fi.ftype) {
                // Packed or unpacked — check wire type
                body.push_str("                    if _tag.wire_type == ::oxiproto_core::wire::WireType::Len {\n");
                body.push_str("                        let _packed = buf.read_length_delimited().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n");
                body.push_str("                        let mut _pb = ::oxiproto_core::wire::DecodeBuffer::new(_packed);\n");
                body.push_str("                        while !_pb.is_empty() {\n");
                let decode = scalar_decode_stmts(fi.ftype, "_val", "                            ");
                body.push_str("                            let mut _val = Default::default();\n");
                body.push_str(&decode);
                body.push_str(&format!(
                    "                            self.{}.push(_val);\n",
                    fi.name
                ));
                body.push_str("                        }\n");
                body.push_str("                    } else {\n");
                let decode2 = scalar_decode_stmts(fi.ftype, "_val", "                        ");
                body.push_str("                        let mut _val = Default::default();\n");
                body.push_str(&decode2);
                body.push_str(&format!(
                    "                        self.{}.push(_val);\n",
                    fi.name
                ));
                body.push_str("                    }\n");
            }
        } else if fi.ftype == Type::Message as i32 {
            body.push_str("                    let _bytes = buf.read_length_delimited().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n");
            body.push_str("                    let mut _inner_buf = ::oxiproto_core::wire::DecodeBuffer::new(_bytes);\n");
            body.push_str(&format!(
                "                    if self.{}.is_none() {{ self.{0} = Some(Default::default()); }}\n",
                fi.name
            ));
            body.push_str(&format!(
                "                    if let Some(ref mut _m) = self.{} {{\n",
                fi.name
            ));
            body.push_str(
                "                        ::oxiproto_core::OxiMessage::merge(_m.as_mut(), &mut _inner_buf)?;\n",
            );
            body.push_str("                    }\n");
        } else {
            let decode = scalar_decode_stmts(
                fi.ftype,
                &format!("self.{}", fi.name),
                "                    ",
            );
            body.push_str(&decode);
        }
        body.push_str("                }\n");
    }

    // Oneof fields
    for (oi, oneof) in msg.oneof_decl.iter().enumerate() {
        let ofields = &oneof_field_sets[oi];
        if ofields.is_empty() {
            continue;
        }
        let oname = oneof.name.as_deref().unwrap_or("unknown");
        // Use struct-level oneof type name prefix — needs the parent struct name
        // We'll pass it as parameter

        for of in ofields {
            let field_num = of.number.unwrap_or(0);
            let fname = of.name.as_deref().unwrap_or("unknown");
            let variant = crate::emit::to_pascal_case_pub(fname);
            let ftype = of.r#type.unwrap_or(Type::String as i32);

            body.push_str(&format!("                {field_num} => {{\n"));
            if ftype == Type::Message as i32 {
                body.push_str("                    let _bytes = buf.read_length_delimited().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n");
                body.push_str("                    let mut _inner_buf = ::oxiproto_core::wire::DecodeBuffer::new(_bytes);\n");
                body.push_str("                    let mut _inner: Box<_> = Default::default();\n");
                body.push_str(
                    "                    ::oxiproto_core::OxiMessage::merge(_inner.as_mut(), &mut _inner_buf)?;\n",
                );
                body.push_str(&format!(
                    "                    self.{oname} = Some({variant}(_inner));\n"
                ));
            } else {
                let decode = scalar_decode_stmts(ftype, "_ov", "                    ");
                body.push_str("                    let mut _ov = Default::default();\n");
                body.push_str(&decode);
                body.push_str(&format!(
                    "                    self.{oname} = Some({variant}(_ov));\n"
                ));
            }
            body.push_str("                }\n");
        }
    }

    // Map fields
    emit_map_merge(&mut body, msg, map_field_names)?;

    // Unknown fields
    body.push_str("                _ => {\n");
    body.push_str("                    match _tag.wire_type {\n");
    body.push_str("                        ::oxiproto_core::wire::WireType::Varint => {\n");
    body.push_str("                            let _v = buf.read_varint().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n");
    body.push_str(
        "                            self._unknown.push_varint(_tag.field_number, _v);\n",
    );
    body.push_str("                        }\n");
    body.push_str("                        ::oxiproto_core::wire::WireType::I64 => {\n");
    body.push_str("                            let _v = buf.read_fixed64().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n");
    body.push_str(
        "                            self._unknown.push_fixed64(_tag.field_number, _v);\n",
    );
    body.push_str("                        }\n");
    body.push_str("                        ::oxiproto_core::wire::WireType::Len => {\n");
    body.push_str("                            let _v = buf.read_length_delimited().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?.to_vec();\n");
    body.push_str(
        "                            self._unknown.push_length_delimited(_tag.field_number, _v);\n",
    );
    body.push_str("                        }\n");
    body.push_str("                        ::oxiproto_core::wire::WireType::I32 => {\n");
    body.push_str("                            let _v = buf.read_fixed32().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n");
    body.push_str(
        "                            self._unknown.push_fixed32(_tag.field_number, _v);\n",
    );
    body.push_str("                        }\n");
    body.push_str("                        _ => {\n");
    body.push_str("                            buf.skip_field(_tag.wire_type).map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n");
    body.push_str("                        }\n");
    body.push_str("                    }\n");
    body.push_str("                }\n");
    body.push_str("            }\n");
    body.push_str("        }\n");
    body.push_str("        Ok(())\n");
    Ok(body)
}

fn emit_map_merge(
    body: &mut String,
    msg: &DescriptorProto,
    map_field_names: &std::collections::HashSet<String>,
) -> Result<(), CodegenError> {
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
            let field_name = field.name.as_deref().unwrap_or("");
            if !map_field_names.contains(field_name) {
                continue;
            }
            let field_number = field.number.unwrap_or(0) as u32;

            let key_field = nested
                .field
                .iter()
                .find(|f| f.name.as_deref() == Some("key"));
            let val_field = nested
                .field
                .iter()
                .find(|f| f.name.as_deref() == Some("value"));

            if let (Some(kf), Some(vf)) = (key_field, val_field) {
                let ktype = kf.r#type.unwrap_or(Type::String as i32);
                let vtype = vf.r#type.unwrap_or(Type::String as i32);

                body.push_str(&format!("                {field_number} => {{\n"));
                body.push_str("                    let _entry_bytes = buf.read_length_delimited().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n");
                body.push_str("                    let mut _eb = ::oxiproto_core::wire::DecodeBuffer::new(_entry_bytes);\n");
                body.push_str("                    let mut _k = Default::default();\n");
                body.push_str("                    let mut _v = Default::default();\n");
                body.push_str("                    while !_eb.is_empty() {\n");
                body.push_str("                        let _et = _eb.read_tag().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n");
                body.push_str("                        match _et.field_number {\n");
                body.push_str("                            1 => {\n");
                let k_decode = scalar_decode_stmts(ktype, "_k", "                                ");
                body.push_str(&k_decode);
                body.push_str("                            }\n");
                body.push_str("                            2 => {\n");
                if vtype == Type::Message as i32 {
                    body.push_str("                                let _vb = _eb.read_length_delimited().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;\n");
                    body.push_str("                                let mut _vbuf = ::oxiproto_core::wire::DecodeBuffer::new(_vb);\n");
                    body.push_str("                                ::oxiproto_core::OxiMessage::merge(&mut _v, &mut _vbuf)?;\n");
                } else {
                    let v_decode =
                        scalar_decode_stmts(vtype, "_v", "                                ");
                    body.push_str(&v_decode);
                }
                body.push_str("                            }\n");
                body.push_str("                            _ => { _eb.skip_field(_et.wire_type).map_err(::oxiproto_core::OxiProtoError::WireFormatError)?; }\n");
                body.push_str("                        }\n");
                body.push_str("                    }\n");
                body.push_str(&format!(
                    "                    self.{field_name}.insert(_k, _v);\n"
                ));
                body.push_str("                }\n");
            }
        }
    }
    Ok(())
}

// ── clear emission ────────────────────────────────────────────────────────────

fn emit_clear_body(msg: &DescriptorProto) -> Result<String, CodegenError> {
    let mut body = String::new();
    let fields = collect_fields(msg)?;
    let mut emitted_oneofs: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for fi in &fields {
        if fi.in_oneof {
            let oi = fi.field.oneof_index.unwrap_or(0) as usize;
            if emitted_oneofs.contains(&oi) {
                continue;
            }
            emitted_oneofs.insert(oi);
            if let Some(oneof) = msg.oneof_decl.get(oi) {
                let oname = oneof.name.as_deref().unwrap_or("unknown");
                body.push_str(&format!("        self.{oname} = None;\n"));
            }
            continue;
        }
        body.push_str(&format!("        self.{} = Default::default();\n", fi.name));
    }
    body.push_str("        self._unknown.clear();\n");
    Ok(body)
}

// ── unknown encoded_len ───────────────────────────────────────────────────────

/// Returns Rust code that adds the encoded_len for UnknownFields.
/// The `UnknownFields` type needs an `encoded_len()` method.
/// Looking at the source, it has `encode_to()` but not `encoded_len()`.
/// We'll compute it by measuring: iterate and sum sizes.
fn unknown_fields_encoded_len_code() -> String {
    // Provide an inline helper since UnknownFields doesn't have encoded_len()
    // We need to add this method — or compute it inline.
    // Let's compute inline via an iterator
    "self._unknown.iter().map(|f| {\n            let tag_val = (u64::from(f.field_number) << 3) | (f.value.wire_type() as u64);\n            let tag_len = ::oxiproto_core::wire::varint::encoded_len_varint(tag_val);\n            let val_len = match &f.value {\n                ::oxiproto_core::wire::unknown::UnknownValue::Varint(v) => ::oxiproto_core::wire::varint::encoded_len_varint(*v),\n                ::oxiproto_core::wire::unknown::UnknownValue::Fixed64(_) => 8usize,\n                ::oxiproto_core::wire::unknown::UnknownValue::LengthDelimited(d) => ::oxiproto_core::wire::varint::encoded_len_varint(d.len() as u64) + d.len(),\n                ::oxiproto_core::wire::unknown::UnknownValue::Fixed32(_) => 4usize,\n                ::oxiproto_core::wire::unknown::UnknownValue::Group(d) => d.len(),\n            };\n            tag_len + val_len\n        }).sum::<usize>()".to_string()
}

// ── public API ────────────────────────────────────────────────────────────────

/// Emit `impl ::oxiproto_core::OxiMessage for {struct_name}` block.
pub fn emit_oxi_message_impl(
    msg: &DescriptorProto,
    struct_name: &str,
    file_package: &str,
    map_field_names: &std::collections::HashSet<String>,
) -> Result<String, CodegenError> {
    let oneof_names: Vec<String> = msg
        .oneof_decl
        .iter()
        .filter_map(|o| o.name.as_deref().map(|n| n.to_owned()))
        .collect();

    let encoded_len_body =
        emit_encoded_len_body_v2(msg, struct_name, map_field_names, &oneof_names)?;
    let encode_raw_body = emit_encode_raw_body(msg, map_field_names)?;
    let merge_body = emit_merge_body(msg, map_field_names)?;
    let clear_body = emit_clear_body(msg)?;

    let mut out = String::new();
    out.push_str(&format!(
        "impl ::oxiproto_core::OxiMessage for {struct_name} {{\n"
    ));

    // encoded_len
    out.push_str("    fn encoded_len(&self) -> usize {\n");
    out.push_str(&encoded_len_body);
    out.push_str("    }\n\n");

    // encode_raw
    out.push_str("    fn encode_raw(&self, buf: &mut ::oxiproto_core::wire::EncodeBuffer) {\n");
    out.push_str(&encode_raw_body);
    out.push_str("    }\n\n");

    // merge
    out.push_str(
        "    fn merge(&mut self, buf: &mut ::oxiproto_core::wire::DecodeBuffer<'_>) -> ::oxiproto_core::OxiProtoResult<()> {\n",
    );
    out.push_str(&merge_body);
    out.push_str("    }\n\n");

    // clear
    out.push_str("    fn clear(&mut self) {\n");
    out.push_str(&clear_body);
    out.push_str("    }\n");

    out.push_str("}\n\n");
    let _ = file_package;
    Ok(out)
}

/// Emit `impl ::oxiproto_core::OxiName for {struct_name}` block.
pub fn emit_oxi_name_impl(msg: &DescriptorProto, struct_name: &str, file_package: &str) -> String {
    let proto_name = msg.name.as_deref().unwrap_or(struct_name);
    let mut out = String::new();
    out.push_str(&format!(
        "impl ::oxiproto_core::OxiName for {struct_name} {{\n"
    ));
    out.push_str(&format!(
        "    const NAME: &'static str = \"{proto_name}\";\n"
    ));
    out.push_str(&format!(
        "    const PACKAGE: &'static str = \"{file_package}\";\n"
    ));
    out.push_str("}\n\n");
    out
}

/// Revised encoded_len body using inline oneof handling.
fn emit_encoded_len_body_v2(
    msg: &DescriptorProto,
    struct_name: &str,
    map_field_names: &std::collections::HashSet<String>,
    _oneof_names: &[String],
) -> Result<String, CodegenError> {
    let _ = struct_name;
    let mut body = String::new();
    body.push_str("        let mut len = 0usize;\n");

    let fields = collect_fields(msg)?;
    let mut emitted_oneofs: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Collect oneof field sets
    let mut oneof_field_sets: Vec<Vec<&FieldDescriptorProto>> =
        vec![Vec::new(); msg.oneof_decl.len()];
    for fi in &fields {
        if let Some(oi) = fi.field.oneof_index {
            if let Some(set) = oneof_field_sets.get_mut(oi as usize) {
                set.push(fi.field);
            }
        }
    }

    for fi in &fields {
        if map_field_names.contains(fi.name) {
            continue; // handled in emit_map_encoded_len
        }

        if fi.in_oneof {
            let oi = fi.field.oneof_index.unwrap_or(0) as usize;
            if emitted_oneofs.contains(&oi) {
                continue;
            }
            emitted_oneofs.insert(oi);
            if let Some(oneof) = msg.oneof_decl.get(oi) {
                let oname = oneof.name.as_deref().unwrap_or("unknown");
                let ofields = &oneof_field_sets[oi];
                let oneof_type = format!("{struct_name}_{}", to_pascal_case(oname));
                body.push_str(&format!("        if let Some(ref _ov) = self.{oname} {{\n"));
                body.push_str("            match _ov {\n");
                for of in ofields {
                    let vname = of.name.as_deref().unwrap_or("unknown");
                    let variant = crate::emit::to_pascal_case_pub(vname);
                    let vtype = of.r#type.unwrap_or(Type::String as i32);
                    let field_num = of.number.unwrap_or(0) as u32;
                    let _ = oneof_type.as_str();
                    if vtype == Type::Message as i32 {
                        let tag = tag_len_expr(field_num, 2);
                        body.push_str(&format!("                {variant}(_inner) => {{\n"));
                        body.push_str("                    let _ml = _inner.encoded_len();\n");
                        body.push_str(&format!("                    len += {tag} + ::oxiproto_core::wire::varint::encoded_len_varint(_ml as u64) + _ml;\n"));
                        body.push_str("                }\n");
                    } else {
                        body.push_str(&format!("                {variant}(_val) => {{\n"));
                        let wt = wire_type_for_field(vtype);
                        let tag = tag_len_expr(field_num, wt);
                        let vlen = packed_elem_len_expr(vtype, "*_val");
                        body.push_str(&format!("                    len += {tag} + {vlen};\n"));
                        body.push_str("                }\n");
                    }
                }
                body.push_str("            }\n");
                body.push_str("        }\n");
            }
            continue;
        }

        if fi.is_repeated {
            if fi.ftype == Type::Message as i32 {
                let tag = tag_len_expr(fi.number, 2);
                body.push_str(&format!("        for _item in &self.{} {{\n", fi.name));
                body.push_str("            let _item_len = _item.encoded_len();\n");
                body.push_str(&format!(
                    "            len += {tag} + ::oxiproto_core::wire::varint::encoded_len_varint(_item_len as u64) + _item_len;\n"
                ));
                body.push_str("        }\n");
            } else if fi.ftype == Type::String as i32 || fi.ftype == Type::Bytes as i32 {
                let tag = tag_len_expr(fi.number, 2);
                body.push_str(&format!("        for _item in &self.{} {{\n", fi.name));
                body.push_str(&format!(
                    "            len += {tag} + ::oxiproto_core::wire::varint::encoded_len_varint(_item.len() as u64) + _item.len();\n"
                ));
                body.push_str("        }\n");
            } else if is_packable(fi.ftype) {
                let tag = tag_len_expr(fi.number, 2);
                body.push_str(&format!("        if !self.{}.is_empty() {{\n", fi.name));
                body.push_str(&format!(
                    "            let _payload_len: usize = self.{}.iter().map(|_v| {}).sum();\n",
                    fi.name,
                    packed_elem_len_expr(fi.ftype, "*_v")
                ));
                body.push_str(&format!(
                    "            len += {tag} + ::oxiproto_core::wire::varint::encoded_len_varint(_payload_len as u64) + _payload_len;\n"
                ));
                body.push_str("        }\n");
            }
        } else if fi.ftype == Type::Message as i32 {
            body.push_str(&format!(
                "        if let Some(ref _msg) = self.{} {{\n",
                fi.name
            ));
            let tag = tag_len_expr(fi.number, 2);
            body.push_str("            let _msg_len = _msg.encoded_len();\n");
            body.push_str(&format!(
                "            len += {tag} + ::oxiproto_core::wire::varint::encoded_len_varint(_msg_len as u64) + _msg_len;\n"
            ));
            body.push_str("        }\n");
        } else {
            let value_expr = format!("self.{}", fi.name);
            let default_check = is_default_expr(fi.ftype, &value_expr);
            body.push_str(&format!("        if !({default_check}) {{\n"));
            let len_expr = scalar_encoded_len_expr(fi.number, fi.ftype, &value_expr);
            body.push_str(&format!("            len += {len_expr};\n"));
            body.push_str("        }\n");
        }
    }

    // Map fields
    emit_map_encoded_len(&mut body, msg, map_field_names)?;

    // Unknown fields: inline computation
    let unk_len = unknown_fields_encoded_len_code();
    body.push_str(&format!("        len += {unk_len};\n"));
    body.push_str("        len\n");
    let _ = struct_name;
    Ok(body)
}

/// Convert snake_case/SCREAMING_SNAKE to PascalCase (mirrors emit.rs).
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
