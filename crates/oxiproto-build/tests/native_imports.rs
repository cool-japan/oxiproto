#![forbid(unsafe_code)]
#![cfg(feature = "native-parser")]

//! Multi-file import resolution tests for the native parser.
//!
//! Positive tests compile the same protos with both `compile_files_native`
//! and `compile_to_fds` (protox), then structurally compare the results.

use oxiproto_build::{compile_files_native, compile_to_fds};
use prost_types::{DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Temp dir helper with RAII cleanup
// ---------------------------------------------------------------------------

struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path =
            std::env::temp_dir().join(format!("oxiproto_native_imports_{prefix}_{pid}_{id}"));
        std::fs::create_dir_all(&path).expect("create temp dir");
        TempDir { path }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn write(&self, name: &str, content: &str) -> std::path::PathBuf {
        let p = self.path.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        std::fs::write(&p, content).expect("write temp file");
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------
// Structural comparison helpers (keyed by file name, not by index)
// ---------------------------------------------------------------------------

fn normalize_fds_map(fds: &mut FileDescriptorSet) -> HashMap<String, FileDescriptorProto> {
    let mut map = HashMap::new();
    for file in &mut fds.file {
        // Strip source_code_info and file-level options before comparison
        file.source_code_info = None;
        file.options = None;
        for msg in &mut file.message_type {
            normalize_message_options(msg);
        }
        let name = file.name.clone().unwrap_or_default();
        map.insert(name, file.clone());
    }
    map
}

fn normalize_message_options(msg: &mut DescriptorProto) {
    // Preserve map_entry option; clear other options
    let map_entry = msg.options.as_ref().and_then(|o| o.map_entry);
    if let Some(opts) = &mut msg.options {
        opts.uninterpreted_option.clear();
        if map_entry.is_none() {
            msg.options = None;
        }
    }
    for field in &mut msg.field {
        field.options = None;
    }
    for nested in &mut msg.nested_type {
        normalize_message_options(nested);
    }
}

fn assert_field_eq(native: &FieldDescriptorProto, protox: &FieldDescriptorProto, ctx: &str) {
    assert_eq!(native.name, protox.name, "{ctx}: field name mismatch");
    assert_eq!(
        native.number, protox.number,
        "{ctx}: field number mismatch for {:?}",
        native.name
    );
    assert_eq!(
        native.r#type, protox.r#type,
        "{ctx}: field type mismatch for {:?}",
        native.name
    );
    assert_eq!(
        native.type_name, protox.type_name,
        "{ctx}: field type_name mismatch for {:?}",
        native.name
    );
    assert_eq!(
        native.label, protox.label,
        "{ctx}: field label mismatch for {:?}",
        native.name
    );
    assert_eq!(
        native.proto3_optional, protox.proto3_optional,
        "{ctx}: proto3_optional mismatch for {:?}",
        native.name
    );
    assert_eq!(
        native.json_name, protox.json_name,
        "{ctx}: json_name mismatch for {:?}",
        native.name
    );
    assert_eq!(
        native.oneof_index, protox.oneof_index,
        "{ctx}: oneof_index mismatch for {:?}",
        native.name
    );
}

fn assert_message_eq(native: &DescriptorProto, protox: &DescriptorProto, ctx: &str) {
    assert_eq!(native.name, protox.name, "{ctx}: message name mismatch");
    let msg_ctx = format!("{ctx}/{}", native.name.as_deref().unwrap_or("?"));
    assert_eq!(
        native.field.len(),
        protox.field.len(),
        "{msg_ctx}: field count mismatch — native={:?} protox={:?}",
        native.field.iter().map(|f| &f.name).collect::<Vec<_>>(),
        protox.field.iter().map(|f| &f.name).collect::<Vec<_>>(),
    );
    for (nf, pf) in native.field.iter().zip(protox.field.iter()) {
        assert_field_eq(nf, pf, &msg_ctx);
    }
    assert_eq!(
        native.oneof_decl.len(),
        protox.oneof_decl.len(),
        "{msg_ctx}: oneof_decl count mismatch"
    );
    for (no, po) in native.oneof_decl.iter().zip(protox.oneof_decl.iter()) {
        assert_eq!(no.name, po.name, "{msg_ctx}: oneof name mismatch");
    }
    assert_eq!(
        native.nested_type.len(),
        protox.nested_type.len(),
        "{msg_ctx}: nested_type count mismatch"
    );
    for (nm, pm) in native.nested_type.iter().zip(protox.nested_type.iter()) {
        assert_message_eq(nm, pm, &msg_ctx);
    }
    let native_map_entry = native.options.as_ref().and_then(|o| o.map_entry);
    let protox_map_entry = protox.options.as_ref().and_then(|o| o.map_entry);
    assert_eq!(
        native_map_entry, protox_map_entry,
        "{msg_ctx}: map_entry option mismatch"
    );
}

fn assert_file_structure_eq(
    native: &FileDescriptorProto,
    protox: &FileDescriptorProto,
    file_name: &str,
) {
    // Message types
    assert_eq!(
        native.message_type.len(),
        protox.message_type.len(),
        "{file_name}: message count mismatch"
    );
    for (nm, pm) in native.message_type.iter().zip(protox.message_type.iter()) {
        assert_message_eq(nm, pm, file_name);
    }
    // Enum types
    assert_eq!(
        native.enum_type.len(),
        protox.enum_type.len(),
        "{file_name}: enum count mismatch"
    );
    for (ne, pe) in native.enum_type.iter().zip(protox.enum_type.iter()) {
        assert_eq!(ne.name, pe.name, "{file_name}: enum name mismatch");
        assert_eq!(
            ne.value.len(),
            pe.value.len(),
            "{file_name}: enum value count mismatch in {:?}",
            ne.name
        );
        for (nev, pev) in ne.value.iter().zip(pe.value.iter()) {
            assert_eq!(nev.name, pev.name, "{file_name}: enum value name");
            assert_eq!(
                nev.number, pev.number,
                "{file_name}: enum value number for {:?}",
                nev.name
            );
        }
    }
    // Services
    assert_eq!(
        native.service.len(),
        protox.service.len(),
        "{file_name}: service count mismatch"
    );
    for (ns, ps) in native.service.iter().zip(protox.service.iter()) {
        assert_eq!(ns.name, ps.name, "{file_name}: service name mismatch");
        assert_eq!(
            ns.method.len(),
            ps.method.len(),
            "{file_name}: method count mismatch in {:?}",
            ns.name
        );
        for (nm, pm) in ns.method.iter().zip(ps.method.iter()) {
            assert_eq!(nm.name, pm.name, "{file_name}: method name");
            assert_eq!(
                nm.input_type, pm.input_type,
                "{file_name}: input_type for {:?}",
                nm.name
            );
            assert_eq!(
                nm.output_type, pm.output_type,
                "{file_name}: output_type for {:?}",
                nm.name
            );
            assert_eq!(
                nm.client_streaming, pm.client_streaming,
                "{file_name}: client_streaming"
            );
            assert_eq!(
                nm.server_streaming, pm.server_streaming,
                "{file_name}: server_streaming"
            );
        }
    }
}

/// Full positive-case comparison: key by name, check structure + dependency vectors.
fn run_multi_file_comparison(
    root_path: &std::path::Path,
    include_dir: &std::path::Path,
    test_name: &str,
) {
    let mut native_fds = compile_files_native(&[root_path], &[include_dir])
        .unwrap_or_else(|e| panic!("{test_name}: native compile failed: {e}"));

    let mut protox_fds = compile_to_fds(&[root_path], &[include_dir])
        .unwrap_or_else(|e| panic!("{test_name}: protox compile failed: {e}"));

    let native_map = normalize_fds_map(&mut native_fds);
    let protox_map = normalize_fds_map(&mut protox_fds);

    // Same key sets
    let mut native_keys: Vec<&String> = native_map.keys().collect();
    native_keys.sort();
    let mut protox_keys: Vec<&String> = protox_map.keys().collect();
    protox_keys.sort();
    assert_eq!(
        native_keys, protox_keys,
        "{test_name}: file name sets differ"
    );

    // Per-file structure comparison (skip WKTs)
    for name in &native_keys {
        if name.starts_with("google/protobuf/") {
            // WKT: presence only
            assert!(
                protox_map.contains_key(*name),
                "{test_name}: WKT '{name}' missing from protox FDS"
            );
            continue;
        }
        let native_file = &native_map[*name];
        let protox_file = &protox_map[*name];
        assert_eq!(
            native_file.syntax, protox_file.syntax,
            "{test_name}/{name}: syntax"
        );
        assert_eq!(
            native_file.package, protox_file.package,
            "{test_name}/{name}: package"
        );
        assert_eq!(
            native_file.dependency, protox_file.dependency,
            "{test_name}/{name}: dependency"
        );
        assert_eq!(
            native_file.public_dependency, protox_file.public_dependency,
            "{test_name}/{name}: public_dependency"
        );
        assert_eq!(
            native_file.weak_dependency, protox_file.weak_dependency,
            "{test_name}/{name}: weak_dependency"
        );
        assert_file_structure_eq(native_file, protox_file, &format!("{test_name}/{name}"));
    }
}

/// Build name→index map for a FDS file list.
fn name_to_index_map(fds: &FileDescriptorSet) -> HashMap<String, usize> {
    fds.file
        .iter()
        .enumerate()
        .filter_map(|(i, f)| f.name.clone().map(|n| (n, i)))
        .collect()
}

/// Assert topological validity: for each file F, all names in F.dependency
/// appear at an earlier index than F in the FDS.
fn assert_topo_valid(fds: &FileDescriptorSet, test_name: &str) {
    let idx_map = name_to_index_map(fds);
    for (i, file) in fds.file.iter().enumerate() {
        let file_name = file.name.as_deref().unwrap_or("?");
        for dep in &file.dependency {
            let dep_idx = idx_map
                .get(dep)
                .copied()
                .unwrap_or_else(|| panic!("{test_name}: dep '{dep}' of '{file_name}' not in FDS"));
            assert!(
                dep_idx < i,
                "{test_name}: topological violation: '{dep}' (index {dep_idx}) comes after '{file_name}' (index {i})"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Positive tests
// ---------------------------------------------------------------------------

#[test]
fn plain_cross_package_import() {
    let td = TempDir::new("cross_pkg");

    td.write(
        "dep.proto",
        r#"syntax = "proto3";
package dep;
message Dep { int32 value = 1; }
"#,
    );
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package app;
import "dep.proto";
message Root { dep.Dep dep_field = 1; }
"#,
    );

    run_multi_file_comparison(&root_path, td.path(), "cross_package_import");

    // Extra: topological order
    let fds = compile_files_native(&[&root_path], &[td.path()]).unwrap();
    assert_topo_valid(&fds, "cross_package_import");

    // dep.proto must be listed as dependency
    let name_idx = name_to_index_map(&fds);
    assert!(
        name_idx.contains_key("dep.proto"),
        "dep.proto must be in FDS"
    );
    assert!(
        name_idx.contains_key("root.proto"),
        "root.proto must be in FDS"
    );
    let root_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("root.proto"))
        .unwrap();
    assert_eq!(root_fdp.dependency, vec!["dep.proto".to_owned()]);
}

#[test]
fn import_public_re_export() {
    let td = TempDir::new("pub_re_export");

    td.write(
        "a.proto",
        r#"syntax = "proto3";
package ra;
message A { string name = 1; }
"#,
    );
    td.write(
        "b.proto",
        r#"syntax = "proto3";
package rb;
import public "a.proto";
message B { ra.A a_field = 1; }
"#,
    );
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package rc;
import "b.proto";
message Root { rb.B b_field = 1; ra.A a_field = 2; }
"#,
    );

    run_multi_file_comparison(&root_path, td.path(), "import_public_re_export");

    let fds = compile_files_native(&[&root_path], &[td.path()]).unwrap();
    assert_topo_valid(&fds, "import_public_re_export");

    // b.proto must have public_dependency=[0]
    let b_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("b.proto"))
        .unwrap();
    assert_eq!(b_fdp.public_dependency, vec![0i32]);
}

#[test]
fn import_weak() {
    let td = TempDir::new("weak_import");

    td.write(
        "dep.proto",
        r#"syntax = "proto3";
package dep;
message Dep { int32 x = 1; }
"#,
    );
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package root;
import weak "dep.proto";
message R { dep.Dep d = 1; }
"#,
    );

    run_multi_file_comparison(&root_path, td.path(), "import_weak");

    let fds = compile_files_native(&[&root_path], &[td.path()]).unwrap();
    assert_topo_valid(&fds, "import_weak");

    // root.proto must have weak_dependency=[0]
    let root_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("root.proto"))
        .unwrap();
    assert_eq!(root_fdp.weak_dependency, vec![0i32]);
}

