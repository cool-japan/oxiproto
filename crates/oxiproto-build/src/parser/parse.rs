#![forbid(unsafe_code)]

//! Full recursive-descent body parser for `.proto` files.
//!
//! Produces a [`ProtoFile`] AST from a source string.  Unlike the outline
//! parser, this descends into message, enum, service, and oneof bodies.

use crate::parser::{
    ast::{
        Enum, EnumValue, ExtendBlock, ExtensionRange, Field, FieldLabel, FieldType, Import,
        ImportModifier, Message, Method, Oneof, OptionValue, ProtoFile, ProtoOption, Reserved,
        ReservedRange, ReservedRangeTo, ScalarType, Service,
    },
    error::ParseError,
    lexer::Lexer,
    span::{Span, Spanned},
    token::Token,
};

// ---------------------------------------------------------------------------
// Internal type alias
// ---------------------------------------------------------------------------

type PeekLexer<'a> = std::iter::Peekable<Lexer<'a>>;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a `.proto` source string into a [`ProtoFile`] AST.
///
/// # Errors
///
/// Returns [`ParseError`] on lexer errors, unexpected tokens, unexpected EOF,
/// or unbalanced braces.
pub fn parse_file(source: &str) -> Result<ProtoFile, ParseError> {
    let mut lexer = Lexer::new(source).peekable();
    let mut file = ProtoFile::default();

    loop {
        let spanned = match next_significant(&mut lexer)? {
            None => break,
            Some(s) => s,
        };

        match spanned.value {
            Token::Eof => break,
            Token::Syntax => {
                file.syntax = Some(parse_syntax(&mut lexer)?);
            }
            Token::Package => {
                file.package = Some(parse_package(&mut lexer)?);
            }
            Token::Import => {
                file.imports.push(parse_import(&mut lexer, spanned.span)?);
            }
            Token::Option => {
                file.options
                    .push(parse_option_statement(&mut lexer, spanned.span)?);
            }
            Token::Message => {
                file.messages.push(parse_message(&mut lexer, spanned.span)?);
            }
            Token::Enum => {
                file.enums.push(parse_enum(&mut lexer, spanned.span)?);
            }
            Token::Service => {
                file.services.push(parse_service(&mut lexer, spanned.span)?);
            }
            Token::Extend => {
                file.extends.push(parse_extend_block(&mut lexer)?);
            }
            // Skip anything else at the top level (edition, etc.)
            _ => {}
        }
    }

    Ok(file)
}

// ---------------------------------------------------------------------------
// Extend block (proto2)
// ---------------------------------------------------------------------------

/// Parse an `extend TypeName { ... }` block after the `extend` keyword.
fn parse_extend_block(lexer: &mut PeekLexer<'_>) -> Result<ExtendBlock, ParseError> {
    // Parse the extendee name (possibly dotted, possibly leading-dot).
    let extendee = parse_type_ref(lexer)?;

    let open = expect_token(lexer, "{", |t| matches!(t, Token::LBrace))?;
    let open_span = open.span;

    let mut fields = Vec::new();

    loop {
        let tok = match next_significant(lexer)? {
            None => return Err(ParseError::UnbalancedBraces { span: open_span }),
            Some(s) => s,
        };
        match tok.value {
            Token::RBrace => break,
            Token::Repeated => {
                let next = next_or_eof(lexer)?;
                if !is_type_start(&next.value) {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type after 'repeated'".to_owned(),
                        found: next.value.to_string(),
                        span: next.span,
                    });
                }
                fields.push(parse_field_from(next, FieldLabel::Repeated, lexer)?);
            }
            Token::Optional => {
                let next = next_or_eof(lexer)?;
                if !is_type_start(&next.value) {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type after 'optional'".to_owned(),
                        found: next.value.to_string(),
                        span: next.span,
                    });
                }
                fields.push(parse_field_from(next, FieldLabel::Optional, lexer)?);
            }
            Token::Required => {
                let next = next_or_eof(lexer)?;
                if !is_type_start(&next.value) {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type after 'required'".to_owned(),
                        found: next.value.to_string(),
                        span: next.span,
                    });
                }
                fields.push(parse_field_from(next, FieldLabel::Required, lexer)?);
            }
            other => {
                let first = Spanned::new(other, tok.span);
                if is_type_start(&first.value) {
                    // No-label field (singular)
                    fields.push(parse_field_from(first, FieldLabel::Singular, lexer)?);
                } else {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field declaration or '}'".to_owned(),
                        found: first.value.to_string(),
                        span: first.span,
                    });
                }
            }
        }
    }

    Ok(ExtendBlock { extendee, fields })
}

// ---------------------------------------------------------------------------
// Low-level helpers
// ---------------------------------------------------------------------------

/// Returns the next non-comment token.
///
/// - Returns `Ok(None)` on stream exhaustion **or** `Token::Eof`.
/// - Returns `Err` on lex errors.
fn next_significant(lexer: &mut PeekLexer<'_>) -> Result<Option<Spanned<Token>>, ParseError> {
    loop {
        match lexer.next() {
            None => return Ok(None),
            Some(Err(e)) => return Err(ParseError::Lex(e)),
            Some(Ok(s)) => {
                if matches!(s.value, Token::LineComment(_) | Token::BlockComment(_)) {
                    continue;
                }
                if matches!(s.value, Token::Eof) {
                    return Ok(None);
                }
                return Ok(Some(s));
            }
        }
    }
}

/// Like `next_significant` but returns `Err(UnexpectedEof)` instead of `Ok(None)`.
fn next_or_eof(lexer: &mut PeekLexer<'_>) -> Result<Spanned<Token>, ParseError> {
    next_significant(lexer)?.ok_or(ParseError::UnexpectedEof)
}

/// Peek at the next non-comment, non-EOF token without consuming it.
///
/// Returns `true` if there is such a token and `pred` matches it.
fn peek_is(lexer: &mut PeekLexer<'_>, pred: impl Fn(&Token) -> bool) -> bool {
    // We must peek past comments without consuming them.
    // The peekable only gives us one-step peek, so we consume comments first.
    // Since comments are rare, and we don't want to consume, we iterate the
    // raw items buffered in the peek iterator by a single peek.
    // For correctness we only peek one token; comments will be consumed by
    // the next `next_significant` call.
    match lexer.peek() {
        None => false,
        Some(Err(_)) => false,
        Some(Ok(s)) => {
            if matches!(
                s.value,
                Token::LineComment(_) | Token::BlockComment(_) | Token::Eof
            ) {
                false
            } else {
                pred(&s.value)
            }
        }
    }
}

