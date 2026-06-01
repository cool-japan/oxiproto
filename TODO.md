# OxiProto Project TODO

## Status
v0.1.0 released 2026-06-01. Functional protobuf toolkit (~28,800 SLOC, 679 tests).
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
- [~] Phase 2: Native .proto parser -- lexer + parser + import resolution in oxiproto-build (~2000 SLOC)
    - **Refinement (2026-05-29):** Phase 2 body parser is now scoped: Slice P1 lands the proto3 AST + recursive-descent body parser (`parser/ast.rs` + `parser/parse.rs`); Slice P2 lands name resolution + FDS construction for single-file proto3 behind `native-parser` opt-in feature. Import resolution, proto2, source_code_info, full option values deferred.
    - **Refinement (2026-05-29, run 3):** Multi-file import resolution now in progress: include-path file resolver, two-set-DFS recursive loader, cross-file symbol table with public-import transitivity, WKT via `prost_reflect::DescriptorPool::global()`, `build_fds_multi` for topological FDS assembly, `compile_files_native`. `compile_str_native` stays single-file (rejects imports). protox replacement (Builder/compile_str rewire, source_code_info, proto2, custom options) still pending.
        - **Refinement (2026-05-29, run 4):** proto2 syntax support now in progress (Slice P2): `required` label, proto2 `optional` (no synthetic oneof), `extend`/`extensions`/`extension_range`, `default_value` for scalars. `source_code_info` deferred (needs comment capture in lexer/AST first — structural-only is rework).
        - **Refinement (2026-05-29, run 5):** source_code_info generation now in progress (Slice SCI): comment capture via pre-tokenize side-table, LineTable for O(log n) span conversion, descriptor builder populates Location entries with protobuf path + 0-based span + leading/trailing/detached comments. This is the last native→protox fidelity gap before Builder/compile_str rewire.
        - **Refinement (2026-05-30):** Run 7 in progress — closing two remaining fidelity gaps before the default flip: (a) COPT: scalar custom/extension options silently dropped in native → match protox behavior (probe-first); (b) GRP: proto2 group fields hard-error in native → implement full desugaring. RTE: wire `compile_str`/`Builder::compile`/`Builder::compile_to_fds` to route through native under `--features native-parser`, prove both feature matrices green. `default` flip (FLIP = step b) held for a separate approved step after this run demonstrates green.
        - **Refinement (2026-05-30, FLIP):** native-parser is now the default. All consumers (`oxiproto` facade, `oxiproto-codegen` dev-dep, `oxiproto-cli`) route through the native parser without explicit feature specification. protox remains an unconditional dep for the `--no-default-features` fallback path and cross-validation tests.
- [~] Phase 3: Native codegen -- map/oneof/Default/services/docs done; encode/decode impls (native Message trait) still pending (planned 2026-05-29)
  - **Goal:** OxiMessage/OxiName/OxiOneof/Extensions traits defined in oxiproto-core; oxiproto-codegen emits `impl OxiMessage for T` + `impl OxiName for T` blocks using the native wire module. Wire-byte cross-validation against prost proves compatibility.
  - **Design:** See Slice C (message.rs/name.rs/oneof.rs/extensions.rs in oxiproto-core) and Slice CG (message_impl.rs in oxiproto-codegen) in the plan file. New trait names: OxiMessage, OxiName, OxiOneof to avoid breaking the existing `pub use prost::Message` re-export.
  - **Files:** crates/oxiproto-core/src/{message,name,oneof,extensions}.rs; crates/oxiproto-codegen/src/{message_impl,options,wkt_map}.rs; crates/oxiproto-codegen/tests/{message_emit,build}.rs + fixtures/
  - **Prerequisites:** oxiproto-core::wire complete (done)
  - **Tests:** OxiMessage round-trip for every field type; byte cross-validation vs prost; OxiOneof merge semantics; Extensions set/get/clear
  - **Risk:** Trait shape must be stable before codegen emits impls; locked by hand-written test impl in oxiproto-core
- [ ] Phase 4: Native reflection -- DescriptorPool, DynamicMessage in oxiproto-reflect (~1000 SLOC)
- [x] Phase 5: oxiproto-json -- canonical Protobuf-JSON mapping (camelCase, base64 bytes, RFC3339 timestamps) (~600 SLOC)
- [ ] Phase 6: Edition 2023 support (~300 SLOC)

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
- [~] Add CLI subcommands: describe, encode, decode done; format, lint, breaking, doc pending (planned 2026-05-29)
  - **Goal:** New gen flags (--dry-run, --json, --grpc, --recursive), --quiet/--verbose global flags, colored output, shell completions, filename derivation fix. format/lint/breaking/doc subcommands deferred (require native .proto parser body from Phase 2).
  - **Design:** See Slice CLI in plan. anstyle for colors (pure Rust, already in clap dep tree). clap_complete for shell completions. Recursive scan hard-codes exclusion of target/ and .git/. Filename derivation parses `package` declaration with a tiny inline scanner.
  - **Files:** crates/oxiproto-cli/src/{main,gen,util}.rs; crates/oxiproto-cli/Cargo.toml; crates/oxiproto-cli/tests/cli.rs
  - **Tests:** Every new flag has an assert_cmd integration test; completions exits zero; filename derivation 3 branches
  - **Risk:** anstyle is pure Rust — no new C/C++ deps

## Testing
- [ ] Conformance test suite against canonical protobuf implementations
- [ ] Cross-validate native wire format against prost for correctness
- [ ] Fuzz all parsers (.proto parser, wire format decoder)
- [ ] Property-based testing for encode/decode round-trips

## Performance
- [x] Benchmark native vs prost encode/decode throughput (planned 2026-05-29)
  - **Goal:** Greenfield criterion harness measuring native wire codec + OxiMessage vs prost (no benches exist today).
  - **Design:** `benches/wire.rs` (varint/zigzag/fixed/length-delimited vs prost equivalents); `benches/message.rs` (OxiMessage encode/decode vs prost::Message, byte-equal payloads verified before timing). criterion (latest) dev-dep, `[[bench]] harness = false`.
  - **Files:** `crates/oxiproto-core/benches/wire.rs` (new ~140 SLOC), `benches/message.rs` (new ~160 SLOC), `Cargo.toml` (criterion dev-dep + bench entries)
  - **Tests:** `cargo bench -p oxiproto-core --no-run` compiles all benches (acceptance gate). clippy clean.
  - **Risk:** Low; sequenced after NS stabilises Cargo.toml.
- [ ] Benchmark native .proto parsing vs protox
- [ ] Profile and optimize hot paths in wire format codec

## Integration
- [ ] Ensure oxirpc uses oxiproto for all proto operations
- [ ] Coordinate with SciRS2 for model serialization formats
- [ ] Document migration path from prost ecosystem to oxiproto

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
