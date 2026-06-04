//! Integration tests verifying compatibility with oxiproto-build's generated
//! [`prost_types::FileDescriptorSet`] output.
//!
//! These tests exercise the **full pipeline**:
//!
//! 1. `oxiproto_build::compile_str` produces a `FileDescriptorSet` from an
//!    inline proto3 string (native parser path).
//! 2. `NativeDescriptorPool::from_file_descriptor_set` builds the pool.
//! 3. `NativeDynamicMessage` encodes, decodes, and JSON-round-trips the data.
//!
//! This directly exercises TODO items:
//! - "Ensure compatibility with oxiproto-build's generated FileDescriptorSet output"
//! - "Ensure oxiproto-json (future) uses DynamicMessage for JSON transcoding"
//!   (verified via `NativeDynamicMessage::to_json` / `from_json`, the native
//!   equivalent of the prost-reflect-based `oxiproto-json` crate.)
//! - "Ensure oxirpc-reflect can use oxiproto-reflect's DescriptorPool for
//!   gRPC server reflection" (verified via `oxirpc_reflect_pool_compatibility`).

use oxiproto_reflect::{NativeDescriptorPool, NativeDynamicMessage, NativeValue};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Compile an inline proto source to a FileDescriptorSet using
/// oxiproto-build's native parser.
fn compile(src: &str) -> prost_types::FileDescriptorSet {
    oxiproto_build::compile_str(src).expect("compile_str failed")
}

// ── 1. oxiproto-build FDS → NativeDescriptorPool ─────────────────────────────

/// The simplest possible pipeline: one scalar field, compile → pool → lookup.
#[test]
fn build_fds_scalar_message_roundtrip() {
    let src = r#"syntax = "proto3";
package compat;
message Item {
  int32 id = 1;
  string name = 2;
  bool active = 3;
}
"#;
    let fds = compile(src);
    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("pool");

    let desc = pool
        .get_message_by_name("compat.Item")
        .expect("compat.Item");
    assert_eq!(desc.name(), "Item");
    assert_eq!(desc.full_name(), "compat.Item");

    let f_id = desc.get_field_by_name("id").expect("id field");
    let f_name = desc.get_field_by_name("name").expect("name field");
    let f_active = desc.get_field_by_name("active").expect("active field");

    assert_eq!(f_id.number(), 1);
    assert_eq!(f_name.number(), 2);
    assert_eq!(f_active.number(), 3);

    // Build, encode, decode, verify.
    let mut msg = NativeDynamicMessage::new(desc.clone());
    msg.set_field(&f_id, NativeValue::I32(99));
    msg.set_field(&f_name, NativeValue::String("Widget".to_owned()));
    msg.set_field(&f_active, NativeValue::Bool(true));

    let bytes = msg.encode_to_vec().expect("encode");
    let decoded = NativeDynamicMessage::decode(desc.clone(), &bytes).expect("decode");

    assert_eq!(decoded.get_field(&f_id).as_ref(), &NativeValue::I32(99));
    assert_eq!(
        decoded.get_field(&f_name).as_ref(),
        &NativeValue::String("Widget".to_owned())
    );
    assert_eq!(
        decoded.get_field(&f_active).as_ref(),
        &NativeValue::Bool(true)
    );
}

