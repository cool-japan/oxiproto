# oxiproto-reflect — Runtime protobuf reflection for OxiProto

[![Crates.io](https://img.shields.io/crates/v/oxiproto-reflect.svg)](https://crates.io/crates/oxiproto-reflect)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiproto-reflect` provides runtime protobuf reflection: building a descriptor pool from a `FileDescriptorSet` and constructing, inspecting, and mutating `DynamicMessage` instances at runtime — all without any generated Rust types. It is the reflection layer of the OxiProto stack and is re-exported from the `oxiproto` facade behind the `reflect` feature.

The crate ships **two parallel reflection surfaces**. The default surface is a thin, ergonomic facade over [`prost-reflect`](https://crates.io/crates/prost-reflect) (re-exported under the canonical type names). Alongside it lives a self-contained, Pure-Rust **`native`** reflection stack built directly on `oxiproto_core::wire`, exposed under the `native` module and re-exported at the crate root with a `Native`-prefix so the two coexist without name collisions. The whole crate is `#![forbid(unsafe_code)]`.

## Installation

```toml
[dependencies]
oxiproto-reflect = "0.1.3"
```

Or, via the facade:

```toml
[dependencies]
oxiproto = { version = "0.1.3", features = ["reflect"] }
```

## Quick Start

```rust,no_run
use oxiproto_reflect::{pool_from_fds_bytes, dynamic_message};
use prost_reflect::ReflectMessage;

// `fds_bytes` is the raw bytes of a `FileDescriptorSet` proto,
// e.g. produced at build time by `prost_build::Config::file_descriptor_set_path`.
# fn example(fds_bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
let pool = pool_from_fds_bytes(fds_bytes)?;
let msg  = dynamic_message(&pool, "my.package.MyMessage")?;
println!("fields: {:?}", msg.descriptor().fields().collect::<Vec<_>>());
# Ok(())
# }
```

### Native reflection stack

```rust,ignore
use oxiproto_reflect::native::{DescriptorPool, DynamicMessage, Value};

let pool = DescriptorPool::from_file_descriptor_set(fds)?;
let m     = pool.get_message_by_name("M").expect("message M");
let field = m.get_field(1).expect("field 1");

let mut msg = DynamicMessage::new(m);
msg.set_field(&field, Value::I32(150));

// Canonical protobuf encoding of `{ a: 150 }` is `08 96 01`.
assert_eq!(msg.encode_to_vec()?, vec![0x08, 0x96, 0x01]);
```

## API Overview

### Pool & message constructors (crate root)

| Function | Returns | Description |
|----------|---------|-------------|
| `pool_from_fds_bytes(fds_bytes: &[u8])` | `Result<DescriptorPool, ReflectError>` | Decode raw `FileDescriptorSet` bytes and build a pool |
| `pool_from_fds(fds: FileDescriptorSet)` | `Result<DescriptorPool, ReflectError>` | Build a pool from an already-decoded `FileDescriptorSet` (no bytes round-trip) |
| `dynamic_message(pool, full_name)` | `Result<DynamicMessage, ReflectError>` | Construct an empty `DynamicMessage` for a fully-qualified message name |
| `get_service_by_name(pool, full_name)` | `Option<ServiceDescriptor>` | Look up a service descriptor |
| `get_enum_by_name(pool, full_name)` | `Option<EnumDescriptor>` | Look up an enum descriptor |
| `all_messages(pool)` | `impl Iterator<Item = MessageDescriptor>` | Iterate every message (including nested) in the pool |
| `all_services(pool)` | `impl Iterator<Item = ServiceDescriptor>` | Iterate every service in the pool |

### Dynamic field access (the `dynamic` module)

Free-function helpers that look up a field descriptor by name and delegate to the corresponding `DynamicMessage` method. Re-exported at the crate root.

| Function | Returns | Description |
|----------|---------|-------------|
| `set_field_by_name(msg, name, value)` | `Result<(), ReflectError>` | Set a field by name (type-checked against the descriptor) |
| `get_field_by_name(msg, name)` | `Result<Option<Value>, ReflectError>` | Get a field by name (`Ok(None)` if unset / at default) |
| `has_field(msg, name)` | `Result<bool, ReflectError>` | Whether a field is set (non-default) |
| `clear_field(msg, name)` | `Result<(), ReflectError>` | Reset a field to its default |
| `unknown_fields(msg)` | `impl Iterator<Item = &UnknownField>` | Iterate fields preserved from a newer schema during decode |

### `prost-reflect` re-exports (crate root)

The canonical reflection types, re-exported so callers do not need a direct `prost-reflect` dependency.

| Item | Kind | Notes |
|------|------|-------|
| `DescriptorPool` | struct | The descriptor registry |
| `DynamicMessage` | struct | Runtime message; implements `Debug` and `Display` (protobuf text format) |
| `FileDescriptor` | struct | A `.proto` file descriptor |
| `MessageDescriptor` | struct | A message type descriptor |
| `FieldDescriptor` | struct | A single field descriptor |
| `EnumDescriptor` | struct | An enum type descriptor |
| `ServiceDescriptor` | struct | A service descriptor |
| `MethodDescriptor` | struct | An RPC method descriptor |
| `UnknownField` | struct | A preserved unknown field |
| `ReflectValue` | enum | Alias for `prost_reflect::Value` (avoids clashing with `prost_types::Value`) |
| `ReflectMessage` | trait | Re-exported so `msg.descriptor()` works without importing `prost_reflect` |

### Native reflection stack (the `native` module)

A self-contained, Pure-Rust reflection implementation built on `oxiproto_core::wire`, independent of `prost-reflect`. Each type is also re-exported at the crate root under a `Native`-prefixed alias (shown in the second column) so it coexists with the `prost-reflect`-backed surface above.

| `native::` type | Crate-root alias | Description |
|-----------------|------------------|-------------|
| `DescriptorPool` | `NativeDescriptorPool` | Pool built via `DescriptorPool::from_file_descriptor_set`; name lookups + iterators (`all_messages`, `all_enums`, `services`) |
| `DynamicMessage` | `NativeDynamicMessage` | Runtime message with field get/set, oneof exclusivity, and wire encode/decode (`encode_to_vec`) |
| `FileDescriptor` | `NativeFileDescriptor` | A `.proto` file descriptor |
| `MessageDescriptor` | `NativeMessageDescriptor` | A message type descriptor |
| `FieldDescriptor` | `NativeFieldDescriptor` | A field descriptor |
| `EnumDescriptor` | `NativeEnumDescriptor` | An enum type descriptor |
| `EnumValueDescriptor` | `NativeEnumValueDescriptor` | A single enum value descriptor |
| `OneofDescriptor` | `NativeOneofDescriptor` | A oneof group descriptor |
| `ServiceDescriptor` | `NativeServiceDescriptor` | A service descriptor |
| `MethodDescriptor` | `NativeMethodDescriptor` | An RPC method descriptor |
| `Kind` | `NativeKind` | Field kind enum (scalar/message/enum/etc.) |
| `Cardinality` | `NativeCardinality` | Field cardinality enum (optional/repeated/required) |
| `Value` | `NativeValue` | Runtime field value model; accessors `as_i32`/`as_i64`/`as_u32`/`as_u64`/`as_f32`/`as_f64`/`as_bool`/`as_str`/`as_bytes`/`as_enum_number`/`as_message`/`as_list`/`as_map`, plus `is_default` |
| `MapKey` | `NativeMapKey` | Map-key value model for `map<K, V>` fields |

Native `DynamicMessage` field methods include `new`, `descriptor`, `has_field`, `get_field`, `set_field`, `clear_field`, `get_field_by_name`, `set_field_by_name`, `which_oneof`, `iter_fields`, and `unknown_fields` / `unknown_fields_mut`.

### `ReflectError` variants

The crate's error type. Implements `std::error::Error` and `Display`, and converts to/from `oxiproto_core::OxiProtoError`.

| Variant | Description |
|---------|-------------|
| `Decode(prost::DecodeError)` | Raw bytes could not be decoded as a `FileDescriptorSet` |
| `Pool(String)` | The descriptor pool could not be constructed (missing imports, invalid descriptors) |
| `NotFound(String)` | A named symbol (message, service, enum, field) was not in the pool |
| `Field(String)` | Field name or type error during dynamic message access |

## Cross-references

| Crate | Role |
|-------|------|
| [`oxiproto-core`](../oxiproto-core) | Native wire format (`oxiproto_core::wire`) the `native` stack is built on; shared `OxiProtoError` |
| [`oxiproto-json`](../oxiproto-json) | Maps the `DynamicMessage` values built here to/from canonical Protobuf-JSON |
| [`oxiproto-wkt`](../oxiproto-wkt) | Well-Known Type extension traits (Timestamp, Duration, Any, etc.) |
| [`oxiproto`](../oxiproto) | Facade crate; re-exports this crate as `oxiproto::reflect` behind the `reflect` feature |

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
