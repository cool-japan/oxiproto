#![forbid(unsafe_code)]

//! Native proto2/proto3 AST types.
//!
//! Every node that spans source code carries a [`Span`] field.  All types
//! derive `Debug`, `Clone`, and `PartialEq`.

use crate::parser::span::Span;

// ---------------------------------------------------------------------------
// File-level container
// ---------------------------------------------------------------------------

/// The top-level container for a parsed `.proto` file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProtoFile {
    /// Value of the `syntax` statement, e.g. `"proto3"`.
    pub syntax: Option<String>,
    /// Value of the `package` statement, e.g. `"google.protobuf"`.
    pub package: Option<String>,
    /// All `import` statements, in declaration order.
    pub imports: Vec<Import>,
    /// Top-level `option` statements.
    pub options: Vec<ProtoOption>,
    /// Top-level `message` definitions.
    pub messages: Vec<Message>,
    /// Top-level `enum` definitions.
    pub enums: Vec<Enum>,
    /// Top-level `service` definitions.
    pub services: Vec<Service>,
    /// Top-level `extend` blocks (proto2 only).
    pub extends: Vec<ExtendBlock>,
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// A single `import` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    /// The import path string, e.g. `"google/protobuf/timestamp.proto"`.
    pub path: String,
    /// Optional modifier (`public`, `weak`, or none).
    pub modifier: ImportModifier,
    /// Source span of the entire import statement.
    pub span: Span,
}

/// The optional modifier on an `import` statement.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportModifier {
    /// Plain `import "..."`.
    None,
    /// `import public "..."`.
    Public,
    /// `import weak "..."`.
    Weak,
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

/// A `message` definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// The message name.
    pub name: String,
    /// Regular fields (including map fields).
    pub fields: Vec<Field>,
    /// `oneof` blocks.
    pub oneofs: Vec<Oneof>,
    /// Nested `message` definitions.
    pub nested_messages: Vec<Message>,
    /// Nested `enum` definitions.
    pub nested_enums: Vec<Enum>,
    /// `reserved` statements.
    pub reserved: Vec<Reserved>,
    /// `option` statements inside the message body.
    pub options: Vec<ProtoOption>,
    /// `extensions` statements (proto2 only).
    pub extensions: Vec<ExtensionRange>,
    /// Source span of the entire message definition.
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Field
// ---------------------------------------------------------------------------

/// A message field or oneof member field.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Field label (singular, optional, or repeated).
    pub label: FieldLabel,
    /// Field type.
    pub ty: FieldType,
    /// Field name.
    pub name: String,
    /// Field number.
    pub number: i32,
    /// Inline field options (inside `[...]`).
    pub options: Vec<ProtoOption>,
    /// Source span of the entire field declaration.
    pub span: Span,
}

/// The label on a field.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldLabel {
    /// No explicit label (proto3 default: singular).
    Singular,
    /// Explicit `optional` keyword.
    Optional,
    /// `repeated` keyword.
    Repeated,
    /// `required` keyword (proto2 only).
    Required,
}

// ---------------------------------------------------------------------------
// Extension range (proto2)
// ---------------------------------------------------------------------------

/// An `extensions` range inside a proto2 message.
///
/// `extensions 100 to 199;` → `ExtensionRange { start: 100, end: Some(199) }`
/// `extensions 200;` → `ExtensionRange { start: 200, end: None }`
/// `extensions 1000 to max;` → `ExtensionRange { start: 1000, end: None }` (open-ended)
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionRange {
    /// Inclusive start of the range.
    pub start: u32,
    /// Inclusive end of the range, or `None` for open-ended / bare number.
    pub end: Option<u32>,
}

// ---------------------------------------------------------------------------
// Extend block (proto2)
// ---------------------------------------------------------------------------

/// A top-level `extend` block (proto2 only).
///
/// ```proto2
/// extend Foo {
///   optional int32 bar = 100;
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ExtendBlock {
    /// The name of the message being extended.  May be a package-qualified
    /// name such as `"Foo"` or `"foo.Bar"`.
    pub extendee: String,
    /// The extension fields defined in this block.
    pub fields: Vec<Field>,
}

/// The type of a field.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    /// A scalar (primitive) type.
    Scalar(ScalarType),
    /// A `map<K, V>` type.
    Map {
        /// Map key type (must be a scalar, not `bytes` or `float`/`double`
        /// in valid proto3, but we store whatever was parsed).
        key: ScalarType,
        /// Map value type.
        value: Box<FieldType>,
    },
    /// A named message or enum reference, e.g. `"Foo"`, `"foo.Bar"`,
    /// `".google.protobuf.Timestamp"`.
    Named(String),
    /// A proto2 `group` field type.  The string is the group/message name
    /// (capitalized, e.g. `"Result"`).  The actual wire field name is the
    /// lowercased version (e.g. `"result"`).
    ///
    /// A group synthesises a nested message in the enclosing message's
    /// `nested_messages` and emits a field with `type = TYPE_GROUP (10)`.
    Group(String),
}

