# oxiproto-build TODO

## Status
Functional build helper: `compile_protos` and `Builder` chain `protox::compile` (Pure Rust .proto parser) to `prost_build::Config::compile_fds` for Rust code generation. `compile_to_fds` exposes raw FileDescriptorSet. No protoc required. ~100 SLOC production code.

## Core Implementation
- [x] Implement native .proto file lexer (tokenizer for proto2/proto3 syntax) (400-500 SLOC) (planned 2026-05-29)
  - **Goal:** Hand-written Lexer<'a> over &'a str emitting Spanned<Token> with line/col tracking.
  - **Design:** Token enum covers all keywords, literals (int/float/string with all escape forms), punctuation, comments. Skips whitespace; preserves comments for source-code-info. Located at crates/oxiproto-build/src/parser/lexer.rs.
  - **Files:** crates/oxiproto-build/src/parser/{mod,token,span,error,lexer}.rs (all new)
  - **Tests:** Every keyword, literal form, escape sequence, punctuator; line/col tracking; error cases with precise spans.
  - **Risk:** String escape sequences (octal, \xHH, \uXXXX, \UXXXXXXXX) require careful handling.
- [x] Implement native .proto outline parser: top-level structure only (200 SLOC) (planned 2026-05-29)
  - **Goal:** OutlineParser identifies top-level syntax/package/import/option/message/enum/service blocks with spans. Body parsing deferred.
  - **Design:** Located at crates/oxiproto-build/src/parser/outline.rs. Emits FileOutline { syntax, package, imports, options, top_level_items: Vec<TopLevelItem> }. Uses brace-counting to track body_span without parsing body content.
  - **Files:** crates/oxiproto-build/src/parser/outline.rs (new ~200 SLOC); crates/oxiproto-build/tests/{lexer,outline}.rs (new)
  - **Tests:** Outline parser on proto2, proto3, and WKT-style files; correct names + spans; top-level items enumerated.
  - **Risk:** Scoped to outline only — do NOT parse message/enum/service bodies in this slice.
- [x] Implement proto3 syntax support: default values, no required fields, map types (200-250 SLOC) (planned 2026-05-29)
  - **Goal:** Native recursive-descent body parser for proto3 messages, enums, oneofs, map fields, services.
  - **Design:** `parser/ast.rs` (ProtoFile, Message, Field, FieldType{Scalar|Map|Named}, ScalarType, Oneof, Enum, EnumValue, Service, Method, ProtoOption, Reserved); `parser/parse.rs` (recursive-descent Parser reusing Lexer + Spanned + PeekLexer; field: [label] type name = number [opts];; map: map<K,V> name = number;; oneof block; rpc with stream keyword). No resolution/desugaring in this slice.
  - **Files:** `crates/oxiproto-build/src/parser/ast.rs` (new ~260 SLOC), `parser/parse.rs` (new ~620 SLOC), `parser/mod.rs` (modify: add AST + parse_file exports), `tests/parse.rs` (new ~320 SLOC)
  - **Tests:** Every scalar type; singular/optional/repeated; map<string,int32>; 3-level nested; oneof; enum+reserved; services (unary+streaming); field options; reserved ranges incl. `to max`; error cases with precise ParseError spans.
  - **Risk:** Largest slice; mitigated by AST-only scope (no resolution in P1).
