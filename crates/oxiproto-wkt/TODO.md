# oxiproto-wkt TODO

## Status
Well-Known Types interop: `TimestampExt` (now/to_system_time/from_system_time/
to_rfc3339/from_rfc3339/is_valid/add_duration/sub_duration/duration_since + chrono),
`DurationExt` (std conversions/to_duration_string/from_duration_string/is_valid + chrono),
`AnyExt` (pack/unpack/is/type_name), `WrapperExt` for all 9 wrapper types, `ValueExt`
and `StructExt` for Struct/Value, `ListValueExt`, `FieldMaskExt`, `EmptyExt`,
`SourceContextExt`, `TypeExt`, `EnumTypeExt`, `ApiExt`. `time` feature adds
`TimestampTimeExt`/`DurationTimeExt`. Free functions `timestamp_cmp`/`duration_cmp`
for ordering. RFC 3339 uses a pure-Rust calendar algorithm. ~900 SLOC production code.
Criterion benchmarks in `benches/wkt.rs` cover Timestamp/Any/Struct performance.
oxiproto-json now delegates all WKT formatting to oxiproto-wkt traits (2026-06-03).

## Core Implementation
- [x] Implement `AnyExt` trait: pack(message) -> Any, unpack<T>(any) -> T, type_url validation
- [x] Implement `StructExt` trait: empty/insert/get/len/is_empty
- [x] Implement `ValueExt` trait: null/from_bool/from_f64/from_string + typed accessors (as_string, as_number, as_bool)
- [x] Implement `ListValueExt` trait: iteration, typed access, from_vec (40-50 SLOC)
  - **Goal:** Ergonomic access to `prost_types::ListValue` values.
  - **Design:** `from_vec(Vec<Value>) -> ListValue`, `iter(&self) -> impl Iterator<Item=&Value>`, `len(&self) -> usize`, typed accessors via ValueExt. Located in `src/list_value.rs`.
  - **Files:** crates/oxiproto-wkt/src/list_value.rs (new); src/lib.rs (modify: add module + re-export); tests/list_value.rs (new)
  - **Tests:** from_vec round-trip; iter; typed accessors; empty list.
- [x] Implement `FieldMaskExt` trait: path validation + canonical form + set operations (180 SLOC)
  - **Goal:** Path-level FieldMask operations. Apply/merge against a message is deferred (requires native reflection).
  - **Design:** `is_valid_path(&str) -> bool` (snake_case components, dot-separated, no leading/trailing dots), `canonical() -> FieldMask` (sort + dedupe + drop redundant subpaths), `union(&self, &FieldMask) -> FieldMask`, `intersection(&self, &FieldMask) -> FieldMask`. Located in `src/field_mask.rs`.
  - **Files:** crates/oxiproto-wkt/src/field_mask.rs (new); src/lib.rs (modify); tests/field_mask.rs (new)
  - **Tests:** valid/invalid paths; canonical idempotent; union/intersection set semantics; redundant subpath removal in canonical.
- [x] Implement `EmptyExt` for local `Empty` type (30 SLOC)
  - **Goal:** `pub const EMPTY: Empty = Empty {}` + `Empty::new() -> Empty` convenience. Note: prost_types 0.14.x has no `Empty` type; a local equivalent is defined.
  - **Files:** crates/oxiproto-wkt/src/empty.rs (new); src/lib.rs (modify)
  - **Tests:** EMPTY const accessible; new() == EMPTY.
- [x] Implement `WrapperExt` for DoubleValue, FloatValue, Int64Value, UInt64Value, Int32Value, UInt32Value, BoolValue, StringValue, BytesValue: wrap/unwrap convenience
- [x] Implement `SourceContextExt` trait: file_name access (40 SLOC)
  - **Goal:** Ergonomic typed access to `prost_types::SourceContext`.
  - **Files:** crates/oxiproto-wkt/src/source_context.rs (new); src/lib.rs (modify)
  - **Tests:** file_name accessor; new(file_name: impl Into<String>).