/// Expect a specific token (matched by predicate).  Returns the spanned token.
fn expect_token(
    lexer: &mut PeekLexer<'_>,
    expected_desc: &str,
    predicate: impl Fn(&Token) -> bool,
) -> Result<Spanned<Token>, ParseError> {
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

/// Expect an identifier (or keyword used as a name) and return its string
/// value together with its span.
///
/// `ctx` is used in error messages if the token is not a valid name position.
fn expect_ident(lexer: &mut PeekLexer<'_>, _ctx: &str) -> Result<(String, Span), ParseError> {
    let s = next_or_eof(lexer)?;
    match s.value {
        Token::Ident(name) => Ok((name, s.span)),
        // Allow any keyword to serve as an identifier where a name is expected.
        other => Ok((other.to_string(), s.span)),
    }
}

/// Expect a `Token::IntLit` and convert it to `i32`.
fn expect_int_i32(lexer: &mut PeekLexer<'_>, ctx: &str) -> Result<(i32, Span), ParseError> {
    let s = next_or_eof(lexer)?;
    match s.value {
        Token::IntLit(n) => i32::try_from(n)
            .map_err(|_| ParseError::UnexpectedToken {
                expected: format!("valid {ctx} (i32)"),
                found: n.to_string(),
                span: s.span,
            })
            .map(|v| (v, s.span)),
        other => Err(ParseError::UnexpectedToken {
            expected: format!("integer literal ({ctx})"),
            found: other.to_string(),
            span: s.span,
        }),
    }
}

/// Expect an `=`.
fn expect_equals(lexer: &mut PeekLexer<'_>) -> Result<(), ParseError> {
    expect_token(lexer, "=", |t| matches!(t, Token::Equals))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Type-token helpers
// ---------------------------------------------------------------------------

/// Returns `true` if `tok` can be the first token of a field type (excludes
/// `map` because map fields are dispatched separately).
fn is_type_start(tok: &Token) -> bool {
    matches!(
        tok,
        Token::Double
            | Token::Float
            | Token::Int32
            | Token::Int64
            | Token::Uint32
            | Token::Uint64
            | Token::Sint32
            | Token::Sint64
            | Token::Fixed32
            | Token::Fixed64
            | Token::Sfixed32
            | Token::Sfixed64
            | Token::Bool
            | Token::StringType
            | Token::BytesType
            | Token::Ident(_)
            | Token::Dot
    )
}

/// Convert a scalar keyword token to `ScalarType`.
/// Returns `None` for non-scalar tokens.
fn token_to_scalar(tok: &Token) -> Option<ScalarType> {
    match tok {
        Token::Double => Some(ScalarType::Double),
        Token::Float => Some(ScalarType::Float),
        Token::Int32 => Some(ScalarType::Int32),
        Token::Int64 => Some(ScalarType::Int64),
        Token::Uint32 => Some(ScalarType::Uint32),
        Token::Uint64 => Some(ScalarType::Uint64),
        Token::Sint32 => Some(ScalarType::Sint32),
        Token::Sint64 => Some(ScalarType::Sint64),
        Token::Fixed32 => Some(ScalarType::Fixed32),
        Token::Fixed64 => Some(ScalarType::Fixed64),
        Token::Sfixed32 => Some(ScalarType::Sfixed32),
        Token::Sfixed64 => Some(ScalarType::Sfixed64),
        Token::Bool => Some(ScalarType::Bool),
        Token::StringType => Some(ScalarType::String),
        Token::BytesType => Some(ScalarType::Bytes),
        _ => None,
    }
}

/// Parse a field type starting from `first` (already consumed from the lexer).
///
/// For `Dot` or `Ident` tokens, continues collecting dotted path segments.
fn parse_field_type_from(
    first: Spanned<Token>,
    lexer: &mut PeekLexer<'_>,
) -> Result<FieldType, ParseError> {
    if let Some(sc) = token_to_scalar(&first.value) {
        return Ok(FieldType::Scalar(sc));
    }

    match first.value {
        Token::Dot => {
            // Leading-dot type ref: `.foo.Bar`
            let mut path = String::from(".");
            let (seg, _) = expect_ident(lexer, "type name segment")?;
            path.push_str(&seg);
            // Collect additional `.Seg` parts
            loop {
                if peek_is(lexer, |t| matches!(t, Token::Dot)) {
                    let _ = lexer.next(); // consume dot
                    let (seg2, _) = expect_ident(lexer, "type name segment")?;
                    path.push('.');
                    path.push_str(&seg2);
                } else {
                    break;
                }
            }
            Ok(FieldType::Named(path))
        }
        Token::Ident(name) => {
            let mut path = name;
            // Collect `.Seg` parts
            loop {
                if peek_is(lexer, |t| matches!(t, Token::Dot)) {
                    let _ = lexer.next(); // consume dot
                    let (seg, _) = expect_ident(lexer, "type name segment")?;
                    path.push('.');
                    path.push_str(&seg);
                } else {
                    break;
                }
            }
            Ok(FieldType::Named(path))
        }
        other => Err(ParseError::UnexpectedToken {
            expected: "field type".to_owned(),
            found: other.to_string(),
            span: first.span,
        }),
    }
}

// ---------------------------------------------------------------------------
// File-level constructs
// ---------------------------------------------------------------------------

/// Parse `= "proto2"|"proto3" ;` after the `syntax` keyword.
fn parse_syntax(lexer: &mut PeekLexer<'_>) -> Result<String, ParseError> {
    expect_equals(lexer)?;
    let s = expect_token(lexer, "string literal", |t| {
        matches!(t, Token::StringLit(_))
    })?;
    let val = match s.value {
        Token::StringLit(v) => v,
        other => {
            return Err(ParseError::UnexpectedToken {
                expected: "string literal".to_owned(),
                found: other.to_string(),
                span: s.span,
            });
        }
    };
    if val != "proto2" && val != "proto3" {
        return Err(ParseError::UnknownSyntax(val));
    }
    expect_semi(lexer)?;
    Ok(val)
}

/// Parse a dotted package name and `;` after the `package` keyword.
fn parse_package(lexer: &mut PeekLexer<'_>) -> Result<String, ParseError> {
    let (first, _) = expect_ident(lexer, "package name")?;
    let mut parts = vec![first];

    loop {
        if peek_is(lexer, |t| matches!(t, Token::Dot)) {
            let _ = lexer.next();
            let (seg, _) = expect_ident(lexer, "package name segment")?;
            parts.push(seg);
        } else {
            break;
        }
    }

    expect_semi(lexer)?;
    Ok(parts.join("."))
}

/// Parse an `import` statement after the `import` keyword.
fn parse_import(lexer: &mut PeekLexer<'_>, kw_span: Span) -> Result<Import, ParseError> {
    // Optional public / weak modifier
    let modifier = if peek_is(lexer, |t| matches!(t, Token::Public)) {
        let _ = lexer.next();
        ImportModifier::Public
    } else if peek_is(lexer, |t| matches!(t, Token::Weak)) {
        let _ = lexer.next();
        ImportModifier::Weak
    } else {
        ImportModifier::None
    };

    let s = expect_token(lexer, "import path (string literal)", |t| {
        matches!(t, Token::StringLit(_))
    })?;
    let path = match s.value {
        Token::StringLit(p) => p,
        other => {
            return Err(ParseError::UnexpectedToken {
                expected: "import path (string literal)".to_owned(),
                found: other.to_string(),
                span: s.span,
            });
        }
    };
    let end_span = s.span;
    expect_semi(lexer)?;

    Ok(Import {
        path,
        modifier,
        span: Span::new(kw_span.start, end_span.end),
    })
}

// ---------------------------------------------------------------------------
// Option statement
// ---------------------------------------------------------------------------

/// Parse an option name string (handles `(foo.bar).baz` extended options).
/// Does not consume the following `=`.
fn parse_option_name_str(lexer: &mut PeekLexer<'_>) -> Result<String, ParseError> {
    let mut name = String::new();

    if peek_is(lexer, |t| matches!(t, Token::LParen)) {
        let _ = lexer.next(); // consume `(`
        name.push('(');
        // Collect dotted name inside parens
        let (first_seg, _) = expect_ident(lexer, "option name")?;
        name.push_str(&first_seg);
        loop {
            if peek_is(lexer, |t| matches!(t, Token::Dot)) {
                let _ = lexer.next();
                let (seg, _) = expect_ident(lexer, "option name segment")?;
                name.push('.');
                name.push_str(&seg);
            } else {
                break;
            }
        }
        expect_token(lexer, ")", |t| matches!(t, Token::RParen))?;
        name.push(')');
        // Optional `.name` suffix
        while peek_is(lexer, |t| matches!(t, Token::Dot)) {
            let _ = lexer.next();
            let (seg, _) = expect_ident(lexer, "option name segment")?;
            name.push('.');
            name.push_str(&seg);
        }
    } else {
        // Plain ident (or dotted plain ident)
        let (first_seg, _) = expect_ident(lexer, "option name")?;
        name.push_str(&first_seg);
        while peek_is(lexer, |t| matches!(t, Token::Dot)) {
            let _ = lexer.next();
            let (seg, _) = expect_ident(lexer, "option name segment")?;
            name.push('.');
            name.push_str(&seg);
        }
    }

    Ok(name)
}

/// Parse an option value (after the `=`).
fn parse_option_value(lexer: &mut PeekLexer<'_>) -> Result<OptionValue, ParseError> {
    let s = next_or_eof(lexer)?;
    match s.value {
        Token::StringLit(v) => Ok(OptionValue::Str(v)),
        Token::IntLit(n) => Ok(OptionValue::Int(n as i64)),
        Token::FloatLit(f) => Ok(OptionValue::Float(f)),
        Token::Minus => {
            // Negative number
            let s2 = next_or_eof(lexer)?;
            match s2.value {
                Token::IntLit(n) => Ok(OptionValue::Int(-(n as i64))),
                Token::FloatLit(f) => Ok(OptionValue::Float(-f)),
                other => Err(ParseError::UnexpectedToken {
                    expected: "integer or float after '-'".to_owned(),
                    found: other.to_string(),
                    span: s2.span,
                }),
            }
        }
        Token::Ident(ref name) => {
            let val = match name.as_str() {
                "true" => OptionValue::Bool(true),
                "false" => OptionValue::Bool(false),
                _ => OptionValue::Ident(name.clone()),
            };
            Ok(val)
        }
        Token::LBrace => {
            // Message-literal option value: `{ key: value, key2: value2, ... }`
            // Parse recursively, allowing nested message literals.
            let mut pairs: Vec<(String, OptionValue)> = Vec::new();
            loop {
                // Consume optional separators (comma, semicolon) between pairs.
                while peek_is(lexer, |t| matches!(t, Token::Comma | Token::Semicolon)) {
                    let _ = next_or_eof(lexer)?;
                }
                // Check for closing brace.
                if peek_is(lexer, |t| matches!(t, Token::RBrace)) {
                    let _ = next_or_eof(lexer)?;
                    break;
                }
                // Expect a field name (identifier or keyword-as-ident).
                let (field_name, _span) = expect_ident(lexer, "message literal field name")?;
                // Expect a colon between field name and value.
                expect_token(lexer, "':'", |t| matches!(t, Token::Colon))?;
                // Parse the value recursively (handles nested message literals).
                let value = parse_option_value(lexer)?;
                pairs.push((field_name, value));
            }
            Ok(OptionValue::MessageLiteral(pairs))
        }
        // Some compilers allow keyword-as-ident in option values (e.g. type names)
        other => Ok(OptionValue::Ident(other.to_string())),
    }
}

/// Parse a full `option name = value ;` statement.
/// `kw_span` is the span of the `option` keyword already consumed.
fn parse_option_statement(
    lexer: &mut PeekLexer<'_>,
    kw_span: Span,
) -> Result<ProtoOption, ParseError> {
    let name = parse_option_name_str(lexer)?;
    expect_equals(lexer)?;
    let value = parse_option_value(lexer)?;
    expect_semi(lexer)?;
    Ok(ProtoOption {
        name,
        value,
        span: kw_span,
    })
}

// ---------------------------------------------------------------------------
// Field options  [deprecated = true, packed = false, ...]
// ---------------------------------------------------------------------------

/// Parse inline field options inside `[...]`.
/// The `[` has already been consumed.
fn parse_field_options(
    lexer: &mut PeekLexer<'_>,
    open_span: Span,
) -> Result<Vec<ProtoOption>, ParseError> {
    let mut opts = Vec::new();

    loop {
        // Check for closing bracket
        if peek_is(lexer, |t| matches!(t, Token::RBracket)) {
            let _ = lexer.next();
            break;
        }

        let name_start = match lexer.peek() {
            None => return Err(ParseError::UnbalancedBraces { span: open_span }),
            Some(Ok(s)) => s.span,
            Some(Err(_)) => open_span,
        };
        let name = parse_option_name_str(lexer)?;
        expect_equals(lexer)?;
        let value = parse_option_value(lexer)?;
        opts.push(ProtoOption {
            name,
            value,
            span: name_start,
        });

        // Separator
        if peek_is(lexer, |t| matches!(t, Token::Comma)) {
            let _ = lexer.next();
        } else {
            // Expect closing bracket
            expect_token(lexer, "]", |t| matches!(t, Token::RBracket))?;
            break;
        }
    }

    Ok(opts)
}

// ---------------------------------------------------------------------------
// Reserved statement
// ---------------------------------------------------------------------------

/// Parse a `reserved` statement after the `reserved` keyword.
/// Returns either `Reserved::Ranges(...)` or `Reserved::Names(...)`.
fn parse_reserved(lexer: &mut PeekLexer<'_>) -> Result<Reserved, ParseError> {
    // Peek first token to determine whether it's ranges or names
    let first = next_or_eof(lexer)?;
    match first.value {
        Token::StringLit(s) => {
            // Reserved names
            let mut names = vec![s];
            loop {
                if peek_is(lexer, |t| matches!(t, Token::Comma)) {
                    let _ = lexer.next();
                    let s2 = expect_token(lexer, "string literal", |t| {
                        matches!(t, Token::StringLit(_))
                    })?;
                    match s2.value {
                        Token::StringLit(name) => names.push(name),
                        other => {
                            return Err(ParseError::UnexpectedToken {
                                expected: "string literal (reserved name)".to_owned(),
                                found: other.to_string(),
                                span: s2.span,
                            });
                        }
                    }
                } else {
                    break;
                }
            }
            expect_semi(lexer)?;
            Ok(Reserved::Names(names))
        }
        Token::IntLit(n) => {
            let from = i32::try_from(n).map_err(|_| ParseError::UnexpectedToken {
                expected: "valid reserved range start (i32)".to_owned(),
                found: n.to_string(),
                span: first.span,
            })?;
            let mut ranges = Vec::new();
            ranges.push(parse_reserved_range_tail(lexer, from, first.span)?);

            loop {
                if peek_is(lexer, |t| matches!(t, Token::Comma)) {
                    let _ = lexer.next();
                    let (next_from, nspan) = expect_int_i32(lexer, "reserved range start")?;
                    ranges.push(parse_reserved_range_tail(lexer, next_from, nspan)?);
                } else {
                    break;
                }
            }
            expect_semi(lexer)?;
            Ok(Reserved::Ranges(ranges))
        }
        other => Err(ParseError::UnexpectedToken {
            expected: "reserved range (integer) or name (string)".to_owned(),
            found: other.to_string(),
            span: first.span,
        }),
    }
}

/// Parse the optional `to N|max` tail of a reserved range, given `from`.
fn parse_reserved_range_tail(
    lexer: &mut PeekLexer<'_>,
    from: i32,
    _from_span: Span,
) -> Result<ReservedRange, ParseError> {
    if peek_is(lexer, |t| matches!(t, Token::To)) {
        let _ = lexer.next(); // consume `to`
        let end_tok = next_or_eof(lexer)?;
        match end_tok.value {
            Token::Ident(ref name) if name == "max" => Ok(ReservedRange {
                from,
                to: ReservedRangeTo::Max,
            }),
            Token::IntLit(n) => {
                let to_num = i32::try_from(n).map_err(|_| ParseError::UnexpectedToken {
                    expected: "valid reserved range end (i32)".to_owned(),
                    found: n.to_string(),
                    span: end_tok.span,
                })?;
                Ok(ReservedRange {
                    from,
                    to: ReservedRangeTo::Number(to_num),
                })
            }
            other => Err(ParseError::UnexpectedToken {
                expected: "integer or 'max' (reserved range end)".to_owned(),
                found: other.to_string(),
                span: end_tok.span,
            }),
        }
    } else {
        // Single-number range: `reserved N;` — treated as N to N
        Ok(ReservedRange {
            from,
            to: ReservedRangeTo::Number(from),
        })
    }
}

// ---------------------------------------------------------------------------
// Field parsing
// ---------------------------------------------------------------------------

/// Parse a field given its `label` and the first token of its type (already
/// consumed from the lexer).
fn parse_field_from(
    first_type_token: Spanned<Token>,
    label: FieldLabel,
    lexer: &mut PeekLexer<'_>,
) -> Result<Field, ParseError> {
    let start_span = first_type_token.span;
    let ty = parse_field_type_from(first_type_token, lexer)?;
    let (name, _) = expect_ident(lexer, "field name")?;
    expect_equals(lexer)?;

    // Field number — may have leading `-` (syntactically accepted, semantically rejected)
    let number = if peek_is(lexer, |t| matches!(t, Token::Minus)) {
        let _ = lexer.next();
        let (n, _) = expect_int_i32(lexer, "field number")?;
        -n
    } else {
        let (n, _) = expect_int_i32(lexer, "field number")?;
        n
    };

    let options = if peek_is(lexer, |t| matches!(t, Token::LBracket)) {
        let open = lexer.next();
        let open_span = open
            .and_then(|r| r.ok())
            .map(|s| s.span)
            .unwrap_or(start_span);
        parse_field_options(lexer, open_span)?
    } else {
        Vec::new()
    };

    let semi = expect_token(lexer, ";", |t| matches!(t, Token::Semicolon))?;

    Ok(Field {
        label,
        ty,
        name,
        number,
        options,
        span: Span::new(start_span.start, semi.span.end),
    })
}

/// Parse a `map<K, V> name = N [opts] ;` field.
/// The `map` keyword has already been consumed.
fn parse_map_field(lexer: &mut PeekLexer<'_>, kw_span: Span) -> Result<Field, ParseError> {
    expect_token(lexer, "<", |t| matches!(t, Token::LAngle))?;

    let key_tok = next_or_eof(lexer)?;
    let key = token_to_scalar(&key_tok.value).ok_or_else(|| ParseError::UnexpectedToken {
        expected: "scalar key type for map".to_owned(),
        found: key_tok.value.to_string(),
        span: key_tok.span,
    })?;

    expect_token(lexer, ",", |t| matches!(t, Token::Comma))?;

    let val_tok = next_or_eof(lexer)?;
    // Value cannot itself be a Map type in proto3
    if matches!(val_tok.value, Token::Map) {
        return Err(ParseError::UnexpectedToken {
            expected: "field type (not map) for map value".to_owned(),
            found: "map".to_owned(),
            span: val_tok.span,
        });
    }
    let value = parse_field_type_from(val_tok, lexer)?;

    expect_token(lexer, ">", |t| matches!(t, Token::RAngle))?;

    let (name, _) = expect_ident(lexer, "map field name")?;
    expect_equals(lexer)?;
    let (number, _) = expect_int_i32(lexer, "map field number")?;

    let options = if peek_is(lexer, |t| matches!(t, Token::LBracket)) {
        let open = lexer.next();
        let open_span = open.and_then(|r| r.ok()).map(|s| s.span).unwrap_or(kw_span);
        parse_field_options(lexer, open_span)?
    } else {
        Vec::new()
    };

    let semi = expect_token(lexer, ";", |t| matches!(t, Token::Semicolon))?;

    Ok(Field {
        label: FieldLabel::Singular,
        ty: FieldType::Map {
            key,
            value: Box::new(value),
        },
        name,
        number,
        options,
        span: Span::new(kw_span.start, semi.span.end),
    })
}

// ---------------------------------------------------------------------------
// Oneof
// ---------------------------------------------------------------------------

/// Parse a `oneof Name { ... }` block.
/// The `oneof` keyword has already been consumed.
fn parse_oneof(lexer: &mut PeekLexer<'_>, kw_span: Span) -> Result<Oneof, ParseError> {
    let (name, _) = expect_ident(lexer, "oneof name")?;
    let open = expect_token(lexer, "{", |t| matches!(t, Token::LBrace))?;
    let open_span = open.span;

    let mut fields = Vec::new();
    let mut options = Vec::new();
    let close_span;

    loop {
        let tok = match next_significant(lexer)? {
            None => return Err(ParseError::UnbalancedBraces { span: open_span }),
            Some(s) => s,
        };
        match tok.value {
            Token::RBrace => {
                close_span = tok.span;
                break;
            }
            Token::Option => {
                options.push(parse_option_statement(lexer, tok.span)?);
            }
            other => {
                // Must be a field type — no label inside oneof
                let first = Spanned::new(other, tok.span);
                if !is_type_start(&first.value) {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type or '}'".to_owned(),
                        found: first.value.to_string(),
                        span: first.span,
                    });
                }
                fields.push(parse_field_from(first, FieldLabel::Singular, lexer)?);
            }
        }
    }

    Ok(Oneof {
        name,
        fields,
        options,
        span: Span::new(kw_span.start, close_span.end),
    })
}

