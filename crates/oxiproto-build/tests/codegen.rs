use oxiproto_build::BuildError;
use oxiproto_core::OxiProtoError;
use std::env;
use std::fs;
use std::path::PathBuf;

fn tmp_root() -> PathBuf {
    env::temp_dir().join("oxiproto-test")
}

#[test]
fn compile_simple_proto_without_protoc() {
    // Write a minimal .proto to a temp dir
    let tmp = tmp_root().join("simple");
    fs::create_dir_all(&tmp).unwrap();
    let proto_dir = tmp.join("proto");
    fs::create_dir_all(&proto_dir).unwrap();

    let proto_content = r#"syntax = "proto3";
package test;
message Foo {
    string name = 1;
    int64 ts = 2;
}"#;
    fs::write(proto_dir.join("simple.proto"), proto_content).unwrap();

    let out_dir = tmp.join("out");
    fs::create_dir_all(&out_dir).unwrap();

    // Must work WITHOUT protoc — the whole point of oxiproto-build
    oxiproto_build::Builder::new()
        .out_dir(&out_dir)
        .compile(
            &[proto_dir.join("simple.proto")],
            std::slice::from_ref(&proto_dir),
        )
        .expect("compile_protos failed");

    // The generated file should exist
    let entries: Vec<_> = fs::read_dir(&out_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!entries.is_empty(), "No files generated in out_dir");

    // Check the generated content mentions our message
    let generated = entries
        .iter()
        .find(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
        .expect("no .rs file generated");
    let content = fs::read_to_string(generated.path()).unwrap();
    assert!(
        content.contains("Foo") || content.contains("foo"),
        "Generated file doesn't mention Foo:\n{content}"
    );
}

#[test]
fn compile_is_idempotent() {
    let tmp = tmp_root().join("idempotent");
    fs::create_dir_all(&tmp).unwrap();
    let proto_dir = tmp.join("proto");
    fs::create_dir_all(&proto_dir).unwrap();
    let proto = proto_dir.join("msg.proto");
    fs::write(&proto, "syntax = \"proto3\";\nmessage Bar { int32 x = 1; }").unwrap();

    let out1 = tmp.join("out1");
    let out2 = tmp.join("out2");
    fs::create_dir_all(&out1).unwrap();
    fs::create_dir_all(&out2).unwrap();

    oxiproto_build::Builder::new()
        .out_dir(&out1)
        .compile(&[&proto], &[&proto_dir])
        .unwrap();

    oxiproto_build::Builder::new()
        .out_dir(&out2)
        .compile(&[&proto], &[&proto_dir])
        .unwrap();

    let files1: Vec<_> = fs::read_dir(&out1)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let files2: Vec<_> = fs::read_dir(&out2)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(
        files1.len(),
        files2.len(),
        "different number of files generated"
    );
}

// ---------------------------------------------------------------------------
// Builder::btree_map
// ---------------------------------------------------------------------------

#[test]
fn builder_btree_map_generates_btree_map_field() {
    let tmp = tmp_root().join("btree_map");
    fs::create_dir_all(&tmp).unwrap();
    let proto_dir = tmp.join("proto");
    fs::create_dir_all(&proto_dir).unwrap();

    let proto_content = r#"syntax = "proto3";
package btree_test;
message MapHolder {
    map<string, int32> labels = 1;
}
"#;
    fs::write(proto_dir.join("btree.proto"), proto_content).unwrap();

    let out_dir = tmp.join("out");
    fs::create_dir_all(&out_dir).unwrap();

    oxiproto_build::Builder::new()
        .out_dir(&out_dir)
        .btree_map(["."])
        .compile(
            &[proto_dir.join("btree.proto")],
            std::slice::from_ref(&proto_dir),
        )
        .expect("btree_map compile failed");

    // Find the generated file and check it uses BTreeMap.
    let entries: Vec<_> = fs::read_dir(&out_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!entries.is_empty(), "no files generated");

    let generated = entries
        .iter()
        .find(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
        .expect("no .rs file generated");
    let content = fs::read_to_string(generated.path()).unwrap();
    assert!(
        content.contains("BTreeMap"),
        "generated code should use BTreeMap:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Builder::out_dir
// ---------------------------------------------------------------------------

#[test]
fn builder_out_dir_writes_to_specified_directory() {
    let tmp = tmp_root().join("out_dir_check");
    fs::create_dir_all(&tmp).unwrap();
    let proto_dir = tmp.join("proto");
    fs::create_dir_all(&proto_dir).unwrap();

    fs::write(
        proto_dir.join("outdir.proto"),
        "syntax = \"proto3\";\nmessage OutDir { int32 id = 1; }",
    )
    .unwrap();

    let out_dir = tmp.join("rust_out");
    fs::create_dir_all(&out_dir).unwrap();

    oxiproto_build::Builder::new()
        .out_dir(&out_dir)
        .compile(
            &[proto_dir.join("outdir.proto")],
            std::slice::from_ref(&proto_dir),
        )
        .expect("compile with out_dir failed");

    let entries: Vec<_> = fs::read_dir(&out_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
        .collect();
    assert!(!entries.is_empty(), "no .rs files in specified out_dir");
}

// ---------------------------------------------------------------------------
// Builder::file_descriptor_set_path
// ---------------------------------------------------------------------------

#[test]
fn builder_file_descriptor_set_path_writes_fds_bytes() {
    let tmp = tmp_root().join("fds_path");
    fs::create_dir_all(&tmp).unwrap();
    let proto_dir = tmp.join("proto");
    fs::create_dir_all(&proto_dir).unwrap();

    let proto_content = r#"syntax = "proto3";
package fds_test;
message FdsMsg { string data = 1; }
"#;
    fs::write(proto_dir.join("fds.proto"), proto_content).unwrap();

    let out_dir = tmp.join("out");
    fs::create_dir_all(&out_dir).unwrap();

    let fds_path = tmp.join("descriptor.bin");

    oxiproto_build::Builder::new()
        .out_dir(&out_dir)
        .file_descriptor_set_path(&fds_path)
        .compile(
            &[proto_dir.join("fds.proto")],
            std::slice::from_ref(&proto_dir),
        )
        .expect("compile with fds_path failed");

    // The FDS bytes file should exist and be non-empty.
    assert!(
        fds_path.exists(),
        "FDS file was not written to {fds_path:?}"
    );
    let bytes = fs::read(&fds_path).unwrap();
    assert!(!bytes.is_empty(), "FDS file is empty");

    // Verify the bytes decode back to a valid FileDescriptorSet.
    use prost::Message as _;
    let fds = prost_types::FileDescriptorSet::decode(bytes.as_slice())
        .expect("FDS bytes should decode cleanly");
    assert_eq!(fds.file.len(), 1);
    assert_eq!(fds.file[0].package(), "fds_test");
}

// ---------------------------------------------------------------------------
// BuildError conversions
// ---------------------------------------------------------------------------

#[test]
fn build_error_from_oxiproto_parse_error_is_parse_variant() {
    let oxi = OxiProtoError::ParseError("foo.proto:5:3: unexpected token".to_owned());
    let be = BuildError::from(oxi);
    match be {
        BuildError::Parse {
            file,
            line,
            col,
            message,
        } => {
            assert_eq!(file, "foo.proto");
            assert_eq!(line, 5);
            assert_eq!(col, 3);
            assert!(message.contains("unexpected"), "message: {message}");
        }
        other => panic!("expected BuildError::Parse, got {other:?}"),
    }
}

#[test]
fn build_error_to_oxiproto_preserves_message_text() {
    let be = BuildError::Parse {
        file: "a.proto".to_owned(),
        line: 2,
        col: 1,
        message: "bad syntax".to_owned(),
    };
    let display = be.to_string();
    let oxi = OxiProtoError::from(be);
    match oxi {
        OxiProtoError::ParseError(s) => {
            assert!(
                s.contains("bad syntax"),
                "OxiProtoError should contain original message; got: {s}"
            );
            assert_eq!(s, display, "round-trip should produce same Display string");
        }
        other => panic!("expected OxiProtoError::ParseError, got {other:?}"),
    }
}

#[test]
fn build_error_from_oxiproto_codegen_error_is_codegen_variant() {
    let oxi = OxiProtoError::CodegenError("emit failed".to_owned());
    let be = BuildError::from(oxi);
    match be {
        BuildError::Codegen { message } => {
            assert!(message.contains("emit failed"), "message: {message}");
        }
        other => panic!("expected BuildError::Codegen, got {other:?}"),
    }
}
