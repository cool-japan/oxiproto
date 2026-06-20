# oxiproto-json — Canonical Protobuf-JSON mapping for OxiProto

[![Crates.io](https://img.shields.io/crates/v/oxiproto-json.svg)](https://crates.io/crates/oxiproto-json)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiproto-json` implements the [canonical Protobuf-JSON mapping](https://protobuf.dev/programming-guides/proto3/#json) for runtime-reflected messages. It converts between a `prost_reflect::DynamicMessage` and a `serde_json::Value` in both directions, honouring the JSON conventions mandated by the Protobuf spec. It is the JSON layer of the OxiProto stack and is re-exported from the `oxiproto` facade behind the `json` feature.

The mapping is driven entirely by a message's runtime descriptor, so no generated Rust types are required — point it at any `DynamicMessage` (built via `oxiproto-reflect`) and a matching `MessageDescriptor`. The crate is 100% Pure Rust with `#![forbid(unsafe_code)]`.

### Mapping rules

| Protobuf construct | JSON encoding |
|--------------------|---------------|
| Field names | **camelCase** by default (`preserve_proto_field_names(true)` keeps the original `snake_case`) |
| `int64` / `uint64` / `fixed64` / `sfixed64` | JSON **string** (preserves 64-bit precision) |
| `bytes` | **base64** (RFC 4648 §4, standard alphabet, with padding) |
| `google.protobuf.Timestamp` | RFC 3339 string, e.g. `"2023-11-14T22:13:20Z"` |
| `google.protobuf.Duration` | decimal-seconds string, e.g. `"1.5s"` |
| `enum` | **name string** (set `emit_enum_as_number(true)` to emit the integer) |
| `repeated` | JSON array |
| `map<K, V>` | JSON object |
| Default scalar values | **omitted** unless `always_print_fields(true)` |

## Installation

```toml
[dependencies]
oxiproto-json = "0.1.3"
```

Or, via the facade:

```toml
[dependencies]
oxiproto = { version = "0.1.3", features = ["json", "reflect"] }
```

## Quick Start

```rust,no_run
use oxiproto_json::{to_json, from_json, JsonCodec};
use prost_reflect::{DynamicMessage, MessageDescriptor};

# fn example(msg: &DynamicMessage, desc: &MessageDescriptor) -> Result<(), Box<dyn std::error::Error>> {
let codec = JsonCodec::default();

// DynamicMessage -> serde_json::Value (canonical camelCase JSON).
let json_value = to_json(msg, &codec);

// serde_json::Value -> DynamicMessage (round-trip).
let rebuilt = from_json(&json_value, desc, &codec)?;
# Ok(())
# }
```

### Customising the codec

```rust
use oxiproto_json::JsonCodec;

let codec = JsonCodec::new()
    .preserve_proto_field_names(true) // keep snake_case keys
    .always_print_fields(true)        // include proto3 defaults
    .emit_enum_as_number(true);       // enums as integers, not names
```

## API Overview

### Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `to_json` | `to_json(msg: &DynamicMessage, codec: &JsonCodec) -> serde_json::Value` | Serialize a dynamic message to canonical Protobuf-JSON |
| `from_json` | `from_json(value: &serde_json::Value, descriptor: &MessageDescriptor, codec: &JsonCodec) -> Result<DynamicMessage, JsonError>` | Deserialize JSON into a dynamic message validated against `descriptor` |

### `JsonCodec` — serialization configuration

Builder-style configuration following the canonical spec. Construct with `JsonCodec::default()` / `JsonCodec::new()`, then chain the setters (each consumes and returns `Self`).

| Method | Default | Description |
|--------|---------|-------------|
| `new()` / `default()` | — | camelCase keys, defaults omitted, enum names as strings |
| `preserve_proto_field_names(bool)` | `false` | When `true`, emit original `snake_case` proto field names instead of camelCase JSON names |
| `always_print_fields(bool)` | `false` | When `true`, include every field even when it holds its proto3 default value (`0`, `""`, `false`, empty list/map) |
| `emit_enum_as_number(bool)` | `false` | When `true`, serialize enum values as integers rather than their string names |

### `JsonError` variants

Returned by `from_json`. Implements `std::error::Error` and `Display`, and converts to/from `oxiproto_core::OxiProtoError`.

| Variant | Description |
|---------|-------------|
| `WrongType { field, expected, got }` | The JSON value for a field had an incompatible type |
| `UnknownField(String)` | A JSON object key did not match any field in the descriptor |
| `MalformedValue(String)` | A scalar could not be parsed or decoded (e.g. invalid base64) |

## Deferred items

The following are tracked for a future milestone and are **not yet fully spec-compliant**:

- `google.protobuf.Any` — type-URL resolution requires a live `DescriptorPool` with the target message registered; currently serialized as an empty object `{}`.
- Non-finite floats — the spec requires `"Infinity"`, `"-Infinity"`, and `"NaN"` strings; currently emitted as `null`.
- `google.protobuf.Struct`, `Value`, `ListValue` — currently treated as regular messages rather than receiving their special JSON forms.
- `google.protobuf.FieldMask` — currently treated as a regular message.

## Cross-references

| Crate | Role |
|-------|------|
| [`oxiproto-core`](../oxiproto-core) | Native wire format, traits, and the shared `OxiProtoError` |
| [`oxiproto-reflect`](../oxiproto-reflect) | Builds the `DescriptorPool` and `DynamicMessage` values this crate maps to/from JSON |
| [`oxiproto-wkt`](../oxiproto-wkt) | Well-Known Type helpers (Timestamp, Duration, etc.) |
| [`oxiproto`](../oxiproto) | Facade crate; re-exports this crate as `oxiproto::json` behind the `json` feature |

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
