# oxiproto-codegen TODO

## Status
Custom code generator: `generate(fds)` / `generate_with_options(fds, opts)` walk
a `FileDescriptorSet` and emit plain Rust with no prost derive macros. Handles
all scalar types, message fields (Option<Box<T>>), repeated (Vec<T>), nested
messages (3+ levels), enums with discriminants + Default + from_i32, map fields
(HashMap/BTreeMap), oneof groups (Rust enums), service traits (unary + all
streaming combos), doc comments (correctly indented), `#[deprecated]`,
package namespacing, reserved fields, custom attributes, WKT mapping,
`impl OxiMessage for T` / `impl OxiName for T` blocks.
~2300 SLOC production code across 4 source files.

## Core Implementation
- [x] Implement map field codegen: `map<K, V>` -> `HashMap<K, V>` or `BTreeMap<K, V>`
- [x] Implement oneof field codegen: generate Rust enum for each oneof group
- [x] Implement `impl OxiMessage for T` block generation (encode/decode, native wire format, no prost)
  - Emit `impl ::oxiproto_core::OxiMessage for $TYPE` + `impl ::oxiproto_core::OxiName for $TYPE` blocks
  - Per-field dispatch via wire module codecs. Uses OxiMessage/OxiName/OxiOneof traits.
  - Unknown fields stored in `_unknown: wire::UnknownFields` added to every generated struct.
  - merge() dispatches on (field_number, wire_type); unknown tags forwarded to _unknown.
  - clear() resets all fields to Default.
  - Files: src/message_impl.rs (new ~1357 SLOC)
- [x] Implement builder pattern generation for each message (optional, feature-gated) (150-200 SLOC) (done 2026-05-29)
  - **Goal:** FooBuilder struct with fluent setters per field and build() -> Foo. Feature-gated via CodegenOptions::emit_builder (default false).
  - **Design:** New src/builder_impl.rs; emit_builder_for_message(msg, type_name, opts, file_package, registry) appended to each message's output when emit_builder=true. Oneof-member fields skipped. Repeated → add_X, map → insert_X, scalar/message → by-value setter.
  - **Files:** src/builder_impl.rs (new), src/options.rs (+emit_builder), src/emit.rs (injection point + pub(crate) on helpers), src/lib.rs (mod), tests/emit.rs (+5 tests)
- [x] Implement `Default` trait generation for enums (first value is proto3 default); structs derive Default
- [x] Implement `impl OxiName for T` block generation with full_name/type_url
  - const NAME/PACKAGE + default full_name()/type_url() impls. Part of message_impl.rs.
- [x] Implement service trait generation: `pub trait FooService { fn bar(...) -> ... }` (unary + streaming)
- [x] Implement JSON serialization codegen (camelCase field names, self-contained to_json/from_json) (done 2026-05-29)
  - **Goal:** When `CodegenOptions::emit_json` is true, emit **self-contained** `to_json`/`from_json` on generated message structs and enums implementing the canonical protobuf-JSON mapping. Self-contained = generated code uses `::serde_json` + pure-Rust `::base64` (consumer-provided), NOT oxiproto-json/DynamicMessage. Independent of `emit_oxi_message_impl`.
  - **Design:** New `src/json_impl.rs` emitting per-message `to_json`/`from_json` and per-enum `to_json_str`/`from_json_value`. Canonical rules: camelCase via `field.json_name`; 64-bit→JSON string; NaN/Inf→string; bytes→base64; enums→name; map→object with stringified keys; oneof→set variant inline; default-value omission. Required divergences from oxiproto-json for round-trip: float `from_json` accepts NaN/Inf strings; null→skip; unknown field→skip; accepts camelCase AND snake_case keys. WKT: Timestamp→RFC3339, Duration→decimal-seconds (Struct/Value/Any/FieldMask deferred).
  - **Refinement (2026-05-29):** Architecture changed from "delegate to oxiproto-json" to "self-contained per-field codegen" — oxiproto-json is DynamicMessage-based and cannot serve native structs with no runtime descriptor. Defensive guard: `emit_json && package_namespacing` → `CodegenError` (namespaced layout deferred).
  - **Files:** `src/json_impl.rs` (new ~450 SLOC), `src/emit.rs` (seams at 336-346 + ~613 + file prelude), `src/options.rs` (+emit_json), `src/lib.rs` (mod json_impl), `Cargo.toml` (dev-deps serde_json, base64, oxiproto-wkt), `tests/json_emit.rs` (new).
  - **Tests:** Cheap `syn::parse` + `.contains` tests in codegen; **real round-trip RUN tests** via OUT_DIR+`include!` harness in `oxiproto` facade crate (every scalar, 64-bit-as-string, bytes base64, NaN/Inf, repeated, map, oneof, enum, proto3 optional, Timestamp/Duration, camelCase keys, default-omission, null-skip, unknown-field-skip).
  - **Risk:** Canonical-mapping subtleties; highest-risk edit is `oxiproto` facade build.rs (must verify `cargo test -p oxiproto --features codegen,wkt,json` green in isolation before reporting done).
