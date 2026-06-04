//! Protobuf text format encoding/decoding for native [`DynamicMessage`].
//!
//! The text format is a human-readable serialisation of protobuf messages:
//!
//! ```text
//! field_name: value
//! nested_field {
//!   sub_field: 42
//! }
//! repeated_field: 1
//! repeated_field: 2
//! ```
//!
//! Rules implemented:
//! - Singular scalar fields: `name: value`
//! - String fields: `name: "escaped_string"`
//! - Bytes fields: `name: "<base64_or_escaped>"` — we use hex-escaped bytes
//!   (`\xNN`) for binary content.
//! - Enum fields: `name: VALUE_NAME` (or integer for unknown values).
//! - Nested messages: `name { ... }` with 2-space indentation.
//! - Repeated fields: one entry per value, same name repeated.
//! - Map fields: expanded as repeated synthetic entries `name { key: K value: V }`.
//! - Proto3 default-valued singular fields are omitted on output.
//! - `float`/`double` NaN → `nan`, Inf → `inf`, -Inf → `-inf`.
//! - 64-bit integer fields use the integer literal (no quotes).
//!
//! Parsing supports:
//! - `name: value` and `name { ... }` (brace-delimited message) syntaxes.
//! - Quoted string values (double-quotes, with `\n\r\t\\\"` escapes).
//! - Integer, float, boolean (`true`/`false`), `nan`/`inf`/`-inf` literals.
//! - Unquoted identifiers for enum names.
//! - Comment lines starting with `#` are skipped.
//! - Unknown field names are silently skipped.

use std::sync::Arc;

use super::descriptor::{Cardinality, FieldDescriptor, Kind, MessageDescriptor};
use super::dynamic::{is_field_value_default, DynamicMessage};
use super::value::{MapKey, Value};

// ---------------------------------------------------------------------------
// Public error type
// ---------------------------------------------------------------------------

/// Errors produced during protobuf text-format conversion.
#[derive(Debug)]
pub enum TextError {
    /// Malformed text input.
    Parse(String),
    /// Schema mismatch between the text and the message descriptor.
    Schema(String),
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextError::Parse(s) => write!(f, "text format parse error: {s}"),
            TextError::Schema(s) => write!(f, "schema error: {s}"),
        }
    }
}

impl std::error::Error for TextError {}

