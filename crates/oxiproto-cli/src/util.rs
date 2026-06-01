#![forbid(unsafe_code)]

use std::io::IsTerminal;

/// Verbosity level from global flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct Verbosity {
    pub quiet: bool,
    pub verbose: bool,
}

impl Verbosity {
    /// Print an informational message to stderr unless quiet mode is on.
    pub fn info(&self, msg: &str) {
        if !self.quiet {
            eprintln!("{msg}");
        }
    }

    /// Print a verbose progress message to stderr (cyan when terminal).
    pub fn verbose(&self, msg: &str) {
        if self.verbose && !self.quiet {
            if std::io::stderr().is_terminal() {
                use anstyle::{AnsiColor, Color, Style};
                let style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
                eprintln!("{style}{msg}{style:#}");
            } else {
                eprintln!("{msg}");
            }
        }
    }

    /// Print an error message to stderr — always, even in quiet mode.
    pub fn error(&self, msg: &str) {
        if std::io::stderr().is_terminal() {
            use anstyle::{AnsiColor, Color, Style};
            let style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));
            eprintln!("{style}error: {msg}{style:#}");
        } else {
            eprintln!("error: {msg}");
        }
    }
}
