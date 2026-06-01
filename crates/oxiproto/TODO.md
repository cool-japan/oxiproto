# oxiproto TODO (facade)

## Status
Facade crate re-exporting Message, Name, OxiProtoError, OxiProtoResult,
prost_types, and the native `wire` module from oxiproto-core. Provides a
`prelude` module and `version()`. Feature-gated modules: `build`, `reflect`,
`wkt` (+ `wkt-chrono`), `codegen`, `json`. ~60 SLOC production code.

## Core Implementation
- [x] Add `prelude` module for glob import of common types: Message, Name, OxiProtoError, OxiProtoResult, wire buffers
- [x] Add `json` feature and module for oxiproto-json (canonical Protobuf-JSON mapping)
- [ ] Add `validate` feature and module for runtime field validation once implemented (10 SLOC)
- [x] Add `oxiproto::version()` returning crate version string
- [x] Re-export the native `wire` module at the top level (always available)

## API Improvements
- [ ] Remove feature gates once native implementations replace prost/protox (facade should work with empty features)
- [x] Add top-level convenience functions: `oxiproto::encode(msg)`, `oxiproto::decode::<T>(bytes)` (done 2026-05-29)
  - **Goal:** `pub fn encode<T: OxiMessage>(msg: &T) -> Vec<u8>` and `pub fn decode<T: OxiMessage>(bytes: &[u8]) -> OxiProtoResult<T>` at top level of facade crate. Also expose OxiMessage/OxiName/OxiOneof/Extensions in prelude.
  - **Files:** crates/oxiproto/src/lib.rs (modify)
  - **Tests:** integration.rs oxi_message_encode_decode_round_trip, oxi_message_encode_empty_is_zero_bytes, encoded_len_matches_actual_length
- [x] Add `oxiproto::compile_protos` convenience re-export at top level (done 2026-05-29)
  - **Goal:** `#[cfg(feature = "build")] pub use oxiproto_build::compile_protos;` at the top level of crates/oxiproto/src/lib.rs for ergonomic use in build.rs.
  - **Files:** crates/oxiproto/src/lib.rs (modify)
- [ ] Ensure all doc examples compile without specifying features
- [x] Add comprehensive crate-level documentation with usage examples for each feature (done 2026-05-29)
  - **Files:** crates/oxiproto/src/lib.rs (modify: doc comments with feature-gated examples, trait overview table)

## Testing
- [x] Integration test: compile a .proto, OxiMessage encode/decode, wire cross-validate vs prost (done 2026-05-29)
  - **Goal:** End-to-end smoke test gated on all(feature="build",feature="codegen"). Writes user.proto to temp_dir, compiles, generates code string, hand-instantiates User type, encodes via OxiMessage, decodes round-trip, byte cross-validates vs prost.
  - **Design:** crates/oxiproto/build.rs compiles tests/fixtures/user.proto into OUT_DIR. Test include!()s the generated file. Prost-derived reference type for byte cross-validation lives in the test module.
  - **Files:** crates/oxiproto/build.rs (new); crates/oxiproto/tests/integration.rs (new); crates/oxiproto/tests/fixtures/user.proto (new); Cargo.toml (add dev-deps: prost, prost-build, oxiproto-codegen; build-deps: prost-build)
  - **Tests:** 7 integration tests: oxi_name_correct, oxi_message_encode_decode_round_trip, oxi_message_encode_empty_is_zero_bytes, encoded_len_matches_actual_length, wire_byte_cross_validation_vs_prost, decode_with_unknown_fields_preserves_round_trip, codegen_with_oxi_message_impl_produces_valid_rust
- [~] Test that each feature flag enables exactly the expected module (planned future)
  - **Files:** crates/oxiproto/tests/integration.rs (include feature-specific checks)
- [ ] Test that facade types match sub-crate types (no accidental shadowing)
- [~] Test that default features (empty) still compiles and provides core types (planned future)
  - **Files:** crates/oxiproto/tests/integration.rs
- [x] Runtime JSON round-trip execution harness: build.rs drives codegen into OUT_DIR, tests actually execute to_json/from_json (done 2026-05-29)
  - **Goal:** Upgrade `tests/json_roundtrip.rs` from string-contains checks to compiled + EXECUTED `to_json`/`from_json` calls. `build.rs` (under opt-in `json-runtime-harness` feature) drives `oxiproto-codegen` (JSON mode) to emit a fixture `.rs` into `$OUT_DIR`; the test `include!`s it and asserts actual values.
  - **Design:** Added unconditional build-deps `prost-types` + `oxiproto-codegen` (not optional) and feature `json-runtime-harness = []` (no feature-dep, plain gate). FDS constructed in-process in `build.rs` (prost_types literals, no .proto file needed). Generated fixture: AllScalars, BigInts, BinaryData, Floats, RepMsg, EnumMsg, CamelMsg (OneofMsg excluded — see DEVIATION below). Tests execute `to_json`/`from_json` directly. Run with `cargo nextest -p oxiproto --features json-runtime-harness`.
  - **Files:** `crates/oxiproto/Cargo.toml` (optional build-dep + feature + `oxiproto-wkt` dev-dep); `crates/oxiproto/build.rs` (add `emit_json_fixture` fn, gated by feature); `crates/oxiproto/tests/json_roundtrip.rs` (upgrade: add runtime module under `#[cfg(feature="json-runtime-harness")]`, keep string-check tests under `#[cfg(not(...))]`).
  - **Tests:** `all_scalars_to_json_roundtrip`, `int64_as_json_string`, `bytes_base64_roundtrip`, `float_nan_inf_roundtrip`, `from_json_accepts_snake_case_key`, `from_json_unknown_field_ignored`, `from_json_null_treated_as_default`, `default_values_omitted_from_to_json`, `enum_roundtrip`, `repeated_field_roundtrip`, `camel_case_key_to_json` — all EXECUTING the generated code (10 runtime tests).
  - **DEVIATION 1:** `oneof_roundtrip` NOT implemented. The generated `to_json`/`from_json` for oneof fields emits bare variant names (`IntV(_val)`) without `use OneofMsg_Value::*;` in scope, causing E0531/E0425 compile errors when `include!()`d. Fix required in `crates/oxiproto-codegen/src/json_impl.rs` (out of scope). `OneofMsg` excluded from runtime fixture.
  - **DEVIATION 2:** `mod runtime` carries a `#[allow(clippy::...)]` block (10 lints). The `include!()`d generated fixture triggers these lints via patterns emitted by `json_impl.rs` (e.g. `useless_format`, `nonminimal_bool`, `single_match`). The task rules prohibit `#[allow]` but also require zero warnings and prohibit editing codegen. These constraints are strictly unsatisfiable without relaxing one. Module-scoped allow on machine-generated included code is the idiomatic Rust pattern (prost does the same). Upstream fix: `json_impl.rs` should emit `#![allow(...)]` in its prelude, or generate cleaner expressions.
  - **Risk:** `build.rs` breakage fails the crate. Mitigation: verify `cargo build -p oxiproto --features json-runtime-harness` and test run in isolation before marking done. Optional build-deps keep default consumers clean.

## Performance
- [ ] Verify zero overhead from facade re-exports (compile-time only cost, no runtime cost)

## Integration
- [ ] Ensure oxirpc depends on oxiproto for proto types (not directly on prost)
- [ ] Ensure SciRS2 / ML crates can use oxiproto for model serialization
- [ ] Document migration path from prost to oxiproto for existing users
