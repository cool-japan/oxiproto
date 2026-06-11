# OxiProto Project TODO

## Status
v0.1.3 (work in progress). Functional protobuf toolkit (~42,150 SLOC, 1104 tests).
Native Pure-Rust wire format codec lives in `oxiproto-core::wire`
(varint/zigzag/tag/fixed/length-delimited, DecodeBuffer/EncodeBuffer, UnknownFields).
Native .proto parser (oxiproto-build, `native-parser` feature, now default) handles
proto2+proto3, multi-file import resolution, source_code_info, custom options, group
desugaring. Codegen handles map/oneof/Default/doc-comments/services/JSON/OxiMessage impls.
WKT adds RFC3339, duration strings, Any, FieldMask, Struct, wrappers, chrono/time interop.
CLI gained describe/encode/decode/format/lint/breaking/doc subcommands. oxiproto-json
provides canonical Protobuf-JSON mapping. Zero clippy warnings, zero rustdoc warnings,
no unwrap() in production.

## Milestones

### M0 -- Skeleton (DONE)
- [x] Workspace scaffolding, oxiproto-core re-exporting prost
- [x] deny.toml, Dockerfile.ffi-audit, scripts/ffi-audit.sh

### M1 -- Build helper (DONE)
- [x] oxiproto-build::compile_protos via protox + prost-build (no protoc)

### M2 -- Reflection + WKT (DONE)
- [x] oxiproto-reflect facade over prost-reflect
- [x] oxiproto-wkt with chrono / std::time interop for Timestamp and Duration

### M3 -- Custom codegen (DONE)
- [x] oxiproto-codegen: plain Rust structs/enums from FileDescriptorSet

### M4 -- CLI (DONE)
- [x] oxiproto-cli: gen subcommand for .proto to Rust conversion

## Core Implementation
- [x] Phase 1: Native wire format -- varint, zigzag, field tags, length-delimited, fixed, buffers, unknown fields in oxiproto-core::wire
- [x] Phase 2: Native .proto parser -- lexer + parser + import resolution in oxiproto-build (DONE 2026-05-30)
    - proto3/proto2 full support, multi-file import resolution, source_code_info, group desugaring, COPT preservation, native-parser is now the default.
- [x] Phase 3: Native codegen -- map/oneof/Default/services/docs/OxiMessage/OxiName/OxiOneof/JSON/text impls (DONE 2026-05-29)
  - All traits defined in oxiproto-core; codegen emits impl OxiMessage/OxiName/OxiOneof/Extensions; JSON/text codegen; builder pattern; package namespacing; custom attributes.
- [x] Phase 4: Native reflection -- DescriptorPool, DynamicMessage in oxiproto-reflect (DONE 2026-06-03)
  - NativeDescriptorPool/NativeDynamicMessage with full encode/decode (wire), to_json/from_json, to_text/from_text; FileDescriptor option accessors (java_package, go_package, deprecated, optimize_for); 108 tests green.
- [x] Phase 5: oxiproto-json -- canonical Protobuf-JSON mapping (camelCase, base64 bytes, RFC3339 timestamps) (~600 SLOC)
- [ ] Phase 6: Edition 2023 support (~300 SLOC)
    - **BLOCKED: upstream protobuf Edition 2023 spec not yet finalized; revisit when stable**

## API Improvements
- [x] Unify error handling across all sub-crates (done 2026-05-29)
  - **Goal:** Every sub-crate error type impl From<OxiProtoError> and From<$E> for OxiProtoError. Purely additive — no public API breakage.
  - **Design:** See Slice X in plan. oxiproto-build handles its own BuildError<->OxiProtoError. oxiproto-codegen handles CodegenError<->OxiProtoError. Slice X handles oxiproto-reflect, oxiproto-cli, oxiproto-wkt, oxiproto-json.
  - **Files:** crates/oxiproto-reflect/src/lib.rs; crates/oxiproto-cli/src/main.rs; crates/oxiproto-wkt/src/lib.rs; crates/oxiproto-json/src/lib.rs
  - **Tests:** Smoke test each conversion round-trip preserves message text
  - **Risk:** May need new OxiProtoError variant for generic wrapping; check before adding