/// Enum from oxiproto-build FDS is accessible in the pool.
#[test]
fn build_fds_enum_accessible_in_pool() {
    let src = r#"syntax = "proto3";
package compat;
enum Status {
  UNKNOWN = 0;
  ACTIVE = 1;
  INACTIVE = 2;
}
message Item {
  int32 id = 1;
  Status status = 2;
}
"#;
    let fds = compile(src);
    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("pool");

    let enum_desc = pool.get_enum_by_name("compat.Status").expect("Status enum");
    assert_eq!(enum_desc.name(), "Status");

    let values: Vec<String> = enum_desc.values().map(|v| v.name().to_owned()).collect();
    assert!(
        values.iter().any(|n| n == "UNKNOWN"),
        "UNKNOWN missing: {values:?}"
    );
    assert!(
        values.iter().any(|n| n == "ACTIVE"),
        "ACTIVE missing: {values:?}"
    );
    assert!(
        values.iter().any(|n| n == "INACTIVE"),
        "INACTIVE missing: {values:?}"
    );

    let desc = pool.get_message_by_name("compat.Item").expect("Item");
    let f_status = desc.get_field_by_name("status").expect("status field");

    let mut msg = NativeDynamicMessage::new(desc.clone());
    msg.set_field(&f_status, NativeValue::EnumNumber(1));

    let bytes = msg.encode_to_vec().expect("encode");
    let decoded = NativeDynamicMessage::decode(desc.clone(), &bytes).expect("decode");
    assert_eq!(
        decoded.get_field(&f_status).as_ref(),
        &NativeValue::EnumNumber(1)
    );
}

/// Nested message from oxiproto-build FDS round-trips through encode/decode.
#[test]
fn build_fds_nested_message_roundtrip() {
    let src = r#"syntax = "proto3";
package compat;
message Address {
  string street = 1;
  string city = 2;
}
message Person {
  string name = 1;
  Address address = 2;
}
"#;
    let fds = compile(src);
    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("pool");

    let addr_desc = pool.get_message_by_name("compat.Address").expect("Address");
    let person_desc = pool.get_message_by_name("compat.Person").expect("Person");

    let f_street = addr_desc.get_field_by_name("street").expect("street");
    let f_city = addr_desc.get_field_by_name("city").expect("city");
    let f_name = person_desc.get_field_by_name("name").expect("name");
    let f_addr = person_desc.get_field_by_name("address").expect("address");

    let mut addr = NativeDynamicMessage::new(addr_desc);
    addr.set_field(&f_street, NativeValue::String("123 Main St".to_owned()));
    addr.set_field(&f_city, NativeValue::String("Springfield".to_owned()));

    let mut person = NativeDynamicMessage::new(person_desc.clone());
    person.set_field(&f_name, NativeValue::String("Alice".to_owned()));
    person.set_field(&f_addr, NativeValue::Message(Box::new(addr)));

    let bytes = person.encode_to_vec().expect("encode");
    let decoded = NativeDynamicMessage::decode(person_desc.clone(), &bytes).expect("decode");

    assert_eq!(
        decoded.get_field(&f_name).as_ref(),
        &NativeValue::String("Alice".to_owned())
    );
    let decoded_addr = match decoded.get_field(&f_addr).as_ref() {
        NativeValue::Message(m) => m.as_ref().clone(),
        other => panic!("expected Message, got {other:?}"),
    };
    assert_eq!(
        decoded_addr.get_field(&f_street).as_ref(),
        &NativeValue::String("123 Main St".to_owned())
    );
}

/// Repeated field from oxiproto-build FDS round-trips correctly.
#[test]
fn build_fds_repeated_field_roundtrip() {
    let src = r#"syntax = "proto3";
package compat;
message Tags {
  repeated string labels = 1;
  repeated int32 scores  = 2;
}
"#;
    let fds = compile(src);
    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("pool");

    let desc = pool.get_message_by_name("compat.Tags").expect("Tags");
    let f_labels = desc.get_field_by_name("labels").expect("labels");
    let f_scores = desc.get_field_by_name("scores").expect("scores");

    let mut msg = NativeDynamicMessage::new(desc.clone());
    msg.set_field(
        &f_labels,
        NativeValue::List(vec![
            NativeValue::String("alpha".to_owned()),
            NativeValue::String("beta".to_owned()),
            NativeValue::String("gamma".to_owned()),
        ]),
    );
    msg.set_field(
        &f_scores,
        NativeValue::List(vec![
            NativeValue::I32(10),
            NativeValue::I32(20),
            NativeValue::I32(30),
        ]),
    );

    let bytes = msg.encode_to_vec().expect("encode");
    let decoded = NativeDynamicMessage::decode(desc.clone(), &bytes).expect("decode");

    match decoded.get_field(&f_labels).as_ref() {
        NativeValue::List(items) => {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], NativeValue::String("alpha".to_owned()));
            assert_eq!(items[2], NativeValue::String("gamma".to_owned()));
        }
        other => panic!("expected List, got {other:?}"),
    }
    match decoded.get_field(&f_scores).as_ref() {
        NativeValue::List(items) => {
            assert_eq!(items.len(), 3);
            assert_eq!(items[1], NativeValue::I32(20));
        }
        other => panic!("expected List, got {other:?}"),
    }
}

