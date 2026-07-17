# Contributing to OxiProto

OxiProto is part of the **COOLJAPAN ecosystem** — a family of Pure-Rust
crates maintained by COOLJAPAN OU (Team Kitasan) that avoid C/C++/Fortran
dependencies wherever possible. Contributions are welcome, but must follow
the conventions below so the whole ecosystem stays consistent.

## Building and Testing

```bash
# Build the whole workspace
cargo build --workspace --all-targets

# Run the full test suite (nextest, not `cargo test`)
cargo nextest run --workspace

# Lint with zero tolerance for warnings
cargo clippy --workspace --all-targets -- -D warnings
```

Both commands above must be clean before a PR can be merged. There is no
separate "warnings are OK for now" tier — zero warnings, always.

## Project Rules

- **Pure Rust by default.** No C, C++, or Fortran dependency in the default
  feature set. If a non-Rust dependency is ever unavoidable, it must be
  behind a non-default Cargo feature.
- **No panics on untrusted input.** `.unwrap()` / `.expect()` / `panic!()` /
  `unreachable!()` / `assert!()` must never run on data derived from
  external input (`.proto` source text, wire-format bytes, JSON, CLI
  arguments). Return the crate's existing typed error instead
  (`OxiProtoError`, `BuildError`, `ReflectError`, `CodegenError`,
  `JsonError`, `CliError`, ...). `unwrap`/`expect`/`panic!` are fine in
  `#[test]` functions and doctests, where the input is fully controlled.
- **No clippy warnings.** `cargo clippy --all-targets -- -D warnings` must
  pass with default features before every commit.
- **Workspace dependency inheritance.** Shared dependencies live in
  `[workspace.dependencies]` at the repo root and are pulled into each crate
  with `dep.workspace = true`. Do not pin a version directly in a member
  `Cargo.toml` for a dependency the workspace already centralizes.
- **Files stay under 2000 lines.** Split large modules before they cross the
  limit rather than after.
- **Match existing style.** snake_case identifiers, the crate's own error
  enums (not `Box<dyn Error>`) at API boundaries, and doc-comment density
  consistent with the surrounding code.
- **Latest crate versions.** Prefer the latest version available on
  crates.io for any new dependency.

## Tests

- Use `std::env::temp_dir()` for any test that needs a filesystem path —
  never a hardcoded absolute path.
- New wire-format or parser code should come with property-based tests
  (`proptest`, already a dev-dependency) covering round-trips and
  no-panic-on-arbitrary-input, not just example-based unit tests.

## Pull Requests

- Keep PRs focused on one change; unrelated cleanups belong in a separate PR.
- Describe *why* the change is needed, not just what changed.
- Do not bump crate versions or touch `CHANGELOG.md` — releases are cut by
  the maintainer.

## License

By contributing, you agree that your contributions will be licensed under
the Apache-2.0 license that covers this project.
