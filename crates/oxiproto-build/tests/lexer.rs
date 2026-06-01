#![forbid(unsafe_code)]

//! Tests for the native .proto lexer.

use oxiproto_build::parser::{LexError, Lexer, Span, Token};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collect all tokens from `source`, panicking on any lex error.
fn lex_ok(source: &str) -> Vec<Token> {
    Lexer::new(source)
        .map(|r| r.expect("unexpected lex error").value)
        .collect()
}

/// Collect only the `Result` items (without the wrapping Spanned).
fn lex_results(source: &str) -> Vec<Result<Token, LexError>> {
    Lexer::new(source).map(|r| r.map(|s| s.value)).collect()
}

// ---------------------------------------------------------------------------
// Keyword tokens
// ---------------------------------------------------------------------------

#[test]
fn test_keyword_syntax() {
    assert_eq!(lex_ok("syntax"), vec![Token::Syntax, Token::Eof]);
}

#[test]
fn test_keyword_package() {
    assert_eq!(lex_ok("package"), vec![Token::Package, Token::Eof]);
}

#[test]
fn test_keyword_import() {
    assert_eq!(lex_ok("import"), vec![Token::Import, Token::Eof]);
}

#[test]
fn test_keyword_option() {
    assert_eq!(lex_ok("option"), vec![Token::Option, Token::Eof]);
}

#[test]
fn test_keyword_message() {
    assert_eq!(lex_ok("message"), vec![Token::Message, Token::Eof]);
}

#[test]
fn test_keyword_enum() {
    assert_eq!(lex_ok("enum"), vec![Token::Enum, Token::Eof]);
}

#[test]
fn test_keyword_service() {
    assert_eq!(lex_ok("service"), vec![Token::Service, Token::Eof]);
}

#[test]
fn test_keyword_rpc() {
    assert_eq!(lex_ok("rpc"), vec![Token::Rpc, Token::Eof]);
}

#[test]
fn test_keyword_returns() {
    assert_eq!(lex_ok("returns"), vec![Token::Returns, Token::Eof]);
}

#[test]
fn test_keyword_stream() {
    assert_eq!(lex_ok("stream"), vec![Token::Stream, Token::Eof]);
}

#[test]
fn test_keyword_repeated() {
    assert_eq!(lex_ok("repeated"), vec![Token::Repeated, Token::Eof]);
}

#[test]
fn test_keyword_optional() {
    assert_eq!(lex_ok("optional"), vec![Token::Optional, Token::Eof]);
}

#[test]
fn test_keyword_required() {
    assert_eq!(lex_ok("required"), vec![Token::Required, Token::Eof]);
}

#[test]
fn test_keyword_oneof() {
    assert_eq!(lex_ok("oneof"), vec![Token::Oneof, Token::Eof]);
}

#[test]
fn test_keyword_map() {
    assert_eq!(lex_ok("map"), vec![Token::Map, Token::Eof]);
}

#[test]
fn test_keyword_extensions() {
    assert_eq!(lex_ok("extensions"), vec![Token::Extensions, Token::Eof]);
}

#[test]
fn test_keyword_reserved() {
    assert_eq!(lex_ok("reserved"), vec![Token::Reserved, Token::Eof]);
}

#[test]
fn test_keyword_to() {
    assert_eq!(lex_ok("to"), vec![Token::To, Token::Eof]);
}

#[test]
fn test_keyword_public() {
    assert_eq!(lex_ok("public"), vec![Token::Public, Token::Eof]);
}

#[test]
fn test_keyword_weak() {
    assert_eq!(lex_ok("weak"), vec![Token::Weak, Token::Eof]);
}

#[test]
fn test_keyword_group() {
    assert_eq!(lex_ok("group"), vec![Token::Group, Token::Eof]);
}

#[test]
fn test_scalar_type_keywords() {
    let src = "double float int32 int64 uint32 uint64 sint32 sint64 \
               fixed32 fixed64 sfixed32 sfixed64 bool string bytes";
    let tokens = lex_ok(src);
    assert_eq!(
        tokens,
        vec![
            Token::Double,
            Token::Float,
            Token::Int32,
            Token::Int64,
            Token::Uint32,
            Token::Uint64,
            Token::Sint32,
            Token::Sint64,
            Token::Fixed32,
            Token::Fixed64,
            Token::Sfixed32,
            Token::Sfixed64,
            Token::Bool,
            Token::StringType,
            Token::BytesType,
            Token::Eof,
        ]
    );
}

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