impl From<TextError> for crate::ReflectError {
    fn from(e: TextError) -> Self {
        crate::ReflectError::Field(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Public API on DynamicMessage
// ---------------------------------------------------------------------------

impl DynamicMessage {
    /// Encode this message to the protobuf text format.
    ///
    /// Proto3 default-valued singular fields are omitted.
    ///
    /// # Errors
    ///
    /// Returns [`TextError`] if encoding fails (e.g. an unsupported group
    /// field is encountered).
    pub fn to_text(&self) -> Result<String, TextError> {
        let mut out = String::new();
        encode_message(self, &mut out, 0)?;
        Ok(out)
    }

    /// Decode a protobuf text-format string into a new [`DynamicMessage`] of
    /// the given descriptor.
    ///
    /// Unknown fields are silently skipped. Comments (lines starting with `#`)
    /// are ignored.
    ///
    /// # Errors
    ///
    /// Returns [`TextError`] if the input cannot be parsed or does not match
    /// the schema.
    pub fn from_text(desc: MessageDescriptor, text: &str) -> Result<Self, TextError> {
        let mut parser = Parser::new(text);
        parser.parse_message(desc)
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

const INDENT: &str = "  ";

fn encode_message(msg: &DynamicMessage, out: &mut String, depth: usize) -> Result<(), TextError> {
    let desc = msg.descriptor();
    let prefix = INDENT.repeat(depth);

    for field in desc.fields() {
        let value = msg.get_field(&field);
        if is_field_value_default(&field, &value) {
            continue;
        }
        encode_field_value(&field, &value, &prefix, out, depth)?;
    }
    Ok(())
}

fn encode_field_value(
    field: &FieldDescriptor,
    value: &Value,
    prefix: &str,
    out: &mut String,
    depth: usize,
) -> Result<(), TextError> {
    if field.is_map() {
        return encode_map_field(field, value, prefix, out, depth);
    }
    if matches!(field.cardinality(), Cardinality::Repeated) {
        return encode_repeated_field(field, value, prefix, out, depth);
    }
    encode_singular_field(field, value, prefix, out, depth)
}

fn encode_repeated_field(
    field: &FieldDescriptor,
    value: &Value,
    prefix: &str,
    out: &mut String,
    depth: usize,
) -> Result<(), TextError> {
    if let Value::List(items) = value {
        for item in items {
            encode_singular_field(field, item, prefix, out, depth)?;
        }
        Ok(())
    } else {
        Err(TextError::Schema(format!(
            "expected list for repeated field '{}'",
            field.name()
        )))
    }
}

fn encode_map_field(
    field: &FieldDescriptor,
    value: &Value,
    prefix: &str,
    out: &mut String,
    depth: usize,
) -> Result<(), TextError> {
    let val_field = field.map_entry_value_field().ok_or_else(|| {
        TextError::Schema(format!(
            "map field '{}' missing value field descriptor",
            field.name()
        ))
    })?;
    let key_field = field.map_entry_key_field().ok_or_else(|| {
        TextError::Schema(format!(
            "map field '{}' missing key field descriptor",
            field.name()
        ))
    })?;

    if let Value::Map(entries) = value {
        let mut sorted: Vec<_> = entries.iter().collect();
        sorted.sort_by_key(|(k, _)| map_key_sort_key(k));
        for (k, v) in sorted {
            // Each entry is a sub-message: `field_name { key: K  value: V }`
            out.push_str(prefix);
            out.push_str(field.name());
            out.push_str(" {\n");
            let inner_prefix = format!("{prefix}{INDENT}");
            // key
            encode_singular_field(&key_field, &k.to_value(), &inner_prefix, out, depth + 1)?;
            // value
            encode_singular_field(&val_field, v, &inner_prefix, out, depth + 1)?;
            out.push_str(prefix);
            out.push_str("}\n");
        }
        Ok(())
    } else {
        Err(TextError::Schema(format!(
            "expected map for map field '{}'",
            field.name()
        )))
    }
}

fn encode_singular_field(
    field: &FieldDescriptor,
    value: &Value,
    prefix: &str,
    out: &mut String,
    depth: usize,
) -> Result<(), TextError> {
    match value {
        Value::Message(m) => {
            out.push_str(prefix);
            out.push_str(field.name());
            out.push_str(" {\n");
            encode_message(m, out, depth + 1)?;
            out.push_str(prefix);
            out.push_str("}\n");
        }
        other => {
            out.push_str(prefix);
            out.push_str(field.name());
            out.push_str(": ");
            encode_scalar_value(other, field, out)?;
            out.push('\n');
        }
    }
    Ok(())
}

fn encode_scalar_value(
    value: &Value,
    field: &FieldDescriptor,
    out: &mut String,
) -> Result<(), TextError> {
    match value {
        Value::F64(v) => {
            if v.is_nan() {
                out.push_str("nan");
            } else if *v == f64::INFINITY {
                out.push_str("inf");
            } else if *v == f64::NEG_INFINITY {
                out.push_str("-inf");
            } else {
                out.push_str(&format!("{v}"));
            }
        }
        Value::F32(v) => {
            let v64 = f64::from(*v);
            if v64.is_nan() {
                out.push_str("nan");
            } else if v64 == f64::INFINITY {
                out.push_str("inf");
            } else if v64 == f64::NEG_INFINITY {
                out.push_str("-inf");
            } else {
                out.push_str(&format!("{v}"));
            }
        }
        Value::I32(v) => out.push_str(&v.to_string()),
        Value::I64(v) => out.push_str(&v.to_string()),
        Value::U32(v) => out.push_str(&v.to_string()),
        Value::U64(v) => out.push_str(&v.to_string()),
        Value::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        Value::String(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    '\0' => out.push_str("\\0"),
                    other => out.push(other),
                }
            }
            out.push('"');
        }
        Value::Bytes(b) => {
            out.push('"');
            for byte in b {
                out.push_str(&format!("\\x{byte:02x}"));
            }
            out.push('"');
        }
        Value::EnumNumber(n) => {
            // Prefer the enum value name for readability.
            if let Some(enum_desc) = field.enum_type() {
                if let Some(val_desc) = enum_desc.get_value(*n) {
                    out.push_str(val_desc.name());
                    return Ok(());
                }
            }
            out.push_str(&n.to_string());
        }
        Value::Message(_) | Value::List(_) | Value::Map(_) => {
            return Err(TextError::Schema(
                "unexpected nested structure in scalar context".to_owned(),
            ));
        }
    }
    Ok(())
}

fn map_key_sort_key(k: &MapKey) -> String {
    match k {
        MapKey::String(s) => format!("s{s}"),
        MapKey::I32(v) => format!("n{:020}", v),
        MapKey::I64(v) => format!("n{:020}", v),
        MapKey::U32(v) => format!("n{:020}", v),
        MapKey::U64(v) => format!("n{:020}", v),
        MapKey::Bool(v) => format!("b{}", if *v { 1 } else { 0 }),
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn is_empty(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Skip whitespace (spaces, tabs, newlines) and `#` comment lines.
    fn skip_ws(&mut self) {
        loop {
            while self.pos < self.input.len()
                && matches!(
                    self.input.as_bytes()[self.pos],
                    b' ' | b'\t' | b'\n' | b'\r'
                )
            {
                self.pos += 1;
            }
            // Skip comment lines.
            if self.pos < self.input.len() && self.input.as_bytes()[self.pos] == b'#' {
                while self.pos < self.input.len() && self.input.as_bytes()[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    /// Peek at the next non-whitespace byte.
    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.input.as_bytes().get(self.pos).copied()
    }

    /// Expect a specific character; error if not found.
    fn expect(&mut self, ch: char) -> Result<(), TextError> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        if self.pos < bytes.len() && bytes[self.pos] == ch as u8 {
            self.pos += 1;
            Ok(())
        } else {
            let got = if self.pos < bytes.len() {
                format!("'{}'", bytes[self.pos] as char)
            } else {
                "EOF".to_owned()
            };
            Err(TextError::Parse(format!("expected '{ch}', got {got}")))
        }
    }

    /// Read a bare token (identifier, number, or keyword) up to whitespace,
    /// `:`, `{`, `}`, `;`.
    fn read_token(&mut self) -> Result<String, TextError> {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.input.len() {
            match self.input.as_bytes()[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' | b':' | b'{' | b'}' | b';' | b',' => break,
                _ => self.pos += 1,
            }
        }
        if self.pos == start {
            return Err(TextError::Parse("expected token, got empty".to_owned()));
        }
        Ok(self.input[start..self.pos].to_owned())
    }

    /// Read a double-quoted string value, handling common escapes.
    /// Returns the decoded string as a `String` for string fields.
    fn read_string(&mut self) -> Result<String, TextError> {
        let bytes = self.read_string_as_bytes()?;
        String::from_utf8(bytes)
            .map_err(|e| TextError::Parse(format!("string field contains invalid UTF-8: {e}")))
    }

    /// Read a double-quoted value, returning the raw decoded bytes.
    /// This handles `\xNN` escapes as actual byte values (not Unicode code
    /// points), which is the protobuf text format rule for `bytes` fields.
    fn read_string_as_bytes(&mut self) -> Result<Vec<u8>, TextError> {
        self.expect('"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            if self.pos >= self.input.len() {
                return Err(TextError::Parse("unterminated string literal".to_owned()));
            }
            let b = self.input.as_bytes()[self.pos];
            self.pos += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    if self.pos >= self.input.len() {
                        return Err(TextError::Parse("unterminated escape".to_owned()));
                    }
                    let esc = self.input.as_bytes()[self.pos];
                    self.pos += 1;
                    match esc {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'\\' => out.push(b'\\'),
                        b'"' => out.push(b'"'),
                        b'0' => out.push(0),
                        b'x' | b'X' => {
                            // Hex escape \xNN: a raw byte value (not a char).
                            let h1 = self.read_hex_digit()?;
                            let h2 = self.read_hex_digit()?;
                            out.push(h1 * 16 + h2);
                        }
                        other => {
                            out.push(b'\\');
                            out.push(other);
                        }
                    }
                }
                other => out.push(other),
            }
        }
        Ok(out)
    }

    fn read_hex_digit(&mut self) -> Result<u8, TextError> {
        if self.pos >= self.input.len() {
            return Err(TextError::Parse("unexpected EOF in hex escape".to_owned()));
        }
        let b = self.input.as_bytes()[self.pos];
        self.pos += 1;
        match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            b'A'..=b'F' => Ok(b - b'A' + 10),
            other => Err(TextError::Parse(format!(
                "invalid hex digit: '{}'",
                other as char
            ))),
        }
    }

    fn parse_message(&mut self, desc: MessageDescriptor) -> Result<DynamicMessage, TextError> {
        let mut msg = DynamicMessage::new(desc.clone());
        loop {
            self.skip_ws();
            if self.is_empty() {
                break;
            }
            // Check for end-of-block (closing brace).
            if self.peek() == Some(b'}') {
                break;
            }

            // Read field name.
            let field_name = self.read_token()?;
            self.skip_ws();

            // Look up field by name or json_name.
            let field = desc
                .get_field_by_name(&field_name)
                .or_else(|| desc.get_field_by_json_name(&field_name));

            let field = match field {
                Some(f) => f,
                None => {
                    // Unknown field — skip its value.
                    self.skip_field_value()?;
                    // Optional trailing semicolons.
                    if self.peek() == Some(b';') {
                        self.pos += 1;
                    }
                    continue;
                }
            };

            // Determine delimiter: `:` for scalars, `{` for messages or map entries.
            let value =
                if matches!(field.kind(), Kind::Message(_) | Kind::Group(_)) || field.is_map() {
                    // May have optional `:` or `<` before `{`.
                    self.skip_ws();
                    if self.peek() == Some(b':') {
                        self.pos += 1; // consume ':'
                        self.skip_ws();
                    }
                    if self.peek() == Some(b'<') {
                        // Angle-bracket message syntax (alternative to braces).
                        self.parse_angle_message(field.clone())?
                    } else {
                        self.parse_brace_message(field.clone())?
                    }
                } else {
                    self.expect(':')?;
                    self.parse_scalar_value(&field)?
                };

            // For repeated fields, append to the list; for singular, set directly.
            if matches!(field.cardinality(), Cardinality::Repeated) && !field.is_map() {
                // Append to the existing list.
                let existing = msg
                    .fields
                    .entry(field.number())
                    .or_insert(Value::List(Vec::new()));
                if let Value::List(list) = existing {
                    list.push(value);
                }
            } else if field.is_map() {
                // Map entry comes in as a Value::Map directly from parse_brace_message.
                // Merge into the existing map.
                match value {
                    Value::Map(new_entries) => {
                        let existing = msg
                            .fields
                            .entry(field.number())
                            .or_insert(Value::Map(std::collections::HashMap::new()));
                        if let Value::Map(map) = existing {
                            map.extend(new_entries);
                        }
                    }
                    other => {
                        return Err(TextError::Schema(format!(
                            "expected map for map field '{}', got {:?}",
                            field.name(),
                            other
                        )));
                    }
                }
            } else {
                msg.set_field(&field, value);
            }

            // Optional trailing semicolons or commas.
            self.skip_ws();
            if matches!(self.peek(), Some(b';') | Some(b',')) {
                self.pos += 1;
            }
        }
        Ok(msg)
    }

    /// Parse a brace-delimited sub-message: `{ ... }`.
    fn parse_brace_message(&mut self, field: FieldDescriptor) -> Result<Value, TextError> {
        if field.is_map() {
            return self.parse_map_entry(field);
        }
        if let Kind::Message(msg_index) = field.kind() {
            self.expect('{')?;
            let msg_desc = MessageDescriptor {
                pool: Arc::clone(&field.pool),
                index: msg_index,
            };
            let nested = self.parse_message(msg_desc)?;
            self.expect('}')?;
            Ok(Value::Message(Box::new(nested)))
        } else {
            Err(TextError::Schema(format!(
                "field '{}' is not a message kind",
                field.name()
            )))
        }
    }

    /// Parse an angle-bracket sub-message: `< ... >`.
    fn parse_angle_message(&mut self, field: FieldDescriptor) -> Result<Value, TextError> {
        if let Kind::Message(msg_index) = field.kind() {
            self.expect('<')?;
            let msg_desc = MessageDescriptor {
                pool: Arc::clone(&field.pool),
                index: msg_index,
            };
            let nested = self.parse_message(msg_desc)?;
            self.expect('>')?;
            Ok(Value::Message(Box::new(nested)))
        } else {
            Err(TextError::Schema(format!(
                "field '{}' is not a message kind",
                field.name()
            )))
        }
    }

    /// Parse a map entry `{ key: K  value: V }` and return a single-entry
    /// `Value::Map`.
    fn parse_map_entry(&mut self, field: FieldDescriptor) -> Result<Value, TextError> {
        let key_field = field.map_entry_key_field().ok_or_else(|| {
            TextError::Schema(format!(
                "map field '{}' missing key descriptor",
                field.name()
            ))
        })?;
        let val_field = field.map_entry_value_field().ok_or_else(|| {
            TextError::Schema(format!(
                "map field '{}' missing value descriptor",
                field.name()
            ))
        })?;

        self.expect('{')?;

        let mut key_val: Option<Value> = None;
        let mut entry_val: Option<Value> = None;

        loop {
            self.skip_ws();
            if self.peek() == Some(b'}') {
                break;
            }
            let fname = self.read_token()?;
            match fname.as_str() {
                "key" => {
                    self.expect(':')?;
                    key_val = Some(self.parse_scalar_value(&key_field)?);
                }
                "value" => {
                    let v = if matches!(val_field.kind(), Kind::Message(_)) {
                        self.skip_ws();
                        if self.peek() == Some(b':') {
                            self.pos += 1;
                            self.skip_ws();
                        }
                        self.parse_brace_message(val_field.clone())?
                    } else {
                        self.expect(':')?;
                        self.parse_scalar_value(&val_field)?
                    };
                    entry_val = Some(v);
                }
                _ => {
                    self.skip_field_value()?;
                }
            }
            self.skip_ws();
            if matches!(self.peek(), Some(b';') | Some(b',')) {
                self.pos += 1;
            }
        }

        self.expect('}')?;

        let key = key_val.ok_or_else(|| {
            TextError::Parse(format!("map entry for '{}' missing 'key'", field.name()))
        })?;
        let val = entry_val.ok_or_else(|| {
            TextError::Parse(format!("map entry for '{}' missing 'value'", field.name()))
        })?;

        let map_key = value_to_map_key(key)?;
        let mut map = std::collections::HashMap::new();
        map.insert(map_key, val);
        Ok(Value::Map(map))
    }

    /// Parse a scalar value for the given field.
    fn parse_scalar_value(&mut self, field: &FieldDescriptor) -> Result<Value, TextError> {
        self.skip_ws();
        let next = self
            .peek()
            .ok_or_else(|| TextError::Parse("unexpected EOF".to_owned()))?;

        if next == b'"' {
            // Quoted string — use raw bytes for bytes fields to preserve the
            // exact byte values of \xNN escapes (not re-encoded as UTF-8).
            return match field.kind() {
                Kind::Bytes => {
                    let raw = self.read_string_as_bytes()?;
                    Ok(Value::Bytes(raw))
                }
                _ => {
                    let s = self.read_string()?;
                    parse_string_for_kind(s, field)
                }
            };
        }

        // Read bare token.
        let token = self.read_token()?;
        parse_token_for_field(&token, field)
    }

    /// Skip an unknown field value (either a quoted string, a bare token, or
    /// a brace-delimited block).
    fn skip_field_value(&mut self) -> Result<(), TextError> {
        self.skip_ws();
        let next = self.peek();
        match next {
            Some(b':') => {
                self.pos += 1;
                self.skip_ws();
                if self.peek() == Some(b'"') {
                    let _ = self.read_string()?;
                } else if self.peek() == Some(b'{') {
                    self.skip_block()?;
                } else {
                    let _ = self.read_token()?;
                }
            }
            Some(b'{') => {
                self.skip_block()?;
            }
            _ => {
                let _ = self.read_token()?;
            }
        }
        Ok(())
    }

    /// Skip a `{ ... }` block, handling nested braces.
    fn skip_block(&mut self) -> Result<(), TextError> {
        self.expect('{')?;
        let mut depth = 1usize;
        while self.pos < self.input.len() && depth > 0 {
            match self.input.as_bytes()[self.pos] {
                b'{' => {
                    depth += 1;
                    self.pos += 1;
                }
                b'}' => {
                    depth -= 1;
                    self.pos += 1;
                }
                b'"' => {
                    // Skip string literals to avoid confusing braces inside them.
                    self.pos += 1;
                    while self.pos < self.input.len() {
                        match self.input.as_bytes()[self.pos] {
                            b'"' => {
                                self.pos += 1;
                                break;
                            }
                            b'\\' => {
                                self.pos += 2; // skip escaped char
                            }
                            _ => {
                                self.pos += 1;
                            }
                        }
                    }
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
        if depth != 0 {
            return Err(TextError::Parse("unmatched '{' in text format".to_owned()));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Token-to-value helpers
// ---------------------------------------------------------------------------

fn parse_string_for_kind(s: String, field: &FieldDescriptor) -> Result<Value, TextError> {
    match field.kind() {
        Kind::String => Ok(Value::String(s)),
        Kind::Bytes => {
            // Raw bytes in the string (escape sequences already decoded by read_string).
            Ok(Value::Bytes(s.into_bytes()))
        }
        other => Err(TextError::Schema(format!(
            "field '{}' has kind {:?} but got a quoted string",
            field.name(),
            other
        ))),
    }
}

fn parse_token_for_field(token: &str, field: &FieldDescriptor) -> Result<Value, TextError> {
    match field.kind() {
        Kind::Bool => parse_bool(token),
        Kind::Double => parse_f64(token).map(Value::F64),
        Kind::Float => parse_f32(token).map(Value::F32),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => token
            .parse::<i32>()
            .map(Value::I32)
            .map_err(|_| TextError::Parse(format!("invalid i32: {token}"))),
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => token
            .parse::<i64>()
            .map(Value::I64)
            .map_err(|_| TextError::Parse(format!("invalid i64: {token}"))),
        Kind::Uint32 | Kind::Fixed32 => token
            .parse::<u32>()
            .map(Value::U32)
            .map_err(|_| TextError::Parse(format!("invalid u32: {token}"))),
        Kind::Uint64 | Kind::Fixed64 => token
            .parse::<u64>()
            .map(Value::U64)
            .map_err(|_| TextError::Parse(format!("invalid u64: {token}"))),
        Kind::String => Ok(Value::String(token.to_owned())),
        Kind::Bytes => Ok(Value::Bytes(token.as_bytes().to_vec())),
        Kind::Enum(_) => {
            // Try by-name first; fall back to integer.
            if let Some(enum_desc) = field.enum_type() {
                if let Some(val_desc) = enum_desc.get_value_by_name(token) {
                    return Ok(Value::EnumNumber(val_desc.number()));
                }
            }
            // Try as an integer.
            token.parse::<i32>().map(Value::EnumNumber).map_err(|_| {
                TextError::Parse(format!(
                    "unknown enum value for '{}': {token}",
                    field.name()
                ))
            })
        }
        Kind::Message(_) | Kind::Group(_) => Err(TextError::Parse(format!(
            "field '{}' is a message kind; expected '{{' delimiter, got bare token '{token}'",
            field.name()
        ))),
    }
}

fn parse_bool(token: &str) -> Result<Value, TextError> {
    match token {
        "true" | "True" | "1" => Ok(Value::Bool(true)),
        "false" | "False" | "0" => Ok(Value::Bool(false)),
        other => Err(TextError::Parse(format!("invalid bool: {other}"))),
    }
}

fn parse_f64(token: &str) -> Result<f64, TextError> {
    match token {
        "nan" | "NaN" => Ok(f64::NAN),
        "inf" | "Inf" | "infinity" | "Infinity" => Ok(f64::INFINITY),
        "-inf" | "-Inf" | "-infinity" | "-Infinity" => Ok(f64::NEG_INFINITY),
        other => other
            .parse::<f64>()
            .map_err(|_| TextError::Parse(format!("invalid f64: {other}"))),
    }
}

fn parse_f32(token: &str) -> Result<f32, TextError> {
    match token {
        "nan" | "NaN" => Ok(f32::NAN),
        "inf" | "Inf" | "infinity" | "Infinity" => Ok(f32::INFINITY),
        "-inf" | "-Inf" | "-infinity" | "-Infinity" => Ok(f32::NEG_INFINITY),
        other => other
            .parse::<f32>()
            .map_err(|_| TextError::Parse(format!("invalid f32: {other}"))),
    }
}

fn value_to_map_key(v: Value) -> Result<MapKey, TextError> {
    match v {
        Value::String(s) => Ok(MapKey::String(s)),
        Value::I32(n) => Ok(MapKey::I32(n)),
        Value::I64(n) => Ok(MapKey::I64(n)),
        Value::U32(n) => Ok(MapKey::U32(n)),
        Value::U64(n) => Ok(MapKey::U64(n)),
        Value::Bool(b) => Ok(MapKey::Bool(b)),
        other => Err(TextError::Schema(format!(
            "invalid map key type: {other:?}"
        ))),
    }
}
