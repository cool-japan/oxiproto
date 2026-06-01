#![forbid(unsafe_code)]

//! `oxiproto-build` — Pure Rust `.proto` → Rust codegen, no `protoc` required.
//!
//! Downstream crates add this as a `[build-dependencies]` entry and call
//! [`compile_protos`] (or [`Builder`]) from their `build.rs`.
//!
//! # Quick start
//!
//! ```no_run
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     oxiproto_build::compile_protos(&["proto/service.proto"], &["proto/"])?;
//!     Ok(())
//! }
//! ```

pub mod builder;
pub mod compile_str;
pub mod error;
/// Native `.proto` parser (Slice P fills this in).
pub mod parser;

pub use builder::Builder;
pub use compile_str::compile_str as compile_str_fn;
pub use error::BuildError;

/// Re-export `compile_str` under its canonical name for test compatibility.
pub use compile_str::compile_str;

use std::path::Path;

/// Compile `.proto` files to Rust without requiring `protoc` on `PATH`.
///
/// Generated `.rs` files are written to `OUT_DIR` (set by Cargo in
/// `build.rs` context).
///
/// # Arguments
///
/// * `protos`   — Paths to the `.proto` source files to compile.
/// * `includes` — Include directories used to resolve imports inside the
///   `.proto` files.
///
/// # Errors
///
/// Returns [`oxiproto_core::OxiProtoError::ParseError`] if `protox` cannot
/// parse or resolve the proto sources, or
/// [`oxiproto_core::OxiProtoError::CodegenError`] if `prost-build` cannot
/// emit Rust from the resolved descriptors.
///
/// # Example (in `build.rs`)
///
/// ```no_run
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     oxiproto_build::compile_protos(&["proto/service.proto"], &["proto/"])?;
///     Ok(())
/// }
/// ```
pub fn compile_protos(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> Result<(), oxiproto_core::OxiProtoError> {
    Builder::new()
        .compile(protos, includes)
        .map_err(oxiproto_core::OxiProtoError::from)
}

/// Parse `.proto` files to a [`prost_types::FileDescriptorSet`] without
/// invoking `protoc` or writing any files.
///
/// This is the low-level building block used by [`compile_protos`] and by the
/// `oxiproto-cli` binary.  It delegates to [`protox::compile`] (pure Rust).
///
/// # Errors
///
/// Returns [`oxiproto_core::OxiProtoError::ParseError`] if `protox` cannot
/// parse or resolve the proto sources.
///
/// # Example
///
/// ```no_run
/// let fds = oxiproto_build::compile_to_fds(
///     &["proto/hello.proto"],
///     &["proto/"],
/// )?;
/// println!("{} file(s) in FDS", fds.file.len());
/// # Ok::<(), oxiproto_core::OxiProtoError>(())
/// ```
pub fn compile_to_fds(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> Result<prost_types::FileDescriptorSet, oxiproto_core::OxiProtoError> {
    Builder::new()
        .compile_to_fds(protos, includes)
        .map_err(oxiproto_core::OxiProtoError::from)
}

/// Compile a proto3 source string to a [`prost_types::FileDescriptorSet`]
/// using the native pure-Rust parser (no `protox` dependency).
///
/// Requires the `native-parser` feature.
///
/// This function handles only single-file protos with no imports. For
/// multi-file protos use [`compile_files_native`].
///
/// # Errors
///
/// Returns [`BuildError`] on parse, resolution, or I/O failure.
#[cfg(feature = "native-parser")]
pub fn compile_str_native(
    proto_source: &str,
) -> Result<prost_types::FileDescriptorSet, BuildError> {
    use crate::parser::{build_file_descriptor_set, parse_file, resolve};

    let proto_file = parse_file(proto_source).map_err(|e| BuildError::Parse {
        file: "<inline>.proto".to_owned(),
        line: 0,
        col: 0,
        message: e.to_string(),
    })?;
    // Single-file mode: reject any imports
    if !proto_file.imports.is_empty() {
        return Err(BuildError::Parse {
            file: "<inline>.proto".to_owned(),
            line: 0,
            col: 0,
            message:
                "imports are not supported in compile_str_native; use compile_files_native instead"
                    .to_owned(),
        });
    }
    let resolved = resolve(&proto_file).map_err(|e| BuildError::Parse {
        file: "<inline>.proto".to_owned(),
        line: 0,
        col: 0,
        message: e.to_string(),
    })?;
    Ok(build_file_descriptor_set(
        &resolved,
        "<inline>.proto",
        proto_source,
    ))
}

/// Compile a set of `.proto` files to a [`prost_types::FileDescriptorSet`]
/// using the native parser.
///
/// Include directories are searched in order to resolve imports.
/// Well-known types (google/protobuf/*.proto) are loaded from the bundled pool.
///
/// # Errors
///
/// Returns [`BuildError`] if any file cannot be found, parsed, or type-resolved.
#[cfg(feature = "native-parser")]
pub fn compile_files_native(
    protos: &[impl AsRef<std::path::Path>],
    includes: &[impl AsRef<std::path::Path>],
) -> Result<prost_types::FileDescriptorSet, BuildError> {
    let inc: Vec<std::path::PathBuf> = includes.iter().map(|p| p.as_ref().to_path_buf()).collect();
    parser::loader::compile_files(protos, &inc)
}
