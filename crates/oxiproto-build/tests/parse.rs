//! Integration tests for the full proto3 body parser (`parse_file`).

use oxiproto_build::parser::{
    parse_file, Edition, Field, FieldLabel, FieldType, ImportModifier, OptionValue, ParseError,
    Reserved, ReservedRange, ReservedRangeTo, ScalarType,
};

// ---------------------------------------------------------------------------
// Helper macros / functions
// ---------------------------------------------------------------------------

/// Assert that a parsed field matches the given components.
fn assert_field(f: &Field, label: FieldLabel, ty: &FieldType, name: &str, number: i32) {
    assert_eq!(f.label, label, "field '{}' label mismatch", name);
    assert_eq!(&f.ty, ty, "field '{}' type mismatch", name);
    assert_eq!(f.name, name, "field name mismatch");
    assert_eq!(f.number, number, "field '{}' number mismatch", name);
}

// ---------------------------------------------------------------------------
// Syntax / package / import
// ---------------------------------------------------------------------------

#[test]
fn test_syntax_only() {
    let src = r#"syntax = "proto3";"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.syntax, Some("proto3".to_owned()));
    assert!(f.messages.is_empty());
}

#[test]
fn test_package_and_syntax() {
    let src = r#"
syntax = "proto3";
package com.example.myapp;
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.syntax, Some("proto3".to_owned()));
    assert_eq!(f.package, Some("com.example.myapp".to_owned()));
}

#[test]
fn test_import_plain() {
    let src = r#"
syntax = "proto3";
import "google/protobuf/timestamp.proto";
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.imports.len(), 1);
    assert_eq!(f.imports[0].path, "google/protobuf/timestamp.proto");
    assert_eq!(f.imports[0].modifier, ImportModifier::None);
}

#[test]
fn test_import_public() {
    let src = r#"
syntax = "proto3";
import public "other.proto";
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.imports[0].modifier, ImportModifier::Public);
}

#[test]
fn test_import_weak() {
    let src = r#"
syntax = "proto3";
import weak "optional.proto";
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.imports[0].modifier, ImportModifier::Weak);
}

// ---------------------------------------------------------------------------
// All 15 scalar types — singular label
// ---------------------------------------------------------------------------

#[test]
fn test_all_scalar_types_singular() {
    let src = r#"
syntax = "proto3";
message ScalarMsg {
    double  f_double   = 1;
    float   f_float    = 2;
    int32   f_int32    = 3;
    int64   f_int64    = 4;
    uint32  f_uint32   = 5;
    uint64  f_uint64   = 6;
    sint32  f_sint32   = 7;
    sint64  f_sint64   = 8;
    fixed32 f_fixed32  = 9;
    fixed64 f_fixed64  = 10;
    sfixed32 f_sfixed32 = 11;
    sfixed64 f_sfixed64 = 12;
    bool    f_bool     = 13;
    string  f_string   = 14;
    bytes   f_bytes    = 15;
}
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.messages.len(), 1);
    let msg = &f.messages[0];
    assert_eq!(msg.name, "ScalarMsg");
    assert_eq!(msg.fields.len(), 15);

    let expected: &[(FieldType, &str, i32)] = &[
        (FieldType::Scalar(ScalarType::Double), "f_double", 1),
        (FieldType::Scalar(ScalarType::Float), "f_float", 2),
        (FieldType::Scalar(ScalarType::Int32), "f_int32", 3),
        (FieldType::Scalar(ScalarType::Int64), "f_int64", 4),
        (FieldType::Scalar(ScalarType::Uint32), "f_uint32", 5),
        (FieldType::Scalar(ScalarType::Uint64), "f_uint64", 6),
        (FieldType::Scalar(ScalarType::Sint32), "f_sint32", 7),
        (FieldType::Scalar(ScalarType::Sint64), "f_sint64", 8),
        (FieldType::Scalar(ScalarType::Fixed32), "f_fixed32", 9),
        (FieldType::Scalar(ScalarType::Fixed64), "f_fixed64", 10),
        (FieldType::Scalar(ScalarType::Sfixed32), "f_sfixed32", 11),
        (FieldType::Scalar(ScalarType::Sfixed64), "f_sfixed64", 12),
        (FieldType::Scalar(ScalarType::Bool), "f_bool", 13),
        (FieldType::Scalar(ScalarType::String), "f_string", 14),
        (FieldType::Scalar(ScalarType::Bytes), "f_bytes", 15),
    ];

    for (i, (ty, name, num)) in expected.iter().enumerate() {
        assert_field(&msg.fields[i], FieldLabel::Singular, ty, name, *num);
    }
}

