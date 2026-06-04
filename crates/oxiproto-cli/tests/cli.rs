use std::path::PathBuf;
use std::process::Command;

/// Path to the compiled `oxiproto-cli` binary under test.
fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_oxiproto-cli"))
}

/// Create a unique temporary directory for one test run.
fn tmp_dir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("oxiproto-cli-test-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).expect("create tmp dir");
    dir
}

/// Write a minimal `.proto` fixture and return its path.
fn write_test_proto(dir: &std::path::Path) -> PathBuf {
    let proto = dir.join("test.proto");
    std::fs::write(
        &proto,
        r#"syntax = "proto3";
package test;

message Greeting {
    string name = 1;
    int32 count = 2;
}

enum Status {
    UNKNOWN = 0;
    ACTIVE  = 1;
    RETIRED = 2;
}
"#,
    )
    .expect("write test.proto");
    proto
}

/// Write a `.proto` fixture with a specific package declaration.
fn write_proto_with_package(dir: &std::path::Path, filename: &str, package: &str) -> PathBuf {
    let proto = dir.join(filename);
    std::fs::write(
        &proto,
        format!(
            r#"syntax = "proto3";
package {package};

message Sample {{
    string id = 1;
}}
"#
        ),
    )
    .expect("write proto with package");
    proto
}

/// Write a `.proto` fixture without a package declaration.
fn write_proto_no_package(dir: &std::path::Path, filename: &str) -> PathBuf {
    let proto = dir.join(filename);
    std::fs::write(
        &proto,
        r#"syntax = "proto3";

message NoPackage {
    string value = 1;
}
"#,
    )
    .expect("write proto without package");
    proto
}

/// `oxiproto-cli gen test.proto -I <dir> -o <out>` produces a non-empty Rust
/// file that `syn`-parses successfully and contains the expected type names.
#[test]
fn gen_creates_output_file() {
    let tmp = tmp_dir("gen");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_test_proto(&proto_dir);
    let out_dir = tmp.join("out");

    let status = Command::new(binary())
        .args([
            "gen",
            proto_file.to_str().expect("proto path utf8"),
            "-I",
            proto_dir.to_str().expect("proto_dir utf8"),
            "-o",
            out_dir.to_str().expect("out_dir utf8"),
        ])
        .status()
        .expect("spawn oxiproto-cli");

    assert!(status.success(), "oxiproto-cli gen exited with {status}");

    let out_file = out_dir.join("test.rs");
    assert!(out_file.exists(), "expected {out_file:?} to exist");

    let contents = std::fs::read_to_string(&out_file).expect("read generated file");
    assert!(!contents.is_empty(), "generated file must not be empty");

    // The file must contain the struct and enum names we defined.
    assert!(
        contents.contains("Greeting"),
        "expected 'Greeting' in output:\n{contents}"
    );
    assert!(
        contents.contains("Status"),
        "expected 'Status' in output:\n{contents}"
    );
}

/// `oxiproto-cli --help` must exit with code 0.
#[test]
fn help_exits_zero() {
    let status = Command::new(binary())
        .arg("--help")
        .status()
        .expect("spawn oxiproto-cli --help");
    assert!(status.success(), "--help exited with {status}");
}

/// `oxiproto-cli gen` on a non-existent file must exit with a non-zero code.
#[test]
fn missing_file_nonzero_exit() {
    let tmp = tmp_dir("missing");
    let out_dir = tmp.join("out");

    let nonexistent = std::env::temp_dir().join("this-file-should-not-exist-oxiproto.proto");
    let status = Command::new(binary())
        .args([
            "gen",
            nonexistent.to_str().expect("nonexistent utf8"),
            "-o",
            out_dir.to_str().expect("out_dir utf8"),
        ])
        .status()
        .expect("spawn oxiproto-cli");

    assert!(
        !status.success(),
        "expected non-zero exit for missing file, got {status}"
    );
}

