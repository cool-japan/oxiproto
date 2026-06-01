#![forbid(unsafe_code)]

//! Code-generation options and error types.

use std::collections::BTreeMap;

/// Options controlling code generation from a `FileDescriptorSet`.
#[derive(Debug, Clone)]
pub struct CodegenOptions {
    /// Generate doc comments from proto source info (default: true)
    pub generate_docs: bool,
    /// Generate `Default` impls for enums (default: true)
    pub generate_default: bool,
    /// Use `#[deprecated]` for deprecated fields/messages/enums (default: true)
    pub generate_deprecated: bool,
    /// Use `BTreeMap` for proto map fields instead of `HashMap` (default: false)
    pub btree_map: bool,
    /// Alias for `btree_map` kept for backward compat with existing tests
    pub use_btree_map: bool,
    /// Emit `pub mod` hierarchy matching the proto package structure (default: true).
    /// When false, all types are emitted flat (no module nesting).
    pub package_namespacing: bool,
    /// Per-type custom attributes. Key: fully-qualified proto type name.
    /// Value: list of attribute strings (e.g., `["#[derive(serde::Serialize)]"]`).
    pub type_attributes: BTreeMap<String, Vec<String>>,
    /// Per-field custom attributes. Key: "TypeName.field_name".
    /// Value: list of attribute strings.
    pub field_attributes: BTreeMap<String, Vec<String>>,
    /// Emit `impl OxiMessage for T` + `impl OxiName for T` blocks (default: false).
    /// Requires `oxiproto-core` as a dependency of the crate using generated code.
    pub emit_oxi_message_impl: bool,
    /// Use prettyplease to format generated code (requires `format` feature).
    pub format_output: bool,
    /// Emit `pub trait …` service definitions (default: true).
    /// Set to `false` to suppress service-trait emission (e.g. `--grpc=false` in the CLI).
    pub emit_services: bool,
    /// Emit self-contained `to_json`/`from_json` methods on generated types
    /// (canonical Protobuf-JSON mapping). Requires `serde_json` and `base64`
    /// in the consumer crate. Default: false.
    pub emit_json: bool,
    /// Emit a `FooBuilder` struct with fluent setters for each message (default: false).
    pub emit_builder: bool,
    /// Emit a `to_text_format() -> String` method on each generated message struct (default: false).
    pub emit_text_format: bool,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl CodegenOptions {
    /// Create options with sensible proto3 defaults.
    pub fn new() -> Self {
        Self {
            generate_docs: true,
            generate_default: true,
            generate_deprecated: true,
            btree_map: false,
            use_btree_map: false,
            // Default false for backward compatibility with existing tests and users.
            // Set to true to emit `pub mod` hierarchy matching the proto package.
            package_namespacing: false,
            type_attributes: BTreeMap::new(),
            field_attributes: BTreeMap::new(),
            emit_oxi_message_impl: false,
            format_output: false,
            emit_services: true,
            emit_json: false,
            emit_builder: false,
            emit_text_format: false,
        }
    }

    /// Returns true if BTreeMap should be used for map fields.
    pub fn use_btree_map_effective(&self) -> bool {
        self.btree_map || self.use_btree_map
    }
}

/// Errors produced during code generation.
#[derive(Debug)]
pub enum CodegenError {
    /// A required descriptor field is missing or invalid.
    InvalidDescriptor(String),
    /// An I/O operation failed.
    Io(std::io::Error),
    /// A `syn` parse error (only with `format` feature).
    #[cfg(feature = "format")]
    Parse(syn::Error),
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodegenError::InvalidDescriptor(s) => write!(f, "invalid descriptor: {s}"),
            CodegenError::Io(e) => write!(f, "I/O error: {e}"),
            #[cfg(feature = "format")]
            CodegenError::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}

impl std::error::Error for CodegenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CodegenError::Io(e) => Some(e),
            CodegenError::InvalidDescriptor(_) => None,
            #[cfg(feature = "format")]
            CodegenError::Parse(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for CodegenError {
    fn from(e: std::io::Error) -> Self {
        CodegenError::Io(e)
    }
}

impl From<oxiproto_core::OxiProtoError> for CodegenError {
    fn from(e: oxiproto_core::OxiProtoError) -> Self {
        CodegenError::InvalidDescriptor(e.to_string())
    }
}

impl From<CodegenError> for oxiproto_core::OxiProtoError {
    fn from(e: CodegenError) -> Self {
        oxiproto_core::OxiProtoError::CodegenError(e.to_string())
    }
}
