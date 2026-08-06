# OxiProto Project TODO

## Status
v0.1.6, 2026-08-06. Functional protobuf toolkit (~43,339 SLOC, 1254 tests all-features).
Native Pure-Rust wire format codec lives in `oxiproto-core::wire`
(varint/zigzag/tag/fixed/length-delimited, DecodeBuffer/EncodeBuffer, UnknownFields).
Native .proto parser (oxiproto-build, `native-parser` feature, now default) handles
proto2+proto3+Editions (`edition = "2023"`, full feature resolution), multi-file import
resolution, source_code_info, custom options, group desugaring. Codegen handles map/oneof/Default/doc-comments/services/JSON/OxiMessage impls.
WKT adds RFC3339, duration strings, Any, FieldMask, Struct, wrappers, chrono/time interop.
CLI gained describe/encode/decode/format/lint/breaking/doc subcommands. oxiproto-json
provides canonical Protobuf-JSON mapping. Zero clippy warnings, zero rustdoc warnings,
no unwrap() in production. oxiproto-cli now also ships oxiproto-protoc, a
protoc-argv-compatible shim so PROTOC-pointing third-party build scripts can skip a C++
protoc install too; examples/ (oxiproto-examples) has three runnable examples covering
encode/decode, reflection, and codegen.

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
- [x] Phase 6: Edition 2023 support (DONE 2026-08-04)
    - `oxiproto-build::parser::features`: `FeatureSet` / `FeatureOverrides` for the six Edition 2023
      features (field_presence, enum_type, repeated_field_encoding, utf8_validation,
      message_encoding, json_format) with edition/proto2/proto3 baselines and file → message →
      nested → oneof → field (and enum → enum value) inheritance.
    - Removed-construct enforcement (`optional`/`required`/`group` rejected in edition files;
      `features.*` rejected outside them) and typed errors for unknown/inapplicable features.
    - Descriptor materialisation: LEGACY_REQUIRED → LABEL_REQUIRED, repeated_field_encoding →
      explicit `packed`, DELIMITED → TYPE_GROUP, plus the resolved feature set preserved as
      `features.<name>` `uninterpreted_option` entries (prost-types 0.14 has no `FeatureSet`).
    - Reflection `FieldDescriptor::has_presence()`; presence-aware wire/JSON/text emission;
      codegen honours explicit `packed` and emits group framing for TYPE_GROUP.
- [x] Phase 6b: Editions deferral recovery (DONE 2026-08-04)
    - **Wire-compat fix:** codegen no longer packs a proto2 repeated packable scalar by
      default (`FileSyntax` classification; proto2 expanded, proto3/Editions packed, explicit
      `[packed = ...]` wins) — matching `protoc`. Also fixed generated `encoded_len`, which
      computed the packed size formula for EXPANDED fields and so disagreed with `encode_raw`.
    - **`features.utf8_validation` enforced at decode:** `NONE` decodes an invalid payload into
      the typed `native::Value::UnvalidatedString` (bytes preserved verbatim, valid UTF-8 still
      lands on `Value::String`); `VERIFY` still rejects. `FieldDescriptor::validates_utf8()`.
    - **`features.enum_type` enforced at decode:** a CLOSED enum routes an unrecognised number
      to the unknown-field set (raw varint preserved); OPEN keeps it. `EnumDescriptor::is_closed()`.
    - **prost-reflect facade:** `oxiproto_reflect::editions::downlevel_editions` rewrites a
      `syntax = "editions"` file to its proto2 equivalent (materialising `packed`) so
      `pool_from_fds` / `pool_from_fds_bytes` — and therefore `oxiproto-json` and the CLI's
      `encode`/`decode` — accept edition schemas. Known divergence: `field_presence = IMPLICIT`
      has no proto2 expression; documented in the module.
    - **Edition 2024:** still rejected with the typed `ParseError::UnsupportedEdition`; see
      "Proposed follow-ups" for why.

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
3. ~~Edition 2023 commitment timeline~~ -- resolved: Edition 2023 is implemented (Phase 6).
   Open remainder: adopt Edition 2024 once its feature table is pinned down.
4. Validator integration (buf.validate) -- v0.2+ decision

## Proposed follow-ups

