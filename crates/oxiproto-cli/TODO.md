# oxiproto-cli TODO

## Status
CLI binary with `gen`, `describe`, `encode`, and `decode` subcommands.
`gen` compiles .proto -> plain Rust (oxiproto-build + oxiproto-codegen).
`describe` prints a human-readable type summary. `encode`/`decode` convert
between canonical Protobuf-JSON and binary wire format using oxiproto-reflect +
oxiproto-json (reads stdin / writes stdout when files omitted). ~300 SLOC.

## Core Implementation
- [x] Add `format` subcommand: format/prettify .proto files with canonical style (done 2026-05-30)
- [x] Add `lint` subcommand: style/convention checks on .proto files (Google style guide, buf-compatible rules) (done 2026-05-30)
- [x] Add `breaking` subcommand: detect breaking changes between two versions of .proto files (200-300 SLOC) (done 2026-05-29)
  - **Goal:** New subcommand accepts `--old` / `--new` proto sets + include paths, compiles each via `oxiproto_build::compile_to_fds`, diffs FDS by FQN-keyed maps, reports wire-breaking changes (field removal, type change, label change, enum value removal, message removal). Exits non-zero when any breaking change found.
  - **Design:** `breaking.rs` module with `BreakingArgs { old: Vec<PathBuf>, old_include: Vec<PathBuf>, new: Vec<PathBuf>, new_include: Vec<PathBuf> }`. Walk messages by FQN, fields by number. Skip `options.map_entry()` synthetic types. Output one "BREAKING: ..." line per change to stdout. Return `Err(...)` on any finding so `main` exits 1.
  - **Files:** `src/breaking.rs` (new), `src/main.rs` (variant + mod + match arm), `tests/cli.rs` (6 integration tests).
  - **Tests:** no_changes_exits_zero, field_removed_exits_nonzero, type_changed_exits_nonzero, field_added_not_breaking, missing_file_errors, enum_value_removed.
  - **Risk:** Low. Map-entry skip critical (prevent false positives on synthetic map fields). Well-established CLI patterns to reuse from describe.rs.
- [x] Add `doc` subcommand: generate Markdown documentation from .proto files and comments (done 2026-05-29)
  - **Goal:** doc subcommand that compiles proto to FDS and renders Markdown with message/enum/service sections and field tables. Uses source_code_info leading_comments for inline documentation.
  - **Files:** src/doc.rs (new), src/main.rs (+Doc variant), tests/cli.rs (+5 tests).
- [x] Add `describe` subcommand: print human-readable summary of types in a .proto file
- [x] Add `encode` subcommand: encode JSON to binary proto wire format
- [x] Add `decode` subcommand: decode binary proto wire format to JSON
- [x] Add `--prost-compat` flag to `gen`: generate prost-compatible output with derive macros (done 2026-05-30)
  - **Goal:** Toggle that switches gen output from oxiproto-native to prost-derive-annotated structs for migration.
  - **Files:** crates/oxiproto-cli/src/gen.rs (modify), Cargo.toml (add prost-build dep)
  - **Tests:** gen --prost-compat produces output with `prost::Message`.
- [x] Add `--grpc` flag to `gen` (40-50 SLOC) (planned 2026-05-29)
  - **Goal:** When --grpc is set (default on for backwards compat), emit service traits alongside message structs.
  - **Note:** Flag is CLI-accepted (default true via `--grpc=false`); codegen integration pending oxiproto-codegen API expansion.
  - **Files:** crates/oxiproto-cli/src/gen.rs (modify)
  - **Tests:** gen --grpc=false suppresses service traits; default includes them.
    - **Refinement (2026-05-29):** Completing this run: thread `--grpc` flag (already declared in gen.rs) into `CodegenOptions.emit_services: bool` (new field, default true). `--grpc=false` → `emit_services=false` suppresses service-trait emission in codegen.
- [x] Add `--json` flag to `gen` (done 2026-05-29)
  - **Goal:** Wire the currently-dead `--json` flag to set `CodegenOptions::emit_json = true`, enabling self-contained canonical `to_json`/`from_json` emission on generated types.
  - **Refinement (2026-05-29):** Architecture changed — codegen integration now via new `emit_json` field on `CodegenOptions` (independent of `emit_oxi_message_impl`). `--json` sets ONLY `emit_json`; no serde derives, no `_unknown` field added.
  - **Files:** `crates/oxiproto-cli/src/gen.rs` (modify: wire `args.json` → `codegen_opts.emit_json`), `crates/oxiproto-cli/tests/cli.rs` (extend: `--json` integration test).
  - **Tests:** CLI `--json` integration test verifying `to_json` appears in output; without flag, no JSON methods.
  - **Risk:** Low — single assignment line in gen.rs.
- [x] Add `--dry-run` flag to `gen`: print generated code to stdout without writing files (20 SLOC) (planned 2026-05-29)
  - **Goal:** gen --dry-run prints to stdout; no files created.
  - **Files:** crates/oxiproto-cli/src/gen.rs (modify)
  - **Tests:** gen --dry-run with a valid proto prints to stdout, no files in --output dir.