// ---------------------------------------------------------------------------
// Group field (proto2)
// ---------------------------------------------------------------------------

/// Parse a proto2 group field after the field label (already consumed) and after
/// the `group` keyword (already consumed).
///
/// Syntax: `group GroupName = FieldNumber [options] { ... }`
///
/// Returns `(field, nested_message)` where:
/// - `field` has `ty = FieldType::Group(group_name)`, `name = group_name.to_lowercase()`
/// - `nested_message` is the synthesized `Message { name: group_name, ... }`
///
/// The caller must push `nested_message` into the parent's `nested_messages` vec.
fn parse_group_field(
    lexer: &mut PeekLexer<'_>,
    label: FieldLabel,
    kw_span: Span,
) -> Result<(Field, Message), ParseError> {
    // Read group name (must start with uppercase letter per proto2 spec)
    let (group_name, name_span) = expect_ident(lexer, "group name")?;
    if !group_name.starts_with(|c: char| c.is_uppercase()) {
        return Err(ParseError::MalformedGroupName {
            name: group_name,
            span: name_span,
        });
    }

    expect_equals(lexer)?;

    // Field number
    let (number, _) = expect_int_i32(lexer, "group field number")?;

    // Optional inline options [...]
    let options = if peek_is(lexer, |t| matches!(t, Token::LBracket)) {
        let open = lexer.next();
        let open_span = open.and_then(|r| r.ok()).map(|s| s.span).unwrap_or(kw_span);
        parse_field_options(lexer, open_span)?
    } else {
        Vec::new()
    };

    // Parse the group body as a message body: `{ field_decls... }`
    let open = expect_token(lexer, "{", |t| matches!(t, Token::LBrace))?;
    let open_span = open.span;

    let mut body_fields = Vec::new();
    let mut body_oneofs = Vec::new();
    let mut body_nested = Vec::new();
    let mut body_enums = Vec::new();
    let mut body_reserved = Vec::new();
    let mut body_options = Vec::new();
    let mut body_extensions: Vec<ExtensionRange> = Vec::new();
    let close_span;

    loop {
        let tok = match next_significant(lexer)? {
            None => return Err(ParseError::UnbalancedBraces { span: open_span }),
            Some(s) => s,
        };
        match tok.value {
            Token::RBrace => {
                close_span = tok.span;
                break;
            }
            Token::Reserved => {
                body_reserved.push(parse_reserved(lexer)?);
            }
            Token::Option => {
                body_options.push(parse_option_statement(lexer, tok.span)?);
            }
            Token::Oneof => {
                body_oneofs.push(parse_oneof(lexer, tok.span)?);
            }
            Token::Message => {
                body_nested.push(parse_message(lexer, tok.span)?);
            }
            Token::Enum => {
                body_enums.push(parse_enum(lexer, tok.span)?);
            }
            Token::Map => {
                body_fields.push(parse_map_field(lexer, tok.span)?);
            }
            Token::Repeated => {
                let next = next_or_eof(lexer)?;
                if matches!(next.value, Token::Map) {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type (repeated map is not allowed)".to_owned(),
                        found: "map".to_owned(),
                        span: next.span,
                    });
                }
                if matches!(next.value, Token::Group) {
                    let (gf, gm) = parse_group_field(lexer, FieldLabel::Repeated, next.span)?;
                    body_nested.push(gm);
                    body_fields.push(gf);
                } else if is_type_start(&next.value) {
                    body_fields.push(parse_field_from(next, FieldLabel::Repeated, lexer)?);
                } else {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type after 'repeated'".to_owned(),
                        found: next.value.to_string(),
                        span: next.span,
                    });
                }
            }
            Token::Optional => {
                let next = next_or_eof(lexer)?;
                if matches!(next.value, Token::Group) {
                    let (gf, gm) = parse_group_field(lexer, FieldLabel::Optional, next.span)?;
                    body_nested.push(gm);
                    body_fields.push(gf);
                } else if is_type_start(&next.value) {
                    body_fields.push(parse_field_from(next, FieldLabel::Optional, lexer)?);
                } else {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type after 'optional'".to_owned(),
                        found: next.value.to_string(),
                        span: next.span,
                    });
                }
            }
            Token::Required => {
                let next = next_or_eof(lexer)?;
                if matches!(next.value, Token::Group) {
                    let (gf, gm) = parse_group_field(lexer, FieldLabel::Required, next.span)?;
                    body_nested.push(gm);
                    body_fields.push(gf);
                } else if is_type_start(&next.value) {
                    body_fields.push(parse_field_from(next, FieldLabel::Required, lexer)?);
                } else {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type after 'required'".to_owned(),
                        found: next.value.to_string(),
                        span: next.span,
                    });
                }
            }
            Token::Extensions => {
                let ext_ranges = parse_extensions_statement(lexer)?;
                body_extensions.extend(ext_ranges);
            }
            Token::Group => {
                // Bare `group GroupName = N { ... }` — treat as singular (proto2 implicit optional).
                let (gf, gm) = parse_group_field(lexer, FieldLabel::Optional, tok.span)?;
                body_nested.push(gm);
                body_fields.push(gf);
            }
            other => {
                let first = Spanned::new(other, tok.span);
                if is_type_start(&first.value) {
                    body_fields.push(parse_field_from(first, FieldLabel::Singular, lexer)?);
                } else {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field, oneof, message, enum, option, reserved, or '}'"
                            .to_owned(),
                        found: first.value.to_string(),
                        span: first.span,
                    });
                }
            }
        }
    }

    let nested_msg = Message {
        name: group_name.clone(),
        fields: body_fields,
        oneofs: body_oneofs,
        nested_messages: body_nested,
        nested_enums: body_enums,
        reserved: body_reserved,
        options: body_options,
        extensions: body_extensions,
        span: Span::new(open_span.start, close_span.end),
    };

    // The field name is the lowercased group name (protoc convention).
    let field_name = group_name.to_lowercase();
    let field = Field {
        label,
        ty: FieldType::Group(group_name),
        name: field_name,
        number,
        options,
        span: Span::new(kw_span.start, close_span.end),
    };

    Ok((field, nested_msg))
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

