# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.4] - 2026-07-27

### Added
- **`oxiproto-protoc`** — new `protoc`-argv-compatible binary (`oxiproto-cli/src/bin/oxiproto-protoc.rs`) implementing the descriptor-set-generation subset of `protoc` (`-I`/`-o`/`--include_imports`/`--include_source_info`) backed by OxiProto's pure-Rust parser. Pointing the `PROTOC` environment variable at it lets third-party build scripts (`prost-build`, `tonic-build`, and anything built on them) compile `.proto` sources without a C++ `protoc` installation.
- New `oxiproto-examples` workspace member (`examples/`) with three runnable examples: `encode_decode` (hand-written `OxiMessage` impl and wire-format round-trip), `reflection` (`DynamicMessage` / `DescriptorPool` usage), and `codegen_usage` (in-process `.proto` → Rust codegen)
- `DecodeBuffer::nested()` / `DecodeBuffer::depth()` and `oxiproto_core::wire::MAX_DECODE_DEPTH` — public API for the shared decode recursion-depth budget
- `oxiproto-core/tests/fuzz_message_decode.rs` — message-level property/fuzz suite covering arbitrary-bytes-never-panics, encode/decode round-trips, a seeded bit-flip mutation sweep, and the recursion-limit regression
- `CliError` typed error enum (`oxiproto-cli/src/error.rs`) covering every `oxiproto-cli` subcommand failure cause
- `CONTRIBUTING.md` and `SECURITY.md` project governance docs

### Changed
- All `oxiproto-cli` subcommands (`gen`, `describe`, `doc`, `format`, `lint`, `man`, `breaking`, `convert`) now return the typed `CliError` instead of `Box<dyn std::error::Error>`
- `oxiproto-codegen` and `oxiproto` crate build scripts no longer shell out to `protoc` via `prost-build::compile_protos`: `oxiproto-codegen/build.rs` now parses its test fixtures with `protox` and generates via `compile_fds`, and `oxiproto/build.rs` now compiles via `oxiproto-build::Builder` directly, removing the workspace's last internal dependency on a C++ `protoc` executable
- Dependencies updated to latest: `prost` / `prost-build` / `prost-types` 0.14.3 → 0.14.4, `prost-reflect` 0.16.4 → 0.16.5, `chrono` 0.4.44 → 0.4.45, `time` 0.3.47 → 0.3.54, `clap` 4.6.1 → 4.6.4, `proptest` 1.6 → 1.11, `base64` 0.22 → 0.23, `syn` 2 → 3, `prettyplease` 0.2 → 0.3 (the latter three are major-version bumps; the workspace was verified to build, lint, and test clean against them)
- Every publishable crate's `Cargo.toml` now declares `readme = "README.md"` so crates.io renders each crate's existing README
- All workspace crates bumped from `0.1.3` to `0.1.4`

### Fixed
- `oxiproto-cli --version` / `-V` now works (previously rejected as an unrecognized argument); it reports the CLI's own crate version
- Broken rustdoc intra-doc link in the new `oxiproto-protoc` binary's crate-level docs

### Security
- Fixed unbounded-recursion stack-overflow denial-of-service across all three nested-message decode paths: native reflection (`DynamicMessage::decode` in `oxiproto-reflect`), group-skipping (`DecodeBuffer::skip_field` in `oxiproto-core`), and codegen-generated `OxiMessage::merge` (`oxiproto-codegen`). A maliciously deep nested-message or group payload could previously exhaust the stack before ever reaching application code. All three paths now descend through the shared `DecodeBuffer::nested()` depth budget (`MAX_DECODE_DEPTH = 100`, matching the `protobuf`/`prost` norm) and return `WireError::RecursionLimitExceeded` once exceeded.

---

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

[0.1.4]: https://github.com/cool-japan/oxiproto/releases/tag/v0.1.4
[0.1.3]: https://github.com/cool-japan/oxiproto/releases/tag/v0.1.3
[0.1.2]: https://github.com/cool-japan/oxiproto/releases/tag/v0.1.2
[0.1.1]: https://github.com/cool-japan/oxiproto/releases/tag/v0.1.1
