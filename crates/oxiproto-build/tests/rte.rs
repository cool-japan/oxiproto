//! RTE (routing rewire) integration tests.
//!
//! These tests verify that `compile_str`, `Builder::compile_to_fds`, and
//! `Builder::compile` route through the native parser when the
//! `native-parser` feature is enabled. They are compiled only under that
//! feature so they do not affect the default (protox) build.

#[cfg(feature = "native-parser")]
mod rte_tests {
    use oxiproto_build::{compile_str, compile_to_fds, BuildError, Builder};

    // -----------------------------------------------------------------------
    // compile_str
    // -----------------------------------------------------------------------

    #[test]
    fn public_compile_str_uses_native_under_feature() {
        let src = r#"syntax = "proto3";
package test;
message PingRequest { string payload = 1; }
"#;
        let fds = compile_str(src).expect("compile_str with native feature should succeed");
        assert_eq!(
            fds.file.len(),
            1,
            "expected exactly one FileDescriptorProto"
        );
        let f = &fds.file[0];
        assert_eq!(f.message_type.len(), 1, "expected exactly one message type");
        assert_eq!(f.message_type[0].name(), "PingRequest");
    }

    #[test]
    fn public_compile_str_native_invalid_proto() {
        // Missing semicolon after field declaration.
        let bad = r#"syntax = "proto3"; message Bad { string x = 1 }"#;
        let err = compile_str(bad).expect_err("should error on bad proto");
        match err {
            BuildError::Parse { .. } => {}
            other => panic!("expected BuildError::Parse, got {other:?}"),
        }
    }

    #[test]
    fn public_compile_str_native_multiple_messages() {
        let src = r#"syntax = "proto3";
package rte;
message Alpha { int32 x = 1; }
message Beta  { string y = 1; }
"#;
        let fds = compile_str(src).expect("multi-message proto should compile via native");
        assert_eq!(fds.file.len(), 1);
        assert_eq!(fds.file[0].message_type.len(), 2);
    }

    // -----------------------------------------------------------------------
    // compile_to_fds (Builder)
    // -----------------------------------------------------------------------

    #[test]
    fn public_compile_to_fds_uses_native_under_feature() {
        use std::fs;

        let dir = std::env::temp_dir().join(format!("oxiproto-rte-test-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");

        let proto_content = r#"syntax = "proto3";
package rtetest;
message Request { string name = 1; }
"#;
        let proto_path = dir.join("rte_test.proto");
        fs::write(&proto_path, proto_content).expect("write proto");

        let fds =
            compile_to_fds(&[&proto_path], &[&dir]).expect("compile_to_fds native should succeed");

        let _ = fs::remove_dir_all(&dir);

        assert!(
            fds.file.iter().any(|f| f.package() == "rtetest"),
            "should have rtetest package in FDS"
        );
    }

    #[test]
    fn compile_to_fds_via_builder_uses_native() {
        use std::fs;

        let dir =
            std::env::temp_dir().join(format!("oxiproto-rte-builder-test-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");

        let proto_content = r#"syntax = "proto3";
package rtebuilder;
message Response { bool ok = 1; int32 code = 2; }
"#;
        let proto_path = dir.join("rte_builder.proto");
        fs::write(&proto_path, proto_content).expect("write proto");

        let fds = Builder::new()
            .compile_to_fds(&[&proto_path], &[&dir])
            .expect("Builder::compile_to_fds native should succeed");

        let _ = fs::remove_dir_all(&dir);

        let matching: Vec<_> = fds
            .file
            .iter()
            .filter(|f| f.package() == "rtebuilder")
            .collect();
        assert_eq!(matching.len(), 1, "expected exactly one rtebuilder file");
        assert_eq!(matching[0].message_type.len(), 1);
        assert_eq!(matching[0].message_type[0].name(), "Response");
    }
}