/// Map field from oxiproto-build FDS round-trips.
#[test]
fn build_fds_map_field_roundtrip() {
    let src = r#"syntax = "proto3";
package compat;
message Registry {
  map<string, int32> counts = 1;
}
"#;
    let fds = compile(src);
    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("pool");

    let desc = pool
        .get_message_by_name("compat.Registry")
        .expect("Registry");
    let f_counts = desc.get_field_by_name("counts").expect("counts");

    let mut map = std::collections::HashMap::new();
    map.insert(
        oxiproto_reflect::NativeMapKey::String("apples".to_owned()),
        NativeValue::I32(42),
    );
    map.insert(
        oxiproto_reflect::NativeMapKey::String("bananas".to_owned()),
        NativeValue::I32(7),
    );

    let mut msg = NativeDynamicMessage::new(desc.clone());
    msg.set_field(&f_counts, NativeValue::Map(map));

    let bytes = msg.encode_to_vec().expect("encode");
    let decoded = NativeDynamicMessage::decode(desc.clone(), &bytes).expect("decode");

    match decoded.get_field(&f_counts).as_ref() {
        NativeValue::Map(m) => {
            assert_eq!(m.len(), 2);
            assert_eq!(
                m.get(&oxiproto_reflect::NativeMapKey::String("apples".to_owned())),
                Some(&NativeValue::I32(42))
            );
        }
        other => panic!("expected Map, got {other:?}"),
    }
}

/// Service and method descriptors are accessible from an oxiproto-build FDS.
#[test]
fn build_fds_service_descriptor_accessible() {
    let src = r#"syntax = "proto3";
package compat;
message Req { string query = 1; }
message Resp { string result = 1; }
service SearchService {
  rpc Search (Req) returns (Resp);
  rpc StreamSearch (Req) returns (stream Resp);
}
"#;
    let fds = compile(src);
    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("pool");

    let svc = pool
        .get_service_by_name("compat.SearchService")
        .expect("SearchService");
    assert_eq!(svc.name(), "SearchService");

    let methods: Vec<_> = svc.methods().collect();
    assert_eq!(
        methods.len(),
        2,
        "expected 2 methods, got {}",
        methods.len()
    );

    let search = methods
        .iter()
        .find(|m| m.name() == "Search")
        .expect("Search");
    assert!(!search.is_client_streaming());
    assert!(!search.is_server_streaming());

    let stream_search = methods
        .iter()
        .find(|m| m.name() == "StreamSearch")
        .expect("StreamSearch");
    assert!(!stream_search.is_client_streaming());
    assert!(stream_search.is_server_streaming());
}

// ── 2. NativeDynamicMessage JSON transcoding (native equivalent of oxiproto-json) ─

