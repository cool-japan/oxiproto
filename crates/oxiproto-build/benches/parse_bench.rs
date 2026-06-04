//! Criterion benchmark comparing the native pure-Rust `.proto` parser
//! against the `protox` path for the same proto3 sources.
//!
//! The native path uses [`oxiproto_build::compile_str_native`] which never
//! touches the filesystem for inline sources.  The protox path uses
//! [`protox::compile`] which requires writing to a temp file and passing a
//! filesystem path.
//!
//! Both paths produce a [`prost_types::FileDescriptorSet`]; the benchmark
//! measures end-to-end throughput (ns/parse) for small, medium, and large
//! proto fixtures.
//!
//! Run with:
//!   cargo bench -p oxiproto-build --bench parse_bench

#![forbid(unsafe_code)]

use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxiproto_build::compile_str_native;

// ---------------------------------------------------------------------------
// Counter for unique temp-file names (protox path)
// ---------------------------------------------------------------------------

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Proto source fixtures
// ---------------------------------------------------------------------------

/// A minimal single-message proto3 file (~60 bytes).
const SMALL_PROTO: &str = r#"syntax = "proto3";
package bench_small;
message Point { double x = 1; double y = 2; double z = 3; }
"#;

/// A proto3 file with messages, enums, a service, map fields, and a oneof
/// (~900 bytes).  Representative of a typical domain model.
const MEDIUM_PROTO: &str = r#"syntax = "proto3";
package bench_medium;

enum Status {
  STATUS_UNSPECIFIED = 0;
  ACTIVE   = 1;
  INACTIVE = 2;
  DELETED  = 3;
}

message Address {
  string street  = 1;
  string city    = 2;
  string state   = 3;
  string zip     = 4;
  string country = 5;
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

message ListPersonsRequest  { string filter = 1; int32 page = 2; int32 page_size = 3; }
message ListPersonsResponse { repeated Person persons = 1; int32 total = 2; bool has_more = 3; }

service PersonService {
  rpc ListPersons (ListPersonsRequest) returns (ListPersonsResponse);
  rpc GetPerson   (ListPersonsRequest) returns (Person);
}
"#;

/// A proto3 file that mirrors the structure of `google/protobuf/descriptor.proto`
/// (~1 800 bytes).  Exercises deep nesting, many messages, and a service.
const LARGE_PROTO: &str = r#"syntax = "proto3";
package bench_large;

enum FieldType {
  FIELD_TYPE_UNSPECIFIED = 0;
  TYPE_DOUBLE   = 1; TYPE_FLOAT    = 2; TYPE_INT64    = 3; TYPE_UINT64   = 4;
  TYPE_INT32    = 5; TYPE_FIXED64  = 6; TYPE_FIXED32  = 7; TYPE_BOOL     = 8;
  TYPE_STRING   = 9; TYPE_GROUP    = 10; TYPE_MESSAGE  = 11; TYPE_BYTES   = 12;
  TYPE_UINT32   = 13; TYPE_ENUM    = 14; TYPE_SFIXED32 = 15; TYPE_SFIXED64= 16;
  TYPE_SINT32   = 17; TYPE_SINT64  = 18;
}

enum FieldLabel {
  FIELD_LABEL_UNSPECIFIED = 0;
  LABEL_OPTIONAL = 1; LABEL_REQUIRED = 2; LABEL_REPEATED = 3;
}

message FieldOption  { string name = 1; string value = 2; }
message FieldOptions {
  bool   packed    = 1; bool   lazy      = 2; bool   deprecated = 3;
  bool   weak      = 4;
  repeated FieldOption uninterpreted_option = 999;
}

message FieldDescriptor {
  string       name          = 1; int32       number        = 3;
  FieldLabel   label         = 4; FieldType   type          = 5;
  string       type_name     = 6; string      default_value = 7;
  FieldOptions options       = 8; int32       oneof_index   = 9;
  string       json_name     = 10;
}

message OneofDescriptor     { string name = 1; }
message EnumValueDescriptor { string name = 1; int32 number = 2; }
message EnumDescriptor {
  string                  name  = 1;
  repeated EnumValueDescriptor value = 2;
}

message MessageDescriptor {
  string                   name       = 1;
  repeated FieldDescriptor field      = 2;
  repeated MessageDescriptor nested_type = 3;
  repeated EnumDescriptor  enum_type  = 4;
  repeated OneofDescriptor oneof_decl = 8;
}

message MethodDescriptor {
  string name = 1; string input_type = 2; string output_type = 3;
  bool client_streaming = 5; bool server_streaming = 6;
}

message ServiceDescriptor {
  string                    name   = 1;
  repeated MethodDescriptor method = 2;
}

message FileDescriptor {
  string                     name         = 1; string package = 2;
  repeated string            dependency   = 3;
  repeated MessageDescriptor message_type = 4;
  repeated EnumDescriptor    enum_type    = 5;
  repeated ServiceDescriptor service      = 6;
  string                     syntax       = 12;
}

message FileDescriptorSet  { repeated FileDescriptor file = 1; }
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
  rpc ListFiles  (DescriptorPool)    returns (stream FileDescriptor);
}
"#;

