#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Canonical Protobuf-JSON mapping for OxiProto.
//!
//! This crate implements the [canonical Protobuf-JSON spec] for
//! [`prost_reflect::DynamicMessage`]:
//!
//! - Field names serialized as **camelCase** by default
//!   (`preserve_proto_field_names(true)` keeps the original names).
//! - `int64`/`uint64` encoded as **JSON strings** to preserve precision.
//! - `bytes` fields encoded as **base64** (RFC 4648 §4, standard alphabet
//!   with padding).
//! - `google.protobuf.Timestamp` ↔ RFC 3339 string, e.g.
//!   `"2023-11-14T22:13:20Z"`.
//! - `google.protobuf.Duration` ↔ decimal-seconds string, e.g. `"1.5s"`.
//! - `google.protobuf.FieldMask` ↔ comma-separated camelCase path string,
//!   e.g. `"fooBar,bazQux"`.
//! - `google.protobuf.Value` ↔ the natural JSON scalar/object/array.
//! - `google.protobuf.ListValue` ↔ JSON array.
//! - `google.protobuf.Struct` ↔ JSON object.
//! - `google.protobuf.Any` ↔ `{"@type": "<type_url>", ...fields}` (WKT
//!   primitives are wrapped as `{"@type": ..., "value": ...}`).  Type-URL
//!   resolution uses the descriptor pool already registered on the message.
//! - Infinite / NaN `f32`/`f64` values encoded as `"Infinity"`,
//!   `"-Infinity"`, and `"NaN"` strings; the same strings are accepted on
//!   decode.
//! - Enum values serialized as **name strings**; configurable to emit numbers.
//! - `repeated` → JSON array; `map<K,V>` → JSON object.
//! - Default scalar values **omitted** unless `always_print_fields(true)`.
//!
//! [canonical Protobuf-JSON spec]: https://protobuf.dev/programming-guides/proto3/#json
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use oxiproto_json::{to_json, from_json, JsonCodec};
//! use prost_reflect::{DynamicMessage, MessageDescriptor};
//!
//! # fn example(msg: &DynamicMessage, desc: &MessageDescriptor) {
//! let codec = JsonCodec::default();
//! let json_value = to_json(msg, &codec);
//!
//! let rebuilt = from_json(&json_value, desc, &codec).expect("round-trip");
//! # }
//! ```

mod codec;
mod from_json;
mod to_json;

pub use codec::JsonCodec;
pub use from_json::{from_json, JsonError};
pub use to_json::to_json;