- [x] Implement proto2 syntax support: required/optional/extensions/groups/default values (250-300 SLOC) (planned 2026-05-29)
  - **Goal:** Native parser correctly emits `FileDescriptorProto`s for proto2 files: `required` label (LABEL_REQUIRED), proto2 `optional` (LABEL_OPTIONAL, NO synthetic oneof), `extend` blocks (extension fields), `extensions N to M` ranges, `default_value` for scalar/enum/string fields. Groups rejected with a clear error.
  - **Design:** (1) `FieldLabel::Required` variant in ast.rs; (2) `Extend` keyword in lexer; (3) parser arms for `required`, `extensions`, `extend`, `group` (group → error); (4) descriptor.rs branches on `is_proto2 = syntax=="proto2"` — proto2 `optional` never gets synthetic oneof; (5) `extension_range` populated from parsed ranges; (6) extension fields from `extend` blocks in file-level `extension` vec; (7) `default_value` extracted from `[default=V]` option and formatted per-type; (8) comparator normalizes `default_value` and nulls `uninterpreted_option` rather than chasing byte-identical protoc formatting.
  - **Files:** `parser/ast.rs` (FieldLabel::Required, ExtensionRange, ExtendBlock); `parser/lexer.rs` (Extend keyword); `parser/parse.rs` (required/extensions/extend/group arms, syntax validation); `parser/descriptor.rs` (is_proto2 branch, extension_range, extension fields, default_value); `parser/resolve.rs` (extension FQNs in symbol table); `tests/native_fds.rs` (proto2 tests + comparator normalization).
  - **Tests:** `proto2_required_field`, `proto2_optional_no_synthetic_oneof`, `proto2_extensions_range`, `proto2_extend_block`, `proto2_default_value_scalar`, `proto2_default_value_string`. ALL existing proto3 cross-validation tests must pass (regression guard).
  - **Risk:** Synthetic-oneof branching is the proto3 regression point — run full native_fds suite before marking done. `default_value` divergence mitigated by comparator normalization.
- [x] Implement import resolution: `import "path"`, `import public`, `import weak` with include path search (150-200 SLOC) (planned 2026-05-29) (done 2026-05-29)
  - **Goal:** From a root `.proto` plus include dirs, the native path builds a correct multi-file `FileDescriptorSet` — resolving cross-file/cross-package type references, honoring all import modifiers, loading WKT types from `prost_reflect::DescriptorPool::global()` (byte-identical to protox), emitting one `FileDescriptorProto` per transitively-imported file in topological order.
  - **Design:** New `parser/loader.rs`: include-path resolver + two-set DFS loader (`visited`+`on_stack` for cycle detection) producing topological order. Cross-file `SymbolTable` with public-import re-export transitivity `E(F)=defs(F)∪⋃public-imports`. Real type resolution via `V(F)` (delete guess heuristic in `resolve.rs:155-164`). `build_fds_multi` emitting one FDP per file. `compile_files_native(protos, includes)` public entry point. `native-parser` feature gains `dep:prost-reflect` (optional, already in workspace lockfile).
  - **Files:** `src/parser/loader.rs` (new), `src/parser/resolve.rs` (rewrite), `src/parser/descriptor.rs` (global enum set, public/weak dep indices, `build_fds_multi`), `src/parser/span.rs` (+offset_to_line_col), `src/parser/mod.rs`, `src/lib.rs`, `Cargo.toml`, `tests/native_imports.rs` (new).
  - **Tests:** New `native_imports.rs` with structural cross-validation vs protox. Comparator asserts: equal file-name key sets; deep per-FDP comparison; all three dependency vectors; topological validity by name (NOT zip-by-index). Cases: plain cross-pkg import, `import public`, `import weak`, 3-file chain, WKT import, negative (unresolvable path, unknown type, import cycle).
  - **Risk:** Hardest slice. WKT from global pool prevents divergence. Documented limitation: root-anchored fallback approximates protoc's first-component anchoring; adversarial sibling-package nesting out of scope for preview.
- [x] Implement package namespacing and fully-qualified name resolution (100-150 SLOC) (planned 2026-05-29)
  - **Goal:** Resolve `Named(...)` type references to leading-dot fully-qualified names using innermost-scope-first lookup. Validate no duplicate field numbers. Build symbol table from AST.
  - **Design:** `parser/resolve.rs` — two-pass: (1) collect all symbols (`.pkg.Msg`, `.pkg.Msg.Nested`, enums); (2) resolve each field type_name/rpc input_type/output_type to leading-dot FQN. Error on unknown refs and duplicate field numbers.
  - **Files:** `crates/oxiproto-build/src/parser/resolve.rs` (new ~320 SLOC)
  - **Tests:** Covered by Slice P2 structural cross-validation.
