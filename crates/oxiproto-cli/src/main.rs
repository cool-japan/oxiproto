#![forbid(unsafe_code)]

use clap::{CommandFactory, Parser, Subcommand};

/// Compile .proto files to plain Rust structs (no protoc required).
#[derive(Parser)]
#[command(
    name = "oxiproto-cli",
    about = "Compile .proto files to plain Rust structs"
)]
struct Cli {
    /// Suppress all non-error output.
    #[arg(
        long,
        short = 'q',
        global = true,
        help = "Suppress all non-error output"
    )]
    quiet: bool,

    /// Print verbose progress messages.
    #[arg(long, short = 'v', global = true, help = "Print verbose progress")]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile .proto files to plain Rust structs via the OxiProto codegen path.
    Gen(gen::GenArgs),
    /// Print a human-readable summary of types in a .proto file.
    Describe(describe::DescribeArgs),
    /// Encode canonical Protobuf-JSON to binary protobuf wire format.
    Encode(convert::ConvertArgs),
    /// Decode binary protobuf wire format to canonical Protobuf-JSON.
    Decode(convert::ConvertArgs),
    /// Detect wire-breaking changes between two versions of .proto files.
    Breaking(breaking::BreakingArgs),
    /// Generate Markdown documentation from .proto files
    Doc(doc::DocArgs),
    /// Format .proto files to canonical style
    Format(format::FormatArgs),
    /// Lint .proto files for style violations
    Lint(lint::LintArgs),
    /// Generate shell completions for the given shell.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

fn main() {
    let cli = Cli::parse();
    let verbosity = util::Verbosity {
        quiet: cli.quiet,
        verbose: cli.verbose,
    };
    let result = match cli.command {
        Command::Gen(args) => gen::run(args, verbosity),
        Command::Describe(args) => describe::run(args, verbosity),
        Command::Encode(args) => convert::run_encode(args, verbosity),
        Command::Decode(args) => convert::run_decode(args, verbosity),
        Command::Breaking(args) => breaking::run(args, verbosity),
        Command::Doc(args) => doc::run(args, verbosity),
        Command::Format(args) => format::run(args, verbosity),
        Command::Lint(args) => lint::run(args, verbosity),
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            let bin_name = cmd.get_name().to_owned();
            clap_complete::generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
            Ok(())
        }
    };
    if let Err(e) = result {
        verbosity.error(&e.to_string());
        std::process::exit(1);
    }
}

mod breaking;
mod convert;
mod describe;
mod doc;
mod format;
mod gen;
mod lint;
mod util;
