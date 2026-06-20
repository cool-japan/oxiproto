# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.3] - 2026-06-19

### Changed
- All workspace crates bumped from `0.1.2` to `0.1.3`.

---

## [0.1.2] - 2026-06-10

### Added
- **`oxiproto-json` WKT encode/decode (full proto3 JSON spec compliance):**
  - `google.protobuf.FieldMask` — encode paths as comma-separated camelCase string; decode back to snake_case path list via `camel_to_snake` helper
  - `google.protobuf.Value` — encode/decode all `kind` variants: `null_value`, `bool_value`, `number_value`, `string_value`, `struct_value`, `list_value`
  - `google.protobuf.ListValue` — encode/decode as JSON array of `Value` items
  - `google.protobuf.Struct` — encode/decode as JSON object with string keys and `Value` entries
  - `google.protobuf.Any` — encode/decode with `@type` URL field and nested message body
- **Float/double NaN and Infinity** — `from_json` now accepts `"NaN"`, `"Infinity"`, `"-Infinity"` strings for `float` and `double` fields per proto3 JSON spec
- `wkt_json.rs` integration test suite (565 lines, 47 test cases) covering Inf/NaN decode, FieldMask round-trips, Struct/Value/ListValue encoding, and Any encode/decode
- `camel_to_snake` conversion helper in `oxiproto-json::from_json` (inverse of existing `snake_to_camel`)

### Changed
- `to_json` and `from_json` doc-comments updated: removed "Deferred" notes, replaced with complete WKT support summary
- Benchmark files (`oxiproto-build/benches/parse.rs`, `oxiproto-cli/benches/startup.rs`, `oxiproto-codegen/benches/codegen.rs`) migrated from deprecated `criterion::black_box` to `std::hint::black_box`
- Workspace dependencies unified to use workspace references (`criterion.workspace = true` etc.)
- All workspace crates bumped from `0.1.1` to `0.1.2`

## [0.1.1] - 2026-06-04

### Added
- `Builder::incremental(cache_path)` — enable incremental compilation in `oxiproto-build` using a FNV-1a 64-bit fingerprint cache; skips codegen entirely when all input `.proto` files are unchanged
- `Builder::native_impl(bool)` — new `native-codegen` feature flag on `oxiproto-build` that emits `OxiMessage` + `OxiName` impl blocks alongside prost-generated code into `*_oxi.rs` files per proto package
- `Edition` type in `oxiproto-build` parser AST — parses `edition = "2023";` statements in `.proto` files; `UnsupportedEdition` and `SyntaxAndEditionConflict` parse errors added
- `Token::Edition` keyword token added to the native parser lexer
- `oxiproto_core::arena` module — `ArenaVec<T>`, `StringPool`, `BytesArena`, and `ArenaDecoder` types for slab-based pre-allocation of repeated protobuf fields, reducing heap fragmentation on hot decode paths
- `oxiproto_core::reflect_bridge` module — bridge between the native `OxiMessage`/`OxiName` traits and `prost_reflect::DynamicMessage` for runtime reflection
- `oxiproto_core::wire::alloc_profile` module — allocation profiling utilities for wire-format encode/decode performance analysis
- `DynamicMessage::to_json` / `to_json_string` / `from_json` / `from_json_str` — canonical proto3 JSON encoding and decoding on `oxiproto-reflect` dynamic messages, including 64-bit integer string encoding, `NaN`/`Infinity` float literals, base64 bytes, and enum name mapping
- Native text-format encode/decode (`oxiproto-reflect`) — `DynamicMessage` text-format serialisation and parsing in a new `native::text` module
- `oxiproto-cli man` subcommand — generates ROFF man pages for all CLI commands via `clap_mangen`, written to a configurable output directory
- `oxiproto::migration` module — documentation-only guide mapping `prost` / `prost-build` APIs to their OxiProto equivalents (derive macros, build scripts, trait table)
- `generate_to_writer` / `generate_to_writer_default` functions in `oxiproto-codegen` — stream generated Rust source into any `std::io::Write` sink without an extra `String` copy
- Criterion benchmark suites for `oxiproto-build` (parse throughput, import resolution, deep chains, diamond graphs, wide fan-out), `oxiproto-codegen`, `oxiproto-reflect` (dynamic dispatch, memory, pool), and `oxiproto-cli` (startup latency)
- `proptest`-based property tests and fuzz corpus tests for `oxiproto-core` wire encoding and `oxiproto-build` parser
- Cross-validation test suite (`prost_cross_validate.rs`) comparing OxiProto wire output byte-for-byte against prost for all scalar and composite field types
- `oxiproto-build` dev-dependency on `oxiproto-codegen` restored (was temporarily removed for publish)
- `proptest`, `criterion`, and `clap_mangen` added to workspace dependencies

### Changed
- `file_syntax_string` helper introduced in `oxiproto-build` descriptor builder: `edition = "2023"` files now emit `"editions"` as the `FileDescriptorProto.syntax` sentinel, matching the protoc wire format
- `is_proto2` detection refactored into `file_is_proto2()` helper used consistently across both `build_file_descriptor_proto` call sites
- All workspace crates bumped from `0.1.0` to `0.1.1`

## [0.1.0] - 2026-06-01

Initial 0.1.0 release.

[0.1.3]: https://github.com/cool-japan/oxiproto/releases/tag/v0.1.3
[0.1.2]: https://github.com/cool-japan/oxiproto/releases/tag/v0.1.2
[0.1.1]: https://github.com/cool-japan/oxiproto/releases/tag/v0.1.1