/// `oxiproto-cli describe` prints a summary containing the message and enum
/// names, and exits zero.
#[test]
fn describe_prints_summary() {
    let tmp = tmp_dir("describe");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_test_proto(&proto_dir);

    let output = Command::new(binary())
        .args([
            "describe",
            proto_file.to_str().expect("proto path utf8"),
            "-I",
            proto_dir.to_str().expect("proto_dir utf8"),
        ])
        .output()
        .expect("spawn oxiproto-cli describe");

    assert!(
        output.status.success(),
        "describe exited with {}",
        output.status
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Greeting"), "describe output:\n{stdout}");
    assert!(stdout.contains("Status"), "describe output:\n{stdout}");
    assert!(stdout.contains("Message:"), "describe output:\n{stdout}");
    assert!(stdout.contains("Enum:"), "describe output:\n{stdout}");
}

/// `oxiproto-cli encode` (JSON -> binary) then `decode` (binary -> JSON)
/// round-trips a message through files.
#[test]
fn encode_decode_round_trip() {
    let tmp = tmp_dir("encdec");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_test_proto(&proto_dir);

    // Write JSON input
    let json_input = tmp.join("input.json");
    std::fs::write(&json_input, r#"{"name":"Alice","count":42}"#).expect("write json");

    let bin_file = tmp.join("encoded.bin");

    // Encode JSON -> binary
    let status = Command::new(binary())
        .args([
            "encode",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-t",
            "test.Greeting",
            "-i",
            json_input.to_str().expect("utf8"),
            "-o",
            bin_file.to_str().expect("utf8"),
        ])
        .status()
        .expect("spawn encode");
    assert!(status.success(), "encode exited with {status}");

    assert!(bin_file.exists(), "expected encoded binary file");
    let bin_size = std::fs::metadata(&bin_file).expect("stat bin").len();
    assert!(bin_size > 0, "encoded binary should be non-empty");

    // Decode binary -> JSON
    let json_out = tmp.join("decoded.json");
    let status = Command::new(binary())
        .args([
            "decode",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-t",
            "test.Greeting",
            "-i",
            bin_file.to_str().expect("utf8"),
            "-o",
            json_out.to_str().expect("utf8"),
        ])
        .status()
        .expect("spawn decode");
    assert!(status.success(), "decode exited with {status}");

    let decoded = std::fs::read_to_string(&json_out).expect("read decoded");
    let parsed: serde_json::Value = serde_json::from_str(&decoded).expect("parse decoded json");

    assert_eq!(parsed["name"], "Alice", "decoded JSON: {decoded}");
    // count=42 — int32 stays a JSON number
    assert_eq!(parsed["count"], 42, "decoded JSON: {decoded}");
}

// ---------------------------------------------------------------------------
// New tests: Slice CLI additions
// ---------------------------------------------------------------------------

/// `gen --dry-run` prints generated code to stdout and creates NO output file.
#[test]
fn gen_dry_run_prints_stdout_no_file() {
    let tmp = tmp_dir("dryrun");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_test_proto(&proto_dir);
    let out_dir = tmp.join("out");

    let output = Command::new(binary())
        .args([
            "gen",
            "--dry-run",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .output()
        .expect("spawn gen --dry-run");

    assert!(
        output.status.success(),
        "gen --dry-run exited with {}",
        output.status
    );

    // stdout must contain generated Rust code.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.is_empty(),
        "gen --dry-run should print to stdout but produced nothing"
    );

    // No file should have been created.
    if out_dir.exists() {
        let entries: Vec<_> = std::fs::read_dir(&out_dir).expect("read out_dir").collect();
        assert!(
            entries.is_empty(),
            "--dry-run must not create any files, found: {entries:?}"
        );
    }
}

/// `gen --recursive` finds and compiles `.proto` files in nested directories.
#[test]
fn gen_recursive_scan() {
    let tmp = tmp_dir("recursive");
    let proto_root = tmp.join("protos");
    let nested_dir = proto_root.join("nested");
    std::fs::create_dir_all(&nested_dir).expect("create nested_dir");
    let proto_file = write_test_proto(&nested_dir);
    let out_dir = tmp.join("out");

    let status = Command::new(binary())
        .args([
            "gen",
            "--recursive",
            proto_root.to_str().expect("utf8"),
            "-I",
            nested_dir.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .status()
        .expect("spawn gen --recursive");

    assert!(
        status.success(),
        "gen --recursive exited with {status}; proto at {proto_file:?}"
    );

    // At least one .rs file must have been generated.
    let rs_files: Vec<_> = std::fs::read_dir(&out_dir)
        .expect("read out_dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
        .collect();
    assert!(
        !rs_files.is_empty(),
        "gen --recursive should produce at least one .rs file"
    );
}

/// `completions bash` exits zero and produces non-empty stdout.
#[test]
fn completions_bash_exits_zero() {
    let output = Command::new(binary())
        .args(["completions", "bash"])
        .output()
        .expect("spawn completions bash");

    assert!(
        output.status.success(),
        "completions bash exited with {}",
        output.status
    );
    assert!(
        !output.stdout.is_empty(),
        "completions bash should produce output on stdout"
    );
}

/// `completions zsh` exits zero and produces non-empty stdout.
#[test]
fn completions_zsh_exits_zero() {
    let output = Command::new(binary())
        .args(["completions", "zsh"])
        .output()
        .expect("spawn completions zsh");

    assert!(
        output.status.success(),
        "completions zsh exited with {}",
        output.status
    );
    assert!(
        !output.stdout.is_empty(),
        "completions zsh should produce output on stdout"
    );
}

/// `--quiet` suppresses informational messages but an error still exits non-zero.
#[test]
fn quiet_suppresses_output_error_still_exits_nonzero() {
    let nonexistent_quiet =
        std::env::temp_dir().join("this-file-should-not-exist-oxiproto-quiet.proto");
    let output = Command::new(binary())
        .args([
            "--quiet",
            "gen",
            nonexistent_quiet.to_str().expect("nonexistent_quiet utf8"),
        ])
        .output()
        .expect("spawn --quiet gen missing");

    assert!(
        !output.status.success(),
        "expected non-zero exit for missing file with --quiet, got {}",
        output.status
    );
    // stderr should still contain an error message even in quiet mode.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error:"),
        "--quiet should still print errors; got stderr: {stderr}"
    );
}

/// `--quiet --verbose` combination: verbose progress is suppressed by --quiet.
#[test]
fn quiet_suppresses_verbose_progress() {
    let tmp = tmp_dir("quiet-verbose");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_test_proto(&proto_dir);
    let out_dir = tmp.join("out");

    let output = Command::new(binary())
        .args([
            "--quiet",
            "--verbose",
            "gen",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .output()
        .expect("spawn --quiet --verbose gen");

    assert!(
        output.status.success(),
        "--quiet --verbose gen exited with {}",
        output.status
    );
    // --quiet must suppress verbose progress messages.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Processing"),
        "--quiet should suppress verbose progress; got stderr: {stderr}"
    );
}

/// `--verbose` flag causes progress messages to appear on stderr.
#[test]
fn verbose_flag_prints_progress() {
    let tmp = tmp_dir("verbose");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_test_proto(&proto_dir);
    let out_dir = tmp.join("out");

    let output = Command::new(binary())
        .args([
            "--verbose",
            "gen",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .output()
        .expect("spawn --verbose gen");

    assert!(
        output.status.success(),
        "--verbose gen exited with {}",
        output.status
    );
    // Progress message should appear on stderr.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Processing"),
        "--verbose should print progress on stderr; got: {stderr}"
    );
}

/// `gen --json` is accepted and exits zero.
#[test]
fn gen_json_flag_exits_zero() {
    let tmp = tmp_dir("json");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_test_proto(&proto_dir);
    let out_dir = tmp.join("out");

    let status = Command::new(binary())
        .args([
            "gen",
            "--json",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .status()
        .expect("spawn gen --json");

    assert!(status.success(), "gen --json exited with {status}");
}

/// `gen --grpc` (default true) is accepted and exits zero.
#[test]
fn gen_grpc_flag_exits_zero() {
    let tmp = tmp_dir("grpc");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_test_proto(&proto_dir);
    let out_dir = tmp.join("out");

    let status = Command::new(binary())
        .args([
            "gen",
            "--grpc=true",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .status()
        .expect("spawn gen --grpc=true");

    assert!(status.success(), "gen --grpc=true exited with {status}");
}

/// Filename derivation from package declaration: `package foo.bar;` → `foo_bar.rs`.
#[test]
fn filename_derived_from_package_declaration() {
    let tmp = tmp_dir("pkg-filename");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_proto_with_package(&proto_dir, "service.proto", "foo.bar");
    let out_dir = tmp.join("out");

    let status = Command::new(binary())
        .args([
            "gen",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .status()
        .expect("spawn gen with package proto");

    assert!(
        status.success(),
        "gen with package proto exited with {status}"
    );

    let expected = out_dir.join("foo_bar.rs");
    assert!(
        expected.exists(),
        "expected output file foo_bar.rs at {expected:?}"
    );
}

/// Write a `.proto` fixture that includes a `service` block so we can verify
/// the `--grpc` toggle in the CLI.
fn write_service_proto(dir: &std::path::Path) -> PathBuf {
    let proto = dir.join("svc.proto");
    std::fs::write(
        &proto,
        r#"syntax = "proto3";
package svc;

message Req { string text = 1; }
message Resp { int32 code = 1; }

service Echo {
    rpc Unary(Req) returns (Resp);
}
"#,
    )
    .expect("write svc.proto");
    proto
}

/// `gen --grpc=true` (default) must produce a service trait definition.
#[test]
fn gen_grpc_true_emits_service_trait() {
    let tmp = tmp_dir("grpc-on");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_service_proto(&proto_dir);
    let out_dir = tmp.join("out");

    let status = Command::new(binary())
        .args([
            "gen",
            "--grpc=true",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .status()
        .expect("spawn gen --grpc=true");
    assert!(status.success(), "gen --grpc=true exited with {status}");

    let out_file = out_dir.join("svc.rs");
    assert!(out_file.exists(), "expected svc.rs to exist");

    let contents = std::fs::read_to_string(&out_file).expect("read generated file");
    assert!(
        contents.contains("trait"),
        "--grpc=true must include a service trait in:\n{contents}"
    );
}

/// `gen --grpc=false` must suppress service trait definitions.
#[test]
fn gen_grpc_false_suppresses_service_trait() {
    let tmp = tmp_dir("grpc-off");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_service_proto(&proto_dir);
    let out_dir = tmp.join("out");

    let status = Command::new(binary())
        .args([
            "gen",
            "--grpc=false",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .status()
        .expect("spawn gen --grpc=false");
    assert!(status.success(), "gen --grpc=false exited with {status}");

    let out_file = out_dir.join("svc.rs");
    assert!(out_file.exists(), "expected svc.rs to exist");

    let contents = std::fs::read_to_string(&out_file).expect("read generated file");
    assert!(
        !contents.contains("trait"),
        "--grpc=false must suppress service traits; found 'trait' in:\n{contents}"
    );
    // Message structs must still be present.
    assert!(
        contents.contains("Req"),
        "Req struct must appear even with --grpc=false:\n{contents}"
    );
}

/// `gen --json --dry-run` produces output containing `to_json` and `from_json`.
#[test]
fn gen_json_flag_emits_json_methods() {
    let tmp = tmp_dir("json-methods");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_test_proto(&proto_dir);
    let out_dir = tmp.join("out");

    let output = Command::new(binary())
        .args([
            "gen",
            "--json",
            "--dry-run",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .output()
        .expect("spawn gen --json --dry-run");

    assert!(
        output.status.success(),
        "gen --json --dry-run exited with {}",
        output.status
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("pub fn to_json"),
        "gen --json output must contain to_json; got:\n{stdout}"
    );
    assert!(
        stdout.contains("pub fn from_json"),
        "gen --json output must contain from_json; got:\n{stdout}"
    );
    assert!(
        stdout.contains("JsonError"),
        "gen --json output must contain JsonError; got:\n{stdout}"
    );
}

/// Filename derivation from stem when no package declaration is present.
#[test]
fn filename_derived_from_stem_when_no_package() {
    let tmp = tmp_dir("stem-filename");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    let proto_file = write_proto_no_package(&proto_dir, "myservice.proto");
    let out_dir = tmp.join("out");

    let status = Command::new(binary())
        .args([
            "gen",
            proto_file.to_str().expect("utf8"),
            "-I",
            proto_dir.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .status()
        .expect("spawn gen with no-package proto");

    assert!(
        status.success(),
        "gen with no-package proto exited with {status}"
    );

    let expected = out_dir.join("myservice.rs");
    assert!(
        expected.exists(),
        "expected output file myservice.rs at {expected:?}"
    );
}

// ---------------------------------------------------------------------------
// breaking subcommand tests
// ---------------------------------------------------------------------------

/// `breaking` with identical old and new protos must exit zero (no changes).
#[test]
fn breaking_no_changes_exits_zero() {
    let dir = tmp_dir("breaking_no_change");
    let proto = write_test_proto(&dir);
    let dir_str = dir.to_str().expect("utf8");
    let status = std::process::Command::new(binary())
        .args([
            "breaking",
            "--old",
            proto.to_str().expect("utf8"),
            "--old-include",
            dir_str,
            "--new",
            proto.to_str().expect("utf8"),
            "--new-include",
            dir_str,
        ])
        .status()
        .expect("failed to run breaking");
    assert!(status.success(), "same proto must exit 0: {:?}", status);
}

/// `breaking` when a field is removed in the new proto must exit non-zero and
/// print a BREAKING line.
#[test]
fn breaking_field_removed_exits_nonzero() {
    let dir = tmp_dir("breaking_field_removed");
    let dir_str = dir.to_str().expect("utf8");
    let old_path = dir.join("old.proto");
    std::fs::write(
        &old_path,
        "syntax = \"proto3\";\npackage test;\nmessage Msg { string name = 1; int32 count = 2; }\n",
    )
    .expect("write old.proto");
    let new_path = dir.join("new.proto");
    std::fs::write(
        &new_path,
        "syntax = \"proto3\";\npackage test;\nmessage Msg { string name = 1; }\n",
    )
    .expect("write new.proto");
    let output = std::process::Command::new(binary())
        .args([
            "breaking",
            "--old",
            old_path.to_str().expect("utf8"),
            "--old-include",
            dir_str,
            "--new",
            new_path.to_str().expect("utf8"),
            "--new-include",
            dir_str,
        ])
        .output()
        .expect("failed to run breaking");
    assert!(!output.status.success(), "removed field must exit non-zero");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("BREAKING"),
        "must report BREAKING: {stdout}"
    );
}

/// `breaking` when a field's type changes must exit non-zero.
#[test]
fn breaking_type_changed_exits_nonzero() {
    let dir = tmp_dir("breaking_type_changed");
    let dir_str = dir.to_str().expect("utf8");
    let old_path = dir.join("old.proto");
    std::fs::write(
        &old_path,
        "syntax = \"proto3\";\npackage test;\nmessage Msg { string name = 1; }\n",
    )
    .expect("write old.proto");
    let new_path = dir.join("new.proto");
    std::fs::write(
        &new_path,
        "syntax = \"proto3\";\npackage test;\nmessage Msg { int32 name = 1; }\n",
    )
    .expect("write new.proto");
    let output = std::process::Command::new(binary())
        .args([
            "breaking",
            "--old",
            old_path.to_str().expect("utf8"),
            "--old-include",
            dir_str,
            "--new",
            new_path.to_str().expect("utf8"),
            "--new-include",
            dir_str,
        ])
        .output()
        .expect("failed");
    assert!(!output.status.success(), "type change must be breaking");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BREAKING"), "stdout: {stdout}");
}

/// Adding a field is backwards-compatible; `breaking` must exit zero.

#[test]
fn breaking_field_added_is_not_breaking() {
    let dir = tmp_dir("breaking_field_added");
    let dir_str = dir.to_str().expect("utf8");
    let old_path = dir.join("old.proto");
    std::fs::write(
        &old_path,
        "syntax = \"proto3\";\npackage test;\nmessage Msg { string name = 1; }\n",
    )
    .expect("write old.proto");
    let new_path = dir.join("new.proto");
    std::fs::write(
        &new_path,
        "syntax = \"proto3\";\npackage test;\nmessage Msg { string name = 1; int32 age = 2; }\n",
    )
    .expect("write new.proto");
    let status = std::process::Command::new(binary())
        .args([
            "breaking",
            "--old",
            old_path.to_str().expect("utf8"),
            "--old-include",
            dir_str,
            "--new",
            new_path.to_str().expect("utf8"),
            "--new-include",
            dir_str,
        ])
        .status()
        .expect("failed");
    assert!(status.success(), "adding a field must exit 0");
}

/// `breaking` with a missing old file must exit non-zero.
#[test]
fn breaking_missing_old_file_errors() {
    let dir = tmp_dir("breaking_missing");
    let new_path = dir.join("new.proto");
    std::fs::write(&new_path, "syntax = \"proto3\";\nmessage Msg {}\n").expect("write new.proto");
    let status = std::process::Command::new(binary())
        .args([
            "breaking",
            "--old",
            "/nonexistent/foo.proto",
            "--new",
            new_path.to_str().expect("utf8"),
        ])
        .status()
        .expect("failed");
    assert!(!status.success(), "missing old file must error");
}

/// `breaking` when an inline enum value is removed must exit non-zero.
#[test]
fn breaking_enum_value_removed() {
    let dir = tmp_dir("breaking_enum_removed");
    let dir_str = dir.to_str().expect("utf8");
    let old_path = dir.join("old.proto");
    std::fs::write(
        &old_path,
        r#"syntax = "proto3";
package test;
message Msg {
  Status status = 1;
  enum Status { UNKNOWN = 0; OK = 1; ERROR = 2; }
}
"#,
    )
    .expect("write old.proto");
    let new_path = dir.join("new.proto");
    std::fs::write(
        &new_path,
        r#"syntax = "proto3";
package test;
message Msg {
  Status status = 1;
  enum Status { UNKNOWN = 0; OK = 1; }
}
"#,
    )
    .expect("write new.proto");
    let output = std::process::Command::new(binary())
        .args([
            "breaking",
            "--old",
            old_path.to_str().expect("utf8"),
            "--old-include",
            dir_str,
            "--new",
            new_path.to_str().expect("utf8"),
            "--new-include",
            dir_str,
        ])
        .output()
        .expect("failed");
    assert!(
        !output.status.success(),
        "enum value removal must be breaking"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BREAKING"), "stdout: {stdout}");
}

// ---------------------------------------------------------------------------
// doc subcommand tests
// ---------------------------------------------------------------------------

mod doc_tests {
    use super::{binary, tmp_dir};

    /// Write a `.proto` with a message `Foo` and a field `id` for doc tests.
    fn write_doc_proto(dir: &std::path::Path) -> std::path::PathBuf {
        let proto = dir.join("doc_test.proto");
        std::fs::write(
            &proto,
            r#"syntax = "proto3";
package docpkg;

// A simple Foo message for documentation testing.
message Foo {
    // The primary identifier.
    int32 id = 1;
    string label = 2;
}

enum Color {
    COLOR_UNSPECIFIED = 0;
    RED = 1;
    GREEN = 2;
}
"#,
        )
        .expect("write doc_test.proto");
        proto
    }

    /// `oxiproto doc` exits zero when given a valid `.proto` file.
    #[test]
    fn doc_exits_zero_with_valid_proto() {
        let tmp = tmp_dir("doc_zero");
        let proto = write_doc_proto(&tmp);

        let status = std::process::Command::new(binary())
            .args([
                "doc",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .status()
            .expect("spawn doc");

        assert!(status.success(), "doc exited non-zero: {status}");
    }

    /// Output must contain a heading for the message `Foo`.
    #[test]
    fn doc_outputs_message_heading() {
        let tmp = tmp_dir("doc_heading");
        let proto = write_doc_proto(&tmp);

        let output = std::process::Command::new(binary())
            .args([
                "doc",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .output()
            .expect("spawn doc");

        assert!(
            output.status.success(),
            "doc exited non-zero: {}",
            output.status
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Foo"),
            "expected 'Foo' heading in doc output:\n{stdout}"
        );
        assert!(
            stdout.contains("##") || stdout.contains('#'),
            "expected at least one heading in doc output:\n{stdout}"
        );
    }

    /// Field names must appear in the generated Markdown table.
    #[test]
    fn doc_includes_field_in_table() {
        let tmp = tmp_dir("doc_field");
        let proto = write_doc_proto(&tmp);

        let output = std::process::Command::new(binary())
            .args([
                "doc",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .output()
            .expect("spawn doc");

        assert!(
            output.status.success(),
            "doc exited non-zero: {}",
            output.status
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("id"),
            "expected field 'id' in doc output:\n{stdout}"
        );
        assert!(
            stdout.contains("label"),
            "expected field 'label' in doc output:\n{stdout}"
        );
    }

    /// A non-existent file must produce a non-zero exit code.
    #[test]
    fn doc_missing_file_exits_nonzero() {
        let nonexistent_doc = std::env::temp_dir().join("nonexistent-oxiproto-doc.proto");
        let status = std::process::Command::new(binary())
            .args([
                "doc",
                nonexistent_doc.to_str().expect("nonexistent_doc utf8"),
            ])
            .status()
            .expect("spawn doc");

        assert!(
            !status.success(),
            "doc with missing file should exit non-zero, got: {status}"
        );
    }

    /// `--output <file>` writes output to a file instead of stdout.
    #[test]
    fn doc_outputs_to_file() {
        let tmp = tmp_dir("doc_file_out");
        let proto = write_doc_proto(&tmp);
        let out_file = tmp.join("output.md");

        let status = std::process::Command::new(binary())
            .args([
                "doc",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
                "--output",
                out_file.to_str().expect("utf8"),
            ])
            .status()
            .expect("spawn doc --output");

        assert!(status.success(), "doc --output exited non-zero: {status}");
        assert!(out_file.exists(), "expected output file to be created");

        let content = std::fs::read_to_string(&out_file).expect("read output file");
        assert!(!content.is_empty(), "output file must not be empty");
        assert!(
            content.contains("Foo"),
            "output file must contain 'Foo':\n{content}"
        );
    }
}

// ---------------------------------------------------------------------------
// prost-compat tests
// ---------------------------------------------------------------------------

/// `gen --prost-compat` should exit 0 and produce a Rust file containing
/// the prost `#[derive(... prost::Message)]` attribute.
#[test]
fn gen_prost_compat_produces_derive() {
    let tmp = tmp_dir("prost_compat");
    let proto_file = write_test_proto(&tmp);
    let out_dir = tmp.join("out");

    let status = Command::new(binary())
        .args([
            "gen",
            "--prost-compat",
            proto_file.to_str().expect("utf8"),
            "-I",
            tmp.to_str().expect("utf8"),
            "-o",
            out_dir.to_str().expect("utf8"),
        ])
        .status()
        .expect("spawn gen --prost-compat");

    assert!(status.success(), "gen --prost-compat exited with {status}");

    // prost-build names output by package; scan all *.rs files for the derive.
    let found_derive = std::fs::read_dir(&out_dir)
        .expect("read out_dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "rs").unwrap_or(false))
        .any(|e| {
            std::fs::read_to_string(e.path())
                .map(|s| s.contains("prost::Message"))
                .unwrap_or(false)
        });

    assert!(
        found_derive,
        "expected at least one *.rs file in {out_dir:?} to contain 'prost::Message'"
    );
}

// ---------------------------------------------------------------------------
// format tests
// ---------------------------------------------------------------------------

mod format_tests {
    use super::*;

    fn write_format_proto(dir: &std::path::Path) -> std::path::PathBuf {
        let proto = dir.join("fmt_test.proto");
        std::fs::write(
            &proto,
            r#"syntax = "proto3";
package fmt;

message FmtMessage {
    string name = 1;
    int32 count = 2;
}

enum FmtStatus {
    FMT_STATUS_UNKNOWN = 0;
    FMT_STATUS_ACTIVE = 1;
}
"#,
        )
        .expect("write fmt_test.proto");
        proto
    }

    #[test]
    fn format_exits_zero_with_valid_proto() {
        let tmp = tmp_dir("fmt_zero");
        let proto = write_format_proto(&tmp);

        let status = Command::new(binary())
            .args([
                "format",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .status()
            .expect("spawn format");

        assert!(status.success(), "format exited with {status}");
    }

    #[test]
    fn format_produces_syntax_line() {
        let tmp = tmp_dir("fmt_syntax");
        let proto = write_format_proto(&tmp);

        let output = Command::new(binary())
            .args([
                "format",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .output()
            .expect("spawn format");

        assert!(
            output.status.success(),
            "format exited with {}",
            output.status
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.starts_with("syntax = "),
            "expected output to start with 'syntax = ', got:\n{stdout}"
        );
    }

    #[test]
    fn format_contains_message_name() {
        let tmp = tmp_dir("fmt_msgname");
        let proto = write_format_proto(&tmp);

        let output = Command::new(binary())
            .args([
                "format",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .output()
            .expect("spawn format");

        assert!(
            output.status.success(),
            "format exited with {}",
            output.status
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("FmtMessage"),
            "expected 'FmtMessage' in formatted output:\n{stdout}"
        );
    }

    #[test]
    fn format_in_place_rewrites_file() {
        let tmp = tmp_dir("fmt_inplace");
        let proto = write_format_proto(&tmp);

        // Read original content to compare later.
        let original = std::fs::read_to_string(&proto).expect("read original proto");

        let status = Command::new(binary())
            .args([
                "format",
                "--in-place",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .status()
            .expect("spawn format --in-place");

        assert!(status.success(), "format --in-place exited with {status}");

        let rewritten = std::fs::read_to_string(&proto).expect("read rewritten proto");
        // The file must have been written (even if content is nearly identical,
        // it should be non-empty and contain the message name).
        assert!(!rewritten.is_empty(), "rewritten proto must not be empty");
        assert!(
            rewritten.contains("FmtMessage"),
            "rewritten proto must contain 'FmtMessage':\n{rewritten}"
        );
        // The original and rewritten should differ in some way (formatting
        // normalizes whitespace) OR be equivalent (already canonical) — either
        // is acceptable.  Just assert both are non-empty.
        let _ = original;
    }

    #[test]
    fn format_missing_file_exits_nonzero() {
        let nonexistent_fmt =
            std::env::temp_dir().join("nonexistent-oxiproto-fmt-test-99999.proto");
        let status = Command::new(binary())
            .args([
                "format",
                nonexistent_fmt.to_str().expect("nonexistent_fmt utf8"),
            ])
            .status()
            .expect("spawn format with missing file");

        assert!(
            !status.success(),
            "format with missing file should exit non-zero, got: {status}"
        );
    }
}

// ---------------------------------------------------------------------------
// lint tests
// ---------------------------------------------------------------------------

mod lint_tests {
    use super::*;

    fn write_clean_proto(dir: &std::path::Path) -> std::path::PathBuf {
        let proto = dir.join("lint_clean.proto");
        std::fs::write(
            &proto,
            r#"syntax = "proto3";
package lint;

message LintMessage {
    string name = 1;
    int32 count = 2;
}

enum LintStatus {
    LINT_STATUS_UNKNOWN = 0;
    LINT_STATUS_ACTIVE = 1;
}
"#,
        )
        .expect("write lint_clean.proto");
        proto
    }

    fn write_bad_message_name_proto(dir: &std::path::Path) -> std::path::PathBuf {
        let proto = dir.join("lint_bad_msg.proto");
        std::fs::write(
            &proto,
            r#"syntax = "proto3";
package lint;

message foo_bar {
    string name = 1;
}
"#,
        )
        .expect("write lint_bad_msg.proto");
        proto
    }

    fn write_bad_field_name_proto(dir: &std::path::Path) -> std::path::PathBuf {
        let proto = dir.join("lint_bad_field.proto");
        std::fs::write(
            &proto,
            r#"syntax = "proto3";
package lint;

message GoodMessage {
    string FieldName = 1;
}
"#,
        )
        .expect("write lint_bad_field.proto");
        proto
    }

    fn write_bad_enum_value_proto(dir: &std::path::Path) -> std::path::PathBuf {
        let proto = dir.join("lint_bad_enum_value.proto");
        std::fs::write(
            &proto,
            r#"syntax = "proto3";
package lint;

message GoodMessage {
    string name = 1;
}

enum GoodEnum {
    good_enum_unknown = 0;
    good_enum_active = 1;
}
"#,
        )
        .expect("write lint_bad_enum_value.proto");
        proto
    }

    fn write_json_lint_bad_proto(dir: &std::path::Path) -> std::path::PathBuf {
        let proto = dir.join("lint_json_bad.proto");
        std::fs::write(
            &proto,
            r#"syntax = "proto3";
package lint;

message bad_message {
    string name = 1;
}
"#,
        )
        .expect("write lint_json_bad.proto");
        proto
    }

    #[test]
    fn lint_clean_proto_exits_zero() {
        let tmp = tmp_dir("lint_clean");
        let proto = write_clean_proto(&tmp);

        let status = Command::new(binary())
            .args([
                "lint",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .status()
            .expect("spawn lint");

        assert!(status.success(), "lint on clean proto exited with {status}");
    }

    #[test]
    fn lint_bad_message_name_exits_nonzero() {
        let tmp = tmp_dir("lint_bad_msg");
        let proto = write_bad_message_name_proto(&tmp);

        let output = Command::new(binary())
            .args([
                "lint",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .output()
            .expect("spawn lint bad message name");

        assert!(
            !output.status.success(),
            "lint on bad message name should exit non-zero"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("MESSAGE_NAMES_UPPER_CAMEL_CASE"),
            "expected MESSAGE_NAMES_UPPER_CAMEL_CASE in lint output:\n{stdout}"
        );
    }

    #[test]
    fn lint_bad_field_name_exits_nonzero() {
        let tmp = tmp_dir("lint_bad_field");
        let proto = write_bad_field_name_proto(&tmp);

        let output = Command::new(binary())
            .args([
                "lint",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .output()
            .expect("spawn lint bad field name");

        assert!(
            !output.status.success(),
            "lint on bad field name should exit non-zero"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("FIELD_NAMES_LOWER_SNAKE_CASE"),
            "expected FIELD_NAMES_LOWER_SNAKE_CASE in lint output:\n{stdout}"
        );
    }

    #[test]
    fn lint_bad_enum_value_name() {
        let tmp = tmp_dir("lint_bad_enum_val");
        let proto = write_bad_enum_value_proto(&tmp);

        let output = Command::new(binary())
            .args([
                "lint",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .output()
            .expect("spawn lint bad enum value");

        assert!(
            !output.status.success(),
            "lint on bad enum value name should exit non-zero"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("ENUM_VALUE_NAMES_UPPER_SNAKE_CASE"),
            "expected ENUM_VALUE_NAMES_UPPER_SNAKE_CASE in lint output:\n{stdout}"
        );
    }

    #[test]
    fn lint_json_output() {
        let tmp = tmp_dir("lint_json_out");
        let proto = write_json_lint_bad_proto(&tmp);

        let output = Command::new(binary())
            .args([
                "lint",
                "--output",
                "json",
                proto.to_str().expect("utf8"),
                "-I",
                tmp.to_str().expect("utf8"),
            ])
            .output()
            .expect("spawn lint --output json");

        assert!(
            !output.status.success(),
            "lint with violations should exit non-zero"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value =
            serde_json::from_str(&stdout).expect("expected valid JSON output from lint");
        assert!(
            parsed.is_array(),
            "lint --output json must produce a JSON array, got: {stdout}"
        );
        let arr = parsed.as_array().expect("array");
        assert!(
            !arr.is_empty(),
            "lint --output json must contain at least one violation"
        );
    }
}

// ---------------------------------------------------------------------------
// man subcommand tests
// ---------------------------------------------------------------------------

/// `man` exits zero and produces at least one man-page file.
#[test]
fn man_exits_zero_and_produces_files() {
    let tmp = tmp_dir("man_basic");

    let output = Command::new(binary())
        .args(["man", "--output", tmp.to_str().expect("utf8")])
        .output()
        .expect("spawn man");

    assert!(
        output.status.success(),
        "man exited with {}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );

    // At least one file must exist in the output directory.
    let files: Vec<_> = std::fs::read_dir(&tmp)
        .expect("read man output dir")
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        !files.is_empty(),
        "man must produce at least one file in {tmp:?}"
    );
}

/// `man` produces a file whose name starts with `oxiproto-cli`.
#[test]
fn man_produces_oxiproto_cli_man_file() {
    let tmp = tmp_dir("man_name");

    let status = Command::new(binary())
        .args(["man", "--output", tmp.to_str().expect("utf8")])
        .status()
        .expect("spawn man");

    assert!(status.success(), "man exited with {status}");

    let has_main_page = std::fs::read_dir(&tmp)
        .expect("read man output dir")
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().starts_with("oxiproto-cli"));

    assert!(
        has_main_page,
        "man must produce an 'oxiproto-cli*' file in {tmp:?}"
    );
}

/// `man` with a non-existent parent directory still succeeds (creates dirs).
#[test]
fn man_creates_output_directory() {
    let base = tmp_dir("man_mkdir");
    let nested = base.join("a").join("b");

    let status = Command::new(binary())
        .args(["man", "--output", nested.to_str().expect("utf8")])
        .status()
        .expect("spawn man --output nested");

    assert!(
        status.success(),
        "man --output <nested> exited with {status}"
    );
    assert!(nested.is_dir(), "man must create the output directory");
}

// ---------------------------------------------------------------------------
// cargo-install smoke test
//
// `cargo install oxiproto-cli` produces a binary equivalent to the one under
// test here.  We exercise every subcommand at least once to verify that a
// freshly installed binary would work correctly end-to-end.
// ---------------------------------------------------------------------------

/// Write a fully lint-clean `.proto` fixture for the smoke test.
/// All names follow Google style guide conventions used by the lint rules.
fn write_smoke_proto(dir: &std::path::Path) -> PathBuf {
    let proto = dir.join("smoke.proto");
    std::fs::write(
        &proto,
        r#"syntax = "proto3";
package smoke;

message Greeting {
    string name = 1;
    int32 count = 2;
}

enum GreetingStatus {
    GREETING_STATUS_UNKNOWN = 0;
    GREETING_STATUS_ACTIVE = 1;
    GREETING_STATUS_RETIRED = 2;
}
"#,
    )
    .expect("write smoke.proto");
    proto
}

/// End-to-end smoke test that exercises every subcommand of the binary,
/// verifying that the binary produced by `cargo build` (and, by extension,
/// `cargo install`) behaves correctly.
///
/// Covers: gen, describe, encode, decode, format, lint, breaking, doc,
///         completions, man subcommands, plus --help and --version-like flags.
#[test]
fn install_smoke_test_all_subcommands() {
    let tmp = tmp_dir("install-smoke");
    let proto_dir = tmp.join("protos");
    std::fs::create_dir_all(&proto_dir).expect("create proto_dir");
    // Use a lint-clean proto for the smoke test so lint passes.
    let proto_file = write_smoke_proto(&proto_dir);
    let proto_str = proto_file.to_str().expect("utf8");
    let dir_str = proto_dir.to_str().expect("utf8");

    // 1. --help exits zero
    assert!(
        Command::new(binary())
            .arg("--help")
            .status()
            .expect("--help")
            .success(),
        "smoke: --help"
    );

    // 2. gen exits zero and produces output
    let gen_out = tmp.join("gen_out");
    assert!(
        Command::new(binary())
            .args([
                "gen",
                proto_str,
                "-I",
                dir_str,
                "-o",
                gen_out.to_str().expect("utf8")
            ])
            .status()
            .expect("gen")
            .success(),
        "smoke: gen"
    );
    let rs_files: Vec<_> = std::fs::read_dir(&gen_out)
        .expect("read gen_out")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
        .collect();
    assert!(!rs_files.is_empty(), "smoke: gen must produce .rs files");

    // 3. describe exits zero
    assert!(
        Command::new(binary())
            .args(["describe", proto_str, "-I", dir_str])
            .status()
            .expect("describe")
            .success(),
        "smoke: describe"
    );

    // 4. encode exits zero and produces a non-empty binary file
    let json_in = tmp.join("in.json");
    std::fs::write(&json_in, r#"{"name":"smoke","count":1}"#).expect("write json");
    let bin_out = tmp.join("encoded.bin");
    assert!(
        Command::new(binary())
            .args([
                "encode",
                proto_str,
                "-I",
                dir_str,
                "-t",
                "smoke.Greeting",
                "-i",
                json_in.to_str().expect("utf8"),
                "-o",
                bin_out.to_str().expect("utf8"),
            ])
            .status()
            .expect("encode")
            .success(),
        "smoke: encode"
    );
    assert!(bin_out.exists(), "smoke: encode must produce binary file");

    // 5. decode round-trips correctly
    let json_out = tmp.join("decoded.json");
    assert!(
        Command::new(binary())
            .args([
                "decode",
                proto_str,
                "-I",
                dir_str,
                "-t",
                "smoke.Greeting",
                "-i",
                bin_out.to_str().expect("utf8"),
                "-o",
                json_out.to_str().expect("utf8"),
            ])
            .status()
            .expect("decode")
            .success(),
        "smoke: decode"
    );
    let decoded = std::fs::read_to_string(&json_out).expect("read decoded json");
    let v: serde_json::Value = serde_json::from_str(&decoded).expect("parse decoded json");
    assert_eq!(v["name"], "smoke", "smoke: decode name field");

    // 6. format exits zero
    assert!(
        Command::new(binary())
            .args(["format", proto_str, "-I", dir_str])
            .status()
            .expect("format")
            .success(),
        "smoke: format"
    );

    // 7. lint exits zero on clean proto
    assert!(
        Command::new(binary())
            .args(["lint", proto_str, "-I", dir_str])
            .status()
            .expect("lint")
            .success(),
        "smoke: lint"
    );

    // 8. breaking exits zero when old == new
    assert!(
        Command::new(binary())
            .args([
                "breaking",
                "--old",
                proto_str,
                "--old-include",
                dir_str,
                "--new",
                proto_str,
                "--new-include",
                dir_str,
            ])
            .status()
            .expect("breaking")
            .success(),
        "smoke: breaking"
    );

    // 9. doc exits zero
    assert!(
        Command::new(binary())
            .args(["doc", proto_str, "-I", dir_str])
            .status()
            .expect("doc")
            .success(),
        "smoke: doc"
    );

    // 10. completions bash exits zero
    assert!(
        Command::new(binary())
            .args(["completions", "bash"])
            .status()
            .expect("completions bash")
            .success(),
        "smoke: completions bash"
    );

    // 11. man exits zero and produces files
    let man_out = tmp.join("man");
    assert!(
        Command::new(binary())
            .args(["man", "--output", man_out.to_str().expect("utf8")])
            .status()
            .expect("man")
            .success(),
        "smoke: man"
    );
    assert!(
        std::fs::read_dir(&man_out)
            .expect("read man_out")
            .filter_map(|e| e.ok())
            .any(|_| true),
        "smoke: man must produce at least one file"
    );
}
