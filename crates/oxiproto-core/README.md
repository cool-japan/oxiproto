# oxiproto-core — Pure-Rust Protocol Buffers runtime and wire-format primitives

[![Crates.io](https://img.shields.io/crates/v/oxiproto-core.svg)](https://crates.io/crates/oxiproto-core)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiproto-core` is the runtime layer of the **OxiProto** stack — COOLJAPAN's Pure-Rust Protocol Buffers implementation. It defines the encoding/decoding types that generated message code depends on at runtime: a standalone implementation of the protobuf binary **wire format** (varint, ZigZag, tags, fixed-width, length-delimited), the native message/name/oneof traits, proto2 extension storage, and unknown-field preservation.

It contains no code generation (that lives in `oxiproto-codegen`), no `build.rs` glue (`oxiproto-build`), and no CLI (`oxiproto-cli`). The crate is `#![forbid(unsafe_code)]` and supports `no_std` (with `alloc`) by disabling the `std` feature. For convenience it also re-exports the fundamental `prost` traits (`Message`, `Name`) and `prost_types` so downstream crates have a single dependency for both the native and prost-compatible code paths.

## Installation

```toml
[dependencies]
oxiproto-core = "0.1.2"
```

`no_std` (relying on `alloc`):

```toml
[dependencies]
oxiproto-core = { version = "0.1.2", default-features = false, features = ["alloc"] }
```

With Serde derives on the wire helper types:

```toml
[dependencies]
oxiproto-core = { version = "0.1.2", features = ["serde"] }
```

## Quick Start

Encode and decode protobuf wire-format fields directly with the streaming buffers:

```rust
use oxiproto_core::wire::{DecodeBuffer, EncodeBuffer, WireType};

// Encode: field 1 = varint 42, field 2 = string "hello"
let mut enc = EncodeBuffer::new();
enc.write_tag(1, WireType::Varint)?;
enc.write_varint(42);
enc.write_tag(2, WireType::Len)?;
enc.write_string("hello");
let bytes = enc.into_vec();

// Decode the same buffer back
let mut dec = DecodeBuffer::new(&bytes);
let tag = dec.read_tag()?;
assert_eq!(tag.field_number, 1);
assert_eq!(dec.read_varint()?, 42);

let tag = dec.read_tag()?;
assert_eq!(tag.field_number, 2);
assert_eq!(dec.read_string()?, "hello");
assert!(dec.is_empty());
# Ok::<(), oxiproto_core::wire::WireError>(())
```

### Implementing the native message trait

```rust
use oxiproto_core::{OxiMessage, OxiProtoResult};
use oxiproto_core::wire::{DecodeBuffer, EncodeBuffer, WireType};

#[derive(Debug, Default)]
struct Point { x: i32, y: i32 }

impl OxiMessage for Point {
    fn encoded_len(&self) -> usize { /* tag + varint sizes */ 4 }

    fn encode_raw(&self, buf: &mut EncodeBuffer) {
        // proto3: omit default (zero) values
        if self.x != 0 { let _ = buf.write_tag(1, WireType::Varint); buf.write_varint_i32(self.x); }
        if self.y != 0 { let _ = buf.write_tag(2, WireType::Varint); buf.write_varint_i32(self.y); }
    }

    fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = buf.read_tag()?;
            match tag.field_number {
                1 => self.x = buf.read_varint_i32()?,
                2 => self.y = buf.read_varint_i32()?,
                _ => buf.skip_field(tag.wire_type)?,
            }
        }
        Ok(())
    }

    fn clear(&mut self) { *self = Self::default(); }
}

// `encode_to_vec` / `decode` come for free as provided methods.
let bytes = Point { x: 3, y: 4 }.encode_to_vec();
let p = Point::decode(&bytes)?;
assert_eq!((p.x, p.y), (3, 4));
# Ok::<(), oxiproto_core::OxiProtoError>(())
```

## API Overview

### Crate root re-exports

| Item | Source | Description |
|------|--------|-------------|
| `Message` | `prost::Message` | The prost message trait (prost-compatible path) |
| `Name` | `prost::Name` | The prost name trait |
| `prost_types` | `prost_types` | Well-known descriptor / WKT types |
| `OxiMessage` | `message` | Native message trait built on the `wire` buffers |
| `OxiName` | `name` | Native fully-qualified-name / type-URL trait |
| `OxiOneof` | `oneof` | Native oneof-group dispatch trait |
| `Extensions` | `extensions` | Proto2 extension field storage |
| `OxiProtoError` | crate root | Stack-wide error enum (see below) |
| `OxiProtoResult<T>` | crate root | Alias for `Result<T, OxiProtoError>` |

### `OxiMessage` trait

The native message trait. Every generated message type implements it; it works directly with `wire::EncodeBuffer` / `wire::DecodeBuffer` rather than the `bytes` crate.

| Method | Kind | Description |
|--------|------|-------------|
| `encoded_len(&self)` | required | Total encoded size in bytes |
| `encode_raw(&self, buf)` | required | Append fields to an `EncodeBuffer` (proto3 defaults omitted) |
| `merge(&mut self, buf)` | required | Merge wire bytes from a `DecodeBuffer` (repeated → append, singular → last-wins, unknown → skip) |
| `clear(&mut self)` | required | Reset all fields to defaults |
| `decode_raw(buf)` | provided | `Default` instance + `merge` |
| `encode_to_vec(&self)` | provided | Encode to `Vec<u8>` |
| `decode(bytes)` | provided | Decode from a `&[u8]` |

Supertraits: `Sized + Debug + Default + Send + Sync`.

### `OxiName` trait

| Item | Kind | Description |
|------|------|-------------|
| `NAME` | assoc. const | Simple message name (e.g. `"MyMessage"`) |
| `PACKAGE` | assoc. const | Proto package (e.g. `"my.package"`, or `""`) |
| `full_name()` | provided | `"my.package.MyMessage"` |
| `type_url()` | provided | `"type.googleapis.com/my.package.MyMessage"` |

### `OxiOneof` trait

Implemented by generated oneof-group enums so the containing message can dispatch through them.

| Method | Description |
|--------|-------------|
| `discriminant(&self)` | Proto field number of the active variant |
| `encoded_len(&self)` | Encoded size of the active variant (tag + value) |
| `encode(&self, buf)` | Write the active variant into an `EncodeBuffer` |
| `merge_field(field_number, wire_type, buf, slot)` | Decode one field into `slot`; `Ok(false)` if the field number is not part of this oneof |

### `Extensions` — proto2 extension storage

Stores extension values as raw wire bytes keyed by field number (`BTreeMap` for stable order). Lazily encoded on write, decoded on read.

| Method | Description |
|--------|-------------|
| `new()` / `is_empty()` / `len()` | Construct / inspect |
| `has_extension(field_number)` | Presence check |
| `get_extension::<T>(field_number)` | Decode stored bytes as message `T` → `Option<T>` |
| `set_extension::<T>(field_number, &value)` | Encode and store a message-typed extension |
| `clear_extension(field_number)` / `clear()` | Remove one / all |
| `merge_raw(field_number, wire_type, buf)` | Store a raw field verbatim (Len/Varint/I32/I64; groups skipped) |
| `encode_raw(&self, buf)` | Write all extensions in field-number order |
| `encoded_len(&self)` | Total encoded size |

### `wire` module

A complete, standalone protobuf wire-format implementation. Submodules are public so lower-level helpers (e.g. `wire::varint::encoded_len_varint`) are reachable.

#### `WireType` enum

| Variant | Value | Used for |
|---------|-------|----------|
| `Varint` | 0 | `int32/64`, `uint32/64`, `sint32/64`, `bool`, `enum` |
| `I64` | 1 | `fixed64`, `sfixed64`, `double` |
| `Len` | 2 | `string`, `bytes`, embedded messages, packed repeated |
| `SGroup` | 3 | Start group (legacy proto2) |
| `EGroup` | 4 | End group (legacy proto2) |
| `I32` | 5 | `fixed32`, `sfixed32`, `float` |

Methods: `WireType::from_u32(u32) -> Result<Self, WireError>`, `value(self) -> u32`; implements `Display`. With the `serde` feature it derives `Serialize`/`Deserialize`.

#### `DecodeBuffer<'a>` — cursor-based reader (zero-copy)

| Method | Returns | Description |
|--------|---------|-------------|
| `new(&[u8])` | `Self` | Wrap a byte slice |
| `is_empty()` / `remaining()` / `position()` | `bool` / `usize` / `usize` | Cursor state |
| `remaining_bytes()` | `&[u8]` | Unconsumed tail |
| `read_tag()` | `Tag` | Read a field tag |
| `read_varint()` / `read_varint32()` | `u64` / `u32` | LEB128 varint |
| `read_varint_i64()` / `read_varint_i32()` / `read_bool()` | `i64` / `i32` / `bool` | Varint reinterpretations |
| `read_fixed32()` / `read_fixed64()` | `u32` / `u64` | Little-endian fixed-width |
| `read_float()` / `read_double()` | `f32` / `f64` | IEEE-754 little-endian |
| `read_length_delimited()` | `&[u8]` | Length-prefixed payload (borrowed) |
| `read_string()` | `&str` | UTF-8 length-delimited (validates UTF-8) |
| `skip_field(wire_type)` | `()` | Advance past a field (handles nested groups) |

#### `EncodeBuffer` — append-only writer

| Method | Description |
|--------|-------------|
| `new()` / `with_capacity(n)` | Construct |
| `len()` / `is_empty()` / `as_bytes()` / `into_vec()` | Inspect / extract |
| `write_tag(field_number, wire_type)` | Write a field tag (errors on invalid field number) |
| `write_varint`/`write_varint32`/`write_varint_i64`/`write_varint_i32`/`write_bool` | Varint writers |
| `write_fixed32` / `write_fixed64` | Little-endian fixed-width |
| `write_float` / `write_double` | IEEE-754 writers |
| `write_length_delimited(&[u8])` / `write_string(&str)` | Length-prefixed payloads |
| `write_raw(&[u8])` | Raw bytes, no framing |

#### `Tag` and free functions

`Tag { field_number: u32, wire_type: WireType }`. Constants: `MAX_FIELD_NUMBER` (2²⁹−1), `RESERVED_RANGE_START` (19000), `RESERVED_RANGE_END` (19999).

| Group | Functions |
|-------|-----------|
| Tags (`tag`) | `encode_tag`, `decode_tag`, `make_tag` |
| Varint (`varint`) | `encode_varint`, `decode_varint`, `encode_varint32`, `decode_varint32`, `encode_varint_i64`, `decode_varint_i64`, `encode_varint_i32`, `decode_varint_i32`, `encode_varint_bool`, `decode_varint_bool`, `encode_varint_fixed`, `encoded_len_varint` |
| ZigZag (`zigzag`) | `zigzag_encode32`, `zigzag_decode32`, `zigzag_encode64`, `zigzag_decode64` |
| Fixed (`fixed`) | `encode_fixed32`/`decode_fixed32`, `encode_fixed64`/`decode_fixed64`, `encode_float`/`decode_float`, `encode_double`/`decode_double`, `encode_sfixed32`/`decode_sfixed32`, `encode_sfixed64`/`decode_sfixed64` |
| Length-delimited (`length_delimited`) | `encode_length_delimited`, `decode_length_delimited`, `encode_string`, `decode_string`, `encoded_len_length_delimited` |

#### Unknown-field preservation (`unknown`)

| Type | Description |
|------|-------------|
| `UnknownField { field_number, value }` | A single preserved field |
| `UnknownValue` | `Varint(u64)`, `Fixed64(u64)`, `LengthDelimited(Vec<u8>)`, `Fixed32(u32)`, `Group(Vec<u8>)`; `wire_type()` accessor |
| `UnknownFields` | Ordered collection: `push*`, `iter`, `len`, `is_empty`, `get_field`, `clear`, `encoded_len`, `encode_to`; implements `IntoIterator` |

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | yes | Enables `std`; activates `prost/std` and `prost-types/std`. Disable for `no_std`. |
| `alloc` | no | `alloc`-only support for `no_std` builds |
| `serde` | no | Derives `Serialize`/`Deserialize` on `WireType`, `UnknownField`, `UnknownValue`, `UnknownFields` |

## Error Variants — `OxiProtoError`

Stack-wide error enum (`#[non_exhaustive]`); implements `Display` + `core::error::Error`.

| Variant | Description |
|---------|-------------|
| `ParseError(String)` | A `.proto` source could not be parsed or resolved |
| `CodegenError(String)` | Rust code could not be generated from descriptors |
| `IoError(std::io::Error)` | Underlying I/O failure (`std` feature only) |
| `WireFormatError(WireError)` | A wire-format encode/decode error |

`From<std::io::Error>` (with `std`) and `From<wire::WireError>` conversions are provided.

### `WireError` (in `wire`)

| Variant | Description |
|---------|-------------|
| `UnexpectedEof` | Input ended before a complete value |
| `Overflow` | Varint exceeded 10 bytes |
| `InvalidWireType(u32)` | Tag held a wire-type value outside `0..=5` |
| `InvalidFieldNumber(u32)` | Field number 0 or beyond `MAX_FIELD_NUMBER` |
| `TruncatedMessage { declared, available }` | Length prefix exceeded the available bytes |
| `OutOfRange(String)` | Decoded value out of range for the target type |
| `InvalidUtf8(Utf8Error)` | String field was not valid UTF-8 |

## Related crates

- [`oxiproto`](../../README.md) — top-level façade that re-exports the OxiProto stack
- [`oxiproto-codegen`](../oxiproto-codegen) — `FileDescriptorSet` → Rust source generation
- [`oxiproto-build`](../oxiproto-build) — `build.rs` integration (`.proto` → Rust, no `protoc`)
- [`oxiproto-cli`](../oxiproto-cli) — command-line compiler
- `oxiproto-json` — canonical Protobuf-JSON encode/decode
- `oxiproto-reflect` — runtime reflection over descriptors
- `oxiproto-wkt` — Google well-known types

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