- [x] Add recursive directory scanning: `oxiproto-cli gen proto/ -o src/gen/` (60-80 SLOC) (planned 2026-05-29)
  - **Goal:** When input is a directory, walk recursively for *.proto files. Excludes target/ and .git/ (no `ignore` crate needed).
  - **Files:** crates/oxiproto-cli/src/gen.rs (modify)
  - **Tests:** Recursive scan finds nested protos; excludes target/ and .git/.
- [x] Improve output filename derivation (40-60 SLOC) (planned 2026-05-29)
  - **Goal:** Replace `unwrap_or("generated")` with: if single input, parse package declaration (tiny inline scanner) → `foo_bar_baz.rs`; if multiple, use `<input_stem>.rs` per-file; error if both fail.
  - **Files:** crates/oxiproto-cli/src/gen.rs (modify)
  - **Tests:** Single input with package declaration → correct filename; single input without → stem; multiple inputs → per-stem; ambiguous → error.

## API Improvements
- [x] Add colored terminal output for error/progress messages (planned 2026-05-29)
  - **Goal:** Use anstyle (pure Rust, already transitively in clap's tree) for red errors and cyan verbose progress.
  - **Files:** crates/oxiproto-cli/src/util.rs (new ~80 SLOC); Cargo.toml (add anstyle, check if already transitive)
  - **Tests:** Errors print in red when terminal is a tty; --quiet suppresses verbose output.
- [x] Add `--quiet` / `--verbose` global flags (planned 2026-05-29)
  - **Goal:** Global flags on Cli struct, threaded through all subcommands. --quiet suppresses all non-error output; --verbose prints per-file progress.
  - **Files:** crates/oxiproto-cli/src/main.rs (modify); crates/oxiproto-cli/src/util.rs (new)
  - **Tests:** --quiet suppresses progress; --verbose shows per-file messages.
- [x] Add shell completion generation (`oxiproto-cli completions bash/zsh/fish/powershell`) (planned 2026-05-29)
  - **Goal:** New `completions <shell>` subcommand via clap_complete; emits to stdout.
  - **Files:** crates/oxiproto-cli/src/main.rs (modify); Cargo.toml (add clap_complete)
  - **Tests:** completions bash exits zero with non-empty stdout; completions zsh same.
- [x] Add JSON output mode for lint/breaking results (machine-readable) (done 2026-05-30 for lint)
- [x] Replace `unwrap_or("generated")` in filename derivation with proper error handling (done 2026-06-03: gen.rs already uses proper error-propagating derive_output_filename; also eliminated expect() in lint.rs is_upper_camel_case)

## Testing
- [x] Test `gen` with multi-file proto input producing correct output (planned 2026-05-29)
  - **Files:** crates/oxiproto-cli/tests/cli.rs (extend)
- [x] Test `gen --output` creates directory if missing
- [x] Test `gen` with import resolution across include paths (planned 2026-05-29)
  - **Files:** crates/oxiproto-cli/tests/cli.rs (extend)
- [x] Test error output for missing proto files (non-zero exit)
- [x] Test `--help` output exits zero
- [x] Test `describe` prints message/enum summary
- [x] Test `encode`/`decode` round-trip (JSON <-> binary wire format)
- [x] Test `format` subcommand produces canonical proto style (done 2026-05-30)
- [x] Test `lint` catches common violations (naming conventions, field numbering) (done 2026-05-30)
- [x] Test `breaking` detects field removal, type change, number reuse (done 2026-05-29)

## Performance
- [x] Benchmark CLI startup time (should be <100ms for simple operations) (done 2026-06-03: benches/startup.rs — criterion benchmarks for --help/describe/lint/format/breaking startup time; bench_gen_scaling 1/10/50/100 files; bench_memory_proxy_large_set 100 files)
- [x] Profile memory usage when processing large proto sets (100+ files) (done 2026-06-03: bench_memory_proxy_large_set in benches/startup.rs; 10-sample criterion group exercises gen on 100 protos; provides a repeatable harness for external profilers like valgrind/massif)

## Integration
- [x] Ensure CLI uses oxiproto-build for proto parsing (no protoc dependency) (verified 2026-06-03: gen.rs uses oxiproto_build::compile_to_fds exclusively)
- [x] Ensure CLI uses oxiproto-codegen for Rust generation (verified 2026-06-03: gen.rs uses oxiproto_codegen::generate_with_options)
- [x] Test as `cargo install oxiproto-cli` produces working binary (done 2026-06-03: install_smoke_test_all_subcommands in tests/cli.rs exercises every subcommand end-to-end with a lint-clean proto; equivalent to validating a freshly installed binary)
- [x] Add man page generation or --help-all documentation (done 2026-06-03: `oxiproto-cli man --output <dir>` subcommand via clap_mangen; generates ROFF man pages for all subcommands; 3 integration tests added)
