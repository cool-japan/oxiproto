//! End-to-end `encode` / `decode` over a Protobuf Editions schema.
//!
//! These two subcommands are the only place where all three facade layers meet:
//! `oxiproto-build` parses the `edition = "2023";` source, `oxiproto-reflect`
//! turns the descriptor set into a `prost_reflect::DescriptorPool`, and
//! `oxiproto-json` maps between canonical Protobuf-JSON and a
//! `prost_reflect::DynamicMessage`. Before the Editions down-level landed, pool
//! construction failed for any edition schema, so both subcommands were
//! unusable for one of the three syntaxes the parser accepts.

use std::path::PathBuf;
use std::process::Command;

/// Path to the compiled `oxiproto-cli` binary under test.
fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_oxiproto-cli"))
}

/// Create a unique temporary directory for one test run.
fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oxiproto-cli-editions-{}-{}",
        std::process::id(),
        tag
    ));
    std::fs::create_dir_all(&dir).expect("create tmp dir");
    dir
}

/// Write an `edition = "2023";` fixture exercising the features that the
/// down-level has to preserve: default (PACKED) and EXPANDED repeated
/// encodings, and EXPLICIT presence on a singular scalar.
fn write_edition_proto(dir: &std::path::Path) -> PathBuf {
    let proto = dir.join("edition.proto");
    std::fs::write(
        &proto,
        r#"edition = "2023";
package edcli;

message Sample {
  string name = 1;
  int32 count = 2;
  repeated int32 packed = 3;
  repeated int32 expanded = 4 [features.repeated_field_encoding = EXPANDED];
}
"#,
    )
    .expect("write edition.proto");
    proto
}

/// Run one subcommand, returning its captured stdout on success.
fn run(subcommand: &str, proto: &std::path::Path, dir: &std::path::Path, input: &[u8]) -> Vec<u8> {
    let input_path = dir.join(format!("{subcommand}-input"));
    let output_path = dir.join(format!("{subcommand}-output"));
    std::fs::write(&input_path, input).expect("write input");

    let output = Command::new(binary())
        .args([
            subcommand,
            proto.to_str().expect("utf8"),
            "-I",
            dir.to_str().expect("utf8"),
            "-t",
            "edcli.Sample",
            "-i",
            input_path.to_str().expect("utf8"),
            "-o",
            output_path.to_str().expect("utf8"),
        ])
        .output()
        .expect("spawn subcommand");
    assert!(
        output.status.success(),
        "{subcommand} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read(&output_path).expect("read output")
}

/// JSON → wire → JSON through an edition schema, with the wire bytes checked
/// against the resolved `features.repeated_field_encoding` of each field.
#[test]
fn encode_decode_round_trip_for_an_editions_schema() {
    let tmp = tmp_dir("roundtrip");
    let proto = write_edition_proto(&tmp);

    let json = br#"{"name":"Alice","count":42,"packed":[1,2,3],"expanded":[4,5]}"#;
    let wire = run("encode", &proto, &tmp, json);
    assert!(!wire.is_empty(), "encode produced no bytes");

    // Field 3 defaults to PACKED: tag 0x1a (field 3, Len) then a 3-byte run.
    assert!(
        wire.windows(5).any(|w| w == [0x1a, 0x03, 0x01, 0x02, 0x03]),
        "field 3 must be packed: {wire:02x?}"
    );
    // Field 4 opts into EXPANDED: tag 0x20 (field 4, Varint) once per element.
    assert_eq!(
        wire.iter().filter(|b| **b == 0x20).count(),
        2,
        "field 4 must be expanded: {wire:02x?}"
    );

    let decoded = run("decode", &proto, &tmp, &wire);
    let parsed: serde_json::Value =
        serde_json::from_slice(&decoded).expect("decode must emit valid JSON");
    assert_eq!(parsed["name"], "Alice");
    assert_eq!(parsed["count"], 42);
    assert_eq!(parsed["packed"], serde_json::json!([1, 2, 3]));
    assert_eq!(parsed["expanded"], serde_json::json!([4, 5]));
}

/// Editions defaults `features.field_presence` to EXPLICIT, so a field set to
/// its type default is written to the wire and reported back by `decode` —
/// unlike proto3, where the same value is omitted.
#[test]
fn explicit_presence_is_visible_through_the_cli() {
    let tmp = tmp_dir("presence");
    let proto = write_edition_proto(&tmp);

    let wire = run("encode", &proto, &tmp, br#"{"count":0}"#);
    // field 2, Varint, value 0 — a proto3 encoder would emit nothing at all.
    assert_eq!(wire, vec![0x10, 0x00], "wire: {wire:02x?}");

    let decoded = run("decode", &proto, &tmp, &wire);
    let parsed: serde_json::Value =
        serde_json::from_slice(&decoded).expect("decode must emit valid JSON");
    assert_eq!(parsed["count"], 0, "decoded JSON: {parsed}");
}
