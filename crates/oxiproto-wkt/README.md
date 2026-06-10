# oxiproto-wkt — Well-Known Types interop for OxiProto

[![Crates.io](https://img.shields.io/crates/v/oxiproto-wkt.svg)](https://crates.io/crates/oxiproto-wkt)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiproto-wkt` provides ergonomic, Pure-Rust interop for the Google protobuf **Well-Known Types** (the `google.protobuf.*` messages). It layers extension traits onto the WKT structs from [`prost-types`] — adding construction helpers, accessors, validation, set operations, ordering, and conversions to/from standard Rust time types — and supplies its own Pure-Rust equivalents for the WKTs that `prost-types` does not export (the scalar wrappers and `Empty`). It is the WKT layer of the OxiProto stack and is re-exported from the `oxiproto` facade behind the `wkt` feature.

All standard WKT types are re-exported from this crate, so callers typically depend on `oxiproto-wkt` alone. The crate is `#![forbid(unsafe_code)]`; the default code paths use a hand-rolled, dependency-free calendar/RFC-3339 implementation, so no `chrono`/`time` dependency is pulled in unless you opt into the corresponding feature.

[`prost-types`]: https://crates.io/crates/prost-types

## Installation

```toml
[dependencies]
oxiproto-wkt = "0.1.2"

# Optional: chrono interop
# oxiproto-wkt = { version = "0.1.2", features = ["chrono"] }

# Optional: time interop
# oxiproto-wkt = { version = "0.1.2", features = ["time"] }
```

Or, via the facade (use `wkt-chrono` for the `chrono` methods):

```toml
[dependencies]
oxiproto = { version = "0.1.2", features = ["wkt"] }
```

## Quick Start

```rust
use oxiproto_wkt::{Timestamp, TimestampExt, Duration, DurationExt};

// Current wall-clock time as a Timestamp, formatted RFC 3339.
let now = Timestamp::now();
let s = now.to_rfc3339()?;
let parsed = Timestamp::from_rfc3339(&s)?;

// Duration arithmetic and canonical string form.
let d = Duration::from_duration_string("1.5s")?;
let later = now.add_duration(&d)?;
assert_eq!(Duration { seconds: 1, nanos: 500_000_000 }.to_duration_string(), "1.5s");
# Ok::<(), oxiproto_wkt::OverflowError>(())
```

### Packing into `Any`

```rust,ignore
use oxiproto_wkt::{Any, AnyExt, Timestamp, TimestampExt};

let ts = Timestamp::now();
let any = Any::pack(&ts);                 // type.googleapis.com/google.protobuf.Timestamp
assert!(any.is::<Timestamp>());
let back: Option<Timestamp> = any.unpack();
```

## Well-Known Types covered

Re-exported from `prost_types` at the crate root: `Any`, `Duration`, `FieldMask`, `ListValue`, `SourceContext`, `Struct`, `Timestamp`, `Value`. The scalar wrappers and `Empty` are defined natively in this crate because `prost-types` does not export them.

| Well-Known Type | Extension trait / native type | Status |
|-----------------|------------------------------|--------|
| `google.protobuf.Timestamp` | `TimestampExt` (+ `TimestampTimeExt` under `time`) | Re-exported + extended |
| `google.protobuf.Duration` | `DurationExt` (+ `DurationTimeExt` under `time`) | Re-exported + extended |
| `google.protobuf.Any` | `AnyExt` | Re-exported + extended |
| `google.protobuf.FieldMask` | `FieldMaskExt` | Re-exported + extended |
| `google.protobuf.Struct` | `StructExt` | Re-exported + extended |
| `google.protobuf.Value` | `ValueExt` | Re-exported + extended |
| `google.protobuf.ListValue` | `ListValueExt` | Re-exported + extended |
| `google.protobuf.SourceContext` | `SourceContextExt` | Re-exported + extended |
| `google.protobuf.Api` | `ApiExt` | Re-exported + extended |
| `google.protobuf.Type` | `TypeExt` | Re-exported + extended |
| `google.protobuf.Enum` | `EnumTypeExt` | Re-exported + extended |
| `google.protobuf.Empty` | `Empty` / `EmptyExt` / `EMPTY` | Native (not in `prost-types`) |
| `google.protobuf.DoubleValue` | `DoubleValue` + `WrapperExt<f64>` | Native (not in `prost-types`) |
| `google.protobuf.FloatValue` | `FloatValue` + `WrapperExt<f32>` | Native (not in `prost-types`) |
| `google.protobuf.Int64Value` | `Int64Value` + `WrapperExt<i64>` | Native (not in `prost-types`) |
| `google.protobuf.UInt64Value` | `UInt64Value` + `WrapperExt<u64>` | Native (not in `prost-types`) |
| `google.protobuf.Int32Value` | `Int32Value` + `WrapperExt<i32>` | Native (not in `prost-types`) |
| `google.protobuf.UInt32Value` | `UInt32Value` + `WrapperExt<u32>` | Native (not in `prost-types`) |
| `google.protobuf.BoolValue` | `BoolValue` + `WrapperExt<bool>` | Native (not in `prost-types`) |
| `google.protobuf.StringValue` | `StringValue` + `WrapperExt<String>` | Native (not in `prost-types`) |
| `google.protobuf.BytesValue` | `BytesValue` + `WrapperExt<Vec<u8>>` | Native (not in `prost-types`) |

## API Overview

### `TimestampExt` — extension trait for `Timestamp`

| Method | Returns | Description |
|--------|---------|-------------|
| `now()` | `Timestamp` | Current wall-clock time |
| `to_system_time()` | `Result<SystemTime, OverflowError>` | Convert to `std::time::SystemTime` |
| `from_system_time(t)` | `Timestamp` | Build from a `SystemTime` |
| `to_rfc3339()` | `Result<String, OverflowError>` | Format as RFC 3339 (trailing fractional zeros trimmed) |
| `from_rfc3339(s)` | `Result<Timestamp, OverflowError>` | Parse RFC 3339 (with `Z` or `±HH:MM` offset) |
| `is_valid()` | `bool` | Within `0001-01-01` … `9999-12-31` and canonical nanos |
| `add_duration(&Duration)` | `Result<Timestamp, OverflowError>` | Add a proto `Duration` |
| `sub_duration(&Duration)` | `Result<Timestamp, OverflowError>` | Subtract a proto `Duration` |
| `duration_since(&Timestamp)` | `Duration` | Elapsed time from an earlier timestamp (may be negative) |
| `to_chrono_utc()` | `chrono::DateTime<Utc>` | **`chrono` feature** |
| `from_chrono_utc(dt)` | `Timestamp` | **`chrono` feature** |

Free function: `timestamp_cmp(a, b) -> Ordering` (lexicographic by `(seconds, nanos)`; `prost-types` does not derive `Ord` for `Timestamp`).

### `DurationExt` — extension trait for `Duration`

| Method | Returns | Description |
|--------|---------|-------------|
| `to_std_duration()` | `Result<std::time::Duration, OverflowError>` | Convert to `std::time::Duration` (errors on negative) |
| `from_std_duration(d)` | `Result<Duration, OverflowError>` | Build from a `std::time::Duration` |
| `to_duration_string()` | `String` | Canonical decimal-seconds string, e.g. `"1.5s"`, `"-3s"` |
| `from_duration_string(s)` | `Result<Duration, OverflowError>` | Parse a decimal-seconds string |
| `is_valid()` | `bool` | Within ±315,576,000,000 s with sign-agreeing nanos |
| `to_chrono_duration()` | `Result<chrono::Duration, OverflowError>` | **`chrono` feature** |
| `from_chrono_duration(d)` | `Result<Duration, OverflowError>` | **`chrono` feature** |

Free function: `duration_cmp(a, b) -> Ordering` (lexicographic by `(seconds, nanos)` on canonical durations).

### `AnyExt` — extension trait for `Any`

| Method | Returns | Description |
|--------|---------|-------------|
| `pack<T>(msg)` | `Any` | Pack a `prost::Message + prost::Name`, type URL `type.googleapis.com/{full_name}` |
| `pack_with_prefix<T>(msg, prefix)` | `Any` | Pack with a custom type-URL prefix |
| `unpack<T>()` | `Option<T>` | Decode if the type URL matches `T` |
| `is<T>()` | `bool` | Whether this `Any` holds a message of type `T` |
| `type_name()` | `&str` | The type name after the last `/` in the type URL |

### `FieldMaskExt` — extension trait for `FieldMask`

| Method | Returns | Description |
|--------|---------|-------------|
| `is_valid_path(path)` | `bool` | Path matches dot-separated `[a-z_][a-z0-9_]*` components |
| `is_valid()` | `bool` | Every path in the mask is valid |
| `canonical()` | `FieldMask` | Sort, dedupe, and drop paths subsumed by a more-general ancestor |
| `union(&FieldMask)` | `FieldMask` | Canonicalised union |
| `intersection(&FieldMask)` | `FieldMask` | Canonicalised intersection (exact-match paths only) |

### `StructExt` / `ValueExt` / `ListValueExt` — `google.protobuf.Struct` family

`ValueExt` (for `Value`):

| Method | Returns | Description |
|--------|---------|-------------|
| `null()` / `from_bool(b)` / `from_f64(n)` / `from_string(s)` | `Value` | Construct each `Value` kind |
| `as_bool()` / `as_number()` / `as_string()` | `Option<…>` | Extract a typed kind |
| `is_null()` | `bool` | Whether the value is null |

`StructExt` (for `Struct`): `empty()`, `insert(key, value)`, `get(key) -> Option<&Value>`, `len()`, `is_empty()`.

`ListValueExt` (for `ListValue`): `from_vec(Vec<Value>)`, `iter()`, `len()`, `is_empty()`, `get(index) -> Option<&Value>`.

### Reflection/descriptor WKTs — `ApiExt`, `TypeExt`, `EnumTypeExt`, `SourceContextExt`

| Trait (target) | Methods |
|----------------|---------|
| `ApiExt` (`Api`) | `new(name)`, `name()`, `methods()`, `version()`, `with_method(method)` |
| `TypeExt` (`Type`) | `new(name)`, `name()`, `fields()`, `oneofs()` |
| `EnumTypeExt` (`Enum`) | `new(name)`, `name()`, `values()` |
| `SourceContextExt` (`SourceContext`) | `new(file_name)`, `file_name()` |

### Scalar wrapper types & `WrapperExt`

Native structs `DoubleValue`, `FloatValue`, `Int64Value`, `UInt64Value`, `Int32Value`, `UInt32Value`, `BoolValue`, `StringValue`, `BytesValue` — each a single-field `value` struct deriving `Debug`/`Clone`/`PartialEq`/`Default` (and `Copy`/`Eq` where applicable). Each implements `WrapperExt<T>`:

| Method | Description |
|--------|-------------|
| `wrap(value: T)` | Wrap a value into the corresponding wrapper |
| `unwrap_value()` | Extract the inner value |

### `Empty`

A zero-field message equivalent to `google.protobuf.Empty` (which `prost-types` does not export). Provides the `Empty` struct, the `EMPTY` const, and `EmptyExt::new()`.

### `OverflowError`

The crate's error type, returned by the fallible conversion/arithmetic methods. Carries an `operation` name and a human-readable `detail`. Implements `Debug`/`Clone`/`PartialEq`/`Eq`, `Display`, and `std::error::Error`, and converts to/from `oxiproto_core::OxiProtoError`.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `chrono` | off | Adds `TimestampExt::to_chrono_utc` / `from_chrono_utc` and `DurationExt::to_chrono_duration` / `from_chrono_duration` |
| `time` | off | Enables the `time_feature` module: `TimestampTimeExt` (`to_offset_datetime` / `from_offset_datetime`) and `DurationTimeExt` (`to_time_duration` / `from_time_duration`) |

Both are off by default, keeping the default build dependency-free (Pure Rust, hand-rolled calendar math).

## Cross-references

| Crate | Role |
|-------|------|
| [`oxiproto-core`](../oxiproto-core) | Native wire format, traits, and the shared `OxiProtoError` |
| [`oxiproto-reflect`](../oxiproto-reflect) | Runtime reflection (`DescriptorPool`, `DynamicMessage`) |
| [`oxiproto-json`](../oxiproto-json) | Canonical Protobuf-JSON; encodes `Timestamp`/`Duration` per the WKT JSON rules |
| [`oxiproto`](../oxiproto) | Facade crate; re-exports this crate as `oxiproto::wkt` behind the `wkt` / `wkt-chrono` features |

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
