//! Benchmarks for the native .proto parser.
//!
//! Measures parse throughput for:
//!  - A small single-message proto (baseline)
//!  - A medium proto with multiple messages, enums, and a service
//!  - A large proto simulating google/protobuf/*.proto-style files
//!  - Import resolution with a 3-file dependency chain
//!
//! Run with:
//!   cargo bench -p oxiproto-build --features native-parser --bench parse

#![forbid(unsafe_code)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxiproto_build::compile_str_native;

// ---------------------------------------------------------------------------
// Proto source fixtures
// ---------------------------------------------------------------------------

const SMALL_PROTO: &str = r#"syntax = "proto3";
package small;
message Point { double x = 1; double y = 2; double z = 3; }
"#;

const MEDIUM_PROTO: &str = r#"syntax = "proto3";
package medium;

enum Status {
  STATUS_UNSPECIFIED = 0;
  ACTIVE   = 1;
  INACTIVE = 2;
  DELETED  = 3;
  PENDING  = 4;
  ARCHIVED = 5;
}

message Address {
  string street     = 1;
  string city       = 2;
  string state      = 3;
  string zip        = 4;
  string country    = 5;
}

message Person {
  int64   id         = 1;
  string  first_name = 2;
  string  last_name  = 3;
  string  email      = 4;
  Status  status     = 5;
  Address address    = 6;
  repeated string tags = 7;
  map<string, string> metadata = 8;
  oneof contact {
    string phone_number = 9;
    string slack_handle = 10;
  }
}

message Organization {
  int64           id       = 1;
  string          name     = 2;
  repeated Person members  = 3;
  Status          status   = 4;
  map<string, Person> contacts = 5;
}

message ListPersonsRequest  { string filter = 1; int32 page = 2; int32 page_size = 3; }
message ListPersonsResponse { repeated Person persons = 1; int32 total = 2; bool has_more = 3; }
message GetPersonRequest    { int64 id = 1; }
message CreatePersonRequest { Person person = 1; }
message UpdatePersonRequest { int64 id = 1; Person person = 2; }
message DeletePersonRequest { int64 id = 1; }
message DeletePersonResponse {}

service PersonService {
  rpc ListPersons  (ListPersonsRequest)  returns (ListPersonsResponse);
  rpc GetPerson    (GetPersonRequest)    returns (Person);
  rpc CreatePerson (CreatePersonRequest) returns (Person);
  rpc UpdatePerson (UpdatePersonRequest) returns (Person);
  rpc DeletePerson (DeletePersonRequest) returns (DeletePersonResponse);
  rpc WatchPersons (ListPersonsRequest)  returns (stream Person);
}
"#;

/// A large proto with deeply nested messages and many scalar fields,
/// simulating the complexity of google/protobuf/descriptor.proto.
const LARGE_PROTO: &str = r#"syntax = "proto3";
package large;

enum FieldType {
  FIELD_TYPE_UNSPECIFIED = 0;
  TYPE_DOUBLE   = 1;
  TYPE_FLOAT    = 2;
  TYPE_INT64    = 3;
  TYPE_UINT64   = 4;
  TYPE_INT32    = 5;
  TYPE_FIXED64  = 6;
  TYPE_FIXED32  = 7;
  TYPE_BOOL     = 8;
  TYPE_STRING   = 9;
  TYPE_GROUP    = 10;
  TYPE_MESSAGE  = 11;
  TYPE_BYTES    = 12;
  TYPE_UINT32   = 13;
  TYPE_ENUM     = 14;
  TYPE_SFIXED32 = 15;
  TYPE_SFIXED64 = 16;
  TYPE_SINT32   = 17;
  TYPE_SINT64   = 18;
}

enum FieldLabel {
  FIELD_LABEL_UNSPECIFIED = 0;
  LABEL_OPTIONAL = 1;
  LABEL_REQUIRED = 2;
  LABEL_REPEATED = 3;
}

message FieldOptions {
  bool   ctype     = 1;
  bool   packed    = 2;
  bool   lazy      = 5;
  bool   deprecated = 3;
  bool   weak       = 10;
  repeated FieldOption uninterpreted_option = 999;
}

message FieldOption {
  string name  = 1;
  string value = 2;
}