/// Parse a `message Name { ... }` definition.
/// The `message` keyword has already been consumed.
fn parse_message(lexer: &mut PeekLexer<'_>, kw_span: Span) -> Result<Message, ParseError> {
    let (name, _) = expect_ident(lexer, "message name")?;
    let open = expect_token(lexer, "{", |t| matches!(t, Token::LBrace))?;
    let open_span = open.span;

    let mut fields = Vec::new();
    let mut oneofs = Vec::new();
    let mut nested_messages = Vec::new();
    let mut nested_enums = Vec::new();
    let mut reserved = Vec::new();
    let mut options = Vec::new();
    let mut extensions: Vec<ExtensionRange> = Vec::new();
    let close_span;

    loop {
        let tok = match next_significant(lexer)? {
            None => return Err(ParseError::UnbalancedBraces { span: open_span }),
            Some(s) => s,
        };
        match tok.value {
            Token::RBrace => {
                close_span = tok.span;
                break;
            }
            Token::Reserved => {
                reserved.push(parse_reserved(lexer)?);
            }
            Token::Option => {
                options.push(parse_option_statement(lexer, tok.span)?);
            }
            Token::Oneof => {
                oneofs.push(parse_oneof(lexer, tok.span)?);
            }
            Token::Message => {
                nested_messages.push(parse_message(lexer, tok.span)?);
            }
            Token::Enum => {
                nested_enums.push(parse_enum(lexer, tok.span)?);
            }
            Token::Map => {
                fields.push(parse_map_field(lexer, tok.span)?);
            }
            Token::Repeated => {
                // `repeated map<...>` is invalid
                let next = next_or_eof(lexer)?;
                if matches!(next.value, Token::Map) {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type (repeated map is not allowed)".to_owned(),
                        found: "map".to_owned(),
                        span: next.span,
                    });
                }
                // `repeated group GroupName = N { ... }` — proto2 group field
                if matches!(next.value, Token::Group) {
                    let (gf, gm) = parse_group_field(lexer, FieldLabel::Repeated, next.span)?;
                    nested_messages.push(gm);
                    fields.push(gf);
                } else if is_type_start(&next.value) {
                    fields.push(parse_field_from(next, FieldLabel::Repeated, lexer)?);
                } else {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type after 'repeated'".to_owned(),
                        found: next.value.to_string(),
                        span: next.span,
                    });
                }
            }
            Token::Optional => {
                let next = next_or_eof(lexer)?;
                // `optional group GroupName = N { ... }` — proto2 group field
                if matches!(next.value, Token::Group) {
                    let (gf, gm) = parse_group_field(lexer, FieldLabel::Optional, next.span)?;
                    nested_messages.push(gm);
                    fields.push(gf);
                } else if is_type_start(&next.value) {
                    fields.push(parse_field_from(next, FieldLabel::Optional, lexer)?);
                } else {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type after 'optional'".to_owned(),
                        found: next.value.to_string(),
                        span: next.span,
                    });
                }
            }
            Token::Required => {
                // proto2 required field (or required group)
                let next = next_or_eof(lexer)?;
                // `required group GroupName = N { ... }` — proto2 group field
                if matches!(next.value, Token::Group) {
                    let (gf, gm) = parse_group_field(lexer, FieldLabel::Required, next.span)?;
                    nested_messages.push(gm);
                    fields.push(gf);
                } else if is_type_start(&next.value) {
                    fields.push(parse_field_from(next, FieldLabel::Required, lexer)?);
                } else {
                    return Err(ParseError::UnexpectedToken {
                        expected: "field type after 'required'".to_owned(),
                        found: next.value.to_string(),
                        span: next.span,
                    });
                }
            }
            Token::Extensions => {
                // proto2 extensions statement
                let ext_ranges = parse_extensions_statement(lexer)?;
                extensions.extend(ext_ranges);
            }
            Token::Group => {
                // Bare `group GroupName = N { ... }` at message level (implicit optional).
                let (gf, gm) = parse_group_field(lexer, FieldLabel::Optional, tok.span)?;
                nested_messages.push(gm);
                fields.push(gf);
            }
            other => {
                let first = Spanned::new(other, tok.span);
                if is_type_start(&first.value) {
                    fields.push(parse_field_from(first, FieldLabel::Singular, lexer)?);
                } else {
                    return Err(ParseError::UnexpectedToken {
                        expected:
                            "field, oneof, message, enum, option, reserved, extensions, or '}'"
                                .to_owned(),
                        found: first.value.to_string(),
                        span: first.span,
                    });
                }
            }
        }
    }

    Ok(Message {
        name,
        fields,
        oneofs,
        nested_messages,
        nested_enums,
        reserved,
        options,
        extensions,
        span: Span::new(kw_span.start, close_span.end),
    })
}