/// The 15 proto3 scalar (primitive) types.
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum ScalarType {
    /// `double` — 64-bit IEEE 754 floating point.
    Double,
    /// `float` — 32-bit IEEE 754 floating point.
    Float,
    /// `int32` — variable-length 32-bit signed integer.
    Int32,
    /// `int64` — variable-length 64-bit signed integer.
    Int64,
    /// `uint32` — variable-length 32-bit unsigned integer.
    Uint32,
    /// `uint64` — variable-length 64-bit unsigned integer.
    Uint64,
    /// `sint32` — ZigZag-encoded 32-bit signed integer.
    Sint32,
    /// `sint64` — ZigZag-encoded 64-bit signed integer.
    Sint64,
    /// `fixed32` — fixed 4-byte 32-bit unsigned integer.
    Fixed32,
    /// `fixed64` — fixed 8-byte 64-bit unsigned integer.
    Fixed64,
    /// `sfixed32` — fixed 4-byte 32-bit signed integer.
    Sfixed32,
    /// `sfixed64` — fixed 8-byte 64-bit signed integer.
    Sfixed64,
    /// `bool` — boolean.
    Bool,
    /// `string` — UTF-8 string.
    String,
    /// `bytes` — arbitrary byte sequence.
    Bytes,
}

// ---------------------------------------------------------------------------
// Oneof
// ---------------------------------------------------------------------------

/// A `oneof` block inside a message.
#[derive(Debug, Clone, PartialEq)]
pub struct Oneof {
    /// The oneof name.
    pub name: String,
    /// Members of this oneof.  Member fields have no label
    /// (they are implicitly `Optional`).
    pub fields: Vec<Field>,
    /// Options declared inside the oneof block.
    pub options: Vec<ProtoOption>,
    /// Source span of the entire oneof block.
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Enum
// ---------------------------------------------------------------------------

/// An `enum` definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    /// The enum name.
    pub name: String,
    /// Enum value declarations.
    pub values: Vec<EnumValue>,
    /// `reserved` statements inside the enum.
    pub reserved: Vec<Reserved>,
    /// `option` statements inside the enum.
    pub options: Vec<ProtoOption>,
    /// Source span of the entire enum definition.
    pub span: Span,
}

/// A single value inside an `enum` definition.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumValue {
    /// The value name, e.g. `UNKNOWN`.
    pub name: String,
    /// The numeric value, e.g. `0`.
    pub number: i32,
    /// Inline options (inside `[...]`).
    pub options: Vec<ProtoOption>,
    /// Source span of the entire enum value declaration.
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// A `service` definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    /// The service name.
    pub name: String,
    /// RPC method declarations.
    pub methods: Vec<Method>,
    /// `option` statements inside the service.
    pub options: Vec<ProtoOption>,
    /// Source span of the entire service definition.
    pub span: Span,
}

/// A single `rpc` method inside a service.
#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    /// The method name.
    pub name: String,
    /// Input type name (without the `stream` keyword; streaming is in
    /// `client_streaming`).
    pub input_type: String,
    /// Output type name.
    pub output_type: String,
    /// `true` if the request is a client-streaming RPC.
    pub client_streaming: bool,
    /// `true` if the response is a server-streaming RPC.
    pub server_streaming: bool,
    /// `option` statements inside the rpc body (when a `{...}` block is used).
    pub options: Vec<ProtoOption>,
    /// Source span of the entire RPC declaration.
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// A parsed `option` statement or inline field option.
#[derive(Debug, Clone, PartialEq)]
pub struct ProtoOption {
    /// The option name, e.g. `"deprecated"`, `"(foo.bar).baz"`.
    pub name: String,
    /// The option value.
    pub value: OptionValue,
    /// Source span of the option.
    pub span: Span,
}

/// The value of an option.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    /// An identifier value (enum member names, etc.), e.g. `LABEL`.
    Ident(String),
    /// A string literal value.
    Str(String),
    /// An integer value.
    Int(i64),
    /// A floating-point value.
    Float(f64),
    /// A boolean value (`true` or `false`).
    Bool(bool),
    /// A structured proto message literal value, e.g. `{ id: 1, name: "foo" }`.
    /// Each entry is `(field_name, value)`.
    MessageLiteral(Vec<(String, OptionValue)>),
}

// ---------------------------------------------------------------------------
// Reserved
// ---------------------------------------------------------------------------

/// A `reserved` statement inside a message or enum.
#[derive(Debug, Clone, PartialEq)]
pub enum Reserved {
    /// Reserved field/enum-value number ranges.
    Ranges(Vec<ReservedRange>),
    /// Reserved field/enum-value names.
    Names(Vec<String>),
}

/// A single range inside a `reserved` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct ReservedRange {
    /// The start of the range (inclusive).
    pub from: i32,
    /// The end of the range.
    pub to: ReservedRangeTo,
}

/// The upper bound of a reserved range.
#[derive(Debug, Clone, PartialEq)]
pub enum ReservedRangeTo {
    /// An explicit numeric upper bound (inclusive).
    Number(i32),
    /// The `max` keyword — the range extends to the maximum field number.
    Max,
}