// ---------------------------------------------------------------------------
// Repeated fields
// ---------------------------------------------------------------------------

#[test]
fn test_repeated_fields() {
    let src = r#"
syntax = "proto3";
message Rep {
    repeated string  tags   = 1;
    repeated int32   scores = 2;
}
"#;
    let f = parse_file(src).expect("must parse");
    let msg = &f.messages[0];
    assert_eq!(msg.fields.len(), 2);
    assert_field(
        &msg.fields[0],
        FieldLabel::Repeated,
        &FieldType::Scalar(ScalarType::String),
        "tags",
        1,
    );
    assert_field(
        &msg.fields[1],
        FieldLabel::Repeated,
        &FieldType::Scalar(ScalarType::Int32),
        "scores",
        2,
    );
}

// ---------------------------------------------------------------------------
// Optional fields (proto3 explicit optional)
// ---------------------------------------------------------------------------

#[test]
fn test_optional_field() {
    let src = r#"
syntax = "proto3";
message Opt {
    optional int32 x = 1;
    optional string name = 2;
}
"#;
    let f = parse_file(src).expect("must parse");
    let msg = &f.messages[0];
    assert_eq!(msg.fields.len(), 2);
    assert_field(
        &msg.fields[0],
        FieldLabel::Optional,
        &FieldType::Scalar(ScalarType::Int32),
        "x",
        1,
    );
    assert_field(
        &msg.fields[1],
        FieldLabel::Optional,
        &FieldType::Scalar(ScalarType::String),
        "name",
        2,
    );
}

// ---------------------------------------------------------------------------
// Map fields
// ---------------------------------------------------------------------------

#[test]
fn test_map_string_int32() {
    let src = r#"
syntax = "proto3";
message MapMsg {
    map<string, int32> counts = 1;
}
"#;
    let f = parse_file(src).expect("must parse");
    let field = &f.messages[0].fields[0];
    assert_eq!(field.name, "counts");
    assert_eq!(field.number, 1);
    assert_eq!(
        field.ty,
        FieldType::Map {
            key: ScalarType::String,
            value: Box::new(FieldType::Scalar(ScalarType::Int32)),
        }
    );
    assert_eq!(field.label, FieldLabel::Singular);
}

#[test]
fn test_map_int64_bytes() {
    let src = r#"
syntax = "proto3";
message MapMsg2 {
    map<int64, bytes> blobs = 3;
}
"#;
    let f = parse_file(src).expect("must parse");
    let field = &f.messages[0].fields[0];
    assert_eq!(field.name, "blobs");
    assert_eq!(
        field.ty,
        FieldType::Map {
            key: ScalarType::Int64,
            value: Box::new(FieldType::Scalar(ScalarType::Bytes)),
        }
    );
}

// ---------------------------------------------------------------------------
// Nested messages (3 levels)
// ---------------------------------------------------------------------------

#[test]
fn test_nested_messages() {
    let src = r#"
syntax = "proto3";
message Outer {
    int32 outer_val = 1;
    message Inner {
        string inner_val = 1;
        message Leaf {
            bool leaf_flag = 1;
        }
    }
}
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.messages.len(), 1);
    let outer = &f.messages[0];
    assert_eq!(outer.name, "Outer");
    assert_eq!(outer.fields.len(), 1);
    assert_eq!(outer.nested_messages.len(), 1);

    let inner = &outer.nested_messages[0];
    assert_eq!(inner.name, "Inner");
    assert_eq!(inner.fields.len(), 1);
    assert_eq!(inner.nested_messages.len(), 1);

    let leaf = &inner.nested_messages[0];
    assert_eq!(leaf.name, "Leaf");
    assert_eq!(leaf.fields.len(), 1);
    assert_field(
        &leaf.fields[0],
        FieldLabel::Singular,
        &FieldType::Scalar(ScalarType::Bool),
        "leaf_flag",
        1,
    );
}

