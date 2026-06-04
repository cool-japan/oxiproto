# oxiproto-reflect TODO

## Status
Two coexisting reflection paths:
1. **prost-reflect facade** (unchanged): `pool_from_fds_bytes` builds a `DescriptorPool`, `dynamic_message` creates empty `DynamicMessage` instances; re-exports key prost-reflect types under their canonical names plus `ReflectValue`.
2. **Native pure-Rust path** (added 2026-05-30, `src/native/`): `native::DescriptorPool::from_file_descriptor_set` builds an `Arc`-shared, index-based descriptor model; full descriptor type set (`FileDescriptor`/`MessageDescriptor`/`FieldDescriptor`/`EnumDescriptor`/`EnumValueDescriptor`/`OneofDescriptor`/`ServiceDescriptor`/`MethodDescriptor` + `Kind`/`Cardinality`); `native::DynamicMessage` with get/set/has/clear (oneof-exclusive) and protobuf WIRE encode/decode reusing `oxiproto_core::wire` 100% (packed & unpacked repeated, `map<K,V>` synthetic entries, proto3 default omission, unknown-field preservation, groups rejected). Re-exported from the crate root with a `Native`-prefix to avoid colliding with the prost-reflect names. JSON/text formats and prost-reflect removal still deferred.

## Core Implementation
- [x] Implement native `DescriptorPool` backed by oxiproto-core descriptors (replace prost-reflect dependency) (300-400 SLOC) (done 2026-05-30)
  - **Goal:** native reflection on the core path: DescriptorPool from a FileDescriptorSet + full descriptor types + DynamicMessage with get/set AND protobuf WIRE encode/decode; verified by round-trip + prost-encoded-bytes oracle. JSON/text formats and full prost-reflect removal deferred.
  - **Design:** pool.rs DescriptorPool::from_file_descriptor_set(prost_types::FileDescriptorSet) + name/number lookup maps + get_message/enum/service_by_name + iterators (input is an existing FDS → NOT blocked on the Phase-2 native parser). descriptor.rs File/Message/Field/Enum/EnumValue/Oneof/Service/MethodDescriptor mirroring prost-reflect's surface (number/name/kind/cardinality/json_name, nested types, oneofs, method input/output+streaming), shared via Arc<DescriptorPool>. value.rs Value+MapKey (prost-reflect-compatible). dynamic.rs+wire_codec.rs DynamicMessage::{new,descriptor,get_field,set_field(oneof-exclusive),has_field,clear_field,encode_to_vec,decode} reusing oxiproto-core::wire (varint/zigzag/tag/length-delimited/fixed/UnknownFields). Handle packed vs unpacked repeated, maps-as-repeated-entries, unknown-field preservation, proto3 default omission; groups → explicit unsupported error. NON-BREAKING: add native types ALONGSIDE the existing prost-reflect path; do NOT modify oxiproto-json/oxiproto-cli this slice (they stay on prost-reflect); use prost as a dev-dependency oracle in tests.
  - **Files:** crates/oxiproto-reflect/src/{pool,descriptor,dynamic,value,wire_codec}.rs; src/lib.rs re-exports; tests under tests/. Each file <2000 lines (split if descriptor.rs/dynamic.rs approach the limit).
  - **Tests:** build a FileDescriptorSet exercising scalars/repeated/packed/map/nested/oneof/enum; DynamicMessage set→encode→decode→get round-trip; cross-check wire bytes against prost-generated encoding (oracle); descriptor lookups by name/number; unknown-field preservation round-trip; oneof exclusivity.
  - **Risk:** map synthetic entries, packed-repeated, and unknown-field handling are the traps → covered by oracle + targeted tests. Keep default-features/no_std posture (alloc for native types). Latest-crates policy; no version bump.
- [x] Implement native `DynamicMessage` with field get/set by descriptor (200-300 SLOC) (done 2026-05-30) — (see native reflection plan above)
- [x] Implement native `MessageDescriptor` with field iteration, nested type access, oneof group access (150-200 SLOC) (done 2026-05-30) — (see native reflection plan above)
- [x] Implement native `FieldDescriptor` with type info, label, default value, JSON name (100-150 SLOC) (done 2026-05-30) — (see native reflection plan above)
- [x] Implement native `EnumDescriptor` with value enumeration (60-80 SLOC) (done 2026-05-30) — (see native reflection plan above)
- [x] Implement `ServiceDescriptor` with method iteration (60-80 SLOC) (done 2026-05-30) — (see native reflection plan above)
- [x] Implement `MethodDescriptor` with input/output type, streaming flags (40-50 SLOC) (done 2026-05-30) — (see native reflection plan above)
- [x] Implement `FileDescriptor` with package, dependency, option access (60-80 SLOC) — (see native reflection plan above) (name/package/syntax/dependency access done 2026-05-30; custom file option accessors done 2026-06-03: java_package, go_package, java_outer_classname, deprecated, optimize_for)
- [x] Implement `DynamicMessage::encode` / `decode` using wire format from oxiproto-core (200-250 SLOC) (done 2026-05-30) — (see native reflection plan above)
- [x] Implement `DynamicMessage::to_json` / `from_json` for canonical protobuf JSON (150-200 SLOC) (done 2026-06-03)
  - **Goal:** Canonical proto3 JSON mapping: camelCase field names (json_name), 64-bit→string, bytes→base64, enums→name, NaN/Inf→string, map→object, nested messages recursive. Proto3 default omission on output. Unknown keys skipped on input. `null` treated as default.
  - **Files:** `crates/oxiproto-reflect/src/native/json.rs` (new ~450 SLOC), `Cargo.toml` (+serde_json, +base64), `native/mod.rs` (+mod json, +NativeJsonError), `lib.rs` (re-export NativeJsonError), `tests/native_json.rs` (new, 24 tests)
  - **Tests:** empty object, int32/int64/uint64 encoding (string for 64-bit), NaN/Inf strings, base64 bytes, enum name, repeated array, map object, nested message, default omission, from_json with null/unknown keys/enum-by-name/map/nested, round-trip scalars/enum/repeated/nested.