#[test]
fn test_ident_simple() {
    assert_eq!(
        lex_ok("hello"),
        vec![Token::Ident("hello".into()), Token::Eof]
    );
}

#[test]
fn test_ident_with_underscore() {
    assert_eq!(
        lex_ok("_foo_bar"),
        vec![Token::Ident("_foo_bar".into()), Token::Eof]
    );
}

#[test]
fn test_ident_with_digits() {
    assert_eq!(
        lex_ok("foo123"),
        vec![Token::Ident("foo123".into()), Token::Eof]
    );
}

#[test]
fn test_ident_starts_with_underscore() {
    assert_eq!(lex_ok("_"), vec![Token::Ident("_".into()), Token::Eof]);
}

// ---------------------------------------------------------------------------
// Integer literals
// ---------------------------------------------------------------------------

#[test]
fn test_int_decimal() {
    assert_eq!(lex_ok("42"), vec![Token::IntLit(42), Token::Eof]);
}

#[test]
fn test_int_zero() {
    assert_eq!(lex_ok("0"), vec![Token::IntLit(0), Token::Eof]);
}

#[test]
fn test_int_hex() {
    assert_eq!(lex_ok("0xFF"), vec![Token::IntLit(255), Token::Eof]);
}

#[test]
fn test_int_hex_lowercase() {
    assert_eq!(lex_ok("0xff"), vec![Token::IntLit(255), Token::Eof]);
}

#[test]
fn test_int_hex_capital_x() {
    assert_eq!(lex_ok("0XFF"), vec![Token::IntLit(255), Token::Eof]);
}

#[test]
fn test_int_octal() {
    assert_eq!(lex_ok("010"), vec![Token::IntLit(8), Token::Eof]);
}

#[test]
fn test_int_octal_777() {
    assert_eq!(lex_ok("0777"), vec![Token::IntLit(0o777), Token::Eof]);
}

#[test]
fn test_int_large() {
    // 2^63 - 1 fits in u64
    assert_eq!(
        lex_ok("9223372036854775807"),
        vec![Token::IntLit(9223372036854775807u64), Token::Eof]
    );
}

// ---------------------------------------------------------------------------
// Float literals
// ---------------------------------------------------------------------------

#[test]
fn test_float_simple() {
    assert_eq!(lex_ok("1.0"), vec![Token::FloatLit(1.0_f64), Token::Eof]);
}

#[test]
fn test_float_leading_dot() {
    assert_eq!(lex_ok(".5"), vec![Token::FloatLit(0.5_f64), Token::Eof]);
}

#[test]
fn test_float_exponent() {
    assert_eq!(lex_ok("1e10"), vec![Token::FloatLit(1e10_f64), Token::Eof]);
}

#[test]
fn test_float_full() {
    assert_eq!(
        lex_ok("1.5e-3"),
        vec![Token::FloatLit(1.5e-3_f64), Token::Eof]
    );
}

#[test]
fn test_float_inf() {
    let tokens = lex_ok("inf");
    assert_eq!(tokens.len(), 2);
    match &tokens[0] {
        Token::FloatLit(f) => assert!(f.is_infinite() && f.is_sign_positive()),
        other => panic!("expected FloatLit(inf), got {other:?}"),
    }
}

