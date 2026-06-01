# oxiproto-codegen — `.proto` descriptor → plain-Rust source generator

[![Crates.io](https://img.shields.io/crates/v/oxiproto-codegen.svg)](https://crates.io/crates/oxiproto-codegen)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiproto-codegen` is the **code-generation** stage of the **OxiProto** stack — COOLJAPAN's Pure-Rust Protocol Buffers implementation. It walks a `prost_types::FileDescriptorSet` and emits **plain Rust** `struct`s and `enum`s: no `prost` derive macros, no gRPC stubs by default, no validators. The result is ordinary Rust you can read, diff, and check in.

This crate is purely a string-in / string-out transform: it does not parse `.proto` files (that is `oxiproto-build`'s job, which produces the `FileDescriptorSet` this crate consumes). Optional features let it also emit native `OxiMessage`/`OxiName` impls, canonical Protobuf-JSON helpers, fluent builders, text-format printers, and run the output through `prettyplease`. The crate is `#![forbid(unsafe_code)]`.

## Installation

```toml
[dependencies]
oxiproto-codegen = "0.1.0"
```

With `rustfmt`-quality formatting of the generated source via `prettyplease`:

```toml
[dependencies]
oxiproto-codegen = { version = "0.1.0", features = ["format"] }
```

## Quick Start

Generate Rust source from a descriptor set (here produced by `oxiproto-build`):

```rust
use oxiproto_codegen::{generate, generate_with_options, CodegenOptions};

// `fds` is a prost_types::FileDescriptorSet, e.g. from
// oxiproto_build::compile_to_fds(&["proto/hello.proto"], &["proto/"])?
let fds = oxiproto_build::compile_to_fds(&["proto/hello.proto"], &["proto/"])?;

// Simplest path: defaults
let rust_src: String = generate(&fds)?;
println!("{rust_src}");

// With options: native impls + package modules + builders
let opts = CodegenOptions {
    emit_oxi_message_impl: true,
    package_namespacing: true,
    emit_builder: true,
    ..CodegenOptions::new()
};
let rust_src = generate_with_options(&fds, &opts)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Preserve the package hierarchy as a structured tree instead of a flat string:

```rust
use oxiproto_codegen::{generate_module, CodegenOptions};

let tree = generate_module(&fds, &CodegenOptions::new())?;
for path in tree.all_paths() {
    println!("module: {}", path.join("::"));
}
let flat: String = tree.render(); // wrap each package in `pub mod { … }`
# Ok::<(), Box<dyn std::error::Error>>(())
```

## API Overview

### Top-level functions

| Function | Returns | Description |
|----------|---------|-------------|
| `generate(&fds)` | `Result<String, CodegenError>` | Generate Rust source with default options |
| `generate_with_options(&fds, &opts)` | `Result<String, CodegenError>` | Generate with custom `CodegenOptions`; runs `prettyplease` when `format_output` is set (requires `format`) |
| `generate_module(&fds, &opts)` | `Result<ModuleTree, CodegenError>` | Generate a structured `ModuleTree` preserving the package hierarchy |
| `generate_to_file(&fds, path)` | `Result<(), CodegenError>` | `generate` then write to `path` |
| `generate_to_file_with_options(&fds, path, &opts)` | `Result<(), CodegenError>` | `generate_with_options` then write to `path` |

### Lower-level emit functions

| Function | Description |
|----------|-------------|
| `emit_file_descriptor_set(&fds)` | Core emit with defaults (re-exported) |
| `emit_file_descriptor_set_with_options(&fds, &opts)` | Core emit with options (no `prettyplease` pass) |

### `CodegenOptions`

Construct with `CodegenOptions::new()` (or `Default`). Fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `generate_docs` | `bool` | `true` | Emit doc comments from proto source info |
| `generate_default` | `bool` | `true` | Emit `Default` impls for enums |
| `generate_deprecated` | `bool` | `true` | Emit `#[deprecated]` on deprecated items |
| `btree_map` | `bool` | `false` | Use `BTreeMap` for map fields instead of `HashMap` |
| `use_btree_map` | `bool` | `false` | Backward-compat alias for `btree_map` |
| `package_namespacing` | `bool` | `false` | Emit `pub mod` hierarchy matching proto packages |
| `type_attributes` | `BTreeMap<String, Vec<String>>` | empty | Per-type extra attributes, keyed by FQN |
| `field_attributes` | `BTreeMap<String, Vec<String>>` | empty | Per-field extra attributes, keyed by `Type.field` |
| `emit_oxi_message_impl` | `bool` | `false` | Emit `impl OxiMessage`/`OxiName` (needs `oxiproto-core` downstream) |
| `format_output` | `bool` | `false` | Format output via `prettyplease` (needs `format` feature) |
| `emit_services` | `bool` | `true` | Emit `pub trait` service definitions |
| `emit_json` | `bool` | `false` | Emit `to_json`/`from_json` (needs `serde_json` + `base64` downstream) |
| `emit_builder` | `bool` | `false` | Emit a `FooBuilder` with fluent setters per message |
| `emit_text_format` | `bool` | `false` | Emit `to_text_format() -> String` per message |

Helper: `use_btree_map_effective()` → `btree_map || use_btree_map`.

### `ModuleTree`

Structured representation of generated code grouped by package. One node per package segment; the root has an empty `name`.

| Member | Type / Returns | Description |
|--------|----------------|-------------|
| `name` | `String` | One package segment (empty for the root) |
| `items` | `Vec<String>` | Rendered Rust items at this level (one entry per source file) |
| `children` | `Vec<ModuleTree>` | Sub-package nodes |
| `render()` | `String` | Flatten to source, wrapping each child in `pub mod { … }` |
| `all_paths()` | `Vec<Vec<String>>` | All module paths, depth-first |

### `wkt_map` module

| Function | Description |
|----------|-------------|
| `wkt_rust_type(proto_fqn) -> Option<&'static str>` | Map a well-known-type FQN (e.g. `google.protobuf.Timestamp`, leading dot optional) to its Rust path (e.g. `::oxiproto_wkt::Timestamp`); `None` if unknown |

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `format` | no | Pulls in `syn` + `prettyplease`; enables `CodegenError::Parse` and `format_output` handling in `generate_with_options` |

## Error Variants — `CodegenError`

Implements `Display` + `std::error::Error`. Round-trips to/from `oxiproto_core::OxiProtoError` via `From`.

| Variant | Description |
|---------|-------------|
| `InvalidDescriptor(String)` | A required descriptor field was missing or invalid |
| `Io(std::io::Error)` | An I/O operation failed (e.g. `generate_to_file`) |
| `Parse(syn::Error)` | A `syn`/`prettyplease` parse error (only with the `format` feature) |

## Related crates

- [`oxiproto`](../../README.md) — top-level façade that re-exports the OxiProto stack
- [`oxiproto-core`](../oxiproto-core) — runtime traits and wire-format primitives the generated code targets
- [`oxiproto-build`](../oxiproto-build) — produces the `FileDescriptorSet` this crate consumes (use in `build.rs`)
- [`oxiproto-cli`](../oxiproto-cli) — command-line front-end (`gen` subcommand drives this crate)
- `oxiproto-json` — canonical Protobuf-JSON support
- `oxiproto-reflect` — runtime reflection over descriptors
- `oxiproto-wkt` — Google well-known types (target of `wkt_map`)

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