message FieldDescriptor {
  string     name          = 1;
  int32      number        = 3;
  FieldLabel label         = 4;
  FieldType  type          = 5;
  string     type_name     = 6;
  string     default_value = 7;
  FieldOptions options     = 8;
  int32      oneof_index   = 9;
  string     json_name     = 10;
}

message OneofDescriptor {
  string name    = 1;
}

message EnumValueDescriptor {
  string name    = 1;
  int32  number  = 2;
}

message EnumDescriptor {
  string                  name   = 1;
  repeated EnumValueDescriptor value  = 2;
}

message MessageDescriptor {
  string                   name        = 1;
  repeated FieldDescriptor field       = 2;
  repeated MessageDescriptor nested_type = 3;
  repeated EnumDescriptor  enum_type   = 4;
  repeated OneofDescriptor oneof_decl  = 8;
}

message MethodDescriptor {
  string name             = 1;
  string input_type       = 2;
  string output_type      = 3;
  bool   client_streaming = 5;
  bool   server_streaming = 6;
}

message ServiceDescriptor {
  string                   name   = 1;
  repeated MethodDescriptor method = 2;
}

message FileDescriptor {
  string                   name         = 1;
  string                   package      = 2;
  repeated string          dependency   = 3;
  repeated MessageDescriptor message_type = 4;
  repeated EnumDescriptor  enum_type    = 5;
  repeated ServiceDescriptor service     = 6;
  string                   syntax       = 12;
}

message FileDescriptorSet {
  repeated FileDescriptor file = 1;
}

message DescriptorPool {
  repeated FileDescriptorSet files = 1;
  map<string, MessageDescriptor> messages = 2;
  map<string, EnumDescriptor>    enums    = 3;
  map<string, ServiceDescriptor> services = 4;
}

service DescriptorService {
  rpc GetFile    (FileDescriptorSet) returns (FileDescriptor);
  rpc GetMessage (MessageDescriptor) returns (MessageDescriptor);
  rpc GetEnum    (EnumDescriptor)    returns (EnumDescriptor);
  rpc GetService (ServiceDescriptor) returns (ServiceDescriptor);
  rpc ListFiles  (DescriptorPool)    returns (stream FileDescriptor);
}
"#;

// ---------------------------------------------------------------------------
// Benchmark: lex + parse (compile_str_native = parse + resolve + build FDS)
// ---------------------------------------------------------------------------

fn bench_parse_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    group.throughput(Throughput::Bytes(SMALL_PROTO.len() as u64));
    group.bench_function("small_proto", |b| {
        b.iter(|| compile_str_native(black_box(SMALL_PROTO)).unwrap());
    });
    group.finish();
}

fn bench_parse_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    group.throughput(Throughput::Bytes(MEDIUM_PROTO.len() as u64));
    group.bench_function("medium_proto", |b| {
        b.iter(|| compile_str_native(black_box(MEDIUM_PROTO)).unwrap());
    });
    group.finish();
}