#[test]
fn test_float_nan() {
    let tokens = lex_ok("nan");
    assert_eq!(tokens.len(), 2);
    match &tokens[0] {
        Token::FloatLit(f) => assert!(f.is_nan()),
        other => panic!("expected FloatLit(NaN), got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// String literals — basic
// ---------------------------------------------------------------------------

#[test]
fn test_string_double_quoted() {
    assert_eq!(
        lex_ok(r#""hello""#),
        vec![Token::StringLit("hello".into()), Token::Eof]
    );
}

#[test]
fn test_string_single_quoted() {
    assert_eq!(
        lex_ok("'world'"),
        vec![Token::StringLit("world".into()), Token::Eof]
    );
}

#[test]
fn test_string_empty() {
    assert_eq!(
        lex_ok(r#""""#),
        vec![Token::StringLit(String::new()), Token::Eof]
    );
}

// ---------------------------------------------------------------------------
// String escape sequences
// ---------------------------------------------------------------------------

#[test]
fn test_escape_newline() {
    assert_eq!(
        lex_ok(r#""\n""#),
        vec![Token::StringLit("\n".into()), Token::Eof]
    );
}

#[test]
fn test_escape_tab() {
    assert_eq!(
        lex_ok(r#""\t""#),
        vec![Token::StringLit("\t".into()), Token::Eof]
    );
}

#[test]
fn test_escape_backslash() {
    assert_eq!(
        lex_ok(r#""\\""#),
        vec![Token::StringLit("\\".into()), Token::Eof]
    );
}

#[test]
fn test_escape_double_quote() {
    assert_eq!(
        lex_ok(r#""\"""#),
        vec![Token::StringLit("\"".into()), Token::Eof]
    );
}

#[test]
fn test_escape_single_quote() {
    assert_eq!(
        lex_ok(r"'\''"),
        vec![Token::StringLit("'".into()), Token::Eof]
    );
}

#[test]
fn test_escape_alert() {
    assert_eq!(
        lex_ok(r#""\a""#),
        vec![Token::StringLit("\x07".into()), Token::Eof]
    );
}

#[test]
fn test_escape_backspace() {
    assert_eq!(
        lex_ok(r#""\b""#),
        vec![Token::StringLit("\x08".into()), Token::Eof]
    );
}

#[test]
fn test_escape_formfeed() {
    assert_eq!(
        lex_ok(r#""\f""#),
        vec![Token::StringLit("\x0C".into()), Token::Eof]
    );
}

#[test]
fn test_escape_vertical_tab() {
    assert_eq!(
        lex_ok(r#""\v""#),
        vec![Token::StringLit("\x0B".into()), Token::Eof]
    );
}

#[test]
fn test_escape_null() {
    assert_eq!(
        lex_ok(r#""\0""#),
        vec![Token::StringLit("\0".into()), Token::Eof]
    );
}

#[test]
fn test_escape_hex() {
    // \x41 == 'A'
    assert_eq!(
        lex_ok(r#""\x41""#),
        vec![Token::StringLit("A".into()), Token::Eof]
    );
}

#[test]
fn test_escape_unicode_4() {
    // A == 'A'
    assert_eq!(
        lex_ok(r#""A""#),
        vec![Token::StringLit("A".into()), Token::Eof]
    );
}

#[test]
fn test_escape_unicode_8() {
    // \U00000041 == 'A'
    assert_eq!(
        lex_ok(r#""\U00000041""#),
        vec![Token::StringLit("A".into()), Token::Eof]
    );
}

#[test]
fn test_escape_octal_3_digits() {
    // \101 == 0o101 == 65 == 'A'
    assert_eq!(
        lex_ok(r#""\101""#),
        vec![Token::StringLit("A".into()), Token::Eof]
    );
}

#[test]
fn test_escape_octal_1_digit() {
    // \7 == BEL char (7)
    assert_eq!(
        lex_ok(r#""\7""#),
        vec![Token::StringLit("\x07".into()), Token::Eof]
    );
}

#[test]
fn test_escape_octal_stops_at_non_octal() {
    // \1 followed by '2' — lexer reads \1, then '2' is a literal char
    let tokens = lex_ok(r#""\12""#);
    // \12 octal = 10 decimal = '\n'
    assert_eq!(tokens, vec![Token::StringLit("\n".into()), Token::Eof]);
}

// ---------------------------------------------------------------------------
// Line comments
// ---------------------------------------------------------------------------

#[test]
fn test_line_comment() {
    let tokens = lex_ok("// this is a comment\n");
    assert_eq!(
        tokens,
        vec![Token::LineComment(" this is a comment".into()), Token::Eof,]
    );
}

#[test]
fn test_line_comment_at_eof() {
    let tokens = lex_ok("// no newline");
    assert_eq!(
        tokens,
        vec![Token::LineComment(" no newline".into()), Token::Eof]
    );
}

#[test]
fn test_line_comment_span() {
    let source = "// hi\n";
    let spanned: Vec<_> = Lexer::new(source).map(|r| r.expect("ok")).collect();
    // First token: LineComment starting at byte 0
    assert_eq!(spanned[0].span.start, 0);
    // Span ends at byte 5 (after `// hi`, before `\n`)
    assert_eq!(spanned[0].span.end, 5);
}

// ---------------------------------------------------------------------------
// Block comments
// ---------------------------------------------------------------------------

#[test]
fn test_block_comment() {
    let tokens = lex_ok("/* hello world */");
    assert_eq!(
        tokens,
        vec![Token::BlockComment(" hello world ".into()), Token::Eof,]
    );
}

#[test]
fn test_block_comment_multiline() {
    let tokens = lex_ok("/*\nline1\nline2\n*/");
    assert_eq!(
        tokens,
        vec![Token::BlockComment("\nline1\nline2\n".into()), Token::Eof]
    );
}

// ---------------------------------------------------------------------------
// Punctuation tokens
// ---------------------------------------------------------------------------

#[test]
fn test_punctuation_all() {
    let src = "{}()[]<>;,=.:/+-";
    let tokens = lex_ok(src);
    assert_eq!(
        tokens,
        vec![
            Token::LBrace,
            Token::RBrace,
            Token::LParen,
            Token::RParen,
            Token::LBracket,
            Token::RBracket,
            Token::LAngle,
            Token::RAngle,
            Token::Semicolon,
            Token::Comma,
            Token::Equals,
            Token::Dot,
            Token::Colon,
            Token::Slash,
            Token::Plus,
            Token::Minus,
            Token::Eof,
        ]
    );
}

// ---------------------------------------------------------------------------
// Multi-token sequences
// ---------------------------------------------------------------------------

#[test]
fn test_syntax_statement() {
    let tokens = lex_ok(r#"syntax = "proto3";"#);
    assert_eq!(
        tokens,
        vec![
            Token::Syntax,
            Token::Equals,
            Token::StringLit("proto3".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_message_declaration() {
    let tokens = lex_ok("message Foo {}");
    assert_eq!(
        tokens,
        vec![
            Token::Message,
            Token::Ident("Foo".into()),
            Token::LBrace,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

// ---------------------------------------------------------------------------
// Line / column tracking (span offsets)
// ---------------------------------------------------------------------------

#[test]
fn test_span_positions_single_line() {
    let source = "abc def";
    let spanned: Vec<_> = Lexer::new(source).map(|r| r.expect("ok")).collect();
    assert_eq!(spanned[0].span, Span::new(0, 3)); // "abc"
    assert_eq!(spanned[1].span, Span::new(4, 7)); // "def"
}

#[test]
fn test_span_positions_multiline() {
    let source = "a\nb\nc";
    let spanned: Vec<_> = Lexer::new(source).map(|r| r.expect("ok")).collect();
    assert_eq!(spanned[0].span.start, 0); // 'a'
    assert_eq!(spanned[1].span.start, 2); // 'b'
    assert_eq!(spanned[2].span.start, 4); // 'c'
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn test_error_unterminated_string() {
    let results = lex_results(r#""hello"#);
    assert!(results[0].is_err(), "expected unterminated string error");
    match &results[0] {
        Err(LexError::UnterminatedString { .. }) => {}
        other => panic!("wrong error: {other:?}"),
    }
}

#[test]
fn test_error_unterminated_block_comment() {
    let results = lex_results("/* hello");
    assert!(results[0].is_err(), "expected unterminated comment error");
    match &results[0] {
        Err(LexError::UnterminatedBlockComment { .. }) => {}
        other => panic!("wrong error: {other:?}"),
    }
}

#[test]
fn test_error_invalid_escape() {
    let results = lex_results(r#""\q""#);
    assert!(results[0].is_err(), "expected invalid escape error");
    match &results[0] {
        Err(LexError::InvalidEscape { ch: 'q', .. }) => {}
        other => panic!("wrong error: {other:?}"),
    }
}

#[test]
fn test_error_int_overflow() {
    // u64::MAX + 1 = 18446744073709551616 — this overflows
    let results = lex_results("18446744073709551616");
    assert!(results[0].is_err(), "expected int overflow error");
    match &results[0] {
        Err(LexError::IntOverflow { .. }) => {}
        other => panic!("wrong error: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// EOF handling
// ---------------------------------------------------------------------------

#[test]
fn test_eof_is_final_token() {
    let tokens = lex_ok("");
    assert_eq!(tokens, vec![Token::Eof]);
}

#[test]
fn test_no_tokens_after_eof() {
    let mut lexer = Lexer::new("");
    // First call yields Eof
    let first = lexer.next();
    assert!(
        matches!(first, Some(Ok(ref s)) if s.value == Token::Eof),
        "first should be Eof"
    );
    // All subsequent calls yield None
    assert!(lexer.next().is_none(), "should return None after Eof");
    assert!(lexer.next().is_none(), "should return None after Eof (2)");
}

#[test]
fn test_whitespace_only_yields_eof() {
    let tokens = lex_ok("   \t  \n  \r\n  ");
    assert_eq!(tokens, vec![Token::Eof]);
}