// ---------------------------------------------------------------------------
// Nested enum inside message
// ---------------------------------------------------------------------------

#[test]
fn test_nested_enum() {
    let src = r#"
syntax = "proto3";
message Container {
    enum Status {
        UNKNOWN = 0;
        ACTIVE  = 1;
        DELETED = 2;
    }
    Status status = 1;
}
"#;
    let f = parse_file(src).expect("must parse");
    let msg = &f.messages[0];
    assert_eq!(msg.nested_enums.len(), 1);
    let e = &msg.nested_enums[0];
    assert_eq!(e.name, "Status");
    assert_eq!(e.values.len(), 3);
    assert_eq!(e.values[0].name, "UNKNOWN");
    assert_eq!(e.values[0].number, 0);
    assert_eq!(e.values[1].name, "ACTIVE");
    assert_eq!(e.values[1].number, 1);
    assert_eq!(e.values[2].name, "DELETED");
    assert_eq!(e.values[2].number, 2);
}

// ---------------------------------------------------------------------------
// Oneof with multiple scalar members
// ---------------------------------------------------------------------------

#[test]
fn test_oneof() {
    let src = r#"
syntax = "proto3";
message Payload {
    oneof content {
        string text   = 1;
        bytes  data   = 2;
        int32  number = 3;
        bool   flag   = 4;
    }
}
"#;
    let f = parse_file(src).expect("must parse");
    let msg = &f.messages[0];
    assert_eq!(msg.oneofs.len(), 1);
    let oo = &msg.oneofs[0];
    assert_eq!(oo.name, "content");
    assert_eq!(oo.fields.len(), 4);
    assert_field(
        &oo.fields[0],
        FieldLabel::Singular,
        &FieldType::Scalar(ScalarType::String),
        "text",
        1,
    );
    assert_field(
        &oo.fields[1],
        FieldLabel::Singular,
        &FieldType::Scalar(ScalarType::Bytes),
        "data",
        2,
    );
    assert_field(
        &oo.fields[2],
        FieldLabel::Singular,
        &FieldType::Scalar(ScalarType::Int32),
        "number",
        3,
    );
    assert_field(
        &oo.fields[3],
        FieldLabel::Singular,
        &FieldType::Scalar(ScalarType::Bool),
        "flag",
        4,
    );
}

// ---------------------------------------------------------------------------
// Enum with values and a reserved range
// ---------------------------------------------------------------------------

#[test]
fn test_enum_with_reserved() {
    let src = r#"
syntax = "proto3";
enum Color {
    COLOR_UNSPECIFIED = 0;
    RED   = 1;
    GREEN = 2;
    BLUE  = 3;
    reserved 4 to 9;
    reserved "PURPLE", "TEAL";
}
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.enums.len(), 1);
    let e = &f.enums[0];
    assert_eq!(e.name, "Color");
    assert_eq!(e.values.len(), 4);
    assert_eq!(e.reserved.len(), 2);

    match &e.reserved[0] {
        Reserved::Ranges(ranges) => {
            assert_eq!(ranges.len(), 1);
            assert_eq!(ranges[0].from, 4);
            assert_eq!(ranges[0].to, ReservedRangeTo::Number(9));
        }
        Reserved::Names(_) => panic!("expected ranges"),
    }
    match &e.reserved[1] {
        Reserved::Names(names) => {
            assert_eq!(names, &["PURPLE".to_owned(), "TEAL".to_owned()]);
        }
        Reserved::Ranges(_) => panic!("expected names"),
    }
}

// ---------------------------------------------------------------------------
// Service with all streaming variants
// ---------------------------------------------------------------------------