#[test]
fn transitive_three_file_chain() {
    let td = TempDir::new("three_chain");

    td.write(
        "c.proto",
        r#"syntax = "proto3";
package c;
message C { int32 val = 1; }
"#,
    );
    td.write(
        "b.proto",
        r#"syntax = "proto3";
package b;
import "c.proto";
message B { c.C c_field = 1; }
"#,
    );
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package a;
import "b.proto";
message Root { b.B b_field = 1; }
"#,
    );

    run_multi_file_comparison(&root_path, td.path(), "three_chain");

    let fds = compile_files_native(&[&root_path], &[td.path()]).unwrap();
    assert_topo_valid(&fds, "three_chain");

    // All three files must be present
    let names: Vec<_> = fds.file.iter().filter_map(|f| f.name.as_deref()).collect();
    assert!(names.contains(&"c.proto"), "c.proto missing");
    assert!(names.contains(&"b.proto"), "b.proto missing");
    assert!(names.contains(&"root.proto"), "root.proto missing");
}

#[test]
fn imported_type_as_field_and_rpc() {
    let td = TempDir::new("field_and_rpc");

    td.write(
        "types.proto",
        r#"syntax = "proto3";
package types;
message Req { string query = 1; }
message Resp { string result = 1; }
"#,
    );
    let root_path = td.write(
        "svc.proto",
        r#"syntax = "proto3";
package svc;
import "types.proto";
message Wrap { types.Req inner = 1; }
service S { rpc Call(types.Req) returns (types.Resp); }
"#,
    );

    run_multi_file_comparison(&root_path, td.path(), "field_and_rpc");

    let fds = compile_files_native(&[&root_path], &[td.path()]).unwrap();
    assert_topo_valid(&fds, "field_and_rpc");

    let svc_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("svc.proto"))
        .unwrap();
    // Check that the Wrap message has the correct field type_name
    let wrap_msg = svc_fdp
        .message_type
        .iter()
        .find(|m| m.name.as_deref() == Some("Wrap"))
        .unwrap();
    let inner_field = wrap_msg
        .field
        .iter()
        .find(|f| f.name.as_deref() == Some("inner"))
        .unwrap();
    assert_eq!(
        inner_field.type_name,
        Some(".types.Req".to_owned()),
        "Wrap.inner should have type_name .types.Req"
    );
    // Check that the service rpc types are resolved
    let service = &svc_fdp.service[0];
    let method = &service.method[0];
    assert_eq!(method.input_type, Some(".types.Req".to_owned()));
    assert_eq!(method.output_type, Some(".types.Resp".to_owned()));
}

