#![forbid(unsafe_code)]

use super::span::Span;

/// Errors produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum LexError {
    /// An unexpected character was encountered.
    UnexpectedChar { ch: char, span: Span },
    /// A string literal was not closed before end-of-input.
    UnterminatedString { span: Span },
    /// A block comment was not closed before end-of-input.
    UnterminatedBlockComment { span: Span },
    /// An unrecognised escape sequence inside a string literal.
    InvalidEscape { ch: char, span: Span },
    /// An integer literal exceeded `u64::MAX`.
    IntOverflow { span: Span },
    /// A floating-point literal could not be parsed.
    FloatParseError { span: Span },
    /// A `\xHH` hex escape had fewer than 2 valid hex digits.
    InvalidHexEscape { span: Span },
    /// A `\uXXXX` or `\UXXXXXXXX` codepoint was out of range or malformed.
    InvalidUnicodeEscape { span: Span },
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LexError::UnexpectedChar { ch, span } => {
                write!(
                    f,
                    "unexpected character {:?} at byte offset {}",
                    ch, span.start
                )
            }
            LexError::UnterminatedString { span } => {
                write!(
                    f,
                    "unterminated string literal starting at byte {}",
                    span.start
                )
            }
            LexError::UnterminatedBlockComment { span } => {
                write!(
                    f,
                    "unterminated block comment starting at byte {}",
                    span.start
                )
            }
            LexError::InvalidEscape { ch, span } => {
                write!(
                    f,
                    "invalid escape sequence \\{:?} at byte offset {}",
                    ch, span.start
                )
            }
            LexError::IntOverflow { span } => {
                write!(f, "integer literal overflow at byte offset {}", span.start)
            }
            LexError::FloatParseError { span } => {
                write!(
                    f,
                    "cannot parse float literal at byte offset {}",
                    span.start
                )
            }
            LexError::InvalidHexEscape { span } => {
                write!(f, "invalid \\xHH hex escape at byte offset {}", span.start)
            }
            LexError::InvalidUnicodeEscape { span } => {
                write!(f, "invalid unicode escape at byte offset {}", span.start)
            }
        }
    }
}

impl std::error::Error for LexError {}

/// Errors produced by the outline parser.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// A lexer error was encountered while parsing.
    Lex(LexError),
    /// An unexpected token was encountered; carries what was expected and found.
    UnexpectedToken {
        expected: String,
        found: String,
        span: Span,
    },
    /// The token stream ended before the parse was complete.
    UnexpectedEof,
    /// A `{` was opened but never closed.
    UnbalancedBraces { span: Span },
    /// The `syntax` statement contained an unrecognised value.
    UnknownSyntax(String),
    /// A proto2 `group` field name does not start with an uppercase letter.
    MalformedGroupName { name: String, span: Span },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Lex(e) => write!(f, "lex error: {e}"),
            ParseError::UnexpectedToken {
                expected,
                found,
                span,
            } => {
                write!(
                    f,
                    "expected {expected} but found {found} at byte offset {}",
                    span.start
                )
            }
            ParseError::UnexpectedEof => write!(f, "unexpected end of file"),
            ParseError::UnbalancedBraces { span } => {
                write!(
                    f,
                    "unbalanced braces: unclosed '{{' at byte offset {}",
                    span.start
                )
            }
            ParseError::UnknownSyntax(s) => {
                write!(
                    f,
                    "unknown syntax value: expected \"proto2\" or \"proto3\", found {:?}",
                    s
                )
            }
            ParseError::MalformedGroupName { name, span } => {
                write!(
                    f,
                    "proto2 group name must start with an uppercase letter: {:?} at byte offset {}",
                    name, span.start
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError::Lex(e)
    }
}