#[test]
fn test_service_all_streaming_variants() {
    let src = r#"
syntax = "proto3";
message Req {}
message Res {}
service Echo {
    rpc Unary(Req) returns (Res);
    rpc ClientStream(stream Req) returns (Res);
    rpc ServerStream(Req) returns (stream Res);
    rpc Bidi(stream Req) returns (stream Res);
}
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.services.len(), 1);
    let svc = &f.services[0];
    assert_eq!(svc.name, "Echo");
    assert_eq!(svc.methods.len(), 4);

    let unary = &svc.methods[0];
    assert_eq!(unary.name, "Unary");
    assert!(!unary.client_streaming);
    assert!(!unary.server_streaming);
    assert_eq!(unary.input_type, "Req");
    assert_eq!(unary.output_type, "Res");

    let cs = &svc.methods[1];
    assert_eq!(cs.name, "ClientStream");
    assert!(cs.client_streaming);
    assert!(!cs.server_streaming);

    let ss = &svc.methods[2];
    assert_eq!(ss.name, "ServerStream");
    assert!(!ss.client_streaming);
    assert!(ss.server_streaming);

    let bidi = &svc.methods[3];
    assert_eq!(bidi.name, "Bidi");
    assert!(bidi.client_streaming);
    assert!(bidi.server_streaming);
}

// ---------------------------------------------------------------------------
// Field options
// ---------------------------------------------------------------------------

#[test]
fn test_field_option_deprecated() {
    let src = r#"
syntax = "proto3";
message Opts {
    int32 old_field = 1 [deprecated = true];
}
"#;
    let f = parse_file(src).expect("must parse");
    let field = &f.messages[0].fields[0];
    assert_eq!(field.options.len(), 1);
    assert_eq!(field.options[0].name, "deprecated");
    assert_eq!(field.options[0].value, OptionValue::Bool(true));
}

#[test]
fn test_field_option_packed_false() {
    let src = r#"
syntax = "proto3";
message Opts2 {
    repeated int32 vals = 1 [packed = false];
}
"#;
    let f = parse_file(src).expect("must parse");
    let field = &f.messages[0].fields[0];
    assert_eq!(field.options[0].name, "packed");
    assert_eq!(field.options[0].value, OptionValue::Bool(false));
}

// ---------------------------------------------------------------------------
// Reserved — ranges and names
// ---------------------------------------------------------------------------

#[test]
fn test_reserved_individual_numbers() {
    let src = r#"
syntax = "proto3";
message Reserved1 {
    reserved 1, 2, 3;
}
"#;
    let f = parse_file(src).expect("must parse");
    let msg = &f.messages[0];
    assert_eq!(msg.reserved.len(), 1);
    match &msg.reserved[0] {
        Reserved::Ranges(ranges) => {
            assert_eq!(ranges.len(), 3);
            assert_eq!(
                ranges[0],
                ReservedRange {
                    from: 1,
                    to: ReservedRangeTo::Number(1)
                }
            );
            assert_eq!(
                ranges[1],
                ReservedRange {
                    from: 2,
                    to: ReservedRangeTo::Number(2)
                }
            );
            assert_eq!(
                ranges[2],
                ReservedRange {
                    from: 3,
                    to: ReservedRangeTo::Number(3)
                }
            );
        }
        Reserved::Names(_) => panic!("expected ranges"),
    }
}

#[test]
fn test_reserved_range_to_number() {
    let src = r#"
syntax = "proto3";
message Reserved2 {
    reserved 1 to 10;
}
"#;
    let f = parse_file(src).expect("must parse");
    match &f.messages[0].reserved[0] {
        Reserved::Ranges(ranges) => {
            assert_eq!(ranges.len(), 1);
            assert_eq!(ranges[0].from, 1);
            assert_eq!(ranges[0].to, ReservedRangeTo::Number(10));
        }
        Reserved::Names(_) => panic!("expected ranges"),
    }
}

#[test]
fn test_reserved_range_to_max() {
    let src = r#"
syntax = "proto3";
message Reserved3 {
    reserved 15 to max;
}
"#;
    let f = parse_file(src).expect("must parse");
    match &f.messages[0].reserved[0] {
        Reserved::Ranges(ranges) => {
            assert_eq!(ranges[0].from, 15);
            assert_eq!(ranges[0].to, ReservedRangeTo::Max);
        }
        Reserved::Names(_) => panic!("expected ranges"),
    }
}