#[test]
fn wkt_import() {
    let td = TempDir::new("wkt_import");

    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package ts;
import "google/protobuf/timestamp.proto";
message WithTimestamp { google.protobuf.Timestamp created_at = 1; }
"#,
    );

    // WKT test: compile without specifying include dirs (WKT comes from pool)
    let fds = compile_files_native(&[&root_path], &[td.path()])
        .expect("wkt_import: native compile must succeed");

    assert_topo_valid(&fds, "wkt_import");

    // WKT file must be present by name
    let has_ts = fds
        .file
        .iter()
        .any(|f| f.name.as_deref() == Some("google/protobuf/timestamp.proto"));
    assert!(
        has_ts,
        "wkt_import: google/protobuf/timestamp.proto must be in FDS"
    );

    // The root file's field must resolve to .google.protobuf.Timestamp
    let root_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("root.proto"))
        .unwrap();
    let msg = &root_fdp.message_type[0];
    let field = &msg.field[0];
    assert_eq!(
        field.type_name,
        Some(".google.protobuf.Timestamp".to_owned()),
        "wkt_import: field type_name must be .google.protobuf.Timestamp"
    );
}

// ---------------------------------------------------------------------------
// Negative tests
// ---------------------------------------------------------------------------

#[test]
fn unresolvable_import_path_error() {
    let td = TempDir::new("unresolvable");

    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
import "nonexistent_file_xyz.proto";
message Root { int32 x = 1; }
"#,
    );

    let result = compile_files_native(&[&root_path], &[td.path()]);
    match &result {
        Err(oxiproto_build::BuildError::Parse { message, .. }) => {
            assert!(
                message.contains("import not found"),
                "expected 'import not found' in error message, got: {message}"
            );
        }
        other => panic!("expected Parse error with 'import not found', got: {other:?}"),
    }
}

