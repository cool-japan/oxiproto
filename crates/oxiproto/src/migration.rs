// SPDX-License-Identifier: Apache-2.0
// Copyright COOLJAPAN OU (Team Kitasan)

//! # Migration Guide: prost → OxiProto
//!
//! This module documents the conceptual mapping from the `prost` ecosystem to
//! the OxiProto stack.  It is **documentation only** — no runnable code is
//! shipped here.  All items are marked `#[doc(hidden)]` so they do not clutter
//! the public API surface.
//!
//! ## Why migrate?
//!
//! | Concern | prost | OxiProto |
//! |---------|-------|----------|
//! | C dependency | `protoc` optional (protox path) | Zero: 100% Pure Rust |
//! | Custom codec | `prost::Message` trait (fixed) | `OxiMessage` (pluggable) |
//! | Reflection | `prost-reflect` crate | `oxiproto-reflect` |
//! | JSON | `prost-types` + custom | `oxiproto-json` (canonical Protobuf-JSON) |
//! | WKT interop | manual | `oxiproto-wkt` (chrono/time built-in) |
//! | no_std | via `default-features=false` | `oxiproto-core` alloc feature |
//!
//! ---
//!
//! ## 1. Cargo.toml changes
//!
//! ### Before (prost)
//!
//! ```toml
//! [dependencies]
//! prost = "0.14"
//! prost-types = "0.14"
//!
//! [build-dependencies]
//! prost-build = "0.14"
//! ```
//!
//! ### After (OxiProto)
//!
//! ```toml
//! [dependencies]
//! # Core types: OxiMessage, wire format, OxiProtoError
//! oxiproto = "0.1"
//!
//! # Optional features:
//! # oxiproto = { version = "0.1", features = ["reflect", "wkt", "json"] }
//!
//! [build-dependencies]
//! # Build-time .proto compilation (no protoc required)
//! oxiproto = { version = "0.1", features = ["build"] }
//! ```
//!
//! ---
//!
//! ## 2. build.rs changes
//!
//! ### Before (prost-build)
//!
//! ```rust,ignore
//! fn main() {
//!     prost_build::compile_protos(
//!         &["src/model.proto"],
//!         &["src/"],
//!     ).expect("proto compile failed");
//! }
//! ```
//!
//! ### After (oxiproto-build)
//!
//! ```rust,ignore
//! fn main() {
//!     // `compile_protos` from oxiproto-build has the same signature.
//!     oxiproto::build::compile_protos(
//!         &["src/model.proto"],
//!         &["src/"],
//!     ).expect("proto compile failed");
//!
//!     // For OxiMessage impls (native trait, not prost::Message):
//!     let fds = oxiproto::build::compile_to_fds(
//!         &["src/model.proto"],
//!         &["src/"],
//!     ).expect("proto compile failed");
//!
//!     let mut opts = oxiproto::codegen::CodegenOptions::new();
//!     opts.emit_oxi_message_impl = true;  // emit OxiMessage instead of prost::Message
//!     let code = oxiproto::codegen::generate_with_options(&fds, &opts)
//!         .expect("codegen failed");
//!     let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
//!     std::fs::write(out.join("model.rs"), code).unwrap();
//! }
//! ```
//!
//! ---
//!
//! ## 3. Trait equivalence
//!
//! | prost trait / fn | OxiProto equivalent | Notes |
//! |------------------|---------------------|-------|
//! | `prost::Message` | `oxiproto::OxiMessage` | Different encode/decode API |
//! | `msg.encode_to_vec()` | `msg.encode_to_vec()` | Same method name |
//! | `T::decode(&bytes)` | `T::decode(&bytes)` | Same signature |
//! | `msg.encode_length_delimited_to_vec()` | Encode inner then use `EncodeBuffer::write_length_delimited` | Manual in OxiProto |
//! | `prost::Message::merge` | `OxiMessage::merge` | Takes `&mut DecodeBuffer` |
//! | `prost::Name` | `oxiproto::OxiName` | Provides `NAME`, `PACKAGE`, `full_name()`, `type_url()` |
//!
//! ---
//!
//! ## 4. Derive macro equivalence
//!
//! ### Before (prost derive)
//!
//! ```rust,ignore
//! #[derive(prost::Message)]
//! struct User {
//!     #[prost(int32, tag = "1")]
//!     id: i32,
//!     #[prost(string, tag = "2")]
//!     name: String,
//! }
//! ```
//!
//! ### After (OxiProto codegen or manual impl)
//!
//! OxiProto does not currently offer a proc-macro derive.  Two paths:
//!
//! **Path A — codegen from `.proto` (recommended):**
//!
//! Write or keep your `.proto` file. Run `build.rs` using `oxiproto-build` with
//! `emit_oxi_message_impl = true` to generate `impl OxiMessage for User`.
//!
//! **Path B — hand-written impl:**
//!
//! ```rust,ignore
//! use oxiproto::{OxiMessage, OxiProtoResult, wire::{EncodeBuffer, DecodeBuffer, WireType}};
//!
//! #[derive(Debug, Default, PartialEq, Clone)]
//! struct User {
//!     id: i32,
//!     name: String,
//! }
//!
//! impl OxiMessage for User {
//!     fn encoded_len(&self) -> usize {
//!         use oxiproto::wire::varint::encoded_len_varint;
//!         let mut n = 0;
//!         if self.id != 0 { n += encoded_len_varint(8u64) + encoded_len_varint(self.id as i64 as u64); }
//!         if !self.name.is_empty() { n += encoded_len_varint(18u64) + encoded_len_varint(self.name.len() as u64) + self.name.len(); }
//!         n
//!     }
//!
//!     fn encode_raw(&self, buf: &mut EncodeBuffer) {
//!         if self.id != 0 { buf.write_tag(1, WireType::Varint).ok(); buf.write_varint_i32(self.id); }
//!         if !self.name.is_empty() { buf.write_tag(2, WireType::Len).ok(); buf.write_string(&self.name); }
//!     }
//!
//!     fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
//!         while !buf.is_empty() {
//!             let tag = buf.read_tag()?;
//!             match (tag.field_number, tag.wire_type) {
//!                 (1, WireType::Varint) => { self.id = buf.read_varint()? as i32; }
//!                 (2, WireType::Len) => { self.name = buf.read_string()?.to_owned(); }
//!                 (_, wt) => { buf.skip_field(wt)?; }
//!             }
//!         }
//!         Ok(())
//!     }
//!
//!     fn clear(&mut self) { *self = Self::default(); }
//! }
//! ```
//!
//! ---
//!
//! ## 5. Well-Known Types
//!
//! | prost-types | oxiproto-wkt |
//! |-------------|--------------|
//! | `prost_types::Timestamp` | `oxiproto::prost_types::Timestamp` + `TimestampExt` trait |
//! | `prost_types::Duration` | `oxiproto::prost_types::Duration` + `DurationExt` trait |
//! | manual RFC3339 parsing | `oxiproto::wkt::TimestampExt::to_rfc3339()` / `from_rfc3339()` |
//! | manual chrono interop | `TimestampExt::from_datetime()` / `to_datetime()` (requires `wkt-chrono` feature) |
//!
//! Enable with: `oxiproto = { version = "0.1", features = ["wkt"] }` or `"wkt-chrono"`.
//!
//! ---
//!
//! ## 6. Reflection
//!
//! | prost-reflect | oxiproto-reflect |
//! |---------------|-----------------|
//! | `DescriptorPool` | `NativeDescriptorPool` |
//! | `DynamicMessage` | `NativeDynamicMessage` |
//! | `MessageDescriptor` | `oxiproto_reflect::MessageDescriptor` |
//!
//! Enable with: `oxiproto = { version = "0.1", features = ["reflect"] }`.
//!
//! ---
//!
//! ## 7. Error handling
//!
//! | prost | OxiProto |
//! |-------|----------|
//! | `prost::DecodeError` | `OxiProtoError::WireFormatError(WireError)` |
//! | `prost::EncodeError` | `OxiProtoError::WireFormatError(WireError)` |
//! | `?` operator works | `?` operator works (`From<WireError>` is implemented) |
//!
//! ---
//!
//! ## 8. Interoperability: using both prost and OxiProto
//!
//! During a gradual migration you may need both traits at the same time.
//! Because prost's `Message` is still re-exported from `oxiproto_core`, existing
//! prost-derived types continue to compile without change.  You can incrementally
//! migrate message types to `OxiMessage` at your own pace.
//!
//! ```rust,ignore
//! // Still works — prost derive is re-exported
//! use oxiproto::Message as ProstMessage; // = prost::Message
//!
//! // New types use OxiMessage
//! use oxiproto::OxiMessage;
//! ```
//!
//! ---
//!
//! ## 9. JSON encoding
//!
//! | prost-types + serde_json | oxiproto-json |
//! |--------------------------|--------------|
//! | manual per-type serde impl | `oxiproto::json::to_json(&msg)` |
//! | no camelCase or base64 built-in | canonical Protobuf-JSON (RFC 8259, protobuf spec) |
//!
//! Enable with: `oxiproto = { version = "0.1", features = ["json"] }`.
//!
//! ---
//!
//! ## 10. no_std usage
//!
//! ```toml
//! # Disable std, enable alloc (for Vec/String)
//! oxiproto-core = { version = "0.1", default-features = false, features = ["alloc"] }
//! ```
//!
//! The `OxiMessage` trait and the entire `wire` module are available under
//! `alloc` without `std`.  Only `OxiProtoError::IoError` is gated on `std`.

// This module is documentation-only. No public items.