#[test]
fn test_reserved_names() {
    let src = r#"
syntax = "proto3";
message Reserved4 {
    reserved "foo", "bar";
}
"#;
    let f = parse_file(src).expect("must parse");
    match &f.messages[0].reserved[0] {
        Reserved::Names(names) => {
            assert_eq!(names, &["foo".to_owned(), "bar".to_owned()]);
        }
        Reserved::Ranges(_) => panic!("expected names"),
    }
}

// ---------------------------------------------------------------------------
// Comments are ignored — AST structure identical with and without
// ---------------------------------------------------------------------------

#[test]
fn test_comments_ignored() {
    let src_no_comments = r#"
syntax = "proto3";
message Simple {
    int32 value = 1;
}
"#;
    let src_with_comments = r#"
// File comment
syntax = "proto3"; // inline
/* block */ message Simple {
    // field comment
    int32 value = 1; /* end */
}
"#;

    let f1 = parse_file(src_no_comments).expect("must parse without comments");
    let f2 = parse_file(src_with_comments).expect("must parse with comments");

    // Compare structural fields, not spans
    assert_eq!(f1.syntax, f2.syntax);
    assert_eq!(f1.messages.len(), f2.messages.len());
    let m1 = &f1.messages[0];
    let m2 = &f2.messages[0];
    assert_eq!(m1.name, m2.name);
    assert_eq!(m1.fields.len(), m2.fields.len());
    assert_eq!(m1.fields[0].name, m2.fields[0].name);
    assert_eq!(m1.fields[0].number, m2.fields[0].number);
    assert_eq!(m1.fields[0].ty, m2.fields[0].ty);
    assert_eq!(m1.fields[0].label, m2.fields[0].label);
}

// ---------------------------------------------------------------------------
// Type references
// ---------------------------------------------------------------------------

#[test]
fn test_named_type_dotted() {
    let src = r#"
syntax = "proto3";
message WithRef {
    Foo.Bar baz = 1;
}
"#;
    let f = parse_file(src).expect("must parse");
    let field = &f.messages[0].fields[0];
    assert_eq!(field.ty, FieldType::Named("Foo.Bar".to_owned()));
    assert_eq!(field.name, "baz");
}