- [x] Implement service definition parsing (service name, rpc methods, streaming annotations) (80-100 SLOC) (planned 2026-05-29)
  - **Goal:** Parse `service Name { rpc Method(Req) returns (Resp); }` with client/server streaming flags into AST.
  - **Design:** Handled within `parser/parse.rs` (Slice P1). `Method { name, input_type, output_type, client_streaming, server_streaming, options, span }`.
  - **Files:** Covered by Slice P1 (parser/parse.rs, ast.rs).
  - **Tests:** Unary + client-stream + server-stream + bidi streaming service methods parsed correctly.
- [x] Implement option parsing: file-level, message-level, field-level, method-level, custom options (150-200 SLOC) (planned 2026-05-29) (done 2026-05-29)
  - **Note (refinement 2026-05-29):** option statements parsed; full/custom option value semantics deferred
  - **Goal:** Parse option statements (name + OptionValue) at all proto levels. Note: full semantic application of custom options is deferred.
  - **Design:** `ProtoOption { name: String, value: OptionValue(Ident|Str|Int|Float|Bool) }` — captures known options like `deprecated`, `packed`. Full/custom option values (arbitrary message types) deferred to follow-up.
  - **Files:** Covered by Slice P1 (ast.rs, parse.rs).
  - **Note (refinement 2026-05-29):** Statements parsed; full/custom option value semantics deferred.
  - **Refinement (2026-05-29, run 3):** Well-known field options (`deprecated`, `packed`), message/enum/service/method options, file-level options (`java_package`, `go_package`), and `reserved_range`/`reserved_name` applied to descriptor protos in `descriptor.rs`. Custom/extension option values remain deferred. Cross-validation tests added for field deprecated, reserved ranges/names, and file-level options.
  - **Refinement (2026-05-29, run 6):** Message-literal option values now parsed: `OptionValue::MessageLiteral(Vec<(String, OptionValue)>)` added; parse.rs handles `{ key: value ... }` syntax recursively; descriptor.rs serializes as `aggregate_value` in UninterpretedOption. Custom option TYPE resolution (requires loaded extension definitions) still deferred.
- [x] Implement source code info generation (line/column tracking for each descriptor element) (100-150 SLOC) (planned 2026-05-29, done 2026-05-29)
  - **Goal:** Populate `source_code_info` in the native-parser path: each declaration (message, field, enum, enum-value, service, method, oneof) gets a `Location` with correct protobuf path vector, 0-based line/col span, and associated leading/trailing/detached comments extracted from `Token::LineComment`/`Token::BlockComment` tokens the lexer already emits.
  - **Design:** Pre-tokenize pass in `parse_file` separates comment tokens into a `CommentMap` (side-table sorted by span.end) without touching any parse_X functions. Precompute a `LineTable` (Vec<usize> of line-start offsets) for O(log n) span conversion. Thread (src, CommentMap, LineTable) into `build_file_descriptor_proto_with_global_enums`. Builder emits one Location per declaration using protobuf path conventions (file.message=4, file.enum=5, file.service=6; message.field=2, .nested=3, .enum=4, .oneof=8; enum.value=2; service.method=2).
  - **Files:** `parser/parse.rs`, `parser/descriptor.rs`, `parser/span.rs` (+LineTable, +offset_to_proto_span), `parser/mod.rs` (re-exports), `lib.rs`, `parser/loader.rs`, `tests/native_fds.rs` (SCI tests + partial path-keyed comparator against protox).
  - **Tests:** source_code_info_line_comments, source_code_info_block_comments, source_code_info_detached_comment, source_code_info_spans, source_code_info_cross_validate (partial), codegen_oracle (feeds native FDS to codegen, asserts doc comments appear).
  - **Risk:** 0-based vs 1-based span conversion — new offset_to_proto_span separate from existing offset_to_line_col (used for errors, 1-based). Partial comparator (keyed by path, ±1 tolerance on spans) avoids byte-identical protox match requirement.
