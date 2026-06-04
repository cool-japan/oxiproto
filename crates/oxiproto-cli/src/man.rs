#![forbid(unsafe_code)]

//! `man` subcommand — generate man page(s) for `oxiproto-cli`.
//!
//! Uses [`clap_mangen`] to produce standard ROFF-formatted man pages for the
//! top-level command and each subcommand.  All pages are written into the
//! requested output directory (created automatically when absent).

use std::path::PathBuf;

use crate::util::Verbosity;

/// Run the `man` subcommand: generate man page files into `output_dir`.
///
/// Delegates to `clap_mangen::generate_to` which writes one `.1` file for
/// `oxiproto-cli` and one for every visible subcommand.
///
/// # Errors
///
/// Returns an error if the output directory cannot be created or if any man
/// page file cannot be written.
pub fn run(output: PathBuf, verbosity: Verbosity) -> Result<(), Box<dyn std::error::Error>> {
    use clap::CommandFactory;

    std::fs::create_dir_all(&output)?;

    let cmd = super::Cli::command();
    clap_mangen::generate_to(cmd, &output)?;

    verbosity.info(&format!("Man pages written to {}", output.display()));
    Ok(())
}