#[test]
fn test_named_type_leading_dot() {
    let src = r#"
syntax = "proto3";
message WithTimestamp {
    .google.protobuf.Timestamp ts = 1;
}
"#;
    let f = parse_file(src).expect("must parse");
    let field = &f.messages[0].fields[0];
    assert_eq!(
        field.ty,
        FieldType::Named(".google.protobuf.Timestamp".to_owned())
    );
    assert_eq!(field.name, "ts");
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn test_error_missing_semicolon_after_field() {
    let src = r#"
syntax = "proto3";
message Bad {
    int32 x = 1
}
"#;
    let result = parse_file(src);
    assert!(result.is_err());
    match result.expect_err("must be error") {
        ParseError::UnexpectedToken { .. } => {}
        other => panic!("expected UnexpectedToken, got {other:?}"),
    }
}

#[test]
fn test_error_missing_equals_after_name() {
    let src = r#"
syntax = "proto3";
message Bad {
    int32 x 1;
}
"#;
    let result = parse_file(src);
    assert!(result.is_err());
    match result.expect_err("must be error") {
        ParseError::UnexpectedToken { .. } => {}
        other => panic!("expected UnexpectedToken, got {other:?}"),
    }
}

#[test]
fn test_error_unbalanced_brace() {
    let src = r#"
syntax = "proto3";
message Unbalanced {
    int32 x = 1;
"#;
    let result = parse_file(src);
    assert!(result.is_err());
    match result.expect_err("must be error") {
        ParseError::UnbalancedBraces { .. } => {}
        other => panic!("expected UnbalancedBraces, got {other:?}"),
    }
}

#[test]
fn test_error_unexpected_eof_inside_message() {
    // Truncated before the `=` sign
    let src = r#"syntax = "proto3"; message M { int32 x "#;
    let result = parse_file(src);
    assert!(result.is_err());
    match result.expect_err("must be error") {
        ParseError::UnexpectedEof => {}
        other => panic!("expected UnexpectedEof, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Service with rpc body block options
// ---------------------------------------------------------------------------

#[test]
fn test_service_rpc_with_option_block() {
    let src = r#"
syntax = "proto3";
message Req {}
message Res {}
service Greeter {
    rpc SayHello(Req) returns (Res) {
        option deprecated = true;
    }
}
"#;
    let f = parse_file(src).expect("must parse");
    let method = &f.services[0].methods[0];
    assert_eq!(method.name, "SayHello");
    assert_eq!(method.options.len(), 1);
    assert_eq!(method.options[0].name, "deprecated");
    assert_eq!(method.options[0].value, OptionValue::Bool(true));
}

// ---------------------------------------------------------------------------
// Top-level option statement
// ---------------------------------------------------------------------------

#[test]
fn test_top_level_option() {
    let src = r#"
syntax = "proto3";
option java_package = "com.example";
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.options.len(), 1);
    assert_eq!(f.options[0].name, "java_package");
    assert_eq!(
        f.options[0].value,
        OptionValue::Str("com.example".to_owned())
    );
}

// ---------------------------------------------------------------------------
// Empty message
// ---------------------------------------------------------------------------

#[test]
fn test_empty_message() {
    let src = r#"
syntax = "proto3";
message Empty {}
"#;
    let f = parse_file(src).expect("must parse");
    let msg = &f.messages[0];
    assert_eq!(msg.name, "Empty");
    assert!(msg.fields.is_empty());
    assert!(msg.oneofs.is_empty());
    assert!(msg.nested_messages.is_empty());
    assert!(msg.nested_enums.is_empty());
}

// ---------------------------------------------------------------------------
// Extended (parenthesized) option name
// ---------------------------------------------------------------------------

#[test]
fn test_extended_option_name() {
    let src = r#"
syntax = "proto3";
message Ext {
    int32 x = 1 [(validate.rules).int32.gte = 0];
}
"#;
    // We just verify it parses without error; the name will be stored as parsed
    let f = parse_file(src).expect("must parse");
    let field = &f.messages[0].fields[0];
    assert_eq!(field.options.len(), 1);
    assert!(field.options[0].name.contains("validate"));
}

// ---------------------------------------------------------------------------
// Message with all constructs combined
// ---------------------------------------------------------------------------

#[test]
fn test_combined_message() {
    let src = r#"
syntax = "proto3";
package example;
import "google/protobuf/any.proto";
option java_package = "com.example";

message Person {
    option deprecated = false;

    string name = 1;
    int32  age  = 2;
    repeated string emails = 3;
    optional string nickname = 4;
    map<string, int32> scores = 5;
    reserved 100 to max;
    reserved "old_field";

    oneof contact {
        string phone = 6;
        string address = 7;
    }

    enum Role {
        ROLE_UNSPECIFIED = 0;
        ADMIN = 1;
        USER  = 2;
    }

    message Address {
        string street = 1;
        string city   = 2;
    }
}
"#;
    let f = parse_file(src).expect("must parse combined proto");
    assert_eq!(f.syntax, Some("proto3".to_owned()));
    assert_eq!(f.package, Some("example".to_owned()));
    assert_eq!(f.imports.len(), 1);
    assert_eq!(f.options.len(), 1);
    assert_eq!(f.messages.len(), 1);

    let msg = &f.messages[0];
    assert_eq!(msg.name, "Person");
    assert_eq!(msg.fields.len(), 5); // name, age, emails, nickname, scores
    assert_eq!(msg.oneofs.len(), 1);
    assert_eq!(msg.nested_enums.len(), 1);
    assert_eq!(msg.nested_messages.len(), 1);
    assert_eq!(msg.reserved.len(), 2);
    assert_eq!(msg.options.len(), 1);

    // Check the map field
    let map_field = msg
        .fields
        .iter()
        .find(|f| f.name == "scores")
        .expect("scores field");
    assert_eq!(
        map_field.ty,
        FieldType::Map {
            key: ScalarType::String,
            value: Box::new(FieldType::Scalar(ScalarType::Int32)),
        }
    );

    // Check the reserved range
    match &msg.reserved[0] {
        Reserved::Ranges(ranges) => {
            assert_eq!(ranges[0].from, 100);
            assert_eq!(ranges[0].to, ReservedRangeTo::Max);
        }
        Reserved::Names(_) => panic!("expected ranges"),
    }

    // Check nested enum
    let role = &msg.nested_enums[0];
    assert_eq!(role.name, "Role");
    assert_eq!(role.values.len(), 3);

    // Check nested message
    let addr = &msg.nested_messages[0];
    assert_eq!(addr.name, "Address");
    assert_eq!(addr.fields.len(), 2);
}

// ---------------------------------------------------------------------------
// Edition 2023
// ---------------------------------------------------------------------------

/// A minimal `edition = "2023"` file parses successfully.
#[test]
fn test_edition_2023_basic() {
    let src = r#"edition = "2023";
package myedition;
message Hello {
  string name = 1;
  int32  id   = 2;
}
"#;
    let f = parse_file(src).expect("edition 2023 must parse");
    assert_eq!(f.edition, Some(Edition::Edition2023));
    assert_eq!(f.syntax, None);
    assert_eq!(f.package, Some("myedition".to_owned()));
    assert_eq!(f.messages.len(), 1);
    let msg = &f.messages[0];
    assert_eq!(msg.name, "Hello");
    assert_eq!(msg.fields.len(), 2);
}

/// Edition 2023 supports `optional` keyword (explicit field presence).
#[test]
fn test_edition_2023_optional_field() {
    let src = r#"edition = "2023";
message Msg {
  optional string name = 1;
  int32 count = 2;
}
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.edition, Some(Edition::Edition2023));
    let msg = &f.messages[0];
    assert_eq!(msg.fields.len(), 2);
    assert_eq!(msg.fields[0].label, FieldLabel::Optional);
    assert_eq!(msg.fields[1].label, FieldLabel::Singular);
}

/// Edition 2023 supports `repeated` fields.
#[test]
fn test_edition_2023_repeated_field() {
    let src = r#"edition = "2023";
message Collection {
  repeated string items = 1;
  repeated int64 ids = 2;
}
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.edition, Some(Edition::Edition2023));
    let msg = &f.messages[0];
    assert_eq!(msg.fields.len(), 2);
    assert_eq!(msg.fields[0].label, FieldLabel::Repeated);
    assert_eq!(msg.fields[1].label, FieldLabel::Repeated);
}

/// Edition 2023 supports map fields.
#[test]
fn test_edition_2023_map_field() {
    let src = r#"edition = "2023";
message Config {
  map<string, int32> settings = 1;
  map<int64, string> labels   = 2;
}
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.edition, Some(Edition::Edition2023));
    let msg = &f.messages[0];
    assert_eq!(msg.fields.len(), 2);
    assert_eq!(
        msg.fields[0].ty,
        FieldType::Map {
            key: ScalarType::String,
            value: Box::new(FieldType::Scalar(ScalarType::Int32)),
        }
    );
}

/// Edition 2023 supports oneof blocks.
#[test]
fn test_edition_2023_oneof() {
    let src = r#"edition = "2023";
message Event {
  oneof payload {
    string text   = 1;
    bytes  binary = 2;
    int32  code   = 3;
  }
}
"#;
    let f = parse_file(src).expect("must parse");
    let msg = &f.messages[0];
    assert_eq!(msg.oneofs.len(), 1);
    assert_eq!(msg.oneofs[0].name, "payload");
    assert_eq!(msg.oneofs[0].fields.len(), 3);
}

/// Edition 2023 supports enums.
#[test]
fn test_edition_2023_enum() {
    let src = r#"edition = "2023";
enum Status {
  STATUS_UNSPECIFIED = 0;
  ACTIVE   = 1;
  INACTIVE = 2;
}
message Thing {
  Status status = 1;
}
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.edition, Some(Edition::Edition2023));
    assert_eq!(f.enums.len(), 1);
    assert_eq!(f.enums[0].name, "Status");
    assert_eq!(f.enums[0].values.len(), 3);
}

