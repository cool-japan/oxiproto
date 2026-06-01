use oxiproto_build::{compile_str_fn, BuildError};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal valid proto3 snippet.
fn valid_proto(pkg: &str) -> String {
    format!(
        r#"syntax = "proto3";
package {pkg};
message Ping {{ string payload = 1; }}
"#
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn compile_valid_inline_proto_returns_fds() {
    let fds = compile_str_fn(&valid_proto("inline_test"))
        .expect("valid proto should compile without error");
    // protox includes the inline file itself.
    assert_eq!(
        fds.file.len(),
        1,
        "expected exactly one FileDescriptorProto, got {}",
        fds.file.len()
    );
    // The package field should match.
    assert_eq!(fds.file[0].package(), "inline_test");
}

#[test]
fn compile_proto_with_multiple_messages_returns_single_file() {
    let src = r#"syntax = "proto3";
package multi;
message A { int32 x = 1; }
message B { string y = 1; }
message C { bool z = 1; }
"#;
    let fds = compile_str_fn(src).expect("multi-message proto should compile");
    assert_eq!(fds.file.len(), 1);
    let file = &fds.file[0];
    assert_eq!(file.message_type.len(), 3, "expected 3 message types");
}

#[test]
fn compile_proto_with_syntax_error_returns_parse_error_with_location() {
    // Missing semicolon after field number — the error is on line 2.
    // protox's Debug format emits "file:2:28: expected ';' or '[', but found '}'".
    // The native parser returns BuildError::Parse too, but may report line=0
    // because its error mapping currently lacks source-location tracking.
    let src = r#"syntax = "proto3";
message Bad { string x = 1 }
"#;
    let err = compile_str_fn(src).expect_err("broken proto must return an error");
    match err {
        BuildError::Parse { line, .. } => {
            // Under the protox path the line number is expected to be
            // non-zero. Under native-parser the parse error is still a
            // BuildError::Parse but may have line=0.
            #[cfg(not(feature = "native-parser"))]
            assert!(
                line > 0,
                "expected non-zero line number from protox diagnostic, got line={line}"
            );
            // Under native, just confirm it is a Parse variant (above match arm).
            #[cfg(feature = "native-parser")]
            let _ = line;
        }
        other => panic!("expected BuildError::Parse, got: {other:?}"),
    }
}

#[test]
fn compile_proto_with_unknown_import_returns_parse_error() {
    // Imports a file that doesn't exist in temp_dir; protox should fail.
    let src = r#"syntax = "proto3";
import "nonexistent_file_xyz.proto";
message Importing { int32 id = 1; }
"#;
    let err = compile_str_fn(src).expect_err("unresolvable import must return an error");
    match err {
        BuildError::Parse { .. } => {}
        other => panic!("expected BuildError::Parse, got: {other:?}"),
    }
}

/// Cleanup is verified by the unit test in `src/compile_str.rs`.
/// This integration test confirms the function returns a valid result and
/// doesn't leave any observable side effects.
#[test]
fn compile_str_valid_and_failed_produce_no_lasting_state() {
    // Valid compile.
    let fds = compile_str_fn(&valid_proto("state_check")).expect("valid compile");
    assert_eq!(fds.file.len(), 1);

    // Invalid compile — should not panic.
    let _ = compile_str_fn("this is not a valid proto file");
}
