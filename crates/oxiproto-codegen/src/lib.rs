#![forbid(unsafe_code)]

//! `oxiproto-codegen` — Generate plain Rust source code from a `FileDescriptorSet`.
//!
//! This crate walks a `prost_types::FileDescriptorSet` and emits plain Rust
//! structs and enums — no prost derive, no gRPC stubs, no validators.
//!
//! ## Quick start
//!
//! ```no_run
//! use prost_types::FileDescriptorSet;
//!
//! let fds: FileDescriptorSet = /* parse your .proto */ Default::default();
//! let rust_src = oxiproto_codegen::generate(&fds).expect("codegen failed");
//! println!("{rust_src}");
//! ```

pub(crate) mod builder_impl;
mod emit;
mod format;
mod json_impl;
mod message_impl;
mod options;
pub(crate) mod text_impl;
pub(crate) mod type_registry;
pub mod wkt_map;

pub use options::{CodegenError, CodegenOptions};

pub use emit::{emit_file_descriptor_set, emit_file_descriptor_set_with_options, ModuleTree};

/// Generate Rust source code from a `FileDescriptorSet`.
///
/// Returns a `String` of Rust source containing `struct` and `enum` definitions
/// for every message and enum found in the descriptor set.  Fields are mapped
/// to plain Rust types (no `prost` derive macros).
pub fn generate(fds: &prost_types::FileDescriptorSet) -> Result<String, CodegenError> {
    emit::emit_file_descriptor_set(fds)
}

/// Generate Rust source code with custom codegen options.
///
/// This allows enabling additional features like doc comment generation,
/// `Default` impls, deprecation attributes, and BTreeMap for map fields.
pub fn generate_with_options(
    fds: &prost_types::FileDescriptorSet,
    options: &CodegenOptions,
) -> Result<String, CodegenError> {
    let code = emit::emit_file_descriptor_set_with_options(fds, options)?;
    #[cfg(feature = "format")]
    let code = if options.format_output {
        crate::format::format_source(&code)?
    } else {
        code
    };
    Ok(code)
}

/// Generate a structured module tree from a `FileDescriptorSet`.
///
/// Unlike [`generate_with_options`] which returns a flat `String`,
/// this returns a [`ModuleTree`] that preserves the package hierarchy,
/// enabling per-package or per-file output.
///
/// # Errors
///
/// Returns a [`CodegenError`] if the descriptor set is invalid.
pub fn generate_module(
    fds: &prost_types::FileDescriptorSet,
    options: &CodegenOptions,
) -> Result<ModuleTree, CodegenError> {
    emit::generate_module_tree(fds, options)
}

/// Write generated code to a file path.
///
/// Equivalent to calling [`generate`] and then writing the resulting string to
/// `path`.
pub fn generate_to_file(
    fds: &prost_types::FileDescriptorSet,
    path: &std::path::Path,
) -> Result<(), CodegenError> {
    let code = generate(fds)?;
    std::fs::write(path, code).map_err(CodegenError::Io)
}

/// Write generated code to a file path with custom options.
pub fn generate_to_file_with_options(
    fds: &prost_types::FileDescriptorSet,
    path: &std::path::Path,
    options: &CodegenOptions,
) -> Result<(), CodegenError> {
    let code = generate_with_options(fds, options)?;
    std::fs::write(path, code).map_err(CodegenError::Io)
}

/// Stream generated code into any [`std::io::Write`] sink.
///
/// This is the lowest-allocation path: it generates the Rust source and
/// writes it directly to `writer` without building an intermediate `String`
/// copy beyond the one produced by `generate_with_options`.
///
/// # Note on streaming
///
/// Internally the code generator builds a `String` via string concatenation.
/// This function writes that single buffer to `writer` without an extra copy,
/// making it preferable to `generate_with_options` when the caller is writing
/// to a file, socket, or other I/O sink.
///
/// For an in-memory buffer prefer [`generate_with_options`] directly.
///
/// # Errors
///
/// Returns [`CodegenError`] on descriptor errors or on I/O failure.
pub fn generate_to_writer<W: std::io::Write>(
    fds: &prost_types::FileDescriptorSet,
    options: &CodegenOptions,
    writer: &mut W,
) -> Result<(), CodegenError> {
    let code = generate_with_options(fds, options)?;
    writer.write_all(code.as_bytes()).map_err(CodegenError::Io)
}

/// Stream the default generated output into any [`std::io::Write`] sink.
///
/// Convenience wrapper around [`generate_to_writer`] using default
/// [`CodegenOptions`].
pub fn generate_to_writer_default<W: std::io::Write>(
    fds: &prost_types::FileDescriptorSet,
    writer: &mut W,
) -> Result<(), CodegenError> {
    generate_to_writer(fds, &CodegenOptions::default(), writer)
}