- [x] Implement FileDescriptorSet construction from parsed AST (200-300 SLOC) (planned 2026-05-29, native-parser feature only)
  - **Goal:** For single-file proto3, build a faithful prost_types::FileDescriptorSet from the parsed+resolved AST, behind opt-in `native-parser` feature. protox remains the DEFAULT; this is an opt-in preview.
  - **Design:** `parser/descriptor.rs` — apply proto3 desugaring: implicit LABEL_OPTIONAL for singular, LABEL_REPEATED for repeated; proto3 optional → synthetic oneof + proto3_optional=true; map<K,V> → synthetic XxxEntry message with map_entry=true + repeated field; camelCase json_name auto-populated. Entry points: `compile_str_native(source)` + feature-gated Builder path.
  - **Files:** `parser/descriptor.rs` (new ~480 SLOC), `parser/resolve.rs` (new ~320 SLOC), `parser/mod.rs` (modify), `src/lib.rs` (modify: compile_str_native), `Cargo.toml` (add native-parser feature), `tests/native_fds.rs` (new ~280 SLOC)
  - **Tests:** Structural subset cross-validation vs protox (source_code_info cleared on both sides). NOT byte-identical.
  - **Risk:** Hardest slice; structural-subset test prevents silent breakage from deferred source_code_info.
- [x] Implement Edition 2023 syntax support (feature resolution, edition defaults) (200-300 SLOC) (done 2026-06-03)
  - **Implemented:** `Token::Edition` keyword; `Edition` enum in `ast.rs` (`Edition2023`, `Unknown`); `parse_edition_statement` in `parse.rs`; conflict detection (`SyntaxAndEditionConflict`, `UnsupportedEdition` errors); `file_is_proto2`/`file_syntax_string` helpers in `descriptor.rs` (edition 2023 produces `syntax = "editions"` sentinel; uses proto3-like semantics: no required fields, synthetic oneofs for `optional`). 15 parser tests + 9 descriptor cross-validation tests added. Zero warnings.
- [x] Add `Builder::service_generator(fn)` hook for custom service stub generation (40-50 SLOC) (planned 2026-05-29)
  - **Goal:** `builder.service_generator(impl Fn(&ServiceDescriptor) -> String)` stored and passed to codegen.
  - **Files:** crates/oxiproto-build/src/builder.rs (new, extracted from lib.rs)
  - **Tests:** Builder configured with a mock service generator produces expected output.
- [x] Add `Builder::include_file(path)` for writing a single include file with all generated items (30-40 SLOC) (planned 2026-05-29)
  - **Goal:** Writes a generated include.rs containing all modules into a single file.
  - **Files:** crates/oxiproto-build/src/builder.rs
  - **Tests:** include_file produces a single RS file containing all generated structs.
- [x] Add `Builder::skip_message(path)` / `Builder::skip_field(path)` for selective code generation (40-50 SLOC) (planned 2026-05-29)
  - **Goal:** Skip specific messages/fields in generated output.
  - **Files:** crates/oxiproto-build/src/builder.rs
  - **Tests:** Skipped message absent from generated Rust; other messages present.
- [x] Add `Builder::btree_map(path)` to use BTreeMap instead of HashMap for proto map fields (20-30 SLOC) (planned 2026-05-29)
  - **Goal:** Per-message override to use BTreeMap instead of HashMap for proto map fields.
  - **Files:** crates/oxiproto-build/src/builder.rs
  - **Tests:** Builder with btree_map("foo.Bar") generates BTreeMap for that message's map fields.

## API Improvements
- [x] Add `Builder::file_descriptor_set_path(path)` to write serialized FDS for runtime reflection (planned 2026-05-29)
  - **Goal:** After compilation, serialize the FileDescriptorSet to the given path for use by oxiproto-reflect at runtime.
  - **Files:** crates/oxiproto-build/src/builder.rs
  - **Tests:** FDS file written, readable back via pool_from_fds_bytes.
