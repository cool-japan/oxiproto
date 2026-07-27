#![forbid(unsafe_code)]

//! `oxiproto-protoc` — a `protoc`-argv-compatible shim backed by OxiProto's
//! pure-Rust parser.
//!
//! # Why this exists
//!
//! Third-party build scripts (`prost-build`, `tonic-build`, and everything
//! built on them) shell out to a `protoc` executable purely to turn `.proto`
//! sources into a serialised [`FileDescriptorSet`]; they then do their own
//! codegen from that descriptor set. That single step is the reason a C++
//! `protoc` install is a hard prerequisite for those crates.
//!
//! Those crates all honour the `PROTOC` environment variable. Point it at this
//! binary and the prerequisite disappears — we accept the same arguments and
//! write the same descriptor set, produced entirely in Rust.
//!
//! ```text
//! PROTOC=/path/to/oxiproto-protoc cargo build
//! ```
//!
//! # Supported surface
//!
//! Exactly the descriptor-set-generating subset of `protoc`:
//!
//! | Argument | Notes |
//! |---|---|
//! | `-I <dir>`, `-I<dir>`, `--proto_path <dir>`, `--proto_path=<dir>` | Import search path, in order |
//! | `-o <file>`, `-o<file>`, `--descriptor_set_out <file>`, `--descriptor_set_out=<file>` | Where the encoded `FileDescriptorSet` is written |
//! | `--include_imports` | Accepted; imports are *always* included |
//! | `--include_source_info` | Accepted; source info is *always* emitted |
//! | `--experimental_allow_proto3_optional` | Accepted and ignored — proto3 field presence is always supported |
//! | `<file.proto>...` | The files to compile |
//!
//! Language code-generation flags (`--cpp_out`, `--python_out`, `--plugin`,
//! …) are **not** supported. They are rejected with an explicit error rather
//! than silently ignored, so a build that actually needs real `protoc` fails
//! loudly instead of producing nothing.

use std::path::PathBuf;
use std::process::ExitCode;

use prost::Message as _;

/// `protoc` release whose descriptor-set behaviour this shim matches.
///
/// Reported by `--version` in `protoc`'s own `libprotoc <semver>` shape,
/// because build scripts parse that line, with the shim's own identity
/// appended so the output never claims to *be* upstream `protoc`.
const PROTOC_COMPAT_VERSION: &str = "3.21.12";

/// Flags that are meaningful to `protoc` but are unconditional (or irrelevant)
/// here, so accepting them is a no-op rather than an error.
const IGNORED_FLAGS: &[&str] = &[
    // The FDS we emit always contains every transitively imported file.
    "--include_imports",
    // The native parser always populates `source_code_info`.
    "--include_source_info",
    // Only ever existed to unlock proto3 `optional` on older protoc releases.
    "--experimental_allow_proto3_optional",
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("oxiproto-protoc: {msg}");
            ExitCode::FAILURE
        }
    }
}

/// Parsed command line.
#[derive(Debug, Default)]
struct Invocation {
    /// `-I` / `--proto_path` entries, in the order given.
    includes: Vec<PathBuf>,
    /// `-o` / `--descriptor_set_out` target.
    out: Option<PathBuf>,
    /// Positional `.proto` inputs.
    protos: Vec<PathBuf>,
}

fn run(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--version") {
        println!(
            "libprotoc {PROTOC_COMPAT_VERSION} (oxiproto-protoc {} shim)",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    }
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }

    let inv = parse_args(args)?;

    let out = inv.out.ok_or_else(|| {
        "no output requested: this shim only implements descriptor-set generation, \
         so `-o <FILE>` (or `--descriptor_set_out=<FILE>`) is required"
            .to_owned()
    })?;
    if inv.protos.is_empty() {
        return Err("no .proto input files given".to_owned());
    }

    let fds = oxiproto_build::compile_to_fds(&inv.protos, &inv.includes)
        .map_err(|e| format!("failed to compile .proto sources: {e}"))?;

    // protoc creates the descriptor set file but not its parent directory;
    // match that, except we tolerate a missing parent because build scripts
    // routinely point `-o` inside a freshly created OUT_DIR subpath.
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create output directory {}: {e}", parent.display()))?;
        }
    }
    std::fs::write(&out, fds.encode_to_vec())
        .map_err(|e| format!("cannot write descriptor set to {}: {e}", out.display()))?;

    Ok(())
}

/// Parse a `protoc`-style argument list.
///
/// Handles both the split (`-I dir`) and attached (`-Idir`, `--proto_path=dir`)
/// spellings that `protoc` accepts, because different build scripts use
/// different ones.
fn parse_args(args: &[String]) -> Result<Invocation, String> {
    let mut inv = Invocation::default();
    let mut iter = args.iter().peekable();

    while let Some(arg) = iter.next() {
        // `--` ends option parsing; everything after is a positional input.
        if arg == "--" {
            inv.protos.extend(iter.map(PathBuf::from));
            break;
        }

        if IGNORED_FLAGS.contains(&arg.as_str()) {
            continue;
        }

        if let Some(value) = take_value(arg, &mut iter, &["-I", "--proto_path"])? {
            inv.includes.push(PathBuf::from(value));
            continue;
        }

        if let Some(value) = take_value(arg, &mut iter, &["-o", "--descriptor_set_out"])? {
            inv.out = Some(PathBuf::from(value));
            continue;
        }

        if arg.starts_with('-') {
            return Err(format!(
                "unsupported protoc option `{arg}`: oxiproto-protoc implements only \
                 descriptor-set generation (-I / -o / --include_imports / \
                 --include_source_info). A build needing real code-generation plugins \
                 must use upstream protoc."
            ));
        }

        inv.protos.push(PathBuf::from(arg));
    }

    Ok(inv)
}

