# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Protobuf Editions now work through the `prost-reflect` facade.** `prost-reflect` 0.16 recognises exactly two `FileDescriptorProto.syntax` values and refuses anything else, so every consumer of `oxiproto_reflect::pool_from_fds` / `pool_from_fds_bytes` — the re-exported `DynamicMessage`, `oxiproto-json`, and the CLI's `encode` / `decode` subcommands — was unusable for an edition schema. (In 0.16.5 it does not even return the `unknown syntax 'editions'` error cleanly: it panics while *formatting* it.) New `oxiproto_reflect::editions` module with `downlevel_editions` / `is_editions_file` / `has_editions_file` rewrites such a file into its **proto2** equivalent, which is the base that preserves `LABEL_REQUIRED` (from `field_presence = LEGACY_REQUIRED`), `TYPE_GROUP` (from `message_encoding = DELIMITED`), a non-zero first enum value, and EXPLICIT presence. Because proto2 defaults a repeated packable scalar to *expanded* while Editions defaults it to PACKED, the transform writes an explicit `FieldOptions.packed` on every repeated packable field that lacks one, taken from the resolved `features.repeated_field_encoding`. Both pool constructors apply it automatically; files that already declare a syntax pass through byte-identically. Known divergence, documented in the module: `features.field_presence = IMPLICIT` has no proto2 expression, so the facade reports presence for such a field (observable only for an explicitly encoded zero, which no conformant encoder writes). Covered by `crates/oxiproto-reflect/tests/editions_facade.rs` (7 tests, including facade-vs-native byte equality) and `crates/oxiproto-cli/tests/editions_convert.rs` (end-to-end `encode`/`decode` over an edition schema).
- **`features.utf8_validation` is enforced at decode time.** It was resolved and materialised but ignored — every `string` was UTF-8 validated regardless. `VERIFY` (the proto3 and Editions default) still rejects an invalid payload; `NONE` (the proto2 baseline, and any edition scope that asks for it) now accepts it into a new typed `native::Value::UnvalidatedString(Vec<u8>)` variant that preserves the bytes verbatim, so a decode → encode round trip is byte-identical. Valid UTF-8 still decodes to `Value::String` under `NONE`, so enabling it does not change the representation of well-formed data. New accessors: `FieldDescriptor::validates_utf8()`, `Value::as_string_bytes()`, `Value::as_str_lossy()`. The text format renders the variant with `\xNN` escapes and reads them back the same way, so `to_text` → `from_text` recovers the exact bytes (a `VERIFY` field's text parser still refuses invalid UTF-8); canonical Protobuf-JSON has no representation for it and returns the new typed `NativeJsonError::NonUtf8String { field }` rather than losing bytes to a replacement character. A `map` key that is not valid UTF-8 is refused with a typed error on both the wire and text paths, since a key has to be usable as text.
- **`features.enum_type` (enum closedness) is enforced at decode time.** Enums were always treated as open. A *closed* enum (`features.enum_type = CLOSED`, and the proto2 baseline) now routes an unrecognised number to the message's unknown-field set — preserved raw, re-emitted on encode, but not readable through the field — exactly as proto2 implementations have always behaved; an *open* enum (proto3 and the Editions default) keeps the raw number so a value added by a newer schema survives. The raw varint as read from the wire is what gets preserved, so a sign-extended negative number round-trips byte-identically. In a packed run the recognised values stay on the field and the rejected ones become individual unpacked varint entries in the unknown set, matching `protoc`'s placement; a `map` entry whose value a closed enum rejects is moved into the unknown-field set whole (length-delimited, byte-identical) rather than being inserted half-decoded. New accessor: `native::EnumDescriptor::is_closed()`. Covered by `crates/oxiproto-reflect/tests/native_features_decode.rs` (16 tests over all three syntaxes).
- **Protobuf Editions (`edition = "2023";`) support — Phase 6.** The native `.proto` pipeline now implements the Editions feature-resolution mechanism end to end instead of accepting the `edition` statement and approximating proto3:
  - New `oxiproto_build::parser::features` module: `FeatureSet` (a fully resolved set), `FeatureOverrides` (what one scope wrote), and the six Edition 2023 features — `field_presence`, `enum_type`, `repeated_field_encoding`, `utf8_validation`, `message_encoding`, `json_format` — each with the spec's edition/proto2/proto3 baselines. Resolution inherits file → message → nested message → oneof → field, and enum → enum value.
  - Removed-construct enforcement: `optional`, `required`, and `group` are rejected in an edition file (`ParseError::EditionSyntaxNotAllowed`), `features.*` is rejected outside one (`ParseError::FeaturesRequireEdition`), unknown feature names/values are typed errors (`ParseError::UnknownFeature` / `InvalidFeatureValue`), and features set where they cannot apply (presence on a repeated field, `LEGACY_REQUIRED` in a oneof, `DELIMITED` on a scalar, packing on a non-repeated field) return `ParseError::FeatureNotApplicable`.
  - Descriptor materialisation: `field_presence = LEGACY_REQUIRED` becomes `LABEL_REQUIRED`; `repeated_field_encoding` becomes an explicit `FieldOptions.packed`; `message_encoding = DELIMITED` becomes `TYPE_GROUP`. Because `prost-types` 0.14 still models the pre-Editions `descriptor.proto` (no `edition` field, no `options.features`), the *fully resolved* feature set is additionally preserved as `features.<name>` `uninterpreted_option` entries on file/message/field/enum options, so the resolution result survives the `FileDescriptorSet` boundary at file, message, field, enum and enum-value scope.
  - Reflection: `FieldDescriptor::has_presence()` (new) reports presence from proto2/proto3/edition semantics; `is_packed()` honours the edition default and the resolved feature.
  - Codegen: generated `encoded_len`/`encode_raw` honour an explicit `packed = false` (so `EXPANDED` really is expanded), and `TYPE_GROUP` fields generate a real nested message with start/end-group framing.
  - `Builder::compile` routes an edition file to `oxiproto-codegen` instead of prost-build, whose code generator panics on the `syntax = "editions"` sentinel (`unknown syntax: editions`). Without the `native-codegen` feature it now returns a typed `BuildError::Codegen` naming the requirement instead of letting a dependency abort the build script.
  - Tests: `crates/oxiproto-build/tests/editions.rs` (17 tests), `crates/oxiproto-reflect/tests/native_editions.rs` (7 wire-byte tests), `crates/oxiproto/tests/editions_codegen.rs` (6 tests over generated code).
- Native reflection (`oxiproto-reflect`) now encodes/decodes proto2 `group` fields end-to-end instead of rejecting them: `DecodeBuffer::read_group_body` (`oxiproto-core`) extracts a group's body given its already-consumed start-group tag, `WireError::MalformedGroup` reports an unterminated or mismatched end-group tag, and `UnknownFields::push_group` preserves an unrecognised group byte-identically through a decode → encode round-trip. `Kind::Group` is now handled the same as `Kind::Message` in wire encode/decode, JSON (`native::json`), and text-format (`native::text`) — closing the proto2 interop hole for schemas migrating from other protobuf implementations. Covered by `crates/oxiproto-reflect/tests/native_group.rs`.
- `rustfmt.toml` and `clippy.toml` at the workspace root, matching the other COOLJAPAN projects: `rustfmt.toml` pins `edition = "2021"` (the codebase already conforms to rustfmt's stable defaults); `clippy.toml` pins `msrv = "1.89"` (matching `[workspace.package].rust-version`) without a `disallowed-methods` list, since that lint has no test-vs-production distinction.

### Security
- Bounded the native text-format parser/encoder's message nesting depth: `Parser::parse_message` (and `parse_brace_message` / `parse_angle_message` / `parse_map_entry`) now thread a depth counter capped at `MAX_TEXT_DEPTH = 100`, returning `TextError::RecursionLimitExceeded` instead of recursing further; `encode_message` enforces the same bound when re-serialising an in-memory message. Without this, a text-format payload such as `f{f{f{...}}}` nested deeply enough could overflow the stack before application code ever saw it — the same DoS class the 0.1.4 wire-decode fixes addressed, now closed for `DynamicMessage::from_text`/`to_text` too.
- Bounded the `.proto` parser's own nesting depth: `parse_message`, group-body parsing, and nested option-value message literals in `oxiproto-build` now carry a depth counter and return `ParseError::NestingLimitExceeded` once it's exceeded, so a maliciously deep `.proto` source (`message A{message A{...}}}`) can no longer overflow the parser's stack. This is reachable from every CLI subcommand that reads a user-supplied `.proto` path.

### Changed
- **`native::Value` gained a variant (0.x breaking change).** `Value::UnvalidatedString(Vec<u8>)` is additive, but the enum is not `#[non_exhaustive]`, so a downstream exhaustive `match` on `native::Value` needs a new arm. It can only be produced by a `string` field whose resolved `features.utf8_validation` is `NONE`.
- **`utf8_validation` and `enum_type` now apply to legacy syntaxes too, not only to `edition` files.** The Editions feature model defines the proto2 baseline as `utf8_validation = NONE` and `enum_type = CLOSED`, and a `FileDescriptorProto` with no `syntax` statement *is* proto2. Two decode behaviours therefore change for proto2 and syntax-less descriptor sets: a `string` field holding invalid UTF-8 no longer fails the decode (it lands on `Value::UnvalidatedString`), and an undeclared enum number no longer appears on the field (it moves to the unknown-field set). Both match `protoc`; proto3 is unaffected.
- **Presence-aware serialization (behaviour change).** A field *with* presence — proto2 `optional`, proto3 `optional`, a oneof member, a message field, or an edition field whose resolved `features.field_presence` is `EXPLICIT` — that has been explicitly set to its type default is now emitted by the wire, JSON and text encoders. Previously every such value was dropped, which erased the proto2 distinction between "set to 0" and "unset". Fields *without* presence (proto3 singular, `features.field_presence = IMPLICIT`) are unchanged. Downstream golden-byte or golden-JSON fixtures for proto2 schemas that set a zero value can change as a result. `DynamicMessage::has_field` was moved onto the same predicate so it can never disagree with what the encoders emit.
- **`optional` / `required` / `group` are now errors in an `edition` file (behaviour change).** They previously compiled through a proto3 approximation (`optional` even produced a synthetic oneof). Protobuf Editions removed all three: presence is `features.field_presence` and delimited framing is `features.message_encoding = DELIMITED`. Two tests that encoded the old approximation (`native_fds.rs::edition_2023_optional_gets_synthetic_oneof`, `parse.rs::test_edition_2023_optional_field`) were rewritten to assert the rejection; this is intentional, not a regression.
- `oxiproto_core::arena` module docs corrected: the module no longer claims to be "used by generated code when the `arena` feature is active" — no such Cargo feature exists, and `oxiproto-codegen` does not construct `ArenaVec`/`StringPool`/`BytesArena`/`ArenaDecoder`. The module remains available as a standalone, independently-tested utility; wiring it into codegen is noted as a possible future enhancement rather than a shipped capability.
- Corrected a stale comment in `oxiproto-codegen`'s generated-`merge` emission (`message_impl.rs`) that described the repeated-message decode call as "a placeholder that will require OxiMessage bound" — it is the final emitted code, dispatching through the `OxiMessage` trait.
- `TODO.md`'s "Proposed follow-ups" section pruned: it listed Phase 2 body parsing, `no_std` support, benchmarks, the conformance suite, a README refresh, native JSON codegen, Phase 4 native reflection, and a fuzz harness as still-open, all of which had already shipped (see the corresponding `[x]` entries earlier in the same file).

### Fixed
- **Generated code packed proto2 repeated scalars, diverging from `protoc` on the wire.** `oxiproto-codegen` defaulted `FieldInfo::is_packed` to `true` whenever a repeated packable field carried no explicit `[packed = ...]`, so a proto2 schema generated packed bytes where `protoc` emits the expanded form (one tag per element). Every decoder accepts both, but a peer that re-serialises from its own proto2 schema does not produce the same bytes, which breaks byte-comparison, signing, and deterministic-serialisation use cases. The generator now classifies the file (`FileSyntax::{Proto2, Proto3, Editions}`, with an absent `syntax` statement meaning proto2, as `protoc` records it) and applies the file's default; an explicit `[packed = ...]` — which is also how `oxiproto-build` materialises `features.repeated_field_encoding` — still wins. Decoding stays permissive in both directions. Covered by `crates/oxiproto-codegen/tests/packed_defaults.rs` (9 descriptor-driven tests across the three syntaxes) and byte-level tests in `crates/oxiproto/tests/editions_codegen.rs`.
- **Generated `encoded_len` disagreed with `encode_raw` for an EXPANDED repeated field.** `emit_encoded_len_body_v2` computed the *packed* size formula for every repeated packable field, while `emit_encode_raw_body` honoured `packed = false` — so any expanded field with more than two elements reported a length that did not match the bytes written. Because a nested message's length prefix is computed from `encoded_len`, this corrupted the enclosing frame rather than merely mis-sizing a buffer. (The existing Editions test happened to use a two-element list, where the two formulas coincide.) Found by the new proto2 packing tests.
- **Generated packed-repeated decode read from the wrong buffer.** `oxiproto-codegen` emitted the packed element loop as `while !_pb.is_empty() { … buf.read_*() … }` — reading from the *enclosing message's* buffer while testing the nested packed buffer for emptiness. Decoding any packed repeated scalar therefore consumed the following fields and failed with `UnexpectedEof` (or produced garbage). The same defect applied to map-entry key/value decoding, which read from `buf` instead of the entry buffer `_eb`. `scalar_decode_stmts` now takes the buffer name explicitly. Regression: `crates/oxiproto/tests/editions_codegen.rs::packed_repeated_scalar_decodes_from_the_packed_run_only`.
- **Nested types were referenced by an undefined name in flat codegen layout.** `TypeRegistry::resolve` returned the bare last component (`Inner`) for `.pkg.Outer.Inner`, while `emit_message` emits the flattened `Outer_Inner`, so any field referring to a nested message or enum generated code that did not compile. The registry now tracks package prefixes and returns the flattened name. This is unavoidable for `group` fields, which always synthesise a nested message.
- Removed 7 `.expect()` panic sites from the `.proto` lexer/parser hot path (`oxiproto-build/src/parser/{lexer,outline,comments}.rs`), all "peeked-so-this-cannot-fail" invariants that would previously panic instead of returning a `LexError`/`ParseError` if ever violated by a future refactor. The riskiest of these asserted UTF-8 char-boundary validity via direct string indexing (which panics on a bad boundary before the `.expect()` is even reached); it now decodes via `str::get`, which fails gracefully instead. No behavior change for valid input — all 274 `oxiproto-build` tests (including the lexer's escape-sequence and fuzz suites) pass unchanged.

## [0.1.5] - 2026-07-27

### Changed
- All workspace crates bumped from `0.1.4` to `0.1.5`.

---

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
