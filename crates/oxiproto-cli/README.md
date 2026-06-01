# oxiproto-cli — Command-line Protocol Buffers compiler, no `protoc` required

[![Crates.io](https://img.shields.io/crates/v/oxiproto-cli.svg)](https://crates.io/crates/oxiproto-cli)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiproto-cli` is the command-line front-end of the **OxiProto** stack — COOLJAPAN's Pure-Rust Protocol Buffers implementation. It compiles `.proto` files to plain Rust structs and provides a toolbox around protobuf schemas: describe types, encode/decode between Protobuf-JSON and the binary wire format, detect wire-breaking changes, render Markdown docs, format, and lint. **No `protoc` binary is required** — `.proto` parsing is performed entirely in Rust (`oxiproto-build`'s native parser), and codegen runs through `oxiproto-codegen` / `prost-build`.

The binary installs as `oxiproto-cli` and is `#![forbid(unsafe_code)]`.

## Installation

```bash
cargo install oxiproto-cli
```

Verify:

```bash
oxiproto-cli --help
oxiproto-cli gen --help
```

## Quick Start

```bash
# Compile a .proto into a Rust source file in the current directory
oxiproto-cli gen proto/hello.proto -I proto/

# Print generated Rust to stdout without writing files
oxiproto-cli gen proto/hello.proto -I proto/ --dry-run

# Summarize the types declared in a schema
oxiproto-cli describe proto/hello.proto -I proto/

# Encode canonical Protobuf-JSON to binary wire format
echo '{"name":"Ada"}' | oxiproto-cli encode proto/hello.proto -t hello.Greeting -o out.bin

# Decode binary wire format back to Protobuf-JSON
oxiproto-cli decode proto/hello.proto -t hello.Greeting -i out.bin

# Fail if the new schema introduces wire-breaking changes
oxiproto-cli breaking --old old/hello.proto --new new/hello.proto
```

## Global Options

| Flag | Description |
|------|-------------|
| `-q`, `--quiet` | Suppress all non-error output |
| `-v`, `--verbose` | Print verbose progress messages |
| `-h`, `--help` | Print help |

## Commands

### `gen` — compile `.proto` to plain Rust

| Argument / flag | Description |
|-----------------|-------------|
| `<PROTOS>...` | Input `.proto` files or directories (at least one required) |
| `-o`, `--output <DIR>` | Output directory for generated Rust (default: `.`) |
| `-I`, `--include <DIR>` | Include path for resolving imports (repeatable) |
| `--dry-run` | Print generated code to stdout instead of writing files |
| `--json` | Generate JSON serialization impls alongside messages |
| `--grpc <BOOL>` | Generate gRPC service traits (default: `true`) |
| `--recursive` | Process directories recursively for `*.proto` files |
| `--prost-compat` | Generate prost-compatible output with derive macros |

### `describe` — summarize schema types

| Argument / flag | Description |
|-----------------|-------------|
| `<PROTOS>...` | Input `.proto` files (at least one required) |
| `-I`, `--include <DIR>` | Include path for resolving imports (repeatable) |

Prints a human-readable summary of every message, enum, and service.

### `encode` — Protobuf-JSON → binary wire format

| Argument / flag | Description |
|-----------------|-------------|
| `<PROTOS>...` | `.proto` files defining the message type (at least one required) |
| `-t`, `--message-type <NAME>` | Fully-qualified message type, e.g. `my.package.MyMessage` |
| `-i`, `--input <FILE>` | Input file (reads stdin if omitted) |
| `-o`, `--output <FILE>` | Output file (writes stdout if omitted) |
| `-I`, `--include <DIR>` | Include path for resolving imports (repeatable) |

### `decode` — binary wire format → Protobuf-JSON

Same arguments and flags as `encode`; reads binary protobuf and emits canonical Protobuf-JSON.

### `breaking` — detect wire-breaking changes

| Argument / flag | Description |
|-----------------|-------------|
| `--old <FILE>` | Old (baseline) `.proto` files (required, repeatable) |
| `-I`, `--old-include <DIR>` | Include paths for the old protos |
| `--new <FILE>` | New (updated) `.proto` files (required, repeatable) |
| `-J`, `--new-include <DIR>` | Include paths for the new protos |

Exits non-zero when any wire-breaking change is detected.

### `doc` — generate Markdown documentation

| Argument / flag | Description |
|-----------------|-------------|
| `<PROTO_FILE>...` | `.proto` files to document (at least one required) |
| `-I`, `--include <DIR>` | Include directories for import resolution (repeatable) |
| `-o`, `--output <FILE>` | Write output to a file (default: stdout) |

Renders Markdown including leading comments from `source_code_info`.

### `format` — canonical-style formatter

| Argument / flag | Description |
|-----------------|-------------|
| `<PROTO_FILE>...` | `.proto` files to format (at least one required) |
| `-I`, `--include <DIR>` | Include paths for import resolution (repeatable) |
| `--in-place` | Rewrite files in place (default: print to stdout) |

### `lint` — style/naming-convention checker

| Argument / flag | Description |
|-----------------|-------------|
| `<PROTO_FILE>...` | `.proto` files to lint (at least one required) |
| `-I`, `--include <DIR>` | Include paths for import resolution (repeatable) |
| `--output <FORMAT>` | Output format: `text` (default) or `json` |

Returns non-zero when any violation is found. Rules include:

| Rule | Checks |
|------|--------|
| `MESSAGE_NAMES_UPPER_CAMEL_CASE` | Message names are `UpperCamelCase` |
| `ENUM_NAMES_UPPER_CAMEL_CASE` | Enum names are `UpperCamelCase` |
| `ENUM_VALUE_NAMES_UPPER_SNAKE_CASE` | Enum value names are `UPPER_SNAKE_CASE` |
| `ENUM_VALUE_PREFIX` | Enum values are prefixed with the enum name |
| `FIELD_NAMES_LOWER_SNAKE_CASE` | Field names are `lower_snake_case` |
| `SERVICE_NAMES_UPPER_CAMEL_CASE` | Service names are `UpperCamelCase` |
| `RPC_NAMES_UPPER_CAMEL_CASE` | RPC method names are `UpperCamelCase` |

### `completions` — shell completion scripts

| Argument | Description |
|----------|-------------|
| `<SHELL>` | Target shell: `bash`, `zsh`, `fish`, `powershell`, or `elvish` |

Writes a completion script for the given shell to stdout, e.g. `oxiproto-cli completions bash > oxiproto-cli.bash`.

## Exit Status

`0` on success; `1` when a command fails (parse/codegen/I/O error, a detected breaking change, or lint violations). The error message is written to stderr.

## Related crates

- [`oxiproto`](../../README.md) — top-level façade that re-exports the OxiProto stack
- [`oxiproto-core`](../oxiproto-core) — runtime traits and wire-format primitives
- [`oxiproto-codegen`](../oxiproto-codegen) — backs the `gen` command's plain-Rust output
- [`oxiproto-build`](../oxiproto-build) — provides the native `.proto` parser used by every command
- `oxiproto-json` — backs `encode` / `decode` (canonical Protobuf-JSON)
- `oxiproto-reflect` — descriptor pool / dynamic messages used by `encode` / `decode`
- `oxiproto-wkt` — Google well-known types

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