_Pruned 2026-08-03: this section used to also list Phase 2 body parsing, no_std
support, benchmarks, the conformance suite, the README refresh, the Phase 2
"remainder", native JSON codegen, Phase 4 native reflection, the fuzz harness,
and the protox-vs-native-parser benchmark as still-open follow-ups. All of
those shipped (see the `[x]` entries above, plus `crates/oxiproto-core/tests/no_std_smoke.rs`,
`crates/oxiproto-core/benches/{wire,message}.rs`, `crates/oxiproto/tests/conformance.rs`,
and `crates/oxiproto-codegen/src/json_impl.rs` emitting real per-type
`to_json`/`from_json` wired to the CLI's `--json` flag via `crates/oxiproto-cli/src/gen.rs`)
and were removed from this list rather than left to overstate remaining work._

- **Edition 2024** — deliberately *not* accepted (reviewed 2026-08-04, decision unchanged).
  An edition is defined entirely by the feature defaults it changes, so approximating it does
  not degrade gracefully: it emits a descriptor set whose wire/JSON behaviour differs from
  `protoc`'s for the same source while reporting success — the same class of bug the old
  proto3-approximation of Edition 2023 was. Two concrete blockers, both of which must be lifted
  together:
    1. `features.default_symbol_visibility` is bound up with the `export` / `local` symbol
       visibility modifiers, which are *grammar* additions the lexer and statement parser do
       not recognise at all;
    2. the remaining 2024 defaults (including `features.enforce_naming_style`) are not pinned
       down here from a primary source, and `parser::features` resolution is only sound when
       every baseline is exact.
  `parse_edition_statement` therefore returns `ParseError::UnsupportedEdition("2024")`, naming
  the offending value (`tests/editions.rs::edition_2024_is_rejected_rather_than_approximated`).
  The resolution engine itself is edition-agnostic, so lifting this is a table + grammar change,
  not a redesign.
- **oxiproto-validate crate**: blocked on Open Question #4 decision.
- **OxiMessage → Message alias cutover** (follow-up /ultra): 4 trait-level blockers (wkt/any_ext, reflect/lib, cli/convert, build/builder) all tied to prost-derived/DynamicMessage; safe only after all consumers migrate off prost::Message.
- **Custom/extension option values** (follow-up /ultra): applying `unknown`/extension-typed option values (protobuf message-typed custom options) to descriptors. Currently only well-known options applied.
- **protox replacement** (follow-up /ultra): rewire `Builder::compile`, `compile_str`, `compile_protos` off protox once source_code_info, proto2, and custom options close the fidelity gap.


---

<!-- production-readiness-backlog 2026-07-16 -->
## Production-Readiness Backlog — 2026-07-16

_Consolidated from static audit + Opus adversarial bug-hunt (48 verified defects across noffi) + baseline nextest/clippy + design investigation. See `../NOFFI_PRODUCTION_BACKLOG.md` for the full cross-project list and severity/model legend. Not implemented; no commits._

**Confirmed bugs — Opus-verified (unbounded recursion → stack-overflow DoS):**
- [x] **S · critical** `oxiproto-reflect/src/native/wire_codec.rs:177` — `DynamicMessage::decode` recurses into nested messages with no depth limit. R2/N0 — fixed via shared `DecodeBuffer::nested()` depth budget (`MAX_DECODE_DEPTH`); regression test in `crates/oxiproto/tests/recursion_dos.rs`.
- [x] **S · critical** `oxiproto-core/src/wire/buf.rs:193` — `DecodeBuffer::skip_field` recurses on SGroup with no depth limit (reachable from every generated decoder's unknown-field path). R2/N0 — fixed via `skip_field_at` depth tracking, returns `WireError::RecursionLimitExceeded`.
- [x] **S · high** `oxiproto-codegen/src/message_impl.rs:867` — generated `OxiMessage::merge` nested-message decode recurses with no depth guard. R2/N0 — generated code now routes through `buf.nested(..)`, inheriting the same shared budget.
- Fix: shared recursion-depth budget (protobuf norm: 100) across all three decode paths; return DecodeError. DONE.
**Designed / audit:**
- [x] **A/med · P1** examples (empty dir) populated: `examples/encode_decode.rs`, `examples/reflection.rs`, `examples/codegen_usage.rs` (new `oxiproto-examples` workspace member, `cargo run --example <name> -p oxiproto-examples`).
- [x] **A/med · P1** CLI typed errors: `oxiproto-cli` now returns `crate::error::CliError` (typed enum wrapping `OxiProtoError`/`CodegenError`/`ReflectError`/`JsonError`/`serde_json::Error`/`prost::DecodeError`/`io::Error`) end-to-end instead of `Box<dyn std::error::Error>`.
- [x] **A/med · P1** wire fuzz: `crates/oxiproto-core/tests/fuzz_message_decode.rs` — OxiMessage-level arbitrary-bytes-never-panic proptest, encode→decode round-trip proptest, bit-flip mutation proptest, and a seeded-PRNG (xorshift64) adversarial sweep, plus a hand-written-`OxiMessage` recursion-limit regression.
- [x] **A/med · P1** Edition 2023 — **DONE 2026-08-04**: full Editions feature resolution implemented (see Phase 6 above). The old "blocked on upstream" note was superseded: the Edition 2023 feature table (`field_presence`, `enum_type`, `repeated_field_encoding`, `utf8_validation`, `message_encoding`, `json_format`) is now implemented in `oxiproto-build::parser::features`, replacing the previous proto3-approximation that merely accepted the `edition` statement.
