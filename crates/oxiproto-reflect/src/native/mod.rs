//! Native, pure-Rust protobuf reflection.
//!
//! This module provides a self-contained reflection stack built directly on
//! [`oxiproto_core::wire`], independent of `prost-reflect`:
//!
//! - [`DescriptorPool`] — built from a [`prost_types::FileDescriptorSet`],
//!   with fully-qualified-name lookups and iterators.
//! - The descriptor type set: [`FileDescriptor`], [`MessageDescriptor`],
//!   [`FieldDescriptor`], [`EnumDescriptor`], [`EnumValueDescriptor`],
//!   [`OneofDescriptor`], [`ServiceDescriptor`], [`MethodDescriptor`], plus the
//!   [`Kind`] and [`Cardinality`] enums.
//! - [`DynamicMessage`] — runtime message with field get/set, oneof
//!   exclusivity, and protobuf wire encode/decode.
//! - [`Value`] / [`MapKey`] — the runtime field value model.
//!
//! These types are exposed under the `native` namespace (and re-exported from
//! the crate root with a `Native`-prefix) so they coexist with the existing
//! `prost-reflect`-backed surface without name collisions.
//!
//! # Example
//!
//! ```
//! use oxiproto_reflect::native::{DescriptorPool, DynamicMessage, Value};
//! use prost_types::{
//!     DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
//! };
//! use prost_types::field_descriptor_proto::{Label, Type};
//!
//! // Build a descriptor set for `message M { int32 a = 1; }`.
//! let fds = FileDescriptorSet {
//!     file: vec![FileDescriptorProto {
//!         name: Some("m.proto".to_owned()),
//!         syntax: Some("proto3".to_owned()),
//!         message_type: vec![DescriptorProto {
//!             name: Some("M".to_owned()),
//!             field: vec![FieldDescriptorProto {
//!                 name: Some("a".to_owned()),
//!                 number: Some(1),
//!                 label: Some(Label::Optional as i32),
//!                 r#type: Some(Type::Int32 as i32),
//!                 ..Default::default()
//!             }],
//!             ..Default::default()
//!         }],
//!         ..Default::default()
//!     }],
//! };
//!
//! let pool = DescriptorPool::from_file_descriptor_set(fds).unwrap();
//! let m = pool.get_message_by_name("M").unwrap();
//! let field = m.get_field(1).unwrap();
//!
//! let mut msg = DynamicMessage::new(m);
//! msg.set_field(&field, Value::I32(150));
//!
//! // Canonical protobuf encoding of `{ a: 150 }` is `08 96 01`.
//! assert_eq!(msg.encode_to_vec().unwrap(), vec![0x08, 0x96, 0x01]);
//! ```

pub mod descriptor;
pub mod dynamic;
pub mod json;
pub mod pool;
pub mod text;
pub mod value;
pub mod wire_codec;

pub use descriptor::{
    Cardinality, EnumDescriptor, EnumValueDescriptor, FieldDescriptor, FileDescriptor, Kind,
    MessageDescriptor, MethodDescriptor, OneofDescriptor, ServiceDescriptor,
};
pub use dynamic::DynamicMessage;
pub use json::JsonError as NativeJsonError;
pub use pool::{DescriptorPool, PoolInner};
pub use text::TextError as NativeTextError;
pub use value::{MapKey, Value};
