#![forbid(unsafe_code)]

//! Outline parser for `.proto` files.
//!
//! Identifies the top-level structure (syntax, package, imports, options,
//! messages, enums, services) without descending into body definitions.

use super::{error::ParseError, lexer::Lexer, span::Span, token::Token};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A top-level item identified in a `.proto` file.
#[derive(Debug, Clone, PartialEq)]
pub enum TopLevelItem {
    /// A `message` definition.
    Message {
        name: String,
        /// Span covering the full declaration from `message` keyword to `}`.
        span: Span,
        /// Span covering the body `{ ... }`.
        body_span: Span,
    },
    /// An `enum` definition.
    Enum {
        name: String,
        /// Span covering the full declaration from `enum` keyword to `}`.
        span: Span,
        /// Span covering the body `{ ... }`.
        body_span: Span,
    },
    /// A `service` definition.
    Service {
        name: String,
        /// Span covering the full declaration from `service` keyword to `}`.
        span: Span,
        /// Span covering the body `{ ... }`.
        body_span: Span,
    },
}

/// The file outline produced by parsing the top-level structure of a `.proto`
/// source string.
#[derive(Debug, Clone, Default)]
pub struct FileOutline {
    /// e.g. `"proto3"` or `"proto2"`
    pub syntax: Option<String>,
    /// e.g. `"google.protobuf"`
    pub package: Option<String>,
    /// Import paths, in order of declaration.
    pub imports: Vec<String>,
    /// Top-level option names (not parsed in detail).
    pub options: Vec<String>,
    /// All top-level messages, enums, and services in declaration order.
    pub items: Vec<TopLevelItem>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse the outline of a `.proto` source string.
///
/// Returns a [`FileOutline`] describing the top-level structure.  Message,
/// enum, and service bodies are **not** recursively parsed — only their spans
/// are tracked.
///
/// # Errors
///
/// Returns [`ParseError`] on lexer errors or structural problems (e.g.
/// unbalanced braces, unexpected tokens).
pub fn parse_outline(source: &str) -> Result<FileOutline, ParseError> {
    let mut lexer = Lexer::new(source).peekable();
    let mut outline = FileOutline::default();

    loop {
        let spanned = match next_significant(&mut lexer)? {
            None => break,
            Some(s) => s,
        };

        match spanned.value {
            Token::Eof => break,
            Token::Syntax => {
                outline.syntax = Some(parse_syntax(&mut lexer)?);
            }
            Token::Package => {
                outline.package = Some(parse_package(&mut lexer)?);
            }
            Token::Import => {
                let path = parse_import(&mut lexer)?;
                outline.imports.push(path);
            }
            Token::Option => {
                let name = parse_option_name(&mut lexer)?;
                outline.options.push(name);
            }
            Token::Message => {
                let item = parse_named_block(&mut lexer, spanned.span, |name, span, body_span| {
                    TopLevelItem::Message {
                        name,
                        span,
                        body_span,
                    }
                })?;
                outline.items.push(item);
            }
            Token::Enum => {
                let item = parse_named_block(&mut lexer, spanned.span, |name, span, body_span| {
                    TopLevelItem::Enum {
                        name,
                        span,
                        body_span,
                    }
                })?;
                outline.items.push(item);
            }
            Token::Service => {
                let item = parse_named_block(&mut lexer, spanned.span, |name, span, body_span| {
                    TopLevelItem::Service {
                        name,
                        span,
                        body_span,
                    }
                })?;
                outline.items.push(item);
            }
            // Anything else at the top level (extend, reserved, edition, etc.) — skip
            _ => {}
        }
    }

    Ok(outline)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

type PeekLexer<'a> = std::iter::Peekable<Lexer<'a>>;

/// Returns the next non-comment token, or `None` on clean EOF (stream
/// exhausted — as opposed to a `Token::Eof` value).
fn next_significant(
    lexer: &mut PeekLexer<'_>,
) -> Result<Option<super::span::Spanned<Token>>, ParseError> {
    loop {
        match lexer.next() {
            None => return Ok(None),
            Some(Err(e)) => return Err(ParseError::Lex(e)),
            Some(Ok(s)) => {
                // Skip comments
                if matches!(s.value, Token::LineComment(_) | Token::BlockComment(_)) {
                    continue;
                }
                return Ok(Some(s));
            }
        }
    }
}

/// Expect a specific token; return its span, or a `ParseError::UnexpectedToken`.
fn expect_token(
    lexer: &mut PeekLexer<'_>,
    expected_desc: &str,
    predicate: impl Fn(&Token) -> bool,
) -> Result<super::span::Spanned<Token>, ParseError> {
    match next_significant(lexer)? {
        None => Err(ParseError::UnexpectedEof),
        Some(s) => {
            if predicate(&s.value) {
                Ok(s)
            } else {
                Err(ParseError::UnexpectedToken {
                    expected: expected_desc.to_owned(),
                    found: s.value.to_string(),
                    span: s.span,
                })
            }
        }
    }
}

/// Expect a semicolon.
fn expect_semi(lexer: &mut PeekLexer<'_>) -> Result<(), ParseError> {
    expect_token(lexer, ";", |t| matches!(t, Token::Semicolon))?;
    Ok(())
}

/// Parse `= "proto3" ;` following a `syntax` keyword.
fn parse_syntax(lexer: &mut PeekLexer<'_>) -> Result<String, ParseError> {
    expect_token(lexer, "=", |t| matches!(t, Token::Equals))?;
    let s = expect_token(lexer, "string literal", |t| {
        matches!(t, Token::StringLit(_))
    })?;
    let val = match s.value {
        Token::StringLit(v) => v,
        _ => unreachable!(),
    };
    expect_semi(lexer)?;
    Ok(val)
}

/// Parse a dotted identifier `foo.bar.baz ;` following a `package` keyword.
fn parse_package(lexer: &mut PeekLexer<'_>) -> Result<String, ParseError> {
    let mut parts = Vec::new();

    // First ident (required)
    let first = expect_ident_or_keyword_name(lexer, "package name")?;
    parts.push(first);

    // Optional `.ident` repetitions
    loop {
        // Peek ahead without consuming
        match lexer.peek() {
            Some(Ok(s)) if matches!(s.value, Token::Dot) => {
                // consume the dot
                let _ = lexer.next();
                let part = expect_ident_or_keyword_name(lexer, "package name segment")?;
                parts.push(part);
            }
            _ => break,
        }
    }

    expect_semi(lexer)?;
    Ok(parts.join("."))
}

/// Expect either a plain `Ident` or a keyword used as a name (for cases like
/// `package reserved.things`).  Returns the string name.
fn expect_ident_or_keyword_name(
    lexer: &mut PeekLexer<'_>,
    ctx: &str,
) -> Result<String, ParseError> {
    let s = match next_significant(lexer)? {
        None => return Err(ParseError::UnexpectedEof),
        Some(s) => s,
    };
    match s.value {
        Token::Ident(name) => Ok(name),
        // Allow keywords to be used as identifiers in dotted names
        other => Ok(other.to_string()),
    }
    .map_err(|_: std::convert::Infallible| ParseError::UnexpectedToken {
        expected: ctx.to_owned(),
        found: "?".to_owned(),
        span: s.span,
    })
}

/// Parse `["public" | "weak"] "path/to/file.proto" ;` following `import`.
fn parse_import(lexer: &mut PeekLexer<'_>) -> Result<String, ParseError> {
    // Optional public / weak modifier — peek first without consuming.
    match lexer.peek() {
        Some(Ok(s)) if matches!(s.value, Token::Public | Token::Weak) => {
            let _ = lexer.next(); // consume modifier
        }
        _ => {}
    }
    let s = expect_token(lexer, "import path (string literal)", |t| {
        matches!(t, Token::StringLit(_))
    })?;
    let path = match s.value {
        Token::StringLit(p) => p,
        _ => unreachable!(),
    };
    expect_semi(lexer)?;
    Ok(path)
}

/// Parse an `option` statement: skip until `;`, return the raw option name.
///
/// Proto option syntax: `option (foo.bar).baz = value ;`
/// We just collect the name portion (up to `=`) as a string.
fn parse_option_name(lexer: &mut PeekLexer<'_>) -> Result<String, ParseError> {
    let mut name_parts = Vec::<String>::new();
    // Read tokens until we see `=` or `;` (the name section)
    loop {
        match lexer.peek() {
            None => return Err(ParseError::UnexpectedEof),
            Some(Ok(s)) => match &s.value {
                Token::Equals => break,
                Token::Semicolon => {
                    // option with no value — unusual but skip it
                    break;
                }
                Token::LineComment(_) | Token::BlockComment(_) => {
                    let _ = lexer.next();
                }
                Token::LParen | Token::RParen | Token::Dot => {
                    let _ = lexer.next(); // skip structural chars
                }
                Token::Eof => break,
                _ => {
                    let tok = lexer.next().expect("peeked");
                    match tok {
                        Ok(spanned) => name_parts.push(spanned.value.to_string()),
                        Err(e) => return Err(ParseError::Lex(e)),
                    }
                }
            },
            Some(Err(_)) => {
                // consume and propagate
                let e = lexer.next().expect("peeked");
                return Err(ParseError::Lex(e.expect_err("must be err")));
            }
        }
    }
    // Skip the rest of the option (from `=` to `;`)
    loop {
        match lexer.next() {
            None => return Err(ParseError::UnexpectedEof),
            Some(Err(e)) => return Err(ParseError::Lex(e)),
            Some(Ok(s)) => match s.value {
                Token::Semicolon => break,
                Token::Eof => return Err(ParseError::UnexpectedEof),
                _ => {}
            },
        }
    }
    Ok(name_parts.join(""))
}

/// Parse `Name { ... }` and build a `TopLevelItem` via `make`.
///
/// `kw_span` is the span of the keyword (`message` / `enum` / `service`) that
/// was just consumed.
fn parse_named_block(
    lexer: &mut PeekLexer<'_>,
    kw_span: Span,
    make: impl Fn(String, Span, Span) -> TopLevelItem,
) -> Result<TopLevelItem, ParseError> {
    let name_tok = expect_token(lexer, "identifier (name)", |t| matches!(t, Token::Ident(_)))?;
    let name = match name_tok.value {
        Token::Ident(n) => n,
        _ => unreachable!(),
    };

    let lbrace = expect_token(lexer, "{", |t| matches!(t, Token::LBrace))?;
    let body_span = consume_braced_body(lexer, lbrace.span)?;
    let full_span = Span::new(kw_span.start, body_span.end);

    Ok(make(name, full_span, body_span))
}

/// Consume the body `{ ... }` after the opening brace has been consumed.
///
/// Tracks nested braces by depth-counting.  Returns the span covering the
/// opening `{` through the matching `}`.
///
/// # Arguments
///
/// * `open_brace_span` — the span of the `{` that was already consumed.
fn consume_braced_body(
    lexer: &mut PeekLexer<'_>,
    open_brace_span: Span,
) -> Result<Span, ParseError> {
    let mut depth: usize = 1;

    loop {
        match lexer.next() {
            None => {
                return Err(ParseError::UnbalancedBraces {
                    span: open_brace_span,
                });
            }
            Some(Err(e)) => return Err(ParseError::Lex(e)),
            Some(Ok(spanned)) => {
                let token_end = spanned.span.end;
                match spanned.value {
                    Token::LBrace => depth += 1,
                    Token::RBrace => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(Span::new(open_brace_span.start, token_end));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
