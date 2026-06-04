#![forbid(unsafe_code)]
#![cfg(feature = "native-parser")]

//! Tests for interoperability between the native parser's FileDescriptorSet
//! and prost-based serialization/deserialization.
//!
//! These tests verify that the FDS produced by the native parser can be:
//! 1. Encoded to bytes via prost and decoded back without loss.
//! 2. Accepted by `prost_reflect::DescriptorPool` for runtime reflection.
//! 3. Structurally equivalent to a protox-generated FDS after encode/decode roundtrip.

use oxiproto_build::{compile_files_native, compile_str_native, compile_to_fds};
use prost::Message as ProstMessage;
use prost_types::FileDescriptorSet;
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
        let path = std::env::temp_dir().join(format!("oxiproto_fds_interop_{prefix}_{pid}_{id}"));
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
// Helpers
// ---------------------------------------------------------------------------

/// Encode a FileDescriptorSet to bytes and decode it back, asserting lossless.
fn roundtrip_fds(fds: FileDescriptorSet) -> FileDescriptorSet {
    let bytes = fds.encode_to_vec();
    FileDescriptorSet::decode(bytes.as_slice()).expect("FDS decode must succeed")
}

/// Collect message names from FDS (for a specific file).
fn message_names_in_file<'a>(fds: &'a FileDescriptorSet, file_name: &str) -> Vec<&'a str> {
    fds.file
        .iter()
        .find(|f| f.name.as_deref() == Some(file_name))
        .map(|fdp| {
            fdp.message_type
                .iter()
                .filter_map(|m| m.name.as_deref())
                .collect()
        })
        .unwrap_or_default()
}

/// Collect field names for the first message in a given file.
fn field_names_first_message<'a>(fds: &'a FileDescriptorSet, file_name: &str) -> Vec<&'a str> {
    fds.file
        .iter()
        .find(|f| f.name.as_deref() == Some(file_name))
        .and_then(|fdp| fdp.message_type.first())
        .map(|msg| msg.field.iter().filter_map(|f| f.name.as_deref()).collect())
        .unwrap_or_default()
}

/// Build a map from file name to `FileDescriptorProto` (drops source_code_info).
fn fds_to_name_map(
    fds: &mut FileDescriptorSet,
) -> HashMap<String, prost_types::FileDescriptorProto> {
    let mut m = HashMap::new();
    for file in &mut fds.file {
        file.source_code_info = None;
        if let Some(name) = &file.name {
            m.insert(name.clone(), file.clone());
        }
    }
    m
}

// ---------------------------------------------------------------------------
// Test 1 — prost encode/decode roundtrip preserves message count and names
// ---------------------------------------------------------------------------