- [x] Add `Builder::protoc_compat()` mode that produces prost-compatible output for migration (planned 2026-05-29)
  - **Goal:** Toggle that switches codegen from oxiproto-native to prost-compatible output (delegating to prost-build).
  - **Files:** crates/oxiproto-build/src/builder.rs
  - **Tests:** Builder with protoc_compat() produces prost-compatible output.
- [x] Add progress callback for long compilations with many imports (planned 2026-05-29)
  - **Goal:** `Builder::progress(impl Fn(&Path))` invoked per .proto file processed.
  - **Files:** crates/oxiproto-build/src/builder.rs
- [x] Return structured compilation errors with file:line:column info (planned 2026-05-29)
  - **Goal:** `BuildError::Parse { file, line, col, message }` replaces the string-wrapped OxiProtoError::ParseError.
  - **Design:** Extract from protox's miette span API; fallback: parse "file:line:col" prefix from Display output.
  - **Files:** crates/oxiproto-build/src/error.rs (new)
  - **Tests:** Deliberately broken .proto produces BuildError::Parse with non-zero line/col.
- [x] Add `compile_str(proto_source: &str)` for inline proto definitions (planned 2026-05-29)
  - **Goal:** Write proto source to temp_dir, run protox::compile, clean up, return FileDescriptorSet.
  - **Files:** crates/oxiproto-build/src/compile_str.rs (new ~60 SLOC); crates/oxiproto-build/tests/compile_str.rs (new)
  - **Tests:** Inline proto produces valid FDS; broken proto produces BuildError::Parse; temp file cleaned up.

## Testing
- [x] Test lexer: all keyword tokens, identifiers, integer/float/string literals, all escape sequences, comments, punctuation, EOF, error cases (tests/lexer.rs, 2026-05-29)
- [x] Test outline parser: proto2/proto3 files, nested messages, multiple top-level items, no-package, malformed input, public/weak imports, service spans, top-level options (tests/outline.rs, 2026-05-29)
- [x] Test proto3 message with all scalar types generates correct Rust (planned 2026-05-29)
  - Covered by Slice P1 tests/parse.rs and Slice P2 tests/native_fds.rs.
- [x] Test proto2 message with required/optional/extensions (planned 2026-05-29)
  - **Goal:** Cross-validation tests in `tests/native_fds.rs` (gated `native-parser`) covering: required field, proto2 optional (no synthetic oneof), extension ranges, extend blocks, default_value.
  - **Files:** `tests/native_fds.rs` (extend existing test suite)
- [x] Test import resolution across multiple include paths (planned 2026-05-29) (done 2026-05-29)
  - **Files:** `crates/oxiproto-build/tests/native_imports.rs` (new — 6 positive + 3 negative test cases, fixtures under `std::env::temp_dir()`)
- [x] Test nested message and enum codegen (planned 2026-05-29)
  - Covered by Slice P1 tests/parse.rs (AST) and P2 structural cross-validation (3-level nesting).
- [x] Test map field codegen (map<string, int32>) (planned 2026-05-29)
  - Covered by Slice P1 (AST shape) and P2 (map desugaring → XxxEntry cross-validation).
- [x] Test oneof field codegen (planned 2026-05-29)
  - Covered by Slice P1 (AST) and P2 (oneof_decl + synthetic oneof for optional).
- [x] Test service definition parsing (method names, request/response types, streaming) (planned 2026-05-29)
  - Covered by Slice P1 tests/parse.rs.
- [x] Test error reporting for syntax errors (missing semicolons, invalid types) (planned 2026-05-29)
  - Covered by Slice P1 tests/parse.rs error cases.

### Phase 2 Routing