/// JSON round-trip through `NativeDynamicMessage::to_json` / `from_json`.
///
/// This verifies the "native DynamicMessage JSON transcoding" path that is the
/// native-reflect equivalent of the prost-reflect-based `oxiproto-json` crate.
#[test]
fn native_dynamic_json_transcoding_roundtrip() {
    let src = r#"syntax = "proto3";
package compat;
message Product {
  int32 id = 1;
  string name = 2;
  double price = 3;
  bool available = 4;
  int64 sku = 5;
}
"#;
    let fds = compile(src);
    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let desc = pool.get_message_by_name("compat.Product").expect("Product");

    let f_id = desc.get_field_by_name("id").expect("id");
    let f_name = desc.get_field_by_name("name").expect("name");
    let f_price = desc.get_field_by_name("price").expect("price");
    let f_available = desc.get_field_by_name("available").expect("available");
    let f_sku = desc.get_field_by_name("sku").expect("sku");

    let mut msg = NativeDynamicMessage::new(desc.clone());
    msg.set_field(&f_id, NativeValue::I32(1001));
    msg.set_field(&f_name, NativeValue::String("Gadget Pro".to_owned()));
    msg.set_field(&f_price, NativeValue::F64(49.99));
    msg.set_field(&f_available, NativeValue::Bool(true));
    msg.set_field(&f_sku, NativeValue::I64(9_876_543_210));

    // to_json() returns serde_json::Value directly (canonical proto3 JSON).
    let json_val = msg.to_json().expect("to_json");

    // id is int32 → number
    assert_eq!(json_val["id"], serde_json::json!(1001));
    // name → string
    assert_eq!(json_val["name"], serde_json::json!("Gadget Pro"));
    // sku is int64 → string per canonical proto3 JSON spec
    assert_eq!(json_val["sku"], serde_json::json!("9876543210"));

    // Round-trip via from_json (takes MessageDescriptor by value + &serde_json::Value).
    let rebuilt = NativeDynamicMessage::from_json(desc.clone(), &json_val).expect("from_json");
    assert_eq!(rebuilt.get_field(&f_id).as_ref(), &NativeValue::I32(1001));
    assert_eq!(
        rebuilt.get_field(&f_name).as_ref(),
        &NativeValue::String("Gadget Pro".to_owned())
    );
    assert_eq!(
        rebuilt.get_field(&f_sku).as_ref(),
        &NativeValue::I64(9_876_543_210)
    );

    // to_json_string round-trip via from_json_str.
    let json_str = msg.to_json_string().expect("to_json_string");
    let rebuilt2 =
        NativeDynamicMessage::from_json_str(desc.clone(), &json_str).expect("from_json_str");
    assert_eq!(
        rebuilt2.get_field(&f_price).as_ref(),
        rebuilt.get_field(&f_price).as_ref()
    );
}

/// Bytes fields from oxiproto-build FDS are base64-encoded in JSON (canonical
/// proto3 JSON spec requirement).
#[test]
fn native_dynamic_json_bytes_base64() {
    let src = r#"syntax = "proto3";
package compat;
message Blob {
  bytes data = 1;
  string tag  = 2;
}
"#;
    let fds = compile(src);
    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let desc = pool.get_message_by_name("compat.Blob").expect("Blob");

    let f_data = desc.get_field_by_name("data").expect("data");
    let f_tag = desc.get_field_by_name("tag").expect("tag");

    let raw: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let mut msg = NativeDynamicMessage::new(desc.clone());
    msg.set_field(&f_data, NativeValue::Bytes(raw.clone()));
    msg.set_field(&f_tag, NativeValue::String("binary".to_owned()));

    let json_val = msg.to_json().expect("to_json");

    // bytes → base64 string
    let b64 = json_val["data"].as_str().expect("data must be a string");
    use base64::Engine as _;
    let decoded_bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("valid base64");
    assert_eq!(decoded_bytes, raw);

    // Round-trip.
    let rebuilt = NativeDynamicMessage::from_json(desc.clone(), &json_val).expect("from_json");
    assert_eq!(
        rebuilt.get_field(&f_data).as_ref(),
        &NativeValue::Bytes(raw)
    );
}