#[test]
fn fds_prost_roundtrip_preserves_messages() {
    let proto = r#"syntax = "proto3";
package interop;
message Alpha { int32 x = 1; string label = 2; }
message Beta  { bool flag = 1; repeated int64 values = 2; }
"#;
    let fds = compile_str_native(proto).expect("compile_str_native must succeed");

    // Before roundtrip
    let before_names = message_names_in_file(&fds, "<inline>.proto");
    assert_eq!(
        before_names.len(),
        2,
        "expected 2 messages before roundtrip"
    );

    // Roundtrip through prost encode/decode
    let rt_fds = roundtrip_fds(fds.clone());
    let after_names = message_names_in_file(&rt_fds, "<inline>.proto");

    assert_eq!(
        before_names, after_names,
        "message names must survive prost roundtrip"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — prost roundtrip preserves field names and numbers
// ---------------------------------------------------------------------------

#[test]
fn fds_prost_roundtrip_preserves_fields() {
    let proto = r#"syntax = "proto3";
package fields_rt;
message Request {
  string query = 1;
  int32 page = 2;
  int32 page_size = 3;
  bool include_archived = 4;
}
"#;
    let fds = compile_str_native(proto).expect("compile must succeed");
    let rt_fds = roundtrip_fds(fds);

    let field_names = field_names_first_message(&rt_fds, "<inline>.proto");
    assert_eq!(
        field_names,
        vec!["query", "page", "page_size", "include_archived"],
        "field names must survive prost roundtrip"
    );

    // Verify field numbers
    let fdp = rt_fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("<inline>.proto"))
        .expect("file not found");
    let numbers: Vec<i32> = fdp.message_type[0]
        .field
        .iter()
        .filter_map(|f| f.number)
        .collect();
    assert_eq!(
        numbers,
        vec![1, 2, 3, 4],
        "field numbers must survive prost roundtrip"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — prost roundtrip preserves enum types
// ---------------------------------------------------------------------------

#[test]
fn fds_prost_roundtrip_preserves_enums() {
    let proto = r#"syntax = "proto3";
package enum_rt;
enum Status { STATUS_UNSPECIFIED = 0; ACTIVE = 1; INACTIVE = 2; DELETED = 3; }
message Item { Status status = 1; }
"#;
    let fds = compile_str_native(proto).expect("compile must succeed");
    let rt_fds = roundtrip_fds(fds);

    let fdp = rt_fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("<inline>.proto"))
        .expect("file not found");

    assert_eq!(fdp.enum_type.len(), 1, "expected 1 top-level enum");
    let e = &fdp.enum_type[0];
    assert_eq!(e.name.as_deref(), Some("Status"), "enum name mismatch");
    assert_eq!(e.value.len(), 4, "expected 4 enum values");
    let val_names: Vec<&str> = e.value.iter().filter_map(|v| v.name.as_deref()).collect();
    assert_eq!(
        val_names,
        vec!["STATUS_UNSPECIFIED", "ACTIVE", "INACTIVE", "DELETED"]
    );
}

// ---------------------------------------------------------------------------
// Test 4 — prost roundtrip preserves oneof declarations
// ---------------------------------------------------------------------------

#[test]
fn fds_prost_roundtrip_preserves_oneof() {
    let proto = r#"syntax = "proto3";
package oneof_rt;
message Response {
  oneof result {
    string success_msg = 1;
    string error_msg   = 2;
    int32  code        = 3;
  }
}
"#;
    let fds = compile_str_native(proto).expect("compile must succeed");
    let rt_fds = roundtrip_fds(fds);

    let fdp = rt_fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("<inline>.proto"))
        .expect("file not found");

    let msg = &fdp.message_type[0];
    assert_eq!(msg.oneof_decl.len(), 1, "expected 1 oneof_decl");
    assert_eq!(
        msg.oneof_decl[0].name.as_deref(),
        Some("result"),
        "oneof name mismatch"
    );
    // All 3 fields should reference oneof_index = 0
    for field in &msg.field {
        assert_eq!(
            field.oneof_index,
            Some(0),
            "field {:?} must have oneof_index=0",
            field.name
        );
    }
}

// ---------------------------------------------------------------------------
// Test 5 — prost roundtrip preserves map fields (map entry desugaring)
// ---------------------------------------------------------------------------

#[test]
fn fds_prost_roundtrip_preserves_map_fields() {
    let proto = r#"syntax = "proto3";
package map_rt;
message Config { map<string, string> settings = 1; }
"#;
    let fds = compile_str_native(proto).expect("compile must succeed");
    let rt_fds = roundtrip_fds(fds);

    let fdp = rt_fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("<inline>.proto"))
        .expect("file not found");

    let msg = &fdp.message_type[0];
    // There should be one field (the map field) + one nested MapEntry message
    assert_eq!(msg.field.len(), 1, "expected 1 map field");
    assert_eq!(
        msg.field[0].name.as_deref(),
        Some("settings"),
        "map field name"
    );
    // Nested XxxEntry must have map_entry = true
    assert_eq!(msg.nested_type.len(), 1, "expected 1 nested MapEntry type");
    let map_entry = msg.options.as_ref().and_then(|o| o.map_entry);
    // The outer message is NOT the map entry — the nested one is
    let nested_map_entry = msg.nested_type[0]
        .options
        .as_ref()
        .and_then(|o| o.map_entry);
    assert_eq!(
        nested_map_entry,
        Some(true),
        "nested map entry must have map_entry=true; outer msg map_entry={map_entry:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — prost roundtrip preserves service + method signatures
// ---------------------------------------------------------------------------

#[test]
fn fds_prost_roundtrip_preserves_service() {
    let proto = r#"syntax = "proto3";
package svc_rt;
message SearchReq  { string query = 1; }
message SearchResp { repeated string results = 1; }
service SearchService {
  rpc Search(SearchReq) returns (SearchResp);
  rpc StreamSearch(SearchReq) returns (stream SearchResp);
}
"#;
    let fds = compile_str_native(proto).expect("compile must succeed");
    let rt_fds = roundtrip_fds(fds);

    let fdp = rt_fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("<inline>.proto"))
        .expect("file not found");

    assert_eq!(fdp.service.len(), 1, "expected 1 service");
    let svc = &fdp.service[0];
    assert_eq!(svc.name.as_deref(), Some("SearchService"));
    assert_eq!(svc.method.len(), 2, "expected 2 methods");

    let unary = &svc.method[0];
    assert_eq!(unary.name.as_deref(), Some("Search"));
    // client_streaming/server_streaming are optional bools; None and Some(false) both mean false
    assert!(
        !unary.client_streaming.unwrap_or(false),
        "unary.client_streaming must be false"
    );
    assert!(
        !unary.server_streaming.unwrap_or(false),
        "unary.server_streaming must be false"
    );

    let server_stream = &svc.method[1];
    assert_eq!(server_stream.name.as_deref(), Some("StreamSearch"));
    assert!(
        !server_stream.client_streaming.unwrap_or(false),
        "StreamSearch.client_streaming must be false"
    );
    assert_eq!(
        server_stream.server_streaming,
        Some(true),
        "StreamSearch.server_streaming must be Some(true)"
    );
}

// ---------------------------------------------------------------------------
// Test 7 — FDS bytes can be accepted by prost_reflect::DescriptorPool
// ---------------------------------------------------------------------------

#[test]
fn native_fds_accepted_by_prost_reflect_pool() {
    let proto = r#"syntax = "proto3";
package pool_test;
message Item { string name = 1; int32 count = 2; }
enum Color { COLOR_UNSPECIFIED = 0; RED = 1; GREEN = 2; BLUE = 3; }
"#;
    let fds = compile_str_native(proto).expect("compile must succeed");
    let fds_bytes = fds.encode_to_vec();

    // prost_reflect::DescriptorPool must accept the native FDS bytes
    let pool = prost_reflect::DescriptorPool::decode(fds_bytes.as_slice())
        .expect("DescriptorPool::decode must accept native FDS bytes");

    // Verify we can look up the message
    let msg_desc = pool
        .get_message_by_name("pool_test.Item")
        .expect("pool_test.Item must be found in pool");
    assert_eq!(msg_desc.name(), "Item");
    assert_eq!(msg_desc.full_name(), "pool_test.Item");
}

// ---------------------------------------------------------------------------
// Test 8 — multi-file FDS roundtrip (native import resolution)
// ---------------------------------------------------------------------------

#[test]
fn multi_file_fds_roundtrip() {
    let td = TempDir::new("multi_rt");

    td.write(
        "types.proto",
        r#"syntax = "proto3";
package types;
message Point { double x = 1; double y = 2; }
"#,
    );
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package geo;
import "types.proto";
message Line { types.Point start = 1; types.Point end = 2; }
"#,
    );

    let fds = compile_files_native(&[&root_path], &[td.path()])
        .expect("multi_file_fds_roundtrip: compile failed");

    let rt_fds = roundtrip_fds(fds.clone());

    // Both FDS instances must have the same number of files
    assert_eq!(
        fds.file.len(),
        rt_fds.file.len(),
        "file count must survive roundtrip"
    );

    // root.proto structure must survive
    let orig_names = message_names_in_file(&fds, "root.proto");
    let rt_names = message_names_in_file(&rt_fds, "root.proto");
    assert_eq!(orig_names, rt_names, "message names must survive roundtrip");
}