- [x] Resolve silent-drop of scalar custom/extension options (COPT) — probe protox behavior, then match it (way-station) (2026-05-30)
  - **Goal:** `option (foo.bar) = true;` / `= 42;` / `= "str";` at file/message/field scope no longer silently disappears. Behavior matches protox exactly: error if protox errors, preserve as `uninterpreted_option` if protox preserves.
  - **Design:** (1) Probe protox via existing `compile_str` to observe error-vs-uninterpreted for undefined extension options at all scopes. (2a) If protox errors: add `ParseError`/`BuildError` variant for unknown scalar options, return from native builder at the same paths currently dropping them (`descriptor.rs:62/177/273`). (2b) If protox uninterpreted: factor `uninterpreted_from_scalar(name, &OptionValue) -> UninterpretedOption` beside `build_uninterpreted_option_from_literal` and apply at all scopes. Document branch with `// NOTE: way-station — full extension resolution deferred`.
  - **Files:** `src/parser/descriptor.rs` (option builders at `:62`/`:177`/`:273` + helper or error path), possibly `src/parser/error.rs` (new variant)
  - **Tests:** `protox_scalar_custom_option_behavior_probe` (documents protox behavior), native-side branch tests in `tests/native_fds.rs`. No cross-val (normalize_fds strips options).
  - **Risk:** Probe-first design prevents false "preservation" that would diverge from protox. First to descope if run gets heavy.

- [x] Implement proto2 `group` field support (GRP) (2026-05-30)
  - **Goal:** `optional group Result = 1 { optional string url = 2; }` parses into protoc-faithful FDS: field `type=TYPE_GROUP(10)` named after lowercased group name, plus a nested message named exactly after the group, cross-validated against protox.
  - **Design:** Group desugaring: `optional group Foo = N { body }` → (a) nested message `Foo` with body, (b) field `foo` (lowercased) with the outer label, `type = TYPE_GROUP`, `type_name = .pkg.Parent.Foo`. Add `FieldType::Group(String)` to `ast.rs`; add group branch in `parse.rs` field parser; emit `Type::Group` (10) + nested message in `descriptor.rs`; resolve `FieldType::Group` type_name in `resolve.rs`; remove/repurpose the "unsupported" error in `error.rs`.
  - **Files:** `src/parser/ast.rs`, `src/parser/parse.rs`, `src/parser/descriptor.rs`, `src/parser/resolve.rs`, `src/parser/error.rs`
  - **Tests:** `proto2_group_field` via `run_cross_validation` in `tests/native_fds.rs`; malformed-group error test.
  - **Risk:** Field-name lowercasing and nested-message placement must match protoc — mitigated by cross-validation.

- [x] Route public entry points → native parser under `native-parser` feature (RTE) (2026-05-30)
  - **Goal:** With `--features native-parser`, `compile_str`, `Builder::compile`, and `Builder::compile_to_fds` use the native parser. Default features keep using protox. `Cargo.toml` `default` key stays empty (the flip is a separate approved step). Both feature matrices must be green.
  - **Design:** Add `#[cfg(feature = "native-parser")]` branches in `compile_str.rs` (single-file native path, no temp file), `builder.rs` (`compile_to_fds` → `compile_files_native`; `compile` → `compile_files_native` + `prost_build::Config::compile_fds`). Keep `cfg(not(feature))` protox body unchanged. No public signature changes. Prove workspace green under both default and `--features native-parser`.
  - **Files:** `src/compile_str.rs`, `src/builder.rs`
  - **Tests:** `public_compile_str_uses_native_under_feature`, `public_compile_to_fds_uses_native_under_feature` in `tests/compile_str.rs` or `tests/native_fds.rs`; full regression of default path.
  - **Risk:** HIGH blast radius, mitigated by: `default` stays empty, both matrices validated, `compile_str` signature has no includes so native single-file is a faithful match.
- [x] **FLIP** — add `default = ["native-parser"]` to `Cargo.toml`; native-parser is now the default for all consumers (2026-05-30)

- [x] Fuzz the .proto parser with random input (done 2026-06-03)
  - **Goal:** Pure-Rust proptest no-panic harness (10 strategy categories × proptest sampling): arbitrary bytes, valid prefix + corrupted suffix, keyword patterns, deeply nested braces, very long identifiers/strings, Unicode, random field numbers, extreme int/float literals, comment injection, import paths. Never panics — only Ok/Err returned.
  - **Files:** `crates/oxiproto-build/tests/fuzz_proto.rs` (new, 15 proptest tests); `Cargo.toml` (+proptest.workspace); root `Cargo.toml` (+proptest workspace dep 1.6).
  - **Tests:** 15 tests, all pass. Exit code 0.