// ---------------------------------------------------------------------------
// Extensions statement (proto2)
// ---------------------------------------------------------------------------

/// Parse the body of `extensions N [to M|max] [, ...] ;` after the
/// `extensions` keyword has been consumed.
fn parse_extensions_statement(
    lexer: &mut PeekLexer<'_>,
) -> Result<Vec<ExtensionRange>, ParseError> {
    let mut ranges = Vec::new();

    loop {
        // Expect a start number.
        let start_tok = next_or_eof(lexer)?;
        let start: u32 = match start_tok.value {
            Token::IntLit(n) => n as u32,
            other => {
                return Err(ParseError::UnexpectedToken {
                    expected: "integer (extension range start)".to_owned(),
                    found: other.to_string(),
                    span: start_tok.span,
                });
            }
        };

        // Optional `to N|max`
        let end: Option<u32> = if peek_is(lexer, |t| matches!(t, Token::To)) {
            let _ = lexer.next(); // consume `to`
            let end_tok = next_or_eof(lexer)?;
            match end_tok.value {
                Token::Ident(ref s) if s == "max" => {
                    // `max` = 536870911 inclusive
                    Some(536_870_911u32)
                }
                Token::IntLit(n) => Some(n as u32),
                other => {
                    return Err(ParseError::UnexpectedToken {
                        expected: "integer or 'max' (extension range end)".to_owned(),
                        found: other.to_string(),
                        span: end_tok.span,
                    });
                }
            }
        } else {
            // Bare number: `extensions N;` — treat as open-ended (no explicit end)
            None
        };

        ranges.push(ExtensionRange { start, end });

        // A comma means another range follows; otherwise expect `;`
        if peek_is(lexer, |t| matches!(t, Token::Comma)) {
            let _ = lexer.next(); // consume `,`
        } else {
            break;
        }
    }

    expect_semi(lexer)?;
    Ok(ranges)
}

