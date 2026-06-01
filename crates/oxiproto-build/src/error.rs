#![forbid(unsafe_code)]

//! Structured error type for `oxiproto-build` operations.

use oxiproto_core::OxiProtoError;
use std::io;

/// Rich error type for `.proto` compilation operations.
///
/// Carries structured location information where available so that IDEs and
/// build-script consumers can point the user directly to the offending source
/// position.
#[derive(Debug)]
pub enum BuildError {
    /// A `.proto` file has a syntax error with location information.
    Parse {
        /// Path to the file containing the error (empty when not available).
        file: String,
        /// 1-indexed line number (`0` when not available).
        line: u32,
        /// 1-indexed column number (`0` when not available).
        col: u32,
        /// Human-readable error message.
        message: String,
    },
    /// Code generation failed.
    Codegen {
        /// Human-readable description of what went wrong.
        message: String,
    },
    /// An I/O error occurred (e.g. reading a `.proto` file or writing output).
    Io(io::Error),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Parse {
                file,
                line,
                col,
                message,
            } => {
                if file.is_empty() {
                    write!(f, "parse error: {message}")
                } else if *col == 0 {
                    write!(f, "{file}:{line}: {message}")
                } else {
                    write!(f, "{file}:{line}:{col}: {message}")
                }
            }
            BuildError::Codegen { message } => write!(f, "codegen error: {message}"),
            BuildError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BuildError::Io(e) => Some(e),
            BuildError::Parse { .. } | BuildError::Codegen { .. } => None,
        }
    }
}

impl From<OxiProtoError> for BuildError {
    fn from(e: OxiProtoError) -> Self {
        match &e {
            OxiProtoError::ParseError(msg) => {
                // Try to parse "file:line:col: message" prefix from msg.
                BuildError::from_parse_string(msg)
            }
            OxiProtoError::CodegenError(msg) => BuildError::Codegen {
                message: msg.clone(),
            },
            OxiProtoError::IoError(io_err) => {
                // io::Error is not Clone; reconstruct from kind + Display.
                BuildError::Io(io::Error::new(io_err.kind(), io_err.to_string()))
            }
            OxiProtoError::WireFormatError(w) => BuildError::Codegen {
                message: w.to_string(),
            },
            // #[non_exhaustive] — catch all remaining variants.
            _ => BuildError::Codegen {
                message: e.to_string(),
            },
        }
    }
}

impl From<BuildError> for OxiProtoError {
    fn from(e: BuildError) -> Self {
        OxiProtoError::ParseError(e.to_string())
    }
}

impl From<io::Error> for BuildError {
    fn from(e: io::Error) -> Self {
        BuildError::Io(e)
    }
}

impl BuildError {
    /// Attempt to parse a `"file:line:col: message"` or `"file:line: message"`
    /// prefix from `msg`. Falls back to `Parse { file: "", line: 0, col: 0, … }`
    /// when the prefix is absent or malformed.
    pub(crate) fn from_parse_string(msg: &str) -> Self {
        // Strategy: split on ':' and try to read u32 segments at positions 1 and 2.
        // We must handle Windows drive letters like "C:\path\file.proto:3:5: …"
        // by skipping single-char segments as likely drive letters.
        let parts: Vec<&str> = msg.splitn(5, ':').collect();

        // Minimum required: file, line, col, (rest) — or file, line, (rest).
        // We need at least 3 parts to attempt any parsing.
        if parts.len() >= 3 {
            // Handle Windows drive letter prefix: if parts[0] is a single ASCII
            // alpha char the "file" part is actually parts[0] + ":" + parts[1].
            let (file_raw, line_idx, col_idx) = if parts[0].len() == 1
                && parts[0]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
            {
                // Windows drive letter — need at least 5 parts total.
                if parts.len() >= 5 {
                    let file = format!("{}:{}", parts[0], parts[1]);
                    (file, 2usize, 3usize)
                } else {
                    // Not enough parts; fall back.
                    return Self::fallback(msg);
                }
            } else {
                (parts[0].to_owned(), 1usize, 2usize)
            };

            if let Ok(line) = parts[line_idx].trim().parse::<u32>() {
                // We have a valid line number; now try col.
                if let Ok(col) = parts[col_idx].trim().parse::<u32>() {
                    // "file:line:col: message" pattern.
                    let message = parts[(col_idx + 1)..]
                        .join(":")
                        .trim_start_matches(' ')
                        .to_owned();
                    return BuildError::Parse {
                        file: file_raw,
                        line,
                        col,
                        message: if message.is_empty() {
                            msg.to_owned()
                        } else {
                            message
                        },
                    };
                }
                // "file:line: message" pattern (no col).
                let message = parts[(line_idx + 1)..]
                    .join(":")
                    .trim_start_matches(' ')
                    .to_owned();
                return BuildError::Parse {
                    file: file_raw,
                    line,
                    col: 0,
                    message: if message.is_empty() {
                        msg.to_owned()
                    } else {
                        message
                    },
                };
            }
        }

        Self::fallback(msg)
    }

    fn fallback(msg: &str) -> Self {
        BuildError::Parse {
            file: String::new(),
            line: 0,
            col: 0,
            message: msg.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_file_line_col() {
        let e = BuildError::from_parse_string("foo.proto:3:7: unexpected token");
        match e {
            BuildError::Parse {
                file,
                line,
                col,
                message,
            } => {
                assert_eq!(file, "foo.proto");
                assert_eq!(line, 3);
                assert_eq!(col, 7);
                assert!(message.contains("unexpected"));
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn parse_file_line_no_col() {
        let e = BuildError::from_parse_string("bar.proto:10: missing semicolon");
        match e {
            BuildError::Parse {
                file,
                line,
                col,
                message,
            } => {
                assert_eq!(file, "bar.proto");
                assert_eq!(line, 10);
                assert_eq!(col, 0);
                assert!(message.contains("semicolon"));
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn parse_fallback_on_plain_message() {
        let e = BuildError::from_parse_string("something went wrong");
        match e {
            BuildError::Parse {
                file,
                line,
                col,
                message,
            } => {
                assert!(file.is_empty());
                assert_eq!(line, 0);
                assert_eq!(col, 0);
                assert_eq!(message, "something went wrong");
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn display_with_location() {
        let e = BuildError::Parse {
            file: "test.proto".to_owned(),
            line: 5,
            col: 3,
            message: "oops".to_owned(),
        };
        assert_eq!(e.to_string(), "test.proto:5:3: oops");
    }

    #[test]
    fn display_without_location() {
        let e = BuildError::Codegen {
            message: "bad output".to_owned(),
        };
        assert_eq!(e.to_string(), "codegen error: bad output");
    }
}
