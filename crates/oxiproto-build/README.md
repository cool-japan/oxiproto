# oxiproto-build — `build.rs` integration: `.proto` → Rust, no `protoc` required

[![Crates.io](https://img.shields.io/crates/v/oxiproto-build.svg)](https://crates.io/crates/oxiproto-build)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiproto-build` is the **build-script** stage of the **OxiProto** stack — COOLJAPAN's Pure-Rust Protocol Buffers implementation. Downstream crates add it as a `[build-dependencies]` entry and call it from their `build.rs` to compile `.proto` files into Rust at build time. **No `protoc` binary is required** — parsing is done entirely in Rust, either by the bundled native parser (default `native-parser` feature) or by `protox`.

The crate parses and resolves `.proto` sources into a `prost_types::FileDescriptorSet`, then drives `prost-build` to emit Rust into `OUT_DIR`. It also exposes the descriptor set directly (`compile_to_fds`) so other tools — such as `oxiproto-codegen` and `oxiproto-cli` — can consume it without writing files. The crate is `#![forbid(unsafe_code)]`.

## Installation

```toml
[build-dependencies]
oxiproto-build = "0.1.3"
```

Using `protox` instead of the native parser:

```toml
[build-dependencies]
oxiproto-build = { version = "0.1.3", default-features = false }
```

## Quick Start

In your crate's `build.rs`:

```rust,no_run
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // protos to compile, include dirs to resolve imports
    oxiproto_build::compile_protos(&["proto/service.proto"], &["proto/"])?;
    Ok(())
}
```

Then include the generated module in your library:

```rust,ignore
// src/lib.rs — `mypackage` matches the proto `package mypackage;`
pub mod mypackage {
    include!(concat!(env!("OUT_DIR"), "/mypackage.rs"));
}
```

For finer control, drive the [`Builder`]:

```rust,no_run
use oxiproto_build::Builder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Builder::new()
        .out_dir("generated/")
        .btree_map(["."])                       // BTreeMap for all map fields
        .type_attribute("mypkg.Msg", "#[derive(serde::Serialize)]")
        .file_descriptor_set_path("fds.bin")    // also dump the FDS for reflection
        .compile(&["proto/service.proto"], &["proto/"])?;
    Ok(())
}
```

## API Overview

### Free functions

| Function | Returns | Description |
|----------|---------|-------------|
| `compile_protos(protos, includes)` | `Result<(), OxiProtoError>` | Compile `.proto` files to Rust in `OUT_DIR` (convenience wrapper over `Builder`) |
| `compile_to_fds(protos, includes)` | `Result<FileDescriptorSet, OxiProtoError>` | Parse/resolve to a `FileDescriptorSet` without writing files |
| `compile_str(src)` / `compile_str_fn(src)` | `Result<FileDescriptorSet, BuildError>` | Compile a single inline proto3 source string to a `FileDescriptorSet` |
| `compile_str_native(src)` | `Result<FileDescriptorSet, BuildError>` | Native-parser compile of a single inline source (no imports); requires `native-parser` |
| `compile_files_native(protos, includes)` | `Result<FileDescriptorSet, BuildError>` | Native-parser compile of multiple files with import resolution and bundled well-known types; requires `native-parser` |

`protos` / `includes` accept any `&[impl AsRef<Path>]`.

### `Builder`

Self-consuming builder. Construct with `Builder::new()` (or `Builder::default()`), chain configuration, then call a terminal method.

| Configuration method | Description |
|----------------------|-------------|
| `out_dir(dir)` | Override the output directory (defaults to `$OUT_DIR`) |
| `type_attribute(path, attr)` | Add a derive/attribute to the generated type at a proto FQN |
| `field_attribute(path, attr)` | Add an attribute to a specific field by proto FQN |
| `skip_message(path)` | Skip code generation for a message (and orphaned references to it) |
| `skip_field(path)` | Skip a field, given as `"Message.field_name"` |
| `btree_map(paths)` | Use `BTreeMap` instead of `HashMap` for matching `map<…>` fields (`["."]` for all) |
| `file_descriptor_set_path(path)` | Write the serialized `FileDescriptorSet` (for `oxiproto-reflect`) |
| `protoc_compat()` | Delegate fully to `prost-build` defaults for `protoc`-compatible output |
| `service_generator(closure)` | Per-service Rust generator; output is appended to the package's `.rs` file |
| `include_file(path)` | Write an include file listing generated modules |
| `progress(closure)` | Callback invoked with each `.proto` path before compilation |

| Terminal method | Returns | Description |
|-----------------|---------|-------------|
| `compile(protos, includes)` | `Result<(), BuildError>` | Parse → optional FDS dump → service generation → `prost-build` codegen → optional include file |
| `compile_to_fds(protos, includes)` | `Result<FileDescriptorSet, BuildError>` | Parse/resolve only, no Rust output |

### `parser` module (native parser internals)

Exposed for advanced users and tooling. Notable re-exports: `parse_file`, `resolve`, `build_file_descriptor_set`, `parse_outline` (`FileOutline`, `TopLevelItem`), `Lexer`, `Token`, `Span`/`Spanned`, AST node types, and the `LexError` / `ParseError` error types. Submodules: `ast`, `lexer`, `parse`, `outline`, `resolve`, `descriptor`, `loader`, `comments`, `span`, `token`, `error`.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `native-parser` | yes | Use the bundled pure-Rust `.proto` parser (pulls in `prost-reflect`); enables `compile_str_native` / `compile_files_native`. When disabled, parsing delegates to `protox`. |

## Error Variants — `BuildError`

Carries structured source-location info where available; implements `Display` + `std::error::Error`. Converts to/from `oxiproto_core::OxiProtoError`, and `From<std::io::Error>`.

| Variant | Description |
|---------|-------------|
| `Parse { file, line, col, message }` | A `.proto` syntax/semantic error; `line`/`col` are 1-indexed (`0` when unknown) |
| `Codegen { message }` | `prost-build` (or downstream) code generation failed |
| `Io(std::io::Error)` | Reading a `.proto` source or writing output failed |

## Related crates

- [`oxiproto`](../../README.md) — top-level façade that re-exports the OxiProto stack
- [`oxiproto-core`](../oxiproto-core) — runtime traits, wire format, and the shared `OxiProtoError`
- [`oxiproto-codegen`](../oxiproto-codegen) — consumes the `FileDescriptorSet` to emit plain-Rust source
- [`oxiproto-cli`](../oxiproto-cli) — command-line front-end built on this crate's native parser
- `oxiproto-reflect` — loads the dumped `FileDescriptorSet` for runtime reflection
- `oxiproto-wkt` — Google well-known types resolved during native compilation

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