// ---------------------------------------------------------------------------
// Test 9 — native FDS structurally equivalent to protox FDS after roundtrip
// ---------------------------------------------------------------------------

#[test]
fn native_fds_equivalent_to_protox_after_roundtrip() {
    let td = TempDir::new("equiv_rt");
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto3";
package equiv;
message Coords { double lat = 1; double lon = 2; double alt = 3; }
enum Unit { UNIT_UNSPECIFIED = 0; METRIC = 1; IMPERIAL = 2; }
"#,
    );

    let native_fds =
        compile_files_native(&[&root_path], &[td.path()]).expect("native compile failed");
    let protox_fds = compile_to_fds(&[&root_path], &[td.path()]).expect("protox compile failed");

    // Roundtrip both through prost encode/decode
    let mut rt_native = roundtrip_fds(native_fds);
    let mut rt_protox = roundtrip_fds(protox_fds);

    let native_map = fds_to_name_map(&mut rt_native);
    let protox_map = fds_to_name_map(&mut rt_protox);

    let native_root = native_map
        .get("root.proto")
        .expect("native: root.proto missing");
    let protox_root = protox_map
        .get("root.proto")
        .expect("protox: root.proto missing");

    // Message names must match
    let native_msgs: Vec<&str> = native_root
        .message_type
        .iter()
        .filter_map(|m| m.name.as_deref())
        .collect();
    let protox_msgs: Vec<&str> = protox_root
        .message_type
        .iter()
        .filter_map(|m| m.name.as_deref())
        .collect();
    assert_eq!(
        native_msgs, protox_msgs,
        "message names must match between native and protox after roundtrip"
    );

    // Enum names must match
    let native_enums: Vec<&str> = native_root
        .enum_type
        .iter()
        .filter_map(|e| e.name.as_deref())
        .collect();
    let protox_enums: Vec<&str> = protox_root
        .enum_type
        .iter()
        .filter_map(|e| e.name.as_deref())
        .collect();
    assert_eq!(
        native_enums, protox_enums,
        "enum names must match between native and protox after roundtrip"
    );
}

