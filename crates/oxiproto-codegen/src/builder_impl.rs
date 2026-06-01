#![forbid(unsafe_code)]

//! Builder-pattern code generation for protobuf messages.
//!
//! When [`CodegenOptions::emit_builder`] is `true`, each generated message type
//! gets a companion `FooBuilder` struct with fluent setters and a `build()` method.

use prost_types::DescriptorProto;

use crate::emit::{collect_map_entries, field_type_str_with_wkt};
use crate::options::{CodegenError, CodegenOptions};
use crate::type_registry::TypeRegistry;

/// Emit a `FooBuilder` struct and its `impl` block for the given message.
///
/// - Map fields → `pub fn insert_field_name(mut self, k: K, v: V) -> Self`
/// - Repeated fields → `pub fn add_field_name(mut self, v: E) -> Self`
/// - All other fields (scalars, enums, singular messages/WKT) → `pub fn field_name(mut self, v: T) -> Self`
/// - Oneof-member fields and reserved fields are skipped.
///
/// Returns an error only when the descriptor is invalid (e.g. a field is missing
/// its name), matching the contract of the surrounding emit infrastructure.
pub(crate) fn emit_builder_for_message(
    msg: &DescriptorProto,
    type_name: &str,
    _opts: &CodegenOptions,
    file_package: &str,
    registry: &TypeRegistry,
) -> Result<String, CodegenError> {
    // Skip synthetic map entry types (they are never top-level structs).
    if msg.options.as_ref().is_some_and(|o| o.map_entry()) {
        return Ok(String::new());
    }

    let builder_name = format!("{type_name}Builder");

    // Collect reserved field numbers and names so we can skip them.
    let res_nums = reserved_numbers(msg);
    let res_names = reserved_names(msg);

    // Map field names → (key_type, value_type) for insert_X setters.
    let map_entries = collect_map_entries(msg, file_package, registry);

    let mut out = String::new();

    // ── struct definition ──────────────────────────────────────────────────────
    out.push_str(&format!("pub struct {builder_name} {{\n"));
    out.push_str(&format!("    inner: {type_name},\n"));
    out.push_str("}\n");

    // ── Default impl ──────────────────────────────────────────────────────────
    out.push_str(&format!("impl Default for {builder_name} {{\n"));
    out.push_str("    fn default() -> Self {\n");
    out.push_str(&format!(
        "        Self {{ inner: {type_name}::default() }}\n"
    ));
    out.push_str("    }\n");
    out.push_str("}\n");

    // ── impl block ────────────────────────────────────────────────────────────
    out.push_str(&format!("impl {builder_name} {{\n"));

    out.push_str("    pub fn new() -> Self {\n");
    out.push_str("        Self::default()\n");
    out.push_str("    }\n");

    // Emit one setter per field (or skip, per rules).
    for field in &msg.field {
        let fname = field
            .name
            .as_deref()
            .ok_or_else(|| CodegenError::InvalidDescriptor("field missing name".into()))?;
        let field_number = field.number.unwrap_or(0);

        // Skip reserved fields.
        if res_nums.contains(&field_number) || res_names.contains(fname) {
            continue;
        }

        // Skip fields that are members of a oneof group.
        if field.oneof_index.is_some() {
            continue;
        }

        // Map fields — must be checked BEFORE the repeated check because map
        // fields carry the Repeated label in the descriptor.
        if let Some(map_info) = map_entries.get(fname) {
            let k = &map_info.key_type;
            let v = &map_info.value_type;
            out.push_str(&format!(
                "    pub fn insert_{fname}(mut self, k: {k}, v: {v}) -> Self {{\n"
            ));
            out.push_str(&format!("        self.inner.{fname}.insert(k, v);\n"));
            out.push_str("        self\n");
            out.push_str("    }\n");
            continue;
        }

        // Resolve the full Rust type for this field (including Vec<> for repeated,
        // Option<Box<>> for singular messages, etc.).
        let rust_type = field_type_str_with_wkt(field, type_name, file_package, registry)?;

        // Repeated fields — setter pushes a single element.
        if rust_type.starts_with("Vec<") && rust_type.ends_with('>') {
            let element_type = &rust_type[4..rust_type.len() - 1];
            out.push_str(&format!(
                "    pub fn add_{fname}(mut self, v: {element_type}) -> Self {{\n"
            ));
            out.push_str(&format!("        self.inner.{fname}.push(v);\n"));
            out.push_str("        self\n");
            out.push_str("    }\n");
            continue;
        }

        // All other fields — take the exact Rust type by value and assign.
        out.push_str(&format!(
            "    pub fn {fname}(mut self, v: {rust_type}) -> Self {{\n"
        ));
        out.push_str(&format!("        self.inner.{fname} = v;\n"));
        out.push_str("        self\n");
        out.push_str("    }\n");
    }

    // build() method
    out.push_str(&format!("    pub fn build(self) -> {type_name} {{\n"));
    out.push_str("        self.inner\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    Ok(out)
}

/// Collect reserved field numbers from a message descriptor.
fn reserved_numbers(msg: &DescriptorProto) -> std::collections::HashSet<i32> {
    let mut set = std::collections::HashSet::new();
    for range in &msg.reserved_range {
        let start = range.start.unwrap_or(0);
        let end = range.end.unwrap_or(0);
        for n in start..end {
            set.insert(n);
        }
    }
    set
}

/// Collect reserved field names from a message descriptor.
fn reserved_names(msg: &DescriptorProto) -> std::collections::HashSet<&str> {
    msg.reserved_name.iter().map(|s| s.as_str()).collect()
}