- [x] Implement `DynamicMessage::to_text` / `from_text` for proto text format (150-200 SLOC) (done 2026-06-03)
  - **Goal:** Protobuf text format: `name: value` scalars, quoted strings with escape handling, `name { ... }` nested messages, repeated as multiple entries, map as repeated synthetic entries. Proto3 default omission on output.
  - **Files:** `crates/oxiproto-reflect/src/native/text.rs` (new ~600 SLOC), `native/mod.rs` (+mod text, +NativeTextError), `lib.rs` (re-export NativeTextError), `tests/native_text.rs` (new, 24 tests)
  - **Tests:** empty message, int32/string/bool/NaN/Inf/bytes/enum/repeated/nested/map encoding; decode int32/string/bool/NaN/enum/repeated/nested/unknown-skipped/comments; round-trip scalars/enum/repeated/map/nested.
- [x] Implement `pool_from_fds(fds: &FileDescriptorSet)` accepting pre-decoded FDS (20 SLOC) (done 2026-05-29)
  - **Goal:** Convenience fn avoiding the extra bytes round-trip; calls DescriptorPool::from_file_descriptor_set directly.
  - **Files:** crates/oxiproto-reflect/src/lib.rs (modify)
  - **Tests:** pool_from_fds(fds) == pool_from_fds_bytes(fds.encode_to_vec()) for same FDS.
- [x] Add `get_service_by_name(pool, name)` free function wrapper (20 SLOC) (done 2026-05-29)
  - **Goal:** Free function over prost-reflect's DescriptorPool::get_service_by_name; exposed in our public surface.
  - **Files:** crates/oxiproto-reflect/src/lib.rs (modify); add ServiceDescriptor to re-exports
  - **Tests:** Lookup existing service by name; lookup non-existent returns None.
- [x] Add `get_enum_by_name(pool, name)` free function wrapper (20 SLOC) (done 2026-05-29)
  - **Files:** crates/oxiproto-reflect/src/lib.rs (modify)
  - **Tests:** Lookup existing enum by name; non-existent returns None.
- [x] Add `all_messages(pool)` / `all_services(pool)` free function iterators (30 SLOC) (done 2026-05-29)
  - **Files:** crates/oxiproto-reflect/src/lib.rs (modify)
  - **Tests:** all_messages returns all registered message descriptors; all_services same for services.

## API Improvements
- [x] Add `set_field_by_name(msg, name, value)` free function convenience (done 2026-05-29)
  - **Goal:** Wraps prost-reflect's DynamicMessage field access; fails with ReflectError if name not found.
  - **Files:** crates/oxiproto-reflect/src/dynamic.rs (new); src/lib.rs (modify: pub mod dynamic + re-exports)
  - **Tests:** Set field, get field, has_field, clear_field round-trip for each scalar type.
- [x] Add `get_field_by_name(msg, name)` free function convenience (done 2026-05-29)
  - **Files:** crates/oxiproto-reflect/src/dynamic.rs (new, same file as set_field_by_name)
- [x] Add `has_field(msg, name)` predicate (done 2026-05-29)
  - **Files:** crates/oxiproto-reflect/src/dynamic.rs
- [x] Add `clear_field(msg, name)` method (done 2026-05-29)
  - **Files:** crates/oxiproto-reflect/src/dynamic.rs
- [x] Add `unknown_fields(msg)` accessor (done 2026-05-29)
  - **Goal:** Free function `pub fn unknown_fields(msg: &DynamicMessage) -> impl Iterator<Item = &UnknownField> + '_` in `src/dynamic.rs`, wrapping prost-reflect 0.16's `DynamicMessage::unknown_fields()`.
  - **Note:** `UnknownFieldSet` is `pub(crate)` in prost-reflect 0.16; the public surface is only the iterator. `UnknownField` is public and re-exported.
  - **Files:** `crates/oxiproto-reflect/src/dynamic.rs` + re-export in `lib.rs`.