#[test]
fn unknown_type_after_imports_error() {
    let td = TempDir::new("unknown_type");

    td.write(
        "dep.proto",
        r#"syntax = "proto3";
package dep;
message RealMessage { int32 x = 1; }
"#,
    );
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package root;
import "dep.proto";
message Root { dep.Nonexistent field1 = 1; }
"#,
    );

    let result = compile_files_native(&[&root_path], &[td.path()]);
    assert!(
        result.is_err(),
        "expected error for unknown type 'dep.Nonexistent', got Ok"
    );
    // The guess heuristic is removed — this must be a real error
    match &result {
        Err(oxiproto_build::BuildError::Parse { .. }) => {}
        other => panic!("expected Parse error, got: {other:?}"),
    }
}

#[test]
fn import_cycle_error() {
    let td = TempDir::new("cycle");

    td.write(
        "a.proto",
        r#"syntax = "proto3";
package a;
import "b.proto";
message A { b.B b_field = 1; }
"#,
    );
    td.write(
        "b.proto",
        r#"syntax = "proto3";
package b;
import "a.proto";
message B { a.A a_field = 1; }
"#,
    );
    let root_path = td.path().join("a.proto");

    let result = compile_files_native(&[&root_path], &[td.path()]);
    match &result {
        Err(oxiproto_build::BuildError::Parse { message, .. }) => {
            assert!(
                message.contains("cycle"),
                "expected 'cycle' in error message, got: {message}"
            );
        }
        other => panic!("expected Parse error with 'cycle', got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Well-known type compatibility tests (google.protobuf.*)
// ---------------------------------------------------------------------------

/// Helper: compile a single-file proto that imports WKTs and assert that
/// all expected WKT file names appear in the resulting FDS.
fn assert_wkt_in_fds(
    fds: &prost_types::FileDescriptorSet,
    expected_wkt_name: &str,
    test_name: &str,
) {
    assert!(
        fds.file
            .iter()
            .any(|f| f.name.as_deref() == Some(expected_wkt_name)),
        "{test_name}: '{expected_wkt_name}' missing from FDS"
    );
}

/// google/protobuf/duration.proto — Duration field resolved correctly.
#[test]
fn wkt_duration() {
    let td = TempDir::new("wkt_duration");
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package dur;
import "google/protobuf/duration.proto";
message WithDuration { google.protobuf.Duration elapsed = 1; }
"#,
    );

    let fds = compile_files_native(&[&root_path], &[td.path()])
        .expect("wkt_duration: native compile must succeed");

    assert_topo_valid(&fds, "wkt_duration");
    assert_wkt_in_fds(&fds, "google/protobuf/duration.proto", "wkt_duration");

    let root_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("root.proto"))
        .expect("root.proto not found");
    let field = &root_fdp.message_type[0].field[0];
    assert_eq!(
        field.type_name,
        Some(".google.protobuf.Duration".to_owned()),
        "wkt_duration: type_name must be .google.protobuf.Duration"
    );
}

/// google/protobuf/empty.proto — Empty used as RPC request/response.
#[test]
fn wkt_empty_in_service() {
    let td = TempDir::new("wkt_empty");
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package emptysvc;
import "google/protobuf/empty.proto";
service PingService {
  rpc Ping(google.protobuf.Empty) returns (google.protobuf.Empty);
}
"#,
    );

    let fds = compile_files_native(&[&root_path], &[td.path()])
        .expect("wkt_empty_in_service: native compile must succeed");

    assert_topo_valid(&fds, "wkt_empty_in_service");
    assert_wkt_in_fds(&fds, "google/protobuf/empty.proto", "wkt_empty_in_service");

    let root_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("root.proto"))
        .expect("root.proto not found");
    let svc = &root_fdp.service[0];
    let method = &svc.method[0];
    assert_eq!(
        method.input_type.as_deref(),
        Some(".google.protobuf.Empty"),
        "wkt_empty_in_service: input_type must be .google.protobuf.Empty"
    );
    assert_eq!(
        method.output_type.as_deref(),
        Some(".google.protobuf.Empty"),
        "wkt_empty_in_service: output_type must be .google.protobuf.Empty"
    );
}