// ---------------------------------------------------------------------------
// Test 10 — proto2 FDS roundtrip preserves required labels and default_value
// ---------------------------------------------------------------------------

#[test]
fn proto2_fds_roundtrip_preserves_required_and_default() {
    let td = TempDir::new("proto2_rt");
    let root_path = td.write(
        "root.proto",
        r#"syntax = "proto2";
package proto2rt;
message Legacy {
  required string name = 1;
  optional int32 count = 2 [default = 42];
  optional bool active = 3 [default = true];
}
"#,
    );

    let fds = compile_files_native(&[&root_path], &[td.path()])
        .expect("proto2_fds_roundtrip: compile failed");
    let rt_fds = roundtrip_fds(fds);

    let fdp = rt_fds
        .file
        .iter()
        .find(|f| f.name.as_deref() == Some("root.proto"))
        .expect("root.proto not found");

    let msg = &fdp.message_type[0];
    // Field 1: required (LABEL_REQUIRED = 2)
    let required_field = &msg.field[0];
    assert_eq!(
        required_field.label,
        Some(prost_types::field_descriptor_proto::Label::Required as i32),
        "first field must be LABEL_REQUIRED after roundtrip"
    );

    // default_value preserved
    let count_field = &msg.field[1];
    assert_eq!(
        count_field.default_value.as_deref(),
        Some("42"),
        "default_value '42' must survive roundtrip"
    );
    let active_field = &msg.field[2];
    assert_eq!(
        active_field.default_value.as_deref(),
        Some("true"),
        "default_value 'true' must survive roundtrip"
    );
}