- [x] Add no_std support for core wire format (planned 2026-05-29)
  - **Goal:** Make `oxiproto-core` build under `#![no_std]` + `alloc`, embedded-ready. Default stays `std`.
  - **Design:** Add `default=["std"]`, `std=[]`, `alloc=[]` features. `#![cfg_attr(not(feature="std"), no_std)]` + `extern crate alloc`. Mechanical swaps: `std::fmt`->`core::fmt`, `std::str`->`core::str`, `std::slice`->`core::slice`, `std::error::Error`->`core::error::Error`, BTreeMap->`alloc::collections`. Gate `OxiProtoError::IoError` + prost re-exports behind `#[cfg(feature="std")]`. Validated by running `cargo build -p oxiproto-core --no-default-features --features alloc`.
  - **Files:** `crates/oxiproto-core/Cargo.toml`, `src/lib.rs`, `src/wire/*.rs`, `src/message.rs`, `src/name.rs`, `src/oneof.rs`, `src/extensions.rs`, `tests/no_std_smoke.rs` (new)
  - **Tests:** Existing tests pass under `std`. no_std smoke test. Validation build MUST succeed.
  - **Risk:** `prost-types` may pull std; gate the three re-exports behind `std` if so.
- [x] Add compile_str for inline proto definitions (planned 2026-05-29)
  - **Goal:** oxiproto_build::compile_str(proto_source: &str) -> Result<FileDescriptorSet, BuildError>; writes to temp_dir, calls protox::compile, cleans up.
  - **Design:** See Slice B in plan. Uses std::env::temp_dir() per CLAUDE.md testing guidelines. Atomic counter for temp filename uniqueness.
  - **Files:** crates/oxiproto-build/src/compile_str.rs (new); crates/oxiproto-build/tests/compile_str.rs (new)
  - **Tests:** Inline proto produces working FDS; cleanup verified; broken proto produces BuildError::Parse with file:line:col
  - **Risk:** temp_dir cleanup on panic — use RAII guard
- [x] Add CLI subcommands: describe, encode, decode, format, lint, breaking, doc all done (DONE 2026-05-30)
  - All subcommands complete: gen, describe, encode, decode, format, lint, breaking, doc.
  - All flags complete: --dry-run, --json, --grpc, --recursive, --prost-compat, --quiet/--verbose.
  - Shell completions via clap_complete; colored output via anstyle; filename derivation improved.

## Testing
- [x] Conformance test suite against canonical protobuf implementations
  - `crates/oxiproto/tests/conformance.rs`: 11 sections, 38 tests; all encoding guide vectors, wire types, OxiMessage conformance (DONE 2026-06-03)
- [x] Cross-validate native wire format against prost for correctness
  - `crates/oxiproto-core/tests/prost_cross_validate.rs`: all scalar types + repeated + nested; byte-for-byte equality (DONE 2026-06-03)
- [x] Fuzz all parsers (.proto parser, wire format decoder)
  - `crates/oxiproto-core/tests/fuzz_corpus.rs`: deterministic corpus + proptest mutation (bit-flip, truncation, prepend/append); no cargo-fuzz/libFuzzer (Pure Rust) (DONE 2026-06-03)
- [x] Property-based testing for encode/decode round-trips
  - `crates/oxiproto-core/tests/proptest_message.rs`: OxiMessage-level proptest for all field types, idempotency, clear, merge (DONE 2026-06-03)

## Performance
- [x] Benchmark native vs prost encode/decode throughput (planned 2026-05-29)
  - **Goal:** Greenfield criterion harness measuring native wire codec + OxiMessage vs prost (no benches exist today).
  - **Design:** `benches/wire.rs` (varint/zigzag/fixed/length-delimited vs prost equivalents); `benches/message.rs` (OxiMessage encode/decode vs prost::Message, byte-equal payloads verified before timing). criterion (latest) dev-dep, `[[bench]] harness = false`.
  - **Files:** `crates/oxiproto-core/benches/wire.rs` (new ~140 SLOC), `benches/message.rs` (new ~160 SLOC), `Cargo.toml` (criterion dev-dep + bench entries)
  - **Tests:** `cargo bench -p oxiproto-core --no-run` compiles all benches (acceptance gate). clippy clean.
  - **Risk:** Low; sequenced after NS stabilises Cargo.toml.