/// google/protobuf/any.proto — Any field inside a message.
#[test]
fn wkt_any() {
    let td = TempDir::new("wkt_any");
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package anytest;
import "google/protobuf/any.proto";
message Envelope { google.protobuf.Any payload = 1; }
"#,
    );

    let fds = compile_files_native(&[&root_path], &[td.path()])
        .expect("wkt_any: native compile must succeed");

    assert_topo_valid(&fds, "wkt_any");
    assert_wkt_in_fds(&fds, "google/protobuf/any.proto", "wkt_any");

    let root_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("root.proto"))
        .expect("root.proto not found");
    let field = &root_fdp.message_type[0].field[0];
    assert_eq!(
        field.type_name,
        Some(".google.protobuf.Any".to_owned()),
        "wkt_any: type_name must be .google.protobuf.Any"
    );
}

/// google/protobuf/struct.proto — Struct and Value used as fields.
#[test]
fn wkt_struct() {
    let td = TempDir::new("wkt_struct");
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package structtest;
import "google/protobuf/struct.proto";
message JsonPayload {
  google.protobuf.Struct metadata = 1;
  google.protobuf.Value value = 2;
}
"#,
    );

    let fds = compile_files_native(&[&root_path], &[td.path()])
        .expect("wkt_struct: native compile must succeed");

    assert_topo_valid(&fds, "wkt_struct");
    assert_wkt_in_fds(&fds, "google/protobuf/struct.proto", "wkt_struct");

    let root_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("root.proto"))
        .expect("root.proto not found");
    let fields = &root_fdp.message_type[0].field;
    assert_eq!(
        fields[0].type_name,
        Some(".google.protobuf.Struct".to_owned()),
        "wkt_struct: field[0] type_name"
    );
    assert_eq!(
        fields[1].type_name,
        Some(".google.protobuf.Value".to_owned()),
        "wkt_struct: field[1] type_name"
    );
}

