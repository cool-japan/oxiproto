#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Runtime protobuf reflection via prost-reflect.
//!
//! This crate provides a thin facade over [`prost_reflect`] for dynamic
//! protobuf operations: building a [`DescriptorPool`] from a
//! [`prost_types::FileDescriptorSet`] and constructing [`DynamicMessage`]
//! instances at runtime without generated Rust types.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use oxiproto_reflect::{pool_from_fds_bytes, dynamic_message};
//! # use prost_reflect::ReflectMessage;
//!
//! // `fds_bytes` is the raw bytes of a `FileDescriptorSet` proto.
//! # fn example(fds_bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
//! let pool = pool_from_fds_bytes(fds_bytes)?;
//! let msg  = dynamic_message(&pool, "my.package.MyMessage")?;
//! println!("fields: {:?}", msg.descriptor().fields().collect::<Vec<_>>());
//! # Ok(())
//! # }
//! ```
//!
//! ## Debug and Display for DynamicMessage
//!
//! [`DynamicMessage`] implements both [`std::fmt::Debug`] and
//! [`std::fmt::Display`] (protobuf text format). The following example
//! verifies both traits work correctly through this crate's re-exports.
//!
//! ```rust
//! use oxiproto_reflect::{pool_from_fds, DynamicMessage};
//! use prost_types::{
//!     FileDescriptorSet, FileDescriptorProto, DescriptorProto, FieldDescriptorProto,
//! };
//! use prost_types::field_descriptor_proto::{Label, Type};
//!
//! let fds = FileDescriptorSet {
//!     file: vec![FileDescriptorProto {
//!         name: Some("test.proto".to_string()),
//!         syntax: Some("proto3".to_string()),
//!         message_type: vec![DescriptorProto {
//!             name: Some("Ping".to_string()),
//!             field: vec![FieldDescriptorProto {
//!                 name: Some("value".to_string()),
//!                 number: Some(1),
//!                 label: Some(Label::Optional as i32),
//!                 r#type: Some(Type::Int32 as i32),
//!                 json_name: Some("value".to_string()),
//!                 ..Default::default()
//!             }],
//!             ..Default::default()
//!         }],
//!         ..Default::default()
//!     }],
//! };
//!
//! let pool = pool_from_fds(fds).unwrap();
//! let msg_desc = pool.get_message_by_name("Ping").unwrap();
//! let msg = DynamicMessage::new(msg_desc);
//!
//! // Debug format is always available.
//! let debug_str = format!("{msg:?}");
//! assert!(!debug_str.is_empty());
//!
//! // Display uses the protobuf text format; an empty message formats to "".
//! let display_str = format!("{msg}");
//! assert_eq!(display_str, "");
//! ```

pub use prost_reflect::{
    DescriptorPool, DynamicMessage, EnumDescriptor, FieldDescriptor, FileDescriptor,
    MessageDescriptor, MethodDescriptor, ServiceDescriptor, UnknownField,
};

/// Re-export of [`prost_reflect::Value`] under a distinct alias to avoid
/// name conflicts with [`prost_types::Value`].
pub use prost_reflect::Value as ReflectValue;

/// Re-export of the [`prost_reflect::ReflectMessage`] trait so callers can
/// use `msg.descriptor()` without a separate `prost_reflect` dependency.
pub use prost_reflect::ReflectMessage;

pub mod dynamic;

pub use dynamic::{clear_field, get_field_by_name, has_field, set_field_by_name, unknown_fields};

pub mod native;

// Re-export the native reflection types under a `Native`-prefixed alias so they
// coexist with the `prost-reflect`-backed types re-exported above (which keep
// their canonical names for backwards compatibility). The full, unprefixed
// names are also available via the `native` module path, e.g.
// `oxiproto_reflect::native::DescriptorPool`.
pub use native::{
    Cardinality as NativeCardinality, DescriptorPool as NativeDescriptorPool,
    DynamicMessage as NativeDynamicMessage, EnumDescriptor as NativeEnumDescriptor,
    EnumValueDescriptor as NativeEnumValueDescriptor, FieldDescriptor as NativeFieldDescriptor,
    FileDescriptor as NativeFileDescriptor, Kind as NativeKind, MapKey as NativeMapKey,
    MessageDescriptor as NativeMessageDescriptor, MethodDescriptor as NativeMethodDescriptor,
    NativeJsonError, NativeTextError, OneofDescriptor as NativeOneofDescriptor,
    ServiceDescriptor as NativeServiceDescriptor, Value as NativeValue,
};

use prost::Message;
use prost_types::FileDescriptorSet;

