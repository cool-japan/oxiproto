#![forbid(unsafe_code)]

//! Source-code formatting via `prettyplease` (requires the `format` feature).
//!
//! This module is compiled only when the `format` feature is enabled.  It
//! provides a single entry point [`format_source`] that parses a Rust source
//! string with [`syn`] and unparses it with [`prettyplease`], producing
//! canonically formatted Rust.

/// Format Rust source code using `prettyplease`.
///
/// Parses `src` as a Rust source file via `syn::parse_file`, then unparses
/// the resulting syntax tree with `prettyplease::unparse`.  Returns the
/// formatted source on success, or a [`crate::options::CodegenError::Parse`]
/// on syntax error.
///
/// # Errors
///
/// Returns `CodegenError::Parse` when `src` is not valid Rust.
#[cfg(feature = "format")]
pub fn format_source(src: &str) -> Result<String, crate::options::CodegenError> {
    let parsed = syn::parse_file(src).map_err(crate::options::CodegenError::Parse)?;
    Ok(prettyplease::unparse(&parsed))
}
