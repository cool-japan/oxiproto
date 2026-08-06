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
    /// The `edition` statement contained an unrecognised or unsupported value.
    ///
    /// Only `"2023"` is accepted. Later editions are rejected rather than
    /// approximated: an edition *is* its feature-default table, so compiling a
    /// file against guessed defaults would emit a descriptor set that diverges
    /// from `protoc` on the wire while appearing to succeed. See
    /// [`Edition`](crate::parser::ast::Edition) for the specific blockers.
    UnsupportedEdition(String),
    /// Both `syntax` and `edition` were specified in the same file.
    SyntaxAndEditionConflict,
    /// A proto2 `group` field name does not start with an uppercase letter.
    MalformedGroupName { name: String, span: Span },
    /// The source nested message / group / option-literal definitions deeper
    /// than the parser's fixed budget.
    ///
    /// Returned instead of recursing further, so that a maliciously deep
    /// `.proto` source (e.g. `message A{message B{message C{...}}}`) cannot
    /// overflow the parser's stack.
    NestingLimitExceeded { limit: u32, span: Span },
    /// An `option features.<name>` named a feature this implementation does not
    /// know (see [`crate::parser::features::KNOWN_FEATURES`]).
    UnknownFeature {
        /// The feature name, without the `features.` prefix.
        name: String,
        /// Where the offending option appeared.
        span: Span,
    },
    /// A `features.<name>` option had a value that is not one of the feature's
    /// enumerators (e.g. `features.field_presence = SOMETIMES`).
    InvalidFeatureValue {
        /// The feature name, without the `features.` prefix.
        feature: String,
        /// The rejected value, rendered for the diagnostic.
        value: String,
        /// Where the offending option appeared.
        span: Span,
    },
    /// A `features.*` option appeared in a file that uses `syntax` rather than
    /// `edition`.  Feature resolution only exists for Protobuf Editions.
    FeaturesRequireEdition {
        /// The feature name, without the `features.` prefix.
        name: String,
        /// Where the offending option appeared.
        span: Span,
    },
    /// A feature was set on a declaration it cannot apply to (for example
    /// `features.field_presence` on a `repeated` field).
    FeatureNotApplicable {
        /// The feature name, without the `features.` prefix.
        feature: String,
        /// Why the feature does not apply here.
        reason: String,
        /// Where the offending declaration appeared.
        span: Span,
    },
    /// A construct that Protobuf Editions removed was used in an edition file
    /// (the `optional` / `required` labels and `group` fields).
    EditionSyntaxNotAllowed {
        /// The removed construct, e.g. `"the 'required' label"`.
        construct: &'static str,
        /// The edition-native replacement.
        hint: &'static str,
        /// Where the offending declaration appeared.
        span: Span,
    },
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
            ParseError::UnsupportedEdition(s) => {
                write!(f, "unsupported edition: expected \"2023\", found {:?}", s)
            }
            ParseError::SyntaxAndEditionConflict => {
                write!(
                    f,
                    "a .proto file cannot specify both 'syntax' and 'edition'"
                )
            }
            ParseError::MalformedGroupName { name, span } => {
                write!(
                    f,
                    "proto2 group name must start with an uppercase letter: {:?} at byte offset {}",
                    name, span.start
                )
            }
            ParseError::NestingLimitExceeded { limit, span } => {
                write!(
                    f,
                    "nesting depth exceeded the limit of {limit} at byte offset {}",
                    span.start
                )
            }
            ParseError::UnknownFeature { name, span } => {
                write!(
                    f,
                    "unknown edition feature 'features.{name}' at byte offset {}; known features: {}",
                    span.start,
                    crate::parser::features::KNOWN_FEATURES.join(", ")
                )
            }
            ParseError::InvalidFeatureValue {
                feature,
                value,
                span,
            } => {
                write!(
                    f,
                    "invalid value {value} for 'features.{feature}' at byte offset {}",
                    span.start
                )
            }
            ParseError::FeaturesRequireEdition { name, span } => {
                write!(
                    f,
                    "'features.{name}' at byte offset {} requires an 'edition' file; \
                     feature resolution does not apply to syntax = \"proto2\"/\"proto3\"",
                    span.start
                )
            }
            ParseError::FeatureNotApplicable {
                feature,
                reason,
                span,
            } => {
                write!(
                    f,
                    "'features.{feature}' does not apply here at byte offset {}: {reason}",
                    span.start
                )
            }
            ParseError::EditionSyntaxNotAllowed {
                construct,
                hint,
                span,
            } => {
                write!(
                    f,
                    "{construct} was removed in Protobuf Editions (byte offset {}); {hint}",
                    span.start
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