// ---------------------------------------------------------------------------
// Enum
// ---------------------------------------------------------------------------

/// Parse an `enum Name { ... }` definition.
/// The `enum` keyword has already been consumed.
fn parse_enum(lexer: &mut PeekLexer<'_>, kw_span: Span) -> Result<Enum, ParseError> {
    let (name, _) = expect_ident(lexer, "enum name")?;
    let open = expect_token(lexer, "{", |t| matches!(t, Token::LBrace))?;
    let open_span = open.span;

    let mut values = Vec::new();
    let mut reserved = Vec::new();
    let mut options = Vec::new();
    let close_span;

    loop {
        let tok = match next_significant(lexer)? {
            None => return Err(ParseError::UnbalancedBraces { span: open_span }),
            Some(s) => s,
        };
        match tok.value {
            Token::RBrace => {
                close_span = tok.span;
                break;
            }
            Token::Option => {
                options.push(parse_option_statement(lexer, tok.span)?);
            }
            Token::Reserved => {
                reserved.push(parse_reserved(lexer)?);
            }
            Token::Ident(ev_name) => {
                // enum_value_name = number [opts] ;
                let val_start = tok.span;
                expect_equals(lexer)?;

                // Enum values can be negative
                let number = if peek_is(lexer, |t| matches!(t, Token::Minus)) {
                    let _ = lexer.next();
                    let (n, _) = expect_int_i32(lexer, "enum value number")?;
                    -n
                } else {
                    let (n, _) = expect_int_i32(lexer, "enum value number")?;
                    n
                };

                let ev_options = if peek_is(lexer, |t| matches!(t, Token::LBracket)) {
                    let open_ev = lexer.next();
                    let open_ev_span = open_ev
                        .and_then(|r| r.ok())
                        .map(|s| s.span)
                        .unwrap_or(val_start);
                    parse_field_options(lexer, open_ev_span)?
                } else {
                    Vec::new()
                };

                let semi = expect_token(lexer, ";", |t| matches!(t, Token::Semicolon))?;

                values.push(EnumValue {
                    name: ev_name,
                    number,
                    options: ev_options,
                    span: Span::new(val_start.start, semi.span.end),
                });
            }
            other => {
                return Err(ParseError::UnexpectedToken {
                    expected: "enum value name, 'option', 'reserved', or '}'".to_owned(),
                    found: other.to_string(),
                    span: tok.span,
                });
            }
        }
    }

    Ok(Enum {
        name,
        values,
        reserved,
        options,
        span: Span::new(kw_span.start, close_span.end),
    })
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// Parse a `service Name { ... }` definition.
/// The `service` keyword has already been consumed.
fn parse_service(lexer: &mut PeekLexer<'_>, kw_span: Span) -> Result<Service, ParseError> {
    let (name, _) = expect_ident(lexer, "service name")?;
    let open = expect_token(lexer, "{", |t| matches!(t, Token::LBrace))?;
    let open_span = open.span;

    let mut methods = Vec::new();
    let mut options = Vec::new();
    let close_span;

    loop {
        let tok = match next_significant(lexer)? {
            None => return Err(ParseError::UnbalancedBraces { span: open_span }),
            Some(s) => s,
        };
        match tok.value {
            Token::RBrace => {
                close_span = tok.span;
                break;
            }
            Token::Option => {
                options.push(parse_option_statement(lexer, tok.span)?);
            }
            Token::Rpc => {
                methods.push(parse_rpc(lexer, tok.span)?);
            }
            other => {
                return Err(ParseError::UnexpectedToken {
                    expected: "'rpc', 'option', or '}'".to_owned(),
                    found: other.to_string(),
                    span: tok.span,
                });
            }
        }
    }

    Ok(Service {
        name,
        methods,
        options,
        span: Span::new(kw_span.start, close_span.end),
    })
}

/// Parse an `rpc` method definition.
/// The `rpc` keyword has already been consumed.
fn parse_rpc(lexer: &mut PeekLexer<'_>, kw_span: Span) -> Result<Method, ParseError> {
    let (method_name, _) = expect_ident(lexer, "rpc method name")?;
    expect_token(lexer, "(", |t| matches!(t, Token::LParen))?;

    // Optional `stream` → client streaming
    let client_streaming = if peek_is(lexer, |t| matches!(t, Token::Stream)) {
        let _ = lexer.next();
        true
    } else {
        false
    };

    let input_type = parse_type_ref(lexer)?;
    expect_token(lexer, ")", |t| matches!(t, Token::RParen))?;
    expect_token(lexer, "returns", |t| matches!(t, Token::Returns))?;
    expect_token(lexer, "(", |t| matches!(t, Token::LParen))?;

    // Optional `stream` → server streaming
    let server_streaming = if peek_is(lexer, |t| matches!(t, Token::Stream)) {
        let _ = lexer.next();
        true
    } else {
        false
    };

    let output_type = parse_type_ref(lexer)?;
    expect_token(lexer, ")", |t| matches!(t, Token::RParen))?;

    // Either `;` or `{ options... }`
    let (rpc_options, end_span) = parse_rpc_body(lexer)?;

    Ok(Method {
        name: method_name,
        input_type,
        output_type,
        client_streaming,
        server_streaming,
        options: rpc_options,
        span: Span::new(kw_span.start, end_span.end),
    })
}

/// Parse a type reference (possibly leading-dot, possibly dotted path).
/// Returns the string representation.
fn parse_type_ref(lexer: &mut PeekLexer<'_>) -> Result<String, ParseError> {
    let first = next_or_eof(lexer)?;
    match parse_field_type_from(first, lexer)? {
        FieldType::Named(s) => Ok(s),
        FieldType::Scalar(sc) => Ok(scalar_name(sc).to_owned()),
        FieldType::Map { .. } => Err(ParseError::UnexpectedToken {
            expected: "type reference".to_owned(),
            found: "map".to_owned(),
            span: Span::new(0, 0),
        }),
        // Group is never produced by parse_field_type_from; this arm is
        // unreachable but required for exhaustiveness.
        FieldType::Group(name) => Ok(name),
    }
}

/// Return the canonical proto keyword name for a scalar type.
fn scalar_name(sc: ScalarType) -> &'static str {
    match sc {
        ScalarType::Double => "double",
        ScalarType::Float => "float",
        ScalarType::Int32 => "int32",
        ScalarType::Int64 => "int64",
        ScalarType::Uint32 => "uint32",
        ScalarType::Uint64 => "uint64",
        ScalarType::Sint32 => "sint32",
        ScalarType::Sint64 => "sint64",
        ScalarType::Fixed32 => "fixed32",
        ScalarType::Fixed64 => "fixed64",
        ScalarType::Sfixed32 => "sfixed32",
        ScalarType::Sfixed64 => "sfixed64",
        ScalarType::Bool => "bool",
        ScalarType::String => "string",
        ScalarType::Bytes => "bytes",
    }
}