/// If `arg` names one of `keys`, return its value — either attached
/// (`-Idir`, `--proto_path=dir`) or as the following argument (`-I dir`).
///
/// Returns `Ok(None)` when `arg` is not one of `keys`, leaving the iterator
/// untouched so the caller can try the next matcher.
fn take_value<'a, I>(
    arg: &str,
    iter: &mut std::iter::Peekable<I>,
    keys: &[&str],
) -> Result<Option<String>, String>
where
    I: Iterator<Item = &'a String>,
{
    for key in keys {
        // Exact match: value is the next argument.
        if arg == *key {
            return iter
                .next()
                .map(|v| Some(v.clone()))
                .ok_or_else(|| format!("option `{key}` requires a value"));
        }
        // `--key=value`
        if let Some(rest) = arg.strip_prefix(&format!("{key}=")) {
            if rest.is_empty() {
                return Err(format!("option `{key}` requires a value"));
            }
            return Ok(Some(rest.to_owned()));
        }
        // Attached short form (`-Idir`, `-ofile`) — single-dash keys only, so
        // that `--proto_pathfoo` is not silently accepted as a value.
        if key.len() == 2 && !key.starts_with("--") {
            if let Some(rest) = arg.strip_prefix(key) {
                if !rest.is_empty() {
                    return Ok(Some(rest.to_owned()));
                }
            }
        }
    }
    Ok(None)
}

fn print_help() {
    println!(
        "oxiproto-protoc {} — protoc-compatible descriptor-set generator (pure Rust)

Usage: oxiproto-protoc [-I DIR]... -o FILE [OPTIONS] FILE.proto...

Set PROTOC to this binary so that prost-build / tonic-build style build
scripts can compile .proto sources without a C++ protoc installation.

Options:
  -I, --proto_path DIR        Directory to search for imports (repeatable)
  -o, --descriptor_set_out F  Write the encoded FileDescriptorSet to F
      --include_imports       Accepted; imports are always included
      --include_source_info   Accepted; source info is always emitted
      --version               Print version information
  -h, --help                  Print this help

Code-generation options (--cpp_out, --python_out, --plugin, ...) are not
supported and are rejected rather than ignored.",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Invocation {
        let owned: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        parse_args(&owned).expect("parse must succeed")
    }

    fn parse_err(args: &[&str]) -> String {
        let owned: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        parse_args(&owned).expect_err("parse must fail")
    }

    #[test]
    fn parses_the_exact_prost_build_invocation() {
        // This is what prost-build 0.14 emits, in order.
        let inv = parse(&[
            "--include_imports",
            "--include_source_info",
            "-o",
            "/out/fds.bin",
            "-I",
            "/src/proto",
            "/src/proto/service.proto",
        ]);
        assert_eq!(inv.out, Some(PathBuf::from("/out/fds.bin")));
        assert_eq!(inv.includes, vec![PathBuf::from("/src/proto")]);
        assert_eq!(inv.protos, vec![PathBuf::from("/src/proto/service.proto")]);
    }

    #[test]
    fn parses_attached_and_equals_forms() {
        let inv = parse(&[
            "-I/a",
            "--proto_path=/b",
            "--descriptor_set_out=/out.bin",
            "x.proto",
        ]);
        assert_eq!(inv.includes, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(inv.out, Some(PathBuf::from("/out.bin")));
        assert_eq!(inv.protos, vec![PathBuf::from("x.proto")]);
    }

    #[test]
    fn include_order_is_preserved() {
        let inv = parse(&["-I", "/first", "-I", "/second", "-I", "/third", "x.proto"]);
        assert_eq!(
            inv.includes,
            vec![
                PathBuf::from("/first"),
                PathBuf::from("/second"),
                PathBuf::from("/third"),
            ]
        );
    }

    #[test]
    fn experimental_proto3_optional_flag_is_accepted() {
        let inv = parse(&["--experimental_allow_proto3_optional", "-o", "/o", "x.proto"]);
        assert_eq!(inv.protos, vec![PathBuf::from("x.proto")]);
    }

    #[test]
    fn double_dash_ends_option_parsing() {
        let inv = parse(&["-o", "/o", "--", "-weird-name.proto"]);
        assert_eq!(inv.protos, vec![PathBuf::from("-weird-name.proto")]);
    }

    #[test]
    fn codegen_options_are_rejected_not_ignored() {
        let err = parse_err(&["--cpp_out=/gen", "x.proto"]);
        assert!(err.contains("--cpp_out"), "error must name the option: {err}");
    }

    #[test]
    fn missing_value_is_an_error() {
        let err = parse_err(&["-I"]);
        assert!(err.contains("requires a value"), "got: {err}");
    }

    #[test]
    fn long_option_prefix_is_not_treated_as_attached_value() {
        // `--proto_pathfoo` must not parse as `--proto_path=foo`.
        let err = parse_err(&["--proto_pathfoo", "x.proto"]);
        assert!(err.contains("unsupported protoc option"), "got: {err}");
    }
}