## Performance
- [x] Benchmark parse time for large .proto files (google/protobuf/*.proto) (done 2026-06-03)
  - **Implemented:** `benches/parse.rs` — criterion harness with 6 benchmark groups: small/medium/large protos, N-message scaling study, 3-file import chain, deeply-nested message structure. `cargo bench -p oxiproto-build --features native-parser --bench parse --no-run` compiles clean.
- [x] Profile import resolution for deeply nested import chains (done 2026-06-03)
  - **Implemented:** Three new benchmark groups in `benches/parse.rs`: `deep_chain_10` (10-file linear import chain, stresses DFS loader and topological sort), `diamond_4_files` (diamond dependency graph, stresses visited-deduplication path), `wide_fanout_8` (1 root + 8 independent leaf files, stresses parallel loading). All compile and run under `cargo bench -p oxiproto-build --features native-parser --bench parse --no-run`.
- [x] Consider incremental compilation (skip unchanged .proto files) (done 2026-06-03)
  - **Implemented:** `Builder::incremental(cache_path)` method + FNV-1a 64-bit content fingerprinting. Cache stored as tab-separated `path\thex_hash` lines. On `compile()`: fingerprints all input `.proto` files; if cache exists and all hashes match, returns `Ok(())` immediately (skips parse+codegen). Cache updated atomically (write `.tmp`, rename) after each successful compilation. 11 unit tests in `builder::tests` for `fnv1a64`, `fingerprint_files`, `fingerprints_match`, cache roundtrip, error cases.

## Integration
- [ ] Ensure generated code uses oxiproto-core Message trait (not prost::Message) once native core exists
  - **Implemented (2026-06-03):** `OxiMessage`/`OxiName` overlay generation is fully wired up — `Builder::native_impl(true)` (requires `native-codegen` feature) calls `oxiproto_codegen::generate_with_options` to emit `impl OxiMessage for T` and `impl OxiName for T` blocks into `*_oxi.rs` files alongside prost-generated `.rs` files. Tests added: `native_codegen_overlay_generates_oxi_impl_files`, `no_native_codegen_by_default`.
  - **DEFERRED: Full exclusive replacement of `prost::Message` with `OxiMessage` in generated code.** This requires implementing a complete native message codegen engine in place of `prost_build::Config::compile_fds`. Currently `prost::Message` impls are still generated by prost-build; `OxiMessage` is an additive overlay.
- [x] Ensure oxirpc-build can delegate to oxiproto-build for proto parsing
  - **Done (2026-06-03):** `oxirpc-build/src/lib.rs` calls `oxiproto_build::compile_to_fds` as its parsing backend; verified by `compile_to_fds_delegation_contract` and `compile_to_fds_delegation_error_path` integration tests in `tests/codegen.rs`.
- [x] Test compatibility with well-known types (google.protobuf.*) (done 2026-06-03)
  - **Implemented:** 8 new tests in `tests/native_imports.rs`: `wkt_duration`, `wkt_empty_in_service`, `wkt_any`, `wkt_struct`, `wkt_field_mask`, `wkt_wrappers`, `wkt_multiple_in_one_file`, `wkt_cross_validate_with_protox`. All google.protobuf.* types (Timestamp, Duration, Empty, Any, Struct, Value, FieldMask, StringValue, Int32Value, BoolValue) validated.
- [x] Test interop with protoc-generated FileDescriptorSet for migration path (done 2026-06-03)
  - **Implemented:** `tests/fds_interop.rs` — 10 tests covering: prost encode/decode roundtrip (messages, fields, enums, oneof, maps, services), prost_reflect::DescriptorPool acceptance, multi-file FDS roundtrip, structural equivalence vs protox, proto2 required/default_value preservation.
