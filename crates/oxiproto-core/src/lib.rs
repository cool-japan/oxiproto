//! `oxiproto-core` — Pure Rust protobuf core types.
//!
//! Provides re-exports of the fundamental [`prost`] traits and error types
//! used across the OxiProto stack, plus native wire format primitives in the
//! [`wire`] module.
//!
//! ## Wire Format
//!
//! The [`wire`] module provides a complete, standalone implementation of the
//! protobuf binary wire format:
//!
//! - Varint (LEB128) encoding/decoding
//! - ZigZag encoding for signed integers
//! - Field tag encoding/decoding
//! - Fixed 32/64-bit little-endian encoding
//! - Length-delimited fields (strings, bytes, embedded messages)
//! - [`DecodeBuffer`](wire::DecodeBuffer) / [`EncodeBuffer`](wire::EncodeBuffer)
//!   for streaming encode/decode
//! - [`UnknownFields`](wire::UnknownFields) for preserving unrecognized fields

#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod wire;

pub mod arena;
pub mod extensions;
pub mod message;
pub mod name;
pub mod oneof;
pub mod reflect_bridge;

pub use prost::Message;
pub use prost::Name;
pub use prost_types;

pub use extensions::Extensions;
pub use message::OxiMessage;
pub use name::OxiName;
pub use oneof::OxiOneof;

/// Convenience type alias for `Result<T, OxiProtoError>`.
pub type OxiProtoResult<T> = Result<T, OxiProtoError>;

/// Errors produced by the OxiProto stack.
#[derive(Debug)]
#[non_exhaustive]
pub enum OxiProtoError {
    /// A `.proto` source file could not be parsed or resolved.
    ParseError(prost::alloc::string::String),
    /// Rust code could not be generated from the parsed descriptors.
    CodegenError(prost::alloc::string::String),
    /// An underlying I/O operation failed.
    #[cfg(feature = "std")]
    IoError(std::io::Error),
    /// A wire format encoding or decoding error.
    WireFormatError(wire::WireError),
}

impl core::fmt::Display for OxiProtoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            OxiProtoError::ParseError(s) => write!(f, "proto parse error: {s}"),
            OxiProtoError::CodegenError(s) => write!(f, "proto codegen error: {s}"),
            #[cfg(feature = "std")]
            OxiProtoError::IoError(e) => write!(f, "I/O error: {e}"),
            OxiProtoError::WireFormatError(e) => write!(f, "wire format error: {e}"),
        }
    }
}

impl core::error::Error for OxiProtoError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            #[cfg(feature = "std")]
            OxiProtoError::IoError(e) => Some(e),
            OxiProtoError::WireFormatError(e) => Some(e),
            OxiProtoError::ParseError(_) | OxiProtoError::CodegenError(_) => None,
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for OxiProtoError {
    fn from(e: std::io::Error) -> Self {
        OxiProtoError::IoError(e)
    }
}

impl From<wire::WireError> for OxiProtoError {
    fn from(e: wire::WireError) -> Self {
        OxiProtoError::WireFormatError(e)
    }
}