/// google/protobuf/field_mask.proto — FieldMask used as a field type.
#[test]
fn wkt_field_mask() {
    let td = TempDir::new("wkt_field_mask");
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package fmtest;
import "google/protobuf/field_mask.proto";
message UpdateRequest {
  string resource_name = 1;
  google.protobuf.FieldMask update_mask = 2;
}
"#,
    );

    let fds = compile_files_native(&[&root_path], &[td.path()])
        .expect("wkt_field_mask: native compile must succeed");

    assert_topo_valid(&fds, "wkt_field_mask");
    assert_wkt_in_fds(&fds, "google/protobuf/field_mask.proto", "wkt_field_mask");

    let root_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("root.proto"))
        .expect("root.proto not found");
    let field = &root_fdp.message_type[0].field[1];
    assert_eq!(
        field.type_name,
        Some(".google.protobuf.FieldMask".to_owned()),
        "wkt_field_mask: type_name must be .google.protobuf.FieldMask"
    );
}

/// google/protobuf/wrappers.proto — Int32Value and StringValue wrapper types.
#[test]
fn wkt_wrappers() {
    let td = TempDir::new("wkt_wrappers");
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package wrappers;
import "google/protobuf/wrappers.proto";
message Wrapped {
  google.protobuf.StringValue name = 1;
  google.protobuf.Int32Value count = 2;
  google.protobuf.BoolValue enabled = 3;
}
"#,
    );

    let fds = compile_files_native(&[&root_path], &[td.path()])
        .expect("wkt_wrappers: native compile must succeed");

    assert_topo_valid(&fds, "wkt_wrappers");
    assert_wkt_in_fds(&fds, "google/protobuf/wrappers.proto", "wkt_wrappers");

    let root_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("root.proto"))
        .expect("root.proto not found");
    let fields = &root_fdp.message_type[0].field;
    assert_eq!(
        fields[0].type_name,
        Some(".google.protobuf.StringValue".to_owned()),
        "wkt_wrappers: field[0] type_name"
    );
    assert_eq!(
        fields[1].type_name,
        Some(".google.protobuf.Int32Value".to_owned()),
        "wkt_wrappers: field[1] type_name"
    );
    assert_eq!(
        fields[2].type_name,
        Some(".google.protobuf.BoolValue".to_owned()),
        "wkt_wrappers: field[2] type_name"
    );
}

