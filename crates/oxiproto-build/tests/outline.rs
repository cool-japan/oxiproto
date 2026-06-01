#![forbid(unsafe_code)]

//! Tests for the native .proto outline parser.

use oxiproto_build::parser::{parse_outline, ParseError, TopLevelItem};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn outline_ok(source: &str) -> oxiproto_build::parser::FileOutline {
    parse_outline(source).expect("parse_outline should succeed")
}

// ---------------------------------------------------------------------------
// Proto3 file — full structure
// ---------------------------------------------------------------------------

#[test]
fn test_proto3_full() {
    let src = r#"
syntax = "proto3";
package com.example;
import "google/protobuf/timestamp.proto";
import public "other.proto";
option java_package = "com.example";

message Person {
    string name = 1;
    int32 age = 2;
}

enum Status {
    UNKNOWN = 0;
    ACTIVE = 1;
}

service GreetService {
    rpc SayHello (HelloRequest) returns (HelloResponse);
}
"#;
    let outline = outline_ok(src);
    assert_eq!(outline.syntax.as_deref(), Some("proto3"));
    assert_eq!(outline.package.as_deref(), Some("com.example"));
    assert_eq!(outline.imports.len(), 2);
    assert_eq!(outline.imports[0], "google/protobuf/timestamp.proto");
    assert_eq!(outline.imports[1], "other.proto");
    assert!(
        !outline.options.is_empty(),
        "should have at least one option"
    );

    assert_eq!(outline.items.len(), 3);
    match &outline.items[0] {
        TopLevelItem::Message { name, .. } => assert_eq!(name, "Person"),
        other => panic!("expected Message, got {other:?}"),
    }
    match &outline.items[1] {
        TopLevelItem::Enum { name, .. } => assert_eq!(name, "Status"),
        other => panic!("expected Enum, got {other:?}"),
    }
    match &outline.items[2] {
        TopLevelItem::Service { name, .. } => assert_eq!(name, "GreetService"),
        other => panic!("expected Service, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Proto2 file — syntax + nested message (outline sees only top-level)
// ---------------------------------------------------------------------------

#[test]
fn test_proto2_nested_message() {
    let src = r#"
syntax = "proto2";
message Outer {
    message Inner {
        required string id = 1;
    }
    required Inner inner = 1;
}
"#;
    let outline = outline_ok(src);
    assert_eq!(outline.syntax.as_deref(), Some("proto2"));
    // Outline sees ONE top-level message (Outer); Inner is inside the body.
    assert_eq!(outline.items.len(), 1);
    match &outline.items[0] {
        TopLevelItem::Message { name, .. } => assert_eq!(name, "Outer"),
        other => panic!("expected Message, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Multiple messages and enums in the same file
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_top_level_items() {
    let src = r#"
syntax = "proto3";
message Alpha {}
message Beta {}
enum Color { RED = 0; GREEN = 1; }
message Gamma {}
"#;
    let outline = outline_ok(src);
    let names: Vec<&str> = outline
        .items
        .iter()
        .map(|i| match i {
            TopLevelItem::Message { name, .. } => name.as_str(),
            TopLevelItem::Enum { name, .. } => name.as_str(),
            TopLevelItem::Service { name, .. } => name.as_str(),
        })
        .collect();
    assert_eq!(names, ["Alpha", "Beta", "Color", "Gamma"]);
}

// ---------------------------------------------------------------------------
// File with no package
// ---------------------------------------------------------------------------

#[test]
fn test_no_package() {
    let src = r#"syntax = "proto3"; message Lone {}"#;
    let outline = outline_ok(src);
    assert!(outline.package.is_none(), "package should be None");
    assert_eq!(outline.items.len(), 1);
}

// ---------------------------------------------------------------------------
// Malformed file — missing `{`
// ---------------------------------------------------------------------------

#[test]
fn test_missing_open_brace() {
    let src = r#"syntax = "proto3"; message Broken"#;
    let result = parse_outline(src);
    assert!(result.is_err(), "should fail on missing brace");
    match result {
        Err(ParseError::UnexpectedEof) | Err(ParseError::UnexpectedToken { .. }) => {}
        other => panic!("unexpected result: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Import with `public` modifier
// ---------------------------------------------------------------------------

#[test]
fn test_import_public() {
    let src = r#"syntax = "proto3"; import public "extra.proto";"#;
    let outline = outline_ok(src);
    assert_eq!(outline.imports.len(), 1);
    assert_eq!(outline.imports[0], "extra.proto");
}

// ---------------------------------------------------------------------------
// Import with `weak` modifier
// ---------------------------------------------------------------------------

#[test]
fn test_import_weak() {
    let src = r#"syntax = "proto3"; import weak "weak.proto";"#;
    let outline = outline_ok(src);
    assert_eq!(outline.imports.len(), 1);
    assert_eq!(outline.imports[0], "weak.proto");
}

// ---------------------------------------------------------------------------
// Service body — outline just tracks span, doesn't parse methods
// ---------------------------------------------------------------------------

#[test]
fn test_service_body_span() {
    let src = r#"
syntax = "proto3";
service MyService {
    rpc GetFoo (FooReq) returns (FooResp);
    rpc PutFoo (FooPut) returns (FooResp) {}
}
"#;
    let outline = outline_ok(src);
    assert_eq!(outline.items.len(), 1);
    match &outline.items[0] {
        TopLevelItem::Service {
            name,
            span,
            body_span,
        } => {
            assert_eq!(name, "MyService");
            // body_span starts at or before the service body
            assert!(
                body_span.start < body_span.end,
                "body_span should be non-empty"
            );
            // full span starts before (or at) body span
            assert!(span.start <= body_span.start);
            assert_eq!(span.end, body_span.end);
        }
        other => panic!("expected Service, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Top-level option recorded
// ---------------------------------------------------------------------------

#[test]
fn test_top_level_option_recorded() {
    let src = r#"
syntax = "proto3";
option optimize_for = SPEED;
message Dummy {}
"#;
    let outline = outline_ok(src);
    assert!(
        !outline.options.is_empty(),
        "top-level option should be recorded"
    );
    // The option name should be present in the list
    assert!(
        outline.options.iter().any(|o| o.contains("optimize_for")),
        "expected 'optimize_for' in options, got {:?}",
        outline.options
    );
}

// ---------------------------------------------------------------------------
// Dotted package name
// ---------------------------------------------------------------------------

#[test]
fn test_dotted_package() {
    let src = r#"syntax = "proto3"; package google.protobuf;"#;
    let outline = outline_ok(src);
    assert_eq!(outline.package.as_deref(), Some("google.protobuf"));
}

// ---------------------------------------------------------------------------
// File with comments interspersed
// ---------------------------------------------------------------------------

#[test]
fn test_comments_ignored_in_outline() {
    let src = r#"
// File-level comment
syntax = "proto3"; // inline
/* block comment */
package example;
// another comment
message Msg {
    // field comment
    int32 x = 1;
}
"#;
    let outline = outline_ok(src);
    assert_eq!(outline.syntax.as_deref(), Some("proto3"));
    assert_eq!(outline.package.as_deref(), Some("example"));
    assert_eq!(outline.items.len(), 1);
}

// ---------------------------------------------------------------------------
// Span correctness for message items
// ---------------------------------------------------------------------------

#[test]
fn test_message_spans() {
    let src = "message Foo { int32 x = 1; }";
    let outline = outline_ok(src);
    assert_eq!(outline.items.len(), 1);
    match &outline.items[0] {
        TopLevelItem::Message {
            name,
            span,
            body_span,
        } => {
            assert_eq!(name, "Foo");
            // The full span starts at 0 (the 'm' of 'message')
            assert_eq!(span.start, 0);
            // The span ends at the end of the source
            assert_eq!(span.end, src.len());
            // body_span covers the braces
            assert!(body_span.start > 0);
            assert_eq!(body_span.end, src.len());
        }
        other => panic!("expected Message, got {other:?}"),
    }
}