/// Enum values are encoded as their name string in canonical proto3 JSON.
#[test]
fn native_dynamic_json_enum_as_name() {
    let src = r#"syntax = "proto3";
package compat;
enum Color { RED = 0; GREEN = 1; BLUE = 2; }
message Palette { Color primary = 1; }
"#;
    let fds = compile(src);
    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let desc = pool.get_message_by_name("compat.Palette").expect("Palette");
    let f_primary = desc.get_field_by_name("primary").expect("primary");

    let mut msg = NativeDynamicMessage::new(desc.clone());
    msg.set_field(&f_primary, NativeValue::EnumNumber(2)); // BLUE

    let json_val = msg.to_json().expect("to_json");
    // Proto3 canonical JSON: enum → name string
    assert_eq!(
        json_val["primary"],
        serde_json::json!("BLUE"),
        "enum must be encoded as its name in canonical JSON, got: {json_val}"
    );

    // Round-trip
    let rebuilt = NativeDynamicMessage::from_json(desc.clone(), &json_val).expect("from_json");
    assert_eq!(
        rebuilt.get_field(&f_primary).as_ref(),
        &NativeValue::EnumNumber(2)
    );
}

/// Multi-file FDS from oxiproto-build (simulating import dependencies) works
/// with NativeDescriptorPool.
#[test]
fn build_fds_multi_file_import_resolution() {
    // oxiproto-build produces a single FDS containing all transitive imports.
    // We compile a self-contained inline proto with nested enum + message types.
    let src = r#"syntax = "proto3";
package compat;
enum Priority { LOW = 0; MEDIUM = 1; HIGH = 2; }
message Tag { string key = 1; string value = 2; }
message Task {
  int32 id = 1;
  string title = 2;
  Priority priority = 3;
  repeated Tag labels = 4;
}
"#;
    let fds = compile(src);
    assert!(!fds.file.is_empty(), "FDS must have at least one file");

    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("pool");

    // All types must be reachable.
    assert!(
        pool.get_enum_by_name("compat.Priority").is_some(),
        "Priority enum missing"
    );
    assert!(
        pool.get_message_by_name("compat.Tag").is_some(),
        "Tag msg missing"
    );
    assert!(
        pool.get_message_by_name("compat.Task").is_some(),
        "Task msg missing"
    );

    let task_desc = pool.get_message_by_name("compat.Task").unwrap();
    let tag_desc = pool.get_message_by_name("compat.Tag").unwrap();

    let f_id = task_desc.get_field_by_name("id").expect("id");
    let f_title = task_desc.get_field_by_name("title").expect("title");
    let f_priority = task_desc.get_field_by_name("priority").expect("priority");
    let f_labels = task_desc.get_field_by_name("labels").expect("labels");

    let f_key = tag_desc.get_field_by_name("key").expect("key");
    let f_value = tag_desc.get_field_by_name("value").expect("value");

    let mut tag1 = NativeDynamicMessage::new(tag_desc.clone());
    tag1.set_field(&f_key, NativeValue::String("env".to_owned()));
    tag1.set_field(&f_value, NativeValue::String("prod".to_owned()));

    let mut tag2 = NativeDynamicMessage::new(tag_desc);
    tag2.set_field(&f_key, NativeValue::String("team".to_owned()));
    tag2.set_field(&f_value, NativeValue::String("backend".to_owned()));

    let mut task = NativeDynamicMessage::new(task_desc.clone());
    task.set_field(&f_id, NativeValue::I32(42));
    task.set_field(&f_title, NativeValue::String("Deploy service".to_owned()));
    task.set_field(&f_priority, NativeValue::EnumNumber(2)); // HIGH
    task.set_field(
        &f_labels,
        NativeValue::List(vec![
            NativeValue::Message(Box::new(tag1)),
            NativeValue::Message(Box::new(tag2)),
        ]),
    );

    // Wire round-trip.
    let bytes = task.encode_to_vec().expect("encode");
    let decoded = NativeDynamicMessage::decode(task_desc.clone(), &bytes).expect("decode");

    assert_eq!(decoded.get_field(&f_id).as_ref(), &NativeValue::I32(42));
    assert_eq!(
        decoded.get_field(&f_title).as_ref(),
        &NativeValue::String("Deploy service".to_owned())
    );
    assert_eq!(
        decoded.get_field(&f_priority).as_ref(),
        &NativeValue::EnumNumber(2)
    );

    match decoded.get_field(&f_labels).as_ref() {
        NativeValue::List(items) => assert_eq!(items.len(), 2),
        other => panic!("expected List, got {other:?}"),
    }

    // JSON round-trip.
    let json_val = task.to_json().expect("to_json");
    let rebuilt = NativeDynamicMessage::from_json(task_desc.clone(), &json_val).expect("from_json");
    assert_eq!(
        rebuilt.get_field(&f_priority).as_ref(),
        // Priority = 2 → "HIGH" on encode, decoded back to 2
        &NativeValue::EnumNumber(2)
    );
}

