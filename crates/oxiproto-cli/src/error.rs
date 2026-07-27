#![forbid(unsafe_code)]

//! Typed error type for `oxiproto-cli` subcommands.
//!
//! Every subcommand entry point returns `Result<(), CliError>` instead of a
//! type-erased `Box<dyn std::error::Error>`, so failures stay typed end to
//! end: callers (tests, or the CLI embedded as a library) can match on the
//! specific cause instead of parsing `Display` output.

use std::fmt;

/// Errors that can occur while running an `oxiproto-cli` subcommand.
#[derive(Debug)]
pub enum CliError {
    /// A required input path (a `.proto` file or directory) does not exist.
    NotFound(String),
    /// `.proto` compilation (parsing to a `FileDescriptorSet`) failed.
    Build(oxiproto_core::OxiProtoError),
    /// Native Rust codegen from a `FileDescriptorSet` failed.
    Codegen(oxiproto_codegen::CodegenError),
    /// `DescriptorPool` construction or dynamic-message lookup failed.
    Reflect(oxiproto_reflect::ReflectError),
    /// Canonical Protobuf-JSON conversion failed.
    Json(oxiproto_json::JsonError),
    /// `serde_json` (de)serialization of CLI input/output failed.
    SerdeJson(serde_json::Error),
    /// Binary protobuf wire-format decoding failed.
    Decode(prost::DecodeError),
    /// An underlying I/O operation (file, stdin, stdout) failed.
    Io(std::io::Error),
    /// A CLI-level validation or usage error, e.g. an unknown message type,
    /// an empty input set, or lint/breaking-change violations being found.
    Message(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::NotFound(path) => write!(f, "not found: {path}"),
            CliError::Build(e) => write!(f, "{e}"),
            CliError::Codegen(e) => write!(f, "{e}"),
            CliError::Reflect(e) => write!(f, "{e}"),
            CliError::Json(e) => write!(f, "{e}"),
            CliError::SerdeJson(e) => write!(f, "JSON error: {e}"),
            CliError::Decode(e) => write!(f, "decode error: {e}"),
            CliError::Io(e) => write!(f, "I/O error: {e}"),
            CliError::Message(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CliError::Build(e) => Some(e),
            CliError::Codegen(e) => Some(e),
            CliError::Reflect(e) => Some(e),
            CliError::Json(e) => Some(e),
            CliError::SerdeJson(e) => Some(e),
            CliError::Decode(e) => Some(e),
            CliError::Io(e) => Some(e),
            CliError::NotFound(_) | CliError::Message(_) => None,
        }
    }
}

impl From<oxiproto_core::OxiProtoError> for CliError {
    fn from(e: oxiproto_core::OxiProtoError) -> Self {
        CliError::Build(e)
    }
}

impl From<oxiproto_codegen::CodegenError> for CliError {
    fn from(e: oxiproto_codegen::CodegenError) -> Self {
        CliError::Codegen(e)
    }
}

impl From<oxiproto_reflect::ReflectError> for CliError {
    fn from(e: oxiproto_reflect::ReflectError) -> Self {
        CliError::Reflect(e)
    }
}

impl From<oxiproto_json::JsonError> for CliError {
    fn from(e: oxiproto_json::JsonError) -> Self {
        CliError::Json(e)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(e: serde_json::Error) -> Self {
        CliError::SerdeJson(e)
    }
}

impl From<prost::DecodeError> for CliError {
    fn from(e: prost::DecodeError) -> Self {
        CliError::Decode(e)
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::Io(e)
    }
}

impl From<String> for CliError {
    fn from(msg: String) -> Self {
        CliError::Message(msg)
    }
}

impl From<&str> for CliError {
    fn from(msg: &str) -> Self {
        CliError::Message(msg.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_not_found() {
        let e = CliError::NotFound("missing.proto".to_owned());
        assert_eq!(e.to_string(), "not found: missing.proto");
    }

    #[test]
    fn display_message() {
        let e: CliError = "no .proto files to process".into();
        assert_eq!(e.to_string(), "no .proto files to process");
    }

    #[test]
    fn from_io_error_has_source() {
        use std::error::Error as _;
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let e: CliError = io_err.into();
        assert!(matches!(e, CliError::Io(_)));
        assert!(e.source().is_some());
    }

    #[test]
    fn from_oxiproto_core_error() {
        let core_err = oxiproto_core::OxiProtoError::ParseError("bad syntax".to_owned());
        let e: CliError = core_err.into();
        assert!(matches!(e, CliError::Build(_)));
        assert!(e.to_string().contains("bad syntax"));
    }
}