- [x] Implement text format codegen (proto text format output for debugging) (80-100 SLOC)
- [x] Add well-known type special-casing in codegen (~107 SLOC)
  - WKT detection via wkt_map.rs: google.protobuf.Timestamp/Duration/Any/Empty/FieldMask → oxiproto_wkt::*
  - Wrapper types (StringValue, BoolValue, etc.) → Option<inner_type>
  - Hook in emit.rs::field_type_str_with_wkt
- [x] Add doc comment generation from proto comments (source code info), correctly indented
- [x] Handle package namespacing: generate Rust modules matching proto package structure
  - `package_namespacing: bool` (default false for backward compat)
  - foo.bar package → `pub mod foo { pub mod bar { ... } }`
  - Files: src/emit.rs (emit_package_modules helper), src/options.rs
- [x] Handle reserved fields/names
  - Skip emission of reserved field numbers/names; emit `// reserved field {name_or_number}` comment.
  - Files: src/emit.rs (reserved_numbers, reserved_names helpers)
- [x] Implement `#[deprecated]` attribute generation for deprecated fields/messages/enums/services

## API Improvements
- [x] Add `CodegenOptions`: configure docs / defaults / deprecated / btree_map
  - Extracted to src/options.rs with new fields: package_namespacing, type_attributes,
    field_attributes, emit_oxi_message_impl, format_output, btree_map (+ legacy use_btree_map)
- [x] Add `generate_module(fds)` returning a formatted module tree
  - **Goal:** Returns a structured `ModuleTree` instead of a flat String, enabling per-file output.
  - **Files:** crates/oxiproto-codegen/src/lib.rs (modify)
  - **Refinement (2026-05-29):** Full design: `ModuleTree { name: String, items: Vec<String>, children: Vec<ModuleTree> }` with `render() -> String` and `all_paths() -> Vec<Vec<String>>`. `generate_module(fds, opts)` builds the tree by grouping files by package (reusing `emit_file_content`), inserting content into tree nodes via package segment path. Purely additive — no existing `generate_with_options` callers touched. Being implemented this run.
- [x] Add custom attribute injection per message/field
  - CodegenOptions::type_attributes: BTreeMap<String, Vec<String>> matched by proto FQN
  - CodegenOptions::field_attributes: BTreeMap<String, Vec<String>> matched by "Type.field"
  - Tests: Custom derive attribute appears before the relevant struct/field in generated output.
- [x] Add `prettyplease` integration: auto-format generated code (requires `format` feature)
  - **Goal:** Behind `format` feature: parse emitted string with syn::parse_file, unparse via prettyplease.
  - **Files:** crates/oxiproto-codegen/src/format.rs (new ~50 SLOC, behind format feature)
  - **Tests:** Formatted output parses as valid Rust; idempotent.
    - **Refinement (2026-05-29):** Completing this run: new `src/format.rs` module behind `#[cfg(feature="format")]` with `format_source(src) -> Result<String, CodegenError>` via `syn::parse_file` + `prettyplease::unparse`. Wire into `generate_with_options` when `options.format_output` is set. Also adding `emit_services: bool` (default true) to `CodegenOptions` to gate service-trait emission; `--grpc=false` in CLI wires to this. `syn`+`prettyplease` in Cargo.toml under `format` feature.
- [x] Extract CodegenError to src/options.rs with From conversions to/from OxiProtoError