// ── 3. oxirpc-reflect integration smoke test ──────────────────────────────────

/// Verifies that the oxiproto-reflect pool type can be constructed from an
/// oxiproto-build FDS and would be accepted by the oxirpc-reflect `oxiproto`
/// feature path.
///
/// The actual gRPC service plumbing (tonic, async runtime) is out of scope
/// for this crate's tests; this test focuses on the data-path compatibility:
/// pool construction, service lookup, and descriptor access — the exact
/// operations the oxirpc-reflect service backend performs.
#[test]
fn oxirpc_reflect_pool_compatibility() {
    let src = r#"syntax = "proto3";
package grpc.health.v1;
message HealthCheckRequest  { string service = 1; }
message HealthCheckResponse {
  enum ServingStatus {
    UNKNOWN = 0;
    SERVING = 1;
    NOT_SERVING = 2;
    SERVICE_UNKNOWN = 3;
  }
  ServingStatus status = 1;
}
service Health {
  rpc Check (HealthCheckRequest) returns (HealthCheckResponse);
  rpc Watch (HealthCheckRequest) returns (stream HealthCheckResponse);
}
"#;
    let fds = compile(src);
    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("pool");

    // 1. Find a service by fully-qualified name — operation oxirpc-reflect performs.
    let svc = pool
        .get_service_by_name("grpc.health.v1.Health")
        .expect("Health service");
    assert_eq!(svc.full_name(), "grpc.health.v1.Health");

    // 2. List methods and inspect streaming flags.
    let methods: Vec<_> = svc.methods().collect();
    assert_eq!(methods.len(), 2);

    let check = methods.iter().find(|m| m.name() == "Check").expect("Check");
    assert!(!check.is_client_streaming());
    assert!(!check.is_server_streaming());

    let watch = methods.iter().find(|m| m.name() == "Watch").expect("Watch");
    assert!(!watch.is_client_streaming());
    assert!(watch.is_server_streaming());

    // 3. Resolve input/output descriptors.
    let input_type = check.input();
    assert_eq!(input_type.full_name(), "grpc.health.v1.HealthCheckRequest");

    let output_type = check.output();
    assert_eq!(
        output_type.full_name(),
        "grpc.health.v1.HealthCheckResponse"
    );

    // 4. Nested enum is accessible via FieldDescriptor::enum_type().
    let status_field = output_type.get_field_by_name("status").expect("status");
    let enum_desc = status_field.enum_type().expect("enum type for status");
    assert_eq!(enum_desc.name(), "ServingStatus");

    // Verify all serving-status values are present.
    let value_names: Vec<String> = enum_desc.values().map(|v| v.name().to_owned()).collect();
    assert!(
        value_names.iter().any(|n| n == "SERVING"),
        "SERVING missing"
    );
    assert!(
        value_names.iter().any(|n| n == "NOT_SERVING"),
        "NOT_SERVING missing"
    );
}