/// Edition 2023 supports services.
#[test]
fn test_edition_2023_service() {
    let src = r#"edition = "2023";
message Req {}
message Resp {}
service MyService {
  rpc Call (Req) returns (Resp);
  rpc Stream (Req) returns (stream Resp);
}
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.services.len(), 1);
    let svc = &f.services[0];
    assert_eq!(svc.name, "MyService");
    assert_eq!(svc.methods.len(), 2);
    assert!(!svc.methods[0].server_streaming);
    assert!(svc.methods[1].server_streaming);
}

/// Edition 2023 supports file-level options.
#[test]
fn test_edition_2023_file_option() {
    let src = r#"edition = "2023";
option java_package = "com.example";
message Msg { int32 id = 1; }
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.edition, Some(Edition::Edition2023));
    assert_eq!(f.options.len(), 1);
    assert_eq!(f.options[0].name, "java_package");
}

/// Edition 2023 supports nested messages.
#[test]
fn test_edition_2023_nested_message() {
    let src = r#"edition = "2023";
message Outer {
  message Inner {
    int32 value = 1;
  }
  Inner inner = 1;
  string name = 2;
}
"#;
    let f = parse_file(src).expect("must parse");
    let outer = &f.messages[0];
    assert_eq!(outer.nested_messages.len(), 1);
    assert_eq!(outer.nested_messages[0].name, "Inner");
    assert_eq!(outer.fields.len(), 2);
}