// ---------------------------------------------------------------------------
// Protox helper: compile an inline proto source via protox::compile
// ---------------------------------------------------------------------------
//
// protox requires real filesystem paths; we write each call to a unique temp
// file, compile, then remove it.  The I/O overhead is intentional — it
// accurately represents the protox path as used from build scripts.

fn protox_compile_str(src: &str) -> prost_types::FileDescriptorSet {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let temp_dir = std::env::temp_dir();
    let filename = format!("oxiproto_parse_bench_{pid}_{n}.proto");
    let proto_path = temp_dir.join(&filename);

    std::fs::write(&proto_path, src).expect("write temp proto");

    let result = protox::compile(
        std::iter::once(filename.as_str()),
        std::iter::once(temp_dir.as_path()),
    );

    // Always clean up; ignore removal errors.
    let _ = std::fs::remove_file(&proto_path);

    result.expect("protox::compile failed on fixture")
}

// ---------------------------------------------------------------------------
// Benchmark group: native parser vs protox for each fixture size
// ---------------------------------------------------------------------------

fn bench_parse_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_small");
    group.throughput(Throughput::Bytes(SMALL_PROTO.len() as u64));

    group.bench_function("native", |b| {
        b.iter(|| compile_str_native(black_box(SMALL_PROTO)).expect("native parse failed"))
    });

    group.bench_function("protox", |b| {
        b.iter(|| protox_compile_str(black_box(SMALL_PROTO)))
    });

    group.finish();
}

fn bench_parse_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_medium");
    group.throughput(Throughput::Bytes(MEDIUM_PROTO.len() as u64));

    group.bench_function("native", |b| {
        b.iter(|| compile_str_native(black_box(MEDIUM_PROTO)).expect("native parse failed"))
    });

    group.bench_function("protox", |b| {
        b.iter(|| protox_compile_str(black_box(MEDIUM_PROTO)))
    });

    group.finish();
}

fn bench_parse_large(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_large");
    group.throughput(Throughput::Bytes(LARGE_PROTO.len() as u64));

    group.bench_function("native", |b| {
        b.iter(|| compile_str_native(black_box(LARGE_PROTO)).expect("native parse failed"))
    });

    group.bench_function("protox", |b| {
        b.iter(|| protox_compile_str(black_box(LARGE_PROTO)))
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark group: throughput scaling across fixture sizes (native only)
// ---------------------------------------------------------------------------

fn bench_parse_scaling(c: &mut Criterion) {
    let sizes: &[usize] = &[1, 5, 10, 20, 50];
    let mut group = c.benchmark_group("parse_scaling_native");

    for &n_messages in sizes {
        let proto = build_n_message_proto(n_messages);
        group.throughput(Throughput::Bytes(proto.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("n_messages", n_messages),
            &proto,
            |b, src| {
                b.iter(|| compile_str_native(black_box(src.as_str())).expect("scale parse failed"))
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
// Criterion harness
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_parse_small,
    bench_parse_medium,
    bench_parse_large,
    bench_parse_scaling,
);
criterion_main!(benches);