/// Errors produced by reflection operations.
#[derive(Debug)]
pub enum ReflectError {
    /// Failed to decode the raw bytes as a `FileDescriptorSet`.
    Decode(prost::DecodeError),
    /// The descriptor pool could not be constructed from the provided descriptors.
    Pool(String),
    /// A named symbol (message, service, enum, field) was not found in the pool.
    NotFound(String),
    /// Field name or type error during dynamic message access.
    Field(String),
}

impl std::fmt::Display for ReflectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReflectError::Decode(e) => write!(f, "failed to decode FileDescriptorSet: {e}"),
            ReflectError::Pool(e) => write!(f, "failed to build DescriptorPool: {e}"),
            ReflectError::NotFound(name) => write!(f, "'{name}' not found in pool"),
            ReflectError::Field(msg) => write!(f, "field error: {msg}"),
        }
    }
}

impl std::error::Error for ReflectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReflectError::Decode(e) => Some(e),
            ReflectError::Pool(_) | ReflectError::NotFound(_) | ReflectError::Field(_) => None,
        }
    }
}

impl From<oxiproto_core::OxiProtoError> for ReflectError {
    fn from(e: oxiproto_core::OxiProtoError) -> Self {
        ReflectError::Pool(e.to_string())
    }
}

impl From<ReflectError> for oxiproto_core::OxiProtoError {
    fn from(e: ReflectError) -> Self {
        oxiproto_core::OxiProtoError::ParseError(e.to_string())
    }
}

/// Build a [`DescriptorPool`] from the raw bytes of a serialized
/// [`prost_types::FileDescriptorSet`].
///
/// The bytes are typically produced at build time by
/// `prost_build::Config::file_descriptor_set_path`, or constructed
/// programmatically in tests.
///
/// # Errors
///
/// Returns [`ReflectError::Decode`] if `fds_bytes` cannot be decoded as a
/// `FileDescriptorSet`, or [`ReflectError::Pool`] if the pool construction
/// fails (e.g. missing imports or invalid descriptors).
pub fn pool_from_fds_bytes(fds_bytes: &[u8]) -> Result<DescriptorPool, ReflectError> {
    let fds = FileDescriptorSet::decode(fds_bytes).map_err(ReflectError::Decode)?;
    DescriptorPool::from_file_descriptor_set(fds).map_err(|e| ReflectError::Pool(e.to_string()))
}

/// Build a [`DescriptorPool`] directly from a [`FileDescriptorSet`].
///
/// Unlike [`pool_from_fds_bytes`], this function accepts the already-decoded
/// struct and avoids the bytes round-trip.
///
/// # Errors
///
/// Returns [`ReflectError::Pool`] if the pool construction fails (e.g. missing
/// imports or invalid descriptors).
pub fn pool_from_fds(fds: FileDescriptorSet) -> Result<DescriptorPool, ReflectError> {
    DescriptorPool::from_file_descriptor_set(fds).map_err(|e| ReflectError::Pool(e.to_string()))
}

/// Construct an empty [`DynamicMessage`] for the named message in `pool`.
///
/// `full_name` must be the fully-qualified message name, e.g.
/// `"my.package.MyMessage"`.
///
/// # Errors
///
/// Returns [`ReflectError::NotFound`] if `full_name` does not exist in `pool`.
pub fn dynamic_message(
    pool: &DescriptorPool,
    full_name: &str,
) -> Result<DynamicMessage, ReflectError> {
    let msg_desc = pool
        .get_message_by_name(full_name)
        .ok_or_else(|| ReflectError::NotFound(full_name.to_owned()))?;
    Ok(DynamicMessage::new(msg_desc))
}

/// Look up a service descriptor by its fully-qualified name.
///
/// Returns `None` if no service with that name exists in `pool`.
pub fn get_service_by_name(pool: &DescriptorPool, full_name: &str) -> Option<ServiceDescriptor> {
    pool.get_service_by_name(full_name)
}

/// Look up an enum descriptor by its fully-qualified name.
///
/// Returns `None` if no enum with that name exists in `pool`.
pub fn get_enum_by_name(pool: &DescriptorPool, full_name: &str) -> Option<EnumDescriptor> {
    pool.get_enum_by_name(full_name)
}

/// Iterate over all message descriptors registered in the pool.
///
/// Includes nested messages defined inside other messages.
pub fn all_messages(pool: &DescriptorPool) -> impl Iterator<Item = MessageDescriptor> + '_ {
    pool.all_messages()
}

/// Iterate over all service descriptors registered in the pool.
///
/// Forwards to `DescriptorPool::services()` which is the equivalent iterator
/// on prost-reflect 0.16.x (there is no `all_services` method; all services
/// are top-level by definition in protobuf).
pub fn all_services(pool: &DescriptorPool) -> impl Iterator<Item = ServiceDescriptor> + '_ {
    pool.services()
}
