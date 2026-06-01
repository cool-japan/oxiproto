#![forbid(unsafe_code)]

//! Hand-written lexer for `.proto` source files.
//!
//! # Usage
//! ```ignore
//! let lexer = Lexer::new(source);
//! for result in lexer {
//!     let spanned = result?;
//!     // spanned.value is a Token, spanned.span is a Span
//! }
//! ```

use super::{
    error::LexError,
    span::{Span, Spanned},
    token::Token,
};

// ---------------------------------------------------------------------------
// Lexer struct
// ---------------------------------------------------------------------------

/// A streaming tokenizer over a `.proto` source string.
///
/// Implements [`Iterator`]`<Item = Result<`[`Spanned<Token>`]`, `[`LexError`]`>>`.
/// After [`Token::Eof`] is yielded, subsequent calls to [`Iterator::next`] return
/// `None`.
pub struct Lexer<'a> {
    input: &'a str,
    /// Current byte position (into `input`).
    pos: usize,
    /// 1-indexed current line number.
    line: u32,
    /// 1-indexed current column number (byte-based).
    col: u32,
    /// Set to `true` once `Token::Eof` has been returned.
    emitted_eof: bool,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for `input`.
    pub fn new(input: &'a str) -> Self {
        Lexer {
            input,
            pos: 0,
            line: 1,
            col: 1,
            emitted_eof: false,
        }
    }

    /// Current position as a zero-length span.
    pub fn current_span(&self) -> Span {
        Span::new(self.pos, self.pos)
    }

    // ------------------------------------------------------------------
    // Low-level character access
    // ------------------------------------------------------------------

    /// Returns the byte at `self.pos`, or `None` at end-of-input.
    fn peek_byte(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    /// Returns the byte `n` positions ahead of `self.pos`, without advancing.
    fn peek_byte_at(&self, offset: usize) -> Option<u8> {
        self.input.as_bytes().get(self.pos + offset).copied()
    }

    /// Advance by one byte, updating `line` and `col`.
    /// Returns the consumed byte, or `None` if at end-of-input.
    fn advance(&mut self) -> Option<u8> {
        let b = self.input.as_bytes().get(self.pos).copied()?;
        self.pos += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(b)
    }

    /// Skip a `\r\n` or lone `\r` as a single newline, advancing position.
    /// Call only when `peek_byte() == Some(b'\r')`.
    fn advance_cr(&mut self) {
        // consume the \r
        self.pos += 1;
        // skip a following \n if present (CRLF)
        if self.peek_byte() == Some(b'\n') {
            self.pos += 1;
        }
        self.line += 1;
        self.col = 1;
    }

    // ------------------------------------------------------------------
    // Whitespace
    // ------------------------------------------------------------------

    fn skip_whitespace(&mut self) {
        loop {
            match self.peek_byte() {
                Some(b' ') | Some(b'\t') => {
                    self.advance();
                }
                Some(b'\n') => {
                    self.advance();
                }
                Some(b'\r') => {
                    self.advance_cr();
                }
                _ => break,
            }
        }
    }

    // ------------------------------------------------------------------
    // Comments
    // ------------------------------------------------------------------

    /// Lex `// ... \n` as a `Token::LineComment`.
    /// Caller has already verified the next two bytes are `//`.
    fn lex_line_comment(&mut self) -> Spanned<Token> {
        let start = self.pos;
        // consume `//`
        self.pos += 2;
        self.col += 2;
        let text_start = self.pos;
        loop {
            match self.peek_byte() {
                None | Some(b'\n') => break,
                Some(b'\r') => break,
                _ => {
                    self.advance();
                }
            }
        }
        let text = self.input[text_start..self.pos].to_owned();
        Spanned::new(Token::LineComment(text), Span::new(start, self.pos))
    }

    /// Lex `/* ... */` as a `Token::BlockComment`.
    /// Caller has already verified the next two bytes are `/*`.
    fn lex_block_comment(&mut self) -> Result<Spanned<Token>, LexError> {
        let start = self.pos;
        // consume `/*`
        self.pos += 2;
        self.col += 2;
        let text_start = self.pos;
        loop {
            match self.peek_byte() {
                None => {
                    return Err(LexError::UnterminatedBlockComment {
                        span: Span::new(start, self.pos),
                    });
                }
                Some(b'*') if self.peek_byte_at(1) == Some(b'/') => {
                    let text = self.input[text_start..self.pos].to_owned();
                    // consume `*/`
                    self.pos += 2;
                    self.col += 2;
                    return Ok(Spanned::new(
                        Token::BlockComment(text),
                        Span::new(start, self.pos),
                    ));
                }
                Some(b'\n') => {
                    self.advance();
                }
                Some(b'\r') => {
                    self.advance_cr();
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Identifiers and keywords
    // ------------------------------------------------------------------

    /// Lex an identifier or keyword token.
    /// Caller has verified that `peek_byte()` is ASCII alpha or `_`.
    fn lex_ident_or_keyword(&mut self) -> Spanned<Token> {
        let start = self.pos;
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.advance();
            } else {
                break;
            }
        }
        let name = &self.input[start..self.pos];
        // Check inf / nan as float literals first
        let tok = match name {
            "inf" => Token::FloatLit(f64::INFINITY),
            "nan" => Token::FloatLit(f64::NAN),
            other => Token::from_keyword(other).unwrap_or_else(|| Token::Ident(other.to_owned())),
        };
        Spanned::new(tok, Span::new(start, self.pos))
    }

    // ------------------------------------------------------------------
    // Number literals
    // ------------------------------------------------------------------

    /// Lex an integer or float literal.
    /// Caller has verified that `peek_byte()` is an ASCII digit.
    fn lex_number(&mut self) -> Result<Spanned<Token>, LexError> {
        let start = self.pos;

        // Determine base / form
        if self.peek_byte() == Some(b'0') && self.peek_byte_at(1) == Some(b'x')
            || self.peek_byte() == Some(b'0') && self.peek_byte_at(1) == Some(b'X')
        {
            return self.lex_hex_int(start);
        }

        // Collect raw digits (and possible float indicators)
        let mut is_float = false;
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_digit() {
                self.advance();
            } else if b == b'.' && self.peek_byte_at(1).is_some_and(|c| c != b'.') {
                // Only treat as float if the dot is followed by a digit or is
                // immediately the end of the number (e.g., `1.e3`).
                is_float = true;
                self.advance();
                // consume fraction digits
                while self.peek_byte().is_some_and(|c| c.is_ascii_digit()) {
                    self.advance();
                }
                break;
            } else if b == b'e' || b == b'E' {
                is_float = true;
                break;
            } else {
                break;
            }
        }

        // Possible exponent
        if matches!(self.peek_byte(), Some(b'e') | Some(b'E')) {
            is_float = true;
            self.advance(); // consume e/E
            if matches!(self.peek_byte(), Some(b'+') | Some(b'-')) {
                self.advance();
            }
            while self.peek_byte().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        }

        let raw = &self.input[start..self.pos];

        if is_float {
            let v: f64 = raw.parse().map_err(|_| LexError::FloatParseError {
                span: Span::new(start, self.pos),
            })?;
            return Ok(Spanned::new(Token::FloatLit(v), Span::new(start, self.pos)));
        }

        // Integer — octal if starts with 0 followed by more digits
        if raw.starts_with('0') && raw.len() > 1 {
            return self.parse_octal_int(raw, start);
        }

        // Decimal
        let v: u64 = raw.parse().map_err(|_| LexError::IntOverflow {
            span: Span::new(start, self.pos),
        })?;
        Ok(Spanned::new(Token::IntLit(v), Span::new(start, self.pos)))
    }

    /// Lex `0x...` hexadecimal integer.
    fn lex_hex_int(&mut self, start: usize) -> Result<Spanned<Token>, LexError> {
        // consume `0x`
        self.advance();
        self.advance();
        let hex_start = self.pos;
        while self.peek_byte().is_some_and(|b| b.is_ascii_hexdigit()) {
            self.advance();
        }
        let hex_str = &self.input[hex_start..self.pos];
        let v = u64::from_str_radix(hex_str, 16).map_err(|_| LexError::IntOverflow {
            span: Span::new(start, self.pos),
        })?;
        Ok(Spanned::new(Token::IntLit(v), Span::new(start, self.pos)))
    }

    /// Parse a raw decimal-digit string as octal (all digits must be 0–7).
    fn parse_octal_int(&self, raw: &str, start: usize) -> Result<Spanned<Token>, LexError> {
        let end = start + raw.len();
        let v = u64::from_str_radix(raw, 8).map_err(|_| LexError::IntOverflow {
            span: Span::new(start, end),
        })?;
        Ok(Spanned::new(Token::IntLit(v), Span::new(start, end)))
    }

    /// Lex a float that begins with `.` (e.g. `.5`).
    fn lex_dot_float(&mut self) -> Result<Spanned<Token>, LexError> {
        let start = self.pos;
        self.advance(); // consume `.`
        while self.peek_byte().is_some_and(|b| b.is_ascii_digit()) {
            self.advance();
        }
        // possible exponent
        if matches!(self.peek_byte(), Some(b'e') | Some(b'E')) {
            self.advance();
            if matches!(self.peek_byte(), Some(b'+') | Some(b'-')) {
                self.advance();
            }
            while self.peek_byte().is_some_and(|b| b.is_ascii_digit()) {
                self.advance();
            }
        }
        let raw = &self.input[start..self.pos];
        let v: f64 = raw.parse().map_err(|_| LexError::FloatParseError {
            span: Span::new(start, self.pos),
        })?;
        Ok(Spanned::new(Token::FloatLit(v), Span::new(start, self.pos)))
    }

    // ------------------------------------------------------------------
    // String literals
    // ------------------------------------------------------------------

    /// Lex a string literal delimited by `quote` (`"` or `'`).
    fn lex_string(&mut self, quote: u8) -> Result<Spanned<Token>, LexError> {
        let start = self.pos;
        self.advance(); // consume opening quote
        let mut buf = String::new();
        loop {
            match self.peek_byte() {
                None => {
                    return Err(LexError::UnterminatedString {
                        span: Span::new(start, self.pos),
                    });
                }
                Some(b'\n') | Some(b'\r') => {
                    // Proto strings must not span lines without escape.
                    return Err(LexError::UnterminatedString {
                        span: Span::new(start, self.pos),
                    });
                }
                Some(b'\\') => {
                    let ch = self.lex_escape()?;
                    buf.push(ch);
                }
                Some(b) if b == quote => {
                    self.advance(); // consume closing quote
                    return Ok(Spanned::new(
                        Token::StringLit(buf),
                        Span::new(start, self.pos),
                    ));
                }
                _ => {
                    // Safe: we just verified peek_byte() is Some, and advance()
                    // returns the byte we're consuming.
                    let b = self.advance().expect("advance after peek");
                    // All proto string bytes are valid UTF-8 (the source is &str).
                    // Multi-byte UTF-8 sequences arrive as multiple advances;
                    // we need to decode them properly.
                    // Instead of byte-by-byte pushing, slice out the char.
                    let ch_start = self.pos - 1;
                    // Recalculate: we advanced 1 byte; check if it's a multi-byte leader.
                    let _ = b; // silence unused warning; we use char-boundary logic below.
                               // Back up and use char-boundary safe slicing.
                    let char_pos = ch_start;
                    // `self.input` is valid UTF-8; find the char starting at char_pos.
                    let ch = self.input[char_pos..]
                        .chars()
                        .next()
                        .expect("valid utf-8 in source");
                    let ch_len = ch.len_utf8();
                    // We already advanced 1 byte; advance the remaining ch_len - 1 bytes.
                    for _ in 1..ch_len {
                        self.advance();
                    }
                    buf.push(ch);
                }
            }
        }
    }

    /// Parse an escape sequence starting with `\`.
    /// Caller must have verified that `peek_byte() == Some(b'\\')`.
    fn lex_escape(&mut self) -> Result<char, LexError> {
        let esc_start = self.pos;
        self.advance(); // consume `\`
        let escape_byte = match self.peek_byte() {
            None => {
                return Err(LexError::UnterminatedString {
                    span: Span::new(esc_start, self.pos),
                });
            }
            Some(b) => b,
        };

        match escape_byte {
            b'n' => {
                self.advance();
                Ok('\n')
            }
            b'r' => {
                self.advance();
                Ok('\r')
            }
            b't' => {
                self.advance();
                Ok('\t')
            }
            b'\\' => {
                self.advance();
                Ok('\\')
            }
            b'"' => {
                self.advance();
                Ok('"')
            }
            b'\'' => {
                self.advance();
                Ok('\'')
            }
            b'a' => {
                self.advance();
                Ok('\x07')
            }
            b'b' => {
                self.advance();
                Ok('\x08')
            }
            b'f' => {
                self.advance();
                Ok('\x0C')
            }
            b'v' => {
                self.advance();
                Ok('\x0B')
            }
            b'0' => {
                // Could be `\0` (null) or `\0NN` (octal).
                // If the next character after `0` is an octal digit, it's
                // an octal escape; otherwise it's a null.
                self.advance(); // consume '0'
                if self.peek_byte().is_some_and(is_octal_digit) {
                    // Octal with leading 0: collect up to 2 more digits.
                    let mut val: u32 = 0; // leading 0 already consumed
                    for _ in 0..2 {
                        if self.peek_byte().is_some_and(is_octal_digit) {
                            let d = self.advance().expect("peeked") - b'0';
                            val = val * 8 + u32::from(d);
                        } else {
                            break;
                        }
                    }
                    char::from_u32(val).ok_or(LexError::InvalidUnicodeEscape {
                        span: Span::new(esc_start, self.pos),
                    })
                } else {
                    Ok('\0')
                }
            }
            b if is_octal_digit(b) && b != b'0' => {
                // Octal escape: 1–3 digits total.
                let mut val: u32 = u32::from(b - b'0');
                self.advance(); // consume first digit
                for _ in 0..2 {
                    if self.peek_byte().is_some_and(is_octal_digit) {
                        let d = self.advance().expect("peeked") - b'0';
                        val = val * 8 + u32::from(d);
                    } else {
                        break;
                    }
                }
                char::from_u32(val).ok_or(LexError::InvalidUnicodeEscape {
                    span: Span::new(esc_start, self.pos),
                })
            }
            b'x' => {
                self.advance(); // consume 'x'
                self.lex_hex_escape(esc_start, 2).and_then(|code| {
                    u8::try_from(code)
                        .ok()
                        .map(|b| b as char)
                        .ok_or(LexError::InvalidHexEscape {
                            span: Span::new(esc_start, self.pos),
                        })
                })
            }
            b'u' => {
                self.advance(); // consume 'u'
                let code = self.lex_hex_escape(esc_start, 4)?;
                char::from_u32(code).ok_or(LexError::InvalidUnicodeEscape {
                    span: Span::new(esc_start, self.pos),
                })
            }
            b'U' => {
                self.advance(); // consume 'U'
                let code = self.lex_hex_escape(esc_start, 8)?;
                char::from_u32(code).ok_or(LexError::InvalidUnicodeEscape {
                    span: Span::new(esc_start, self.pos),
                })
            }
            other => {
                let ch = other as char;
                self.advance();
                Err(LexError::InvalidEscape {
                    ch,
                    span: Span::new(esc_start, self.pos),
                })
            }
        }
    }

    /// Consume exactly `n` hex digits and return their numeric value.
    /// Returns `Err(LexError::InvalidHexEscape)` if fewer than `n` hex digits
    /// are present.
    fn lex_hex_escape(&mut self, esc_start: usize, n: usize) -> Result<u32, LexError> {
        let mut val: u32 = 0;
        for _ in 0..n {
            match self.peek_byte() {
                Some(b) if b.is_ascii_hexdigit() => {
                    self.advance();
                    let digit = hex_digit_value(b);
                    val = val * 16 + u32::from(digit);
                }
                _ => {
                    return Err(LexError::InvalidHexEscape {
                        span: Span::new(esc_start, self.pos),
                    });
                }
            }
        }
        Ok(val)
    }
}

// ---------------------------------------------------------------------------
// Iterator impl
// ---------------------------------------------------------------------------

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<Spanned<Token>, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.emitted_eof {
            return None;
        }

        self.skip_whitespace();

        let start = self.pos;

        let byte = match self.peek_byte() {
            None => {
                self.emitted_eof = true;
                return Some(Ok(Spanned::new(Token::Eof, Span::new(start, start))));
            }
            Some(b) => b,
        };

        // Comments: check before treating `/` as Slash
        if byte == b'/' {
            match self.peek_byte_at(1) {
                Some(b'/') => return Some(Ok(self.lex_line_comment())),
                Some(b'*') => return Some(self.lex_block_comment()),
                _ => {}
            }
        }

        // Float starting with `.`
        if byte == b'.' && self.peek_byte_at(1).is_some_and(|b| b.is_ascii_digit()) {
            return Some(self.lex_dot_float());
        }

        // Single-character punctuation (and `/` after comment check)
        let punct = match byte {
            b'{' => Some(Token::LBrace),
            b'}' => Some(Token::RBrace),
            b'(' => Some(Token::LParen),
            b')' => Some(Token::RParen),
            b'[' => Some(Token::LBracket),
            b']' => Some(Token::RBracket),
            b'<' => Some(Token::LAngle),
            b'>' => Some(Token::RAngle),
            b';' => Some(Token::Semicolon),
            b',' => Some(Token::Comma),
            b'=' => Some(Token::Equals),
            b'.' => Some(Token::Dot),
            b':' => Some(Token::Colon),
            b'/' => Some(Token::Slash),
            b'+' => Some(Token::Plus),
            b'-' => Some(Token::Minus),
            _ => None,
        };
        if let Some(tok) = punct {
            self.advance();
            return Some(Ok(Spanned::new(tok, Span::new(start, self.pos))));
        }

        // Identifier / keyword / inf / nan
        if byte.is_ascii_alphabetic() || byte == b'_' {
            return Some(Ok(self.lex_ident_or_keyword()));
        }

        // Number
        if byte.is_ascii_digit() {
            return Some(self.lex_number());
        }

        // String literal
        if byte == b'"' || byte == b'\'' {
            return Some(self.lex_string(byte));
        }

        // Unknown character — report and skip
        let ch = self.input[self.pos..].chars().next().unwrap_or('\u{FFFD}');
        let ch_len = ch.len_utf8();
        let err_span = Span::new(self.pos, self.pos + ch_len);
        self.pos += ch_len;
        self.col += 1;
        Some(Err(LexError::UnexpectedChar { ch, span: err_span }))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[inline]
fn is_octal_digit(b: u8) -> bool {
    matches!(b, b'0'..=b'7')
}

#[inline]
fn hex_digit_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}