- [x] Implement `TypeExt` trait: message type descriptors (60 SLOC)
  - **Goal:** Typed accessors for `prost_types::Type` (message type) and `prost_types::Enum`.
  - **Files:** crates/oxiproto-wkt/src/type_ext.rs (new); src/lib.rs (modify)
  - **Tests:** name/fields/options accessors work on a constructed Type.
- [x] Implement `ApiExt` trait: API description with methods and options (70 SLOC)
  - **Goal:** Typed accessors for `prost_types::Api` (name, methods, options, version).
  - **Files:** crates/oxiproto-wkt/src/api_ext.rs (new); src/lib.rs (modify)
  - **Tests:** name/methods/version accessors.
- [x] Add `Timestamp::from_rfc3339(s)` parsing RFC 3339 strings to Timestamp (pure Rust)
- [x] Add `Timestamp::to_rfc3339()` formatting Timestamp as RFC 3339 string (pure Rust)
- [x] Add `Duration::from_duration_string(s)` parsing "1.5s" / "-3600s" format
- [x] Add `Duration::to_duration_string()` producing canonical "1.5s" format
- [x] Implement negative Duration support with proper nanos sign alignment

## API Improvements
- [x] Add `time` crate feature: TimestampTimeExt::to_offset_datetime, DurationTimeExt::to_time_duration (120 SLOC)
  - **Goal:** `time` feature gate providing conversions to/from `time::OffsetDateTime` and `time::Duration`. Coexists with chrono feature independently.
  - **Files:** crates/oxiproto-wkt/src/time_feature.rs (new, behind `time` feature); Cargo.toml (add time optional dep); tests/time_interop.rs (new, behind `time` feature)
  - **Tests:** Timestamp↔OffsetDateTime round-trip; Duration↔time::Duration round-trip; epoch, pre-epoch, fractional seconds.
- [x] Add validation methods: Timestamp::is_valid (seconds in valid range), Duration::is_valid
- [x] Add Timestamp arithmetic: add_duration, sub_duration, duration_since
  - **Goal:** `add_duration(&self, &Duration) -> Result<Timestamp, OverflowError>` (overflow-checked), `sub_duration`, `duration_since(&self, &Timestamp) -> Duration`.
  - **Files:** crates/oxiproto-wkt/src/timestamp.rs (modify)
  - **Tests:** add/sub round-trip; overflow error at boundary; duration_since positive and negative cases.
- [x] Add comparison operators for Timestamp and Duration
  - **Goal:** prost_types 0.14.x Timestamp/Duration do NOT derive PartialOrd/Ord, so free functions `timestamp_cmp`/`duration_cmp` are provided (orphan rule prevents direct impl).
  - **Files:** crates/oxiproto-wkt/src/timestamp.rs (modify); crates/oxiproto-wkt/src/duration.rs (modify)
  - **Tests:** Timestamp ordering: past < future; epoch ordering; Duration: shorter < longer; negative duration ordering.
- [x] Improve error messages in OverflowError with context about the source value (done 2026-05-29)
  - **Goal:** `OverflowError` carries the source value/operation that overflowed for better debugging messages.
  - **Design:** Extend `OverflowError` struct to include `operation: &'static str` (e.g. "add_duration") + `detail: Cow<'static, str>` (e.g. "seconds overflow: value=9999999999"). Update call sites in `timestamp.rs`/`duration.rs` arithmetic to pass context. Keep existing `Display`/`Error`/`From` impls compatible.
  - **Files:** `crates/oxiproto-wkt/src/lib.rs` (OverflowError shape + Display), `src/timestamp.rs` + `src/duration.rs` (update call sites)
  - **Tests:** OverflowError messages verified in chrono interop tests.

## Testing
- [x] Test Timestamp round-trip: SystemTime -> Timestamp -> SystemTime preserves value
  - **Files:** crates/oxiproto-wkt/tests/wkt.rs
- [x] Test Timestamp for dates before Unix epoch (negative seconds, positive nanos)
  - **Files:** crates/oxiproto-wkt/tests/time_interop.rs