- [x] Benchmark native .proto parsing vs protox
    - bench added at crates/oxiproto-build/benches/parse_bench.rs
- [x] Profile and optimize hot paths in wire format codec
    - varint encode/decode throughput: crates/oxiproto-core/benches/wire.rs
    - full message encode/decode baseline: crates/oxiproto-core/benches/message.rs

## Integration
- [x] Ensure oxirpc uses oxiproto for all proto operations
    - oxirpc-build delegates to oxiproto-build::compile_to_fds; confirmed 2026-06-03
- [ ] Coordinate with SciRS2 for model serialization formats
    - **DEFERRED: cross-project; tracked in SciRS2 backlog**
- [x] Document migration path from prost ecosystem to oxiproto
  - `crates/oxiproto/src/migration.rs`: rustdoc-only module with 10 sections: Cargo.toml, build.rs, trait table, derive→impl, WKT, reflection, errors, interop, JSON, no_std (DONE 2026-06-03)

## Open Questions
1. Should OxiRPC absorb OxiProto, or remain a separate consumer?
2. Do we need oxiproto-grpc-codegen, or does gRPC stub emission belong in OxiRPC?
3. Edition 2023 commitment timeline -- wait for protobuf working group stability?
4. Validator integration (buf.validate) -- v0.2+ decision

## Proposed follow-ups

- **Phase 2 body parser** (requires Phase 2 lexer/outline from this run): message/enum/service body parsing, import resolution semantics, source code info, FileDescriptorSet construction (~1500 SLOC; split into 3 follow-up /ultra runs).
- **no_std support for oxiproto-core::wire**: add default=["std"] + alloc feature, gate std::* usage, validate with cargo build --no-default-features --features alloc. Requires CI pipeline first.
- **Phase 6 Edition 2023**: blocked on upstream protobuf working group stability.
- **oxiproto-validate crate**: blocked on Open Question #4 decision.
- **Benchmarks** (criterion harness across all crates): independent follow-up.
- **Conformance test suite** against Google canonical protobuf implementations: independent follow-up.
- **README.md refresh**: update "M0 skeleton" status to reflect M0-M5 + Phase 1 done, Phase 3/CLI partial. Run /readme after next /ultra.
- **Phase 2 remainder** (follow-up /ultra): import resolution (include-path search, public/weak), proto2 syntax, `source_code_info`, full/custom option values, Edition 2023, making `native-parser` the default (replace protox). Several follow-up runs.
- **Native canonical JSON codegen** (follow-up /ultra): emit self-contained `to_json`/`from_json` on generated types. Deferred because oxiproto-json is DynamicMessage-based; native structs need their own field-by-field JSON codec with camelCase, enum-as-name, 64-bit-as-string, base64, NaN/Inf, WKT special-casing. Own ~400-SLOC run; wires the dead `--json` CLI flag then.
- **Phase 4 native reflection** (follow-up /ultra, depends on Phase 2 completion): DescriptorPool / DynamicMessage rewrite.
- **Fuzz harness** (follow-up /ultra): prefer pure-Rust `arbitrary`/proptest no-panic harness for varint + lexer (cargo-fuzz uses libFuzzer which is C++, violating Pure-Rust Policy).
- **OxiMessage → Message alias cutover** (follow-up /ultra): 4 trait-level blockers (wkt/any_ext, reflect/lib, cli/convert, build/builder) all tied to prost-derived/DynamicMessage; safe only after all consumers migrate off prost::Message.
- **Benchmark native .proto parsing vs protox** (follow-up /ultra, depends on Phase 2 native-parser being default).
- **Custom/extension option values** (follow-up /ultra): applying `unknown`/extension-typed option values (protobuf message-typed custom options) to descriptors. Currently only well-known options applied.
- **protox replacement** (follow-up /ultra): rewire `Builder::compile`, `compile_str`, `compile_protos` off protox once source_code_info, proto2, and custom options close the fidelity gap.
