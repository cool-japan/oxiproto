# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.1]: https://github.com/cool-japan/oxiproto/releases/tag/v0.1.1