- [x] Test Timestamp edge cases: epoch itself, far future (year 9999), minimum representable
  - **Files:** crates/oxiproto-wkt/tests/time_interop.rs
- [x] Test Duration round-trip: std::time::Duration -> proto Duration -> std::time::Duration
  - **Files:** crates/oxiproto-wkt/tests/wkt.rs
- [x] Test Duration negative rejection (std Duration cannot be negative)
  - **Files:** crates/oxiproto-wkt/tests/wkt.rs
- [x] Test chrono conversions: DateTime<Utc> -> Timestamp -> DateTime<Utc> (done 2026-05-29)
  - **Goal:** Verify chrono `DateTime<Utc>` → `Timestamp` → `DateTime<Utc>` round-trip equality.
  - **Design:** New `tests/chrono_interop.rs` (or extend `tests/wkt.rs`) behind `wkt-chrono` feature. Tests: round-trip equality; epoch; far-future; far-past (within nanosecond precision).
  - **Files:** `crates/oxiproto-wkt/tests/chrono_interop.rs` (new ~90 SLOC, behind `wkt-chrono`)
- [x] Test chrono Duration conversions with negative durations (done 2026-05-29)
  - **Goal:** Verify chrono `Duration` → `prost_types::Duration` → chrono `Duration` round-trips, including negative durations.
  - **Design:** In the same `tests/chrono_interop.rs`. Positive duration; zero; negative duration (e.g. -500ms); overflow path produces `OverflowError` with context.
  - **Files:** Same chrono_interop.rs as above.
- [x] Test Any pack/unpack round-trip with a concrete message type
- [x] Test Struct/Value typed accessors and field operations
- [x] Test FieldMask path validation and application
  - **Files:** crates/oxiproto-wkt/tests/field_mask.rs (new)
- [x] Test RFC 3339 round-trip (epoch, known dates, sub-second, timezone offsets, pre-epoch)
- [x] Test Duration string round-trip (whole, fractional, negative)

## Performance
- [x] Benchmark Timestamp conversion (SystemTime <-> Timestamp) throughput
  - Criterion harness in `benches/wkt.rs`: from_system_time, to_system_time, round-trip, RFC 3339 parse/format.
- [x] Benchmark Any pack/unpack vs manual encode/decode
  - Criterion harness in `benches/wkt.rs`: pack, unpack, round-trip, manual encode/decode comparison.
- [x] Profile allocation in Struct/Value conversion chains
  - Criterion harness in `benches/wkt.rs`: build_struct_10/100_fields, Value constructors, get/miss.

## Integration
- [x] Ensure oxiproto-json uses WKT extension traits for canonical JSON representation
  - oxiproto-json refactored (2026-06-03): `TimestampExt::to_rfc3339`/`from_rfc3339` and
    `DurationExt::to_duration_string`/`from_duration_string` now used instead of inline chrono.
    chrono dep removed from oxiproto-json; oxiproto-wkt added as dep instead.
- [ ] Ensure oxirpc uses Timestamp/Duration for deadline/timeout metadata
  - DEFERRED: oxirpc is a separate workspace (~/work/oxirpc); blocked on oxirpc integration work.
- [x] Coordinate with oxiproto-reflect for dynamic Any unpacking (done 2026-06-19)
  - **Implemented:** Added `reflect` feature to `oxiproto-wkt` with `oxiproto-reflect` as an optional dependency. `AnyExt::unpack_dynamic(&pool)` added to the `AnyExt` trait (behind `#[cfg(feature = "reflect")]`): resolves the `type_url` type name against a `NativeDescriptorPool`, then calls `NativeDynamicMessage::decode(desc, &self.value)`. Returns `Option<Result<NativeDynamicMessage, ReflectError>>`. 5 tests in `tests/any_dynamic.rs` covering: round-trip (encode Foo → pack into Any → unpack_dynamic → re-encode → bytes equal), wrong type_url returns None, malformed bytes returns Err, empty value returns empty message, type_name extraction. Zero clippy warnings under both default and `--features reflect` builds.