## Testing
- [x] Test map field codegen: `map<string, int32>` produces `HashMap<String, i32>` (+ BTreeMap option)
- [x] Test oneof codegen: oneof group becomes Rust enum with correct variants
- [x] Test nested message codegen: 3+ levels of nesting
- [x] Test service trait codegen: unary, server-streaming, client-streaming, bidi methods
- [x] Test well-known type mapping: Timestamp → ::oxiproto_wkt::Timestamp
- [x] Test doc comment passthrough from proto source (with correct indentation -> valid Rust)
- [x] Test that generated code compiles (syn parse check on every codegen test)
- [x] Test deterministic output: same FDS always produces identical Rust source
- [x] Test package namespacing: foo.bar package → pub mod foo { pub mod bar { ... } }
- [x] Test reserved fields skipped (replaced by // reserved field comment)
- [x] Test custom type attribute injection (appears before struct)
- [x] Test custom field attribute injection (appears before field)
- [x] Test OxiMessage impl parses (scalars, nested, oneof, map)
- [x] Test OxiName constants correct (NAME, PACKAGE)
- [x] Fixture proto files: tests/fixtures/{scalars,nested,oneof_map,services}.proto
- [x] build.rs: prost-build compiles fixture protos for cross-validation infrastructure

## Planned / In-Progress
- [x] Implement namespaced-layout JSON codegen: FQN→Rust-path type registry, cross-package path resolution, `emit_json=true` with `package_namespacing=true` (done 2026-05-29)
  - **Goal:** Lift the defensive guard (`emit_json && package_namespacing`). Emit correct `to_json`/`from_json` under `package_namespacing=true` using relative `super::`/`crate::` paths. Also fixes the latent bug where cross-package struct-field type references under namespacing emit bare identifiers that don't compile.
  - **Design:** New `src/type_registry.rs` (`TypeRegistry { fqns, package_namespacing }`) with `build(fds, pkg_ns)` + `resolve(from_pkg, target_fqn) → String`. `resolve` under flat layout: `last_component(fqn)`. Under namespacing: compute `super::N * depth_difference + down_path` using common-prefix algorithm. Thread `&TypeRegistry` through `generate_with_options` → `emit_message` → `field_type_str`. JSON emitter (`json_impl.rs`) gains `registry` param; replace all `last_component` cross-type refs. `emit_json_file_prelude` emitted per-package module (not once at file root) under namespacing, so `JsonError`/`_json_type` are always in scope. `use ::base64::Engine as _;` moved into each `to_json` body that uses bytes fields.
  - **Files:** `src/type_registry.rs` (new ~100 SLOC); `src/emit.rs` (thread registry, relax guard at :20-25, per-module prelude, fix `field_type_str`); `src/json_impl.rs` (registry param, resolve cross-type refs, inline base64 use); `src/lib.rs` (build+pass registry); `tests/json_emit.rs` (namespaced tests); `tests/emit.rs` (`TypeRegistry::resolve` unit tests + cross-package field-type test).
  - **Tests:** `type_registry_resolve_depths`, `namespaced_json_cross_package`, `namespaced_json_prelude_per_module`, `namespaced_struct_field_cross_package`, `flat_layout_unchanged`, `package_namespacing_and_emit_json_no_error`. All generated outputs `syn::parse_str`-validated.
  - **Risk:** Path correctness at all depths; Rust-keyword module segments (use `r#` prefix). Mitigated by exhaustive `resolve` unit tests and `syn::parse` on all outputs.

## Performance
- [x] Benchmark codegen speed for large descriptor sets (100+ messages)
  - `benches/codegen.rs`: criterion benchmarks for 10/50/100-message FDS (flat), 20-message OxiMessage/JSON variants, module-tree with 4 packages. `cargo bench -p oxiproto-codegen --no-run` green.
- [x] Profile string allocation patterns in emit functions
  - Benchmarks include `streaming_vs_string` group comparing `generate_with_options` (String-building path) vs `generate_to_writer` (single `write_all` call) for 50-message FDS. Baseline established.
- [x] Consider streaming output (Write trait) instead of building full String in memory
  - Implemented `generate_to_writer<W: Write>(fds, opts, writer) -> Result<(), CodegenError>` and `generate_to_writer_default<W: Write>` in `src/lib.rs`. Writes directly to any `std::io::Write` sink without an extra copy. Tested in `tests/integration.rs`.

## Integration
- [x] Ensure oxiproto-cli uses codegen for its `gen` subcommand (already done)
- [x] Ensure generated code is compatible with oxiproto-core Message trait
  - Integration tests in `tests/integration.rs`: verify `impl OxiMessage` has all four required methods (encoded_len, encode_raw, merge, clear); `impl OxiName` has NAME/PACKAGE constants; structs derive Default (required by OxiMessage bound); `_unknown` field present for unknown-tag forwarding; `from_i32` present on enums for decode compat.
- [x] Ensure generated service traits are compatible with oxirpc server/client stubs
  - Integration tests verify: trait name matches proto service name; method names follow snake_case convention; streaming wrappers use `Vec<T>`; `emit_services=false` suppresses emission; cross-package field types resolve correctly under flat layout.