/// Edition 2023 supports reserved ranges and names.
#[test]
fn test_edition_2023_reserved() {
    let src = r#"edition = "2023";
message Versioned {
  reserved 2, 15, 9 to 11;
  reserved "old_field", "another_old_field";
  int32 id = 1;
}
"#;
    let f = parse_file(src).expect("must parse");
    let msg = &f.messages[0];
    assert_eq!(msg.reserved.len(), 2);
}

/// Edition 2023 with imports is parsed correctly.
#[test]
fn test_edition_2023_imports() {
    let src = r#"edition = "2023";
import "google/protobuf/timestamp.proto";
import public "other.proto";
message Msg {
  int32 id = 1;
}
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.edition, Some(Edition::Edition2023));
    assert_eq!(f.imports.len(), 2);
    assert_eq!(f.imports[0].modifier, ImportModifier::None);
    assert_eq!(f.imports[1].modifier, ImportModifier::Public);
}

/// An unsupported edition value (not "2023") produces `UnsupportedEdition` error.
#[test]
fn test_edition_unknown_value_error() {
    let src = r#"edition = "2024";"#;
    let err = parse_file(src).expect_err("must fail on unknown edition");
    assert!(
        matches!(err, ParseError::UnsupportedEdition(ref s) if s == "2024"),
        "expected UnsupportedEdition(\"2024\"), got: {err:?}"
    );
}

/// Setting both `syntax` and `edition` produces `SyntaxAndEditionConflict` error.
#[test]
fn test_syntax_and_edition_conflict() {
    let src = r#"syntax = "proto3";
edition = "2023";
message Msg { int32 id = 1; }
"#;
    let err = parse_file(src).expect_err("must fail on syntax+edition conflict");
    assert!(
        matches!(err, ParseError::SyntaxAndEditionConflict),
        "expected SyntaxAndEditionConflict, got: {err:?}"
    );
}

/// Setting edition before syntax also produces `SyntaxAndEditionConflict`.
#[test]
fn test_edition_then_syntax_conflict() {
    let src = r#"edition = "2023";
syntax = "proto3";
message Msg { int32 id = 1; }
"#;
    let err = parse_file(src).expect_err("must fail on edition+syntax conflict");
    assert!(
        matches!(err, ParseError::SyntaxAndEditionConflict),
        "expected SyntaxAndEditionConflict, got: {err:?}"
    );
}

/// Edition 2023 produces correct `file_syntax_string` value of "editions".
#[test]
fn test_edition_2023_edition_field_is_set() {
    let src = r#"edition = "2023";
message Empty {}
"#;
    let f = parse_file(src).expect("must parse");
    assert_eq!(f.edition, Some(Edition::Edition2023));
    // The `Edition::syntax_sentinel()` should be "editions"
    assert_eq!(Edition::syntax_sentinel(), "editions");
}