/// Multiple WKT types in a single file — topo order must still be valid.
#[test]
fn wkt_multiple_in_one_file() {
    let td = TempDir::new("wkt_multi");
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package multi;
import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";
import "google/protobuf/any.proto";
message Event {
  google.protobuf.Timestamp occurred_at = 1;
  google.protobuf.Duration duration = 2;
  google.protobuf.Any details = 3;
}
"#,
    );

    let fds = compile_files_native(&[&root_path], &[td.path()])
        .expect("wkt_multiple_in_one_file: native compile must succeed");

    assert_topo_valid(&fds, "wkt_multiple_in_one_file");
    assert_wkt_in_fds(
        &fds,
        "google/protobuf/timestamp.proto",
        "wkt_multiple_in_one_file",
    );
    assert_wkt_in_fds(
        &fds,
        "google/protobuf/duration.proto",
        "wkt_multiple_in_one_file",
    );
    assert_wkt_in_fds(
        &fds,
        "google/protobuf/any.proto",
        "wkt_multiple_in_one_file",
    );

    let root_fdp = fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("root.proto"))
        .expect("root.proto not found");
    let fields = &root_fdp.message_type[0].field;
    assert_eq!(
        3,
        fields.len(),
        "wkt_multiple_in_one_file: expected 3 fields"
    );
}

/// Cross-validate WKT imports: both native and protox produce the same
/// dependency list and field type_names for a Timestamp-bearing message.
#[test]
fn wkt_cross_validate_with_protox() {
    let td = TempDir::new("wkt_xval");
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package xval;
import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";
message TimeRange {
  google.protobuf.Timestamp start = 1;
  google.protobuf.Timestamp end = 2;
  google.protobuf.Duration span = 3;
}
"#,
    );

    let mut native_fds = compile_files_native(&[&root_path], &[td.path()])
        .expect("wkt_cross_validate_with_protox: native compile failed");
    let mut protox_fds = compile_to_fds(&[&root_path], &[td.path()])
        .expect("wkt_cross_validate_with_protox: protox compile failed");

    let native_map = normalize_fds_map(&mut native_fds);
    let protox_map = normalize_fds_map(&mut protox_fds);

    // The root file must be present in both
    let native_root = native_map
        .get("root.proto")
        .expect("native: root.proto missing");
    let protox_root = protox_map
        .get("root.proto")
        .expect("protox: root.proto missing");

    assert_file_structure_eq(native_root, protox_root, "wkt_cross_validate_with_protox");
}