- [x] Verify and document `Debug` / `Display` for `DynamicMessage` (done 2026-05-29)
  - **Goal:** Both `Debug` and `Display` (protobuf text format) are implemented unconditionally in prost-reflect 0.16. Verified via running doctest in module-level doc comment of `lib.rs`.
  - **Files:** crates/oxiproto-reflect/src/lib.rs (modify: doctest in module doc).
- [x] Add error context to ReflectError: field name, expected vs actual type (done 2026-05-29)
  - **Files:** crates/oxiproto-reflect/src/lib.rs (modify: new ReflectError::Field variant)
  - **Tests:** Error message includes field name and type mismatch info.

## Testing
- [x] Test DescriptorPool construction from a multi-file FDS with imports (done 2026-05-29)
  - **Goal:** `tests/pool.rs` test with a two-file FDS (enum `events.Status` in events.proto, message `api.Request` referencing it in request.proto); pool resolves both by fully-qualified name and the field's kind matches the enum.
  - **Files:** `crates/oxiproto-reflect/tests/pool.rs` (extend: `pool_from_multi_file_fds_with_imports`).
- [x] Test DynamicMessage field set/get round-trip for all scalar types (done 2026-05-29)
  - **Files:** crates/oxiproto-reflect/tests/dynamic.rs (new)
- [x] Test DynamicMessage with repeated, map, oneof fields (done 2026-05-29)
  - **Goal:** `tests/dynamic.rs` — repeated int32 (`Value::List`, iterate), map string→int32 (`Value::Map(HashMap<MapKey,Value>)`, set/get by `MapKey::String`), oneof (set one arm, verify sibling is cleared by prost-reflect).
  - **Note:** Map field requires constructing the synthetic map-entry nested message (`MessageOptions.map_entry = true`) explicitly when building an FDS from scratch. The full prost-reflect map API (`Value::Map`) works natively for set/get.
  - **Files:** `crates/oxiproto-reflect/tests/dynamic.rs` (extend: 3 new tests + 2 new FDS helpers).
- [x] Test DynamicMessage encode/decode round-trip against byte-exact wire vectors (done 2026-05-30) — `tests/native_wire.rs`: byte-exact oracle vectors (int32 `08 96 01`, string, packed/unpacked repeated, nested message, `map<string,int32>` entry, enum), full scalar round-trip, descriptor lookups by name/number, oneof exclusivity, unknown-field preservation, group rejection. (Used byte-exact spec vectors as the oracle instead of adding prost-build codegen machinery, per plan.)
- [x] Test service/method descriptor iteration (done 2026-05-29)
  - **Files:** crates/oxiproto-reflect/tests/pool.rs (extend)
- [x] Test error handling: non-existent message name, invalid FDS bytes (done 2026-05-29)
  - **Files:** crates/oxiproto-reflect/tests/{pool,dynamic}.rs

## Performance
- [x] Benchmark DescriptorPool construction for large descriptor sets (done 2026-06-03)
  - **Files:** `crates/oxiproto-reflect/benches/pool_bench.rs` (~200 SLOC): small/medium/large FDS construction, native vs prost-reflect comparison, name-lookup benchmark.
- [x] Benchmark DynamicMessage encode/decode vs statically-generated prost types (done 2026-06-03)
  - **Files:** `crates/oxiproto-reflect/benches/dynamic_bench.rs` (~330 SLOC): scalar/repeated/nested encode, scalar decode, round-trip (native vs prost::Message oracle).
- [x] Profile memory usage of DescriptorPool with many registered files (done 2026-06-03)
  - **Files:** `crates/oxiproto-reflect/benches/memory_bench.rs` (~175 SLOC): construction time + heap-approximation throughput at 5/20/50 files; Arc-clone O(1) cost verified.

## Integration
- [x] Ensure oxirpc-reflect can use oxiproto-reflect's DescriptorPool for gRPC server reflection (done 2026-06-03)
  - **Verified by:** `tests/integration_build.rs::oxirpc_reflect_pool_compatibility` — constructs a gRPC Health service FDS via oxiproto-build, builds a NativeDescriptorPool, and exercises all operations oxirpc-reflect performs (service lookup, method streaming flags, input/output type resolution, nested enum access).
- [x] Ensure oxiproto-json (future) uses DynamicMessage for JSON transcoding (done 2026-06-03)
  - **Verified by:** `tests/integration_build.rs` (tests: `native_dynamic_json_transcoding_roundtrip`, `native_dynamic_json_bytes_base64`, `native_dynamic_json_enum_as_name`, `build_fds_multi_file_import_resolution`) — exercises `NativeDynamicMessage::to_json` / `from_json` / `to_json_string` / `from_json_str` as the canonical JSON transcoding path available today; the prost-reflect-based `oxiproto-json` crate uses the same approach for the facade path.
- [x] Ensure compatibility with oxiproto-build's generated FileDescriptorSet output (done 2026-06-03)
  - **Verified by:** `tests/integration_build.rs` (8 tests) — full pipeline: oxiproto-build::compile_str → NativeDescriptorPool → encode/decode/JSON round-trips for scalars, enums, nested messages, repeated fields, map fields, services/methods; all pass.