fn bench_parse_large(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    group.throughput(Throughput::Bytes(LARGE_PROTO.len() as u64));
    group.bench_function("large_proto", |b| {
        b.iter(|| compile_str_native(black_box(LARGE_PROTO)).unwrap());
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: parse throughput vs source size (scaling study)
// ---------------------------------------------------------------------------

fn bench_parse_scaling(c: &mut Criterion) {
    // Generate protos of increasing sizes by repeating field patterns
    let sizes = [1usize, 5, 10, 20, 50];
    let mut group = c.benchmark_group("parse_scaling");

    for &n_messages in &sizes {
        let proto = build_n_message_proto(n_messages);
        group.throughput(Throughput::Bytes(proto.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("n_messages", n_messages),
            &proto,
            |b, src| {
                b.iter(|| compile_str_native(black_box(src.as_str())).unwrap());
            },
        );
    }
    group.finish();
}

fn build_n_message_proto(n: usize) -> String {
    let mut out = String::from("syntax = \"proto3\";\npackage scale;\n");
    for i in 0..n {
        out.push_str(&format!(
            "message Msg{i} {{ int32 id = 1; string name = 2; bool flag = 3; repeated int64 values = 4; }}\n"
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Benchmark: import resolution overhead (3-file chain via temp files)
// ---------------------------------------------------------------------------

fn bench_import_resolution(c: &mut Criterion) {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    static TEMP_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

    // Set up temp files once
    let temp_dir = TEMP_DIR.get_or_init(|| {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("oxiproto_bench_import_{pid}_{id}"));
        std::fs::create_dir_all(&dir).expect("create bench temp dir");

        std::fs::write(
            dir.join("common.proto"),
            r#"syntax = "proto3";
package common;
message Id { int64 value = 1; }
message Name { string first = 1; string last = 2; }
"#,
        )
        .expect("write common.proto");

        std::fs::write(
            dir.join("domain.proto"),
            r#"syntax = "proto3";
package domain;
import "common.proto";
message User { common.Id id = 1; common.Name name = 2; string email = 3; }
"#,
        )
        .expect("write domain.proto");

        std::fs::write(
            dir.join("api.proto"),
            r#"syntax = "proto3";
package api;
import "domain.proto";
message GetUserRequest  { domain.User user = 1; }
message GetUserResponse { domain.User user = 1; bool found = 2; }
service UserService {
  rpc GetUser (GetUserRequest) returns (GetUserResponse);
}
"#,
        )
        .expect("write api.proto");

        dir
    });

    let root = temp_dir.join("api.proto");

    let mut group = c.benchmark_group("import_resolution");
    group.bench_function("three_file_chain", |b| {
        b.iter(|| {
            oxiproto_build::compile_files_native(
                black_box(&[&root]),
                black_box(&[temp_dir.as_path()]),
            )
            .unwrap()
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: deeply nested messages (import resolution scalability)
// ---------------------------------------------------------------------------

fn bench_deep_nesting(c: &mut Criterion) {
    // Build a proto with deeply nested message definitions (not imports)
    let proto = build_deeply_nested_proto(5);

    let mut group = c.benchmark_group("parse_structure");
    group.throughput(Throughput::Bytes(proto.len() as u64));
    group.bench_function("depth_5_nesting", |b| {
        b.iter(|| compile_str_native(black_box(proto.as_str())).unwrap());
    });
    group.finish();
}

fn build_deeply_nested_proto(depth: usize) -> String {
    // Build: message A { message B { message C { ... message Z { int32 x = 1; } }}}
    let mut result = String::from("syntax = \"proto3\";\npackage nested;\n");

    // Opening braces
    for d in 0..depth {
        let indent = "  ".repeat(d);
        result.push_str(&format!("{indent}message Level{d} {{\n"));
    }

    // Innermost field
    let innermost_indent = "  ".repeat(depth);
    result.push_str(&format!("{innermost_indent}  int32 value = 1;\n"));

    // Closing braces
    for d in (0..depth).rev() {
        let indent = "  ".repeat(d);
        result.push_str(&format!("{indent}}}\n"));
    }

    result
}

// ---------------------------------------------------------------------------
// Profile: deeply nested import chains (scalability of the import resolver)
// ---------------------------------------------------------------------------

/// Build and benchmark a chain of N proto files where each imports the previous.
///
/// Graph shape: base → dep1 → dep2 → … → depN−1 → root
/// This directly exercises the DFS loader and multi-file topological sort.
fn bench_deep_import_chain(c: &mut Criterion) {
    use std::sync::OnceLock;

    static TEMP_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

    // Write a 10-file import chain once.
    let chain_len = 10usize;
    let temp_dir = TEMP_DIR.get_or_init(|| {
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("oxiproto_bench_chain_{pid}"));
        std::fs::create_dir_all(&dir).expect("create chain bench temp dir");

        // base.proto — no imports
        std::fs::write(
            dir.join("chain_base.proto"),
            r#"syntax = "proto3";
package chain;
message Base { int64 id = 1; string name = 2; }
"#,
        )
        .expect("write chain_base.proto");

        // chain_1 .. chain_9 each import the previous
        for i in 1..chain_len {
            let prev = if i == 1 {
                "chain_base.proto".to_owned()
            } else {
                format!("chain_{}.proto", i - 1)
            };
            let content = format!(
                r#"syntax = "proto3";
package chain;
import "{prev}";
message Link{i} {{ Base base = 1; int32 depth = 2; repeated string tags = 3; }}
"#
            );
            std::fs::write(dir.join(format!("chain_{i}.proto")), content)
                .expect("write chain proto");
        }

        dir
    });

    let root = temp_dir.join(format!("chain_{}.proto", chain_len - 1));
    let mut group = c.benchmark_group("import_resolution");
    group.bench_function("deep_chain_10", |b| {
        b.iter(|| {
            oxiproto_build::compile_files_native(
                black_box(&[&root]),
                black_box(&[temp_dir.as_path()]),
            )
            .unwrap()
        });
    });
    group.finish();
}

/// Profile import resolution across a diamond dependency graph.
///
/// Shape:
///   base ← left ← top
///   base ← right ← top
/// This stresses the "already visited" deduplication path in the DFS loader.
fn bench_diamond_import(c: &mut Criterion) {
    use std::sync::OnceLock;

    static TEMP_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

    let temp_dir = TEMP_DIR.get_or_init(|| {
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("oxiproto_bench_diamond_{pid}"));
        std::fs::create_dir_all(&dir).expect("create diamond bench temp dir");

        std::fs::write(
            dir.join("diamond_base.proto"),
            r#"syntax = "proto3"; package diamond;
message Base { int64 id = 1; string name = 2; bytes data = 3; }
"#,
        )
        .expect("write diamond_base.proto");

        std::fs::write(
            dir.join("diamond_left.proto"),
            r#"syntax = "proto3"; package diamond;
import "diamond_base.proto";
message Left { Base base = 1; string left_tag = 2; }
"#,
        )
        .expect("write diamond_left.proto");

        std::fs::write(
            dir.join("diamond_right.proto"),
            r#"syntax = "proto3"; package diamond;
import "diamond_base.proto";
message Right { Base base = 1; string right_tag = 2; }
"#,
        )
        .expect("write diamond_right.proto");

        std::fs::write(
            dir.join("diamond_top.proto"),
            r#"syntax = "proto3"; package diamond;
import "diamond_left.proto";
import "diamond_right.proto";
message Top { Left left = 1; Right right = 2; Base base = 3; }
service DiamondService {
  rpc Process (Top) returns (Base);
}
"#,
        )
        .expect("write diamond_top.proto");

        dir
    });

    let root = temp_dir.join("diamond_top.proto");
    let mut group = c.benchmark_group("import_resolution");
    group.bench_function("diamond_4_files", |b| {
        b.iter(|| {
            oxiproto_build::compile_files_native(
                black_box(&[&root]),
                black_box(&[temp_dir.as_path()]),
            )
            .unwrap()
        });
    });
    group.finish();
}

/// Profile import resolution across a wide fan-out (one root, many shallow deps).
///
/// This measures the overhead of loading N independent files,
/// each imported once by a single root file.
fn bench_wide_fanout_import(c: &mut Criterion) {
    use std::sync::OnceLock;

    static TEMP_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

    let fanout = 8usize;
    let temp_dir = TEMP_DIR.get_or_init(|| {
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("oxiproto_bench_fanout_{pid}"));
        std::fs::create_dir_all(&dir).expect("create fanout bench temp dir");

        // Write N leaf protos
        for i in 0..fanout {
            let content = format!(
                r#"syntax = "proto3"; package fanout;
message Leaf{i} {{ int64 id = 1; string name = 2; bool active = 3; }}
"#
            );
            std::fs::write(dir.join(format!("fanout_leaf{i}.proto")), content)
                .expect("write leaf proto");
        }

        // Write root that imports all of them
        let imports: String = (0..fanout)
            .map(|i| format!("import \"fanout_leaf{i}.proto\";\n"))
            .collect();
        let fields: String = (0..fanout)
            .map(|i| format!("  Leaf{i} leaf{i} = {};\n", i + 1))
            .collect();
        let root_content =
            format!("syntax = \"proto3\"; package fanout;\n{imports}message Root {{\n{fields}}}\n");
        std::fs::write(dir.join("fanout_root.proto"), root_content)
            .expect("write fanout_root.proto");

        dir
    });

    let root = temp_dir.join("fanout_root.proto");
    let mut group = c.benchmark_group("import_resolution");
    group.bench_function(format!("wide_fanout_{fanout}"), |b| {
        b.iter(|| {
            oxiproto_build::compile_files_native(
                black_box(&[&root]),
                black_box(&[temp_dir.as_path()]),
            )
            .unwrap()
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_parse_small,
    bench_parse_medium,
    bench_parse_large,
    bench_parse_scaling,
    bench_import_resolution,
    bench_deep_nesting,
    bench_deep_import_chain,
    bench_diamond_import,
    bench_wide_fanout_import,
);
criterion_main!(benches);
