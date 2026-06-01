#![forbid(unsafe_code)]

use clap::Args;
use std::path::{Path, PathBuf};

use crate::util::Verbosity;

/// Arguments for the `gen` subcommand.
#[derive(Args)]
pub struct GenArgs {
    /// Input .proto files or directories (at least one required).
    #[arg(required = true)]
    pub protos: Vec<PathBuf>,

    /// Output directory for generated Rust files.
    #[arg(short, long, default_value = ".")]
    pub output: PathBuf,

    /// Include paths for resolving proto imports (may be repeated).
    #[arg(short = 'I', long)]
    pub include: Vec<PathBuf>,

    /// Print generated code to stdout instead of writing files.
    #[arg(long, help = "Print generated code to stdout instead of writing files")]
    pub dry_run: bool,

    /// Generate JSON serialization impls alongside messages.
    #[arg(long, help = "Generate JSON serialization impls alongside messages")]
    pub json: bool,

    /// Generate gRPC service traits (default: enabled).
    #[arg(
        long,
        action = clap::ArgAction::Set,
        default_value_t = true,
        help = "Generate gRPC service traits"
    )]
    pub grpc: bool,

    /// Process directories recursively for *.proto files.
    #[arg(long, help = "Process directories recursively for *.proto files")]
    pub recursive: bool,

    /// Generate prost-compatible output with derive macros.
    #[arg(long, help = "Generate prost-compatible output with derive macros")]
    pub prost_compat: bool,
}

/// Run the `gen` subcommand: compile each `.proto` to a Rust source file.
///
/// # Errors
///
/// Returns an error if any proto file is missing, if parsing fails, if
/// codegen fails, or if writing output files fails.
pub fn run(args: GenArgs, verbosity: Verbosity) -> Result<(), Box<dyn std::error::Error>> {
    // Collect all proto files from inputs (expanding directories if recursive).
    let mut all_protos: Vec<PathBuf> = Vec::new();
    for input in &args.protos {
        let files = collect_proto_files(input, args.recursive)?;
        if files.is_empty() {
            return Err(format!("no .proto files found at: {}", input.display()).into());
        }
        all_protos.extend(files);
    }

    if all_protos.is_empty() {
        return Err("no .proto files to process".into());
    }

    // Validate each resolved file exists.
    for proto in &all_protos {
        if !proto.exists() {
            return Err(format!("proto file not found: {}", proto.display()).into());
        }
    }

    verbosity.verbose(&format!("Processing {} proto file(s)", all_protos.len()));

    // When dry_run is false, create the output directory.
    if !args.dry_run {
        std::fs::create_dir_all(&args.output)?;
    }

    // Parse .proto → FileDescriptorSet using the pure-Rust native parser
    // exposed by oxiproto-build.  No `protoc` binary is ever invoked.
    let fds = oxiproto_build::compile_to_fds(&all_protos, &args.include)?;

    // When --prost-compat is requested, delegate to prost_build for codegen.
    if args.prost_compat {
        if args.dry_run {
            return Err("--dry-run is not supported with --prost-compat".into());
        }
        std::fs::create_dir_all(&args.output)?;
        let mut config = prost_build::Config::new();
        config.out_dir(&args.output);
        config
            .compile_fds(fds)
            .map_err(|e| format!("prost-compat codegen failed: {e}"))?;
        verbosity.info(&format!(
            "Generated (prost-compat) in {}",
            args.output.display()
        ));
        return Ok(());
    }

    // Generate plain Rust source from the descriptor set.
    let mut codegen_opts = oxiproto_codegen::CodegenOptions::new();
    codegen_opts.emit_services = args.grpc;
    codegen_opts.emit_json = args.json;
    let rust_source = oxiproto_codegen::generate_with_options(&fds, &codegen_opts)?;

    // Derive output filename.
    let out_filename = derive_output_filename(&all_protos[0])?;

    if args.dry_run {
        // Dry-run: print to stdout, skip file writes.
        verbosity.verbose("Dry run: printing to stdout");
        print!("{rust_source}");
    } else {
        let out_file = args.output.join(&out_filename);
        std::fs::write(&out_file, &rust_source)?;
        verbosity.info(&format!("Generated: {}", out_file.display()));
    }

    Ok(())
}

/// Derive the output filename from a proto file path.
///
/// Strategy:
/// 1. Scan the first 20 lines for a `package foo.bar;` declaration.
///    If found, convert dots to underscores → `foo_bar.rs`.
/// 2. Fall back to the file stem → `service.rs` from `service.proto`.
/// 3. Error if the stem is empty.
fn derive_output_filename(proto_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    // Attempt to open and scan the file for a package declaration.
    if let Ok(contents) = std::fs::read_to_string(proto_path) {
        for line in contents.lines().take(20) {
            let trimmed = line.trim();
            // Skip comment lines.
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
                continue;
            }
            if trimmed.starts_with("package ") && trimmed.ends_with(';') {
                let pkg = trimmed
                    .trim_start_matches("package ")
                    .trim_end_matches(';')
                    .trim();
                if !pkg.is_empty() {
                    let name = pkg.replace('.', "_");
                    return Ok(format!("{name}.rs"));
                }
            }
        }
    }

    // Fall back to the file stem.
    let stem = proto_path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            format!(
                "cannot derive output filename from: {}",
                proto_path.display()
            )
        })?;

    Ok(format!("{stem}.rs"))
}

/// Collect proto files from `path`.
///
/// If `path` is a file, returns it directly (regardless of `recursive`).
/// If `path` is a directory, walks it for `*.proto` files.  When `recursive`
/// is `false`, only the top-level directory is scanned.
fn collect_proto_files(path: &Path, recursive: bool) -> std::io::Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_owned()]);
    }
    if !path.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("path not found: {}", path.display()),
        ));
    }
    let mut result = Vec::new();
    collect_proto_recursive(path, recursive, &mut result)?;
    Ok(result)
}

/// Recursively (or not) walk `dir` and append `*.proto` paths to `out`.
fn collect_proto_recursive(
    dir: &Path,
    recursive: bool,
    out: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden/build directories.
        if name_str == ".git" || name_str == "target" {
            continue;
        }

        if path.is_dir() && recursive {
            collect_proto_recursive(&path, recursive, out)?;
        } else if path.extension().map(|e| e == "proto").unwrap_or(false) {
            out.push(path);
        }
    }
    Ok(())
}