/// Parse the trailing part of an `rpc` declaration: either `;` or `{ ... }`.
/// Returns `(options, end_span)`.
fn parse_rpc_body(lexer: &mut PeekLexer<'_>) -> Result<(Vec<ProtoOption>, Span), ParseError> {
    let tok = next_or_eof(lexer)?;
    match tok.value {
        Token::Semicolon => Ok((Vec::new(), tok.span)),
        Token::LBrace => {
            let open_span = tok.span;
            let mut opts = Vec::new();
            let close_span;
            loop {
                let inner = match next_significant(lexer)? {
                    None => return Err(ParseError::UnbalancedBraces { span: open_span }),
                    Some(s) => s,
                };
                match inner.value {
                    Token::RBrace => {
                        close_span = inner.span;
                        break;
                    }
                    Token::Option => {
                        opts.push(parse_option_statement(lexer, inner.span)?);
                    }
                    other => {
                        return Err(ParseError::UnexpectedToken {
                            expected: "'option' or '}' inside rpc body".to_owned(),
                            found: other.to_string(),
                            span: inner.span,
                        });
                    }
                }
            }
            Ok((opts, close_span))
        }
        other => Err(ParseError::UnexpectedToken {
            expected: "';' or '{' after rpc signature".to_owned(),
            found: other.to_string(),
            span: tok.span,
        }),
    }
}
