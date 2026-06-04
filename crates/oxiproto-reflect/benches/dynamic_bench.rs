//! Criterion benchmarks for [`NativeDynamicMessage`] encode/decode.
//!
//! Compares the native pure-Rust [`NativeDynamicMessage`] encode/decode path
//! against a statically-generated `prost::Message` type with an identical
//! schema, giving a direct performance comparison between dynamic reflection
//! and compile-time codegen.
//!
//! Three message shapes are exercised:
//!
//! - **scalar**: 8 fields covering the most common scalar kinds.
//! - **repeated**: a message with a `repeated string` field holding 10 entries.
//! - **nested**: a top-level message with a nested sub-message.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use oxiproto_reflect::{NativeDescriptorPool, NativeDynamicMessage, NativeValue};
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet};

// ── FDS helpers ───────────────────────────────────────────────────────────────

fn field(name: &str, number: i32, ty: Type, label: Label) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_owned()),
        number: Some(number),
        label: Some(label as i32),
        r#type: Some(ty as i32),
        json_name: Some(name.to_owned()),
        ..Default::default()
    }
}

fn scalar_fds() -> FileDescriptorSet {
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("scalar.proto".to_owned()),
            syntax: Some("proto3".to_owned()),
            package: Some("bench".to_owned()),
            message_type: vec![DescriptorProto {
                name: Some("ScalarMsg".to_owned()),
                field: vec![
                    field("f_int32", 1, Type::Int32, Label::Optional),
                    field("f_int64", 2, Type::Int64, Label::Optional),
                    field("f_uint32", 3, Type::Uint32, Label::Optional),
                    field("f_uint64", 4, Type::Uint64, Label::Optional),
                    field("f_bool", 5, Type::Bool, Label::Optional),
                    field("f_float", 6, Type::Float, Label::Optional),
                    field("f_double", 7, Type::Double, Label::Optional),
                    field("f_string", 8, Type::String, Label::Optional),
                ],
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
}

fn repeated_fds() -> FileDescriptorSet {
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("repeated.proto".to_owned()),
            syntax: Some("proto3".to_owned()),
            package: Some("bench".to_owned()),
            message_type: vec![DescriptorProto {
                name: Some("RepMsg".to_owned()),
                field: vec![
                    field("id", 1, Type::Int32, Label::Optional),
                    field("tags", 2, Type::String, Label::Repeated),
                ],
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
}

fn nested_fds() -> FileDescriptorSet {
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("nested.proto".to_owned()),
            syntax: Some("proto3".to_owned()),
            package: Some("bench".to_owned()),
            message_type: vec![
                DescriptorProto {
                    name: Some("Inner".to_owned()),
                    field: vec![
                        field("x", 1, Type::Int32, Label::Optional),
                        field("y", 2, Type::String, Label::Optional),
                    ],
                    ..Default::default()
                },
                DescriptorProto {
                    name: Some("Outer".to_owned()),
                    field: vec![
                        field("id", 1, Type::Int32, Label::Optional),
                        FieldDescriptorProto {
                            name: Some("inner".to_owned()),
                            number: Some(2),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Message as i32),
                            type_name: Some(".bench.Inner".to_owned()),
                            json_name: Some("inner".to_owned()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
    }
}

fn build_native_nested(pool: &NativeDescriptorPool) -> NativeDynamicMessage {
    let inner_desc = pool.get_message_by_name("bench.Inner").expect("Inner");
    let outer_desc = pool.get_message_by_name("bench.Outer").expect("Outer");

    let f_x = inner_desc.get_field_by_name("x").expect("x");
    let f_y = inner_desc.get_field_by_name("y").expect("y");
    let f_inner = outer_desc.get_field_by_name("inner").expect("inner");

    let mut inner = NativeDynamicMessage::new(inner_desc);
    inner.set_field(&f_x, NativeValue::I32(10));
    inner.set_field(&f_y, NativeValue::String("hello world".to_owned()));

    let mut outer = NativeDynamicMessage::new(outer_desc);
    outer.set_field(&f_inner, NativeValue::Message(Box::new(inner)));
    outer
}

// ── Static prost equivalent (scalar shape) ───────────────────────────────────

#[derive(Clone, prost::Message)]
struct ProstScalarMsg {
    #[prost(int32, tag = "1")]
    f_int32: i32,
    #[prost(int64, tag = "2")]
    f_int64: i64,
    #[prost(uint32, tag = "3")]
    f_uint32: u32,
    #[prost(uint64, tag = "4")]
    f_uint64: u64,
    #[prost(bool, tag = "5")]
    f_bool: bool,
    #[prost(float, tag = "6")]
    f_float: f32,
    #[prost(double, tag = "7")]
    f_double: f64,
    #[prost(string, tag = "8")]
    f_string: String,
}

// ── Native message builders ───────────────────────────────────────────────────

fn build_native_scalar(pool: &NativeDescriptorPool) -> NativeDynamicMessage {
    let desc = pool
        .get_message_by_name("bench.ScalarMsg")
        .expect("ScalarMsg");
    let mut msg = NativeDynamicMessage::new(desc.clone());
    let f = |name: &str| desc.get_field_by_name(name).expect("field");
    msg.set_field(&f("f_int32"), NativeValue::I32(42));
    msg.set_field(&f("f_int64"), NativeValue::I64(i64::MAX / 2));
    msg.set_field(&f("f_uint32"), NativeValue::U32(100_000));
    msg.set_field(&f("f_uint64"), NativeValue::U64(u64::MAX / 3));
    msg.set_field(&f("f_bool"), NativeValue::Bool(true));
    msg.set_field(&f("f_float"), NativeValue::F32(std::f32::consts::PI));
    msg.set_field(&f("f_double"), NativeValue::F64(std::f64::consts::E));
    msg.set_field(
        &f("f_string"),
        NativeValue::String("hello world".to_owned()),
    );
    msg
}

fn build_native_repeated(pool: &NativeDescriptorPool) -> NativeDynamicMessage {
    let desc = pool.get_message_by_name("bench.RepMsg").expect("RepMsg");
    let mut msg = NativeDynamicMessage::new(desc.clone());
    let f_id = desc.get_field_by_name("id").expect("id");
    let f_tags = desc.get_field_by_name("tags").expect("tags");
    msg.set_field(&f_id, NativeValue::I32(7));
    let tags: Vec<NativeValue> = (0..10)
        .map(|i| NativeValue::String(format!("tag_{i}")))
        .collect();
    msg.set_field(&f_tags, NativeValue::List(tags));
    msg
}

// ── Encode benchmarks ─────────────────────────────────────────────────────────

fn bench_encode(c: &mut Criterion) {
    // ── Native
    let scalar_pool =
        NativeDescriptorPool::from_file_descriptor_set(scalar_fds()).expect("scalar FDS");
    let repeated_pool =
        NativeDescriptorPool::from_file_descriptor_set(repeated_fds()).expect("repeated FDS");
    let nested_pool =
        NativeDescriptorPool::from_file_descriptor_set(nested_fds()).expect("nested FDS");

    let scalar_native = build_native_scalar(&scalar_pool);
    let repeated_native = build_native_repeated(&repeated_pool);
    let nested_native = build_native_nested(&nested_pool);

    // ── Prost equivalent for the scalar case
    let prost_scalar = ProstScalarMsg {
        f_int32: 42,
        f_int64: i64::MAX / 2,
        f_uint32: 100_000,
        f_uint64: u64::MAX / 3,
        f_bool: true,
        f_float: std::f32::consts::PI,
        f_double: std::f64::consts::E,
        f_string: "hello world".to_owned(),
    };

    let mut group = c.benchmark_group("dynamic_encode");

    group.bench_with_input(
        BenchmarkId::new("native", "scalar"),
        &scalar_native,
        |b, msg| b.iter(|| black_box(msg.encode_to_vec().expect("encode"))),
    );

    group.bench_with_input(
        BenchmarkId::new("prost_static", "scalar"),
        &prost_scalar,
        |b, msg| {
            use prost::Message;
            b.iter(|| black_box(msg.encode_to_vec()))
        },
    );

    group.bench_with_input(
        BenchmarkId::new("native", "repeated"),
        &repeated_native,
        |b, msg| b.iter(|| black_box(msg.encode_to_vec().expect("encode"))),
    );

    group.bench_with_input(
        BenchmarkId::new("native", "nested"),
        &nested_native,
        |b, msg| b.iter(|| black_box(msg.encode_to_vec().expect("encode"))),
    );

    group.finish();
}

// ── Decode benchmarks ─────────────────────────────────────────────────────────

fn bench_decode(c: &mut Criterion) {
    // Pre-encode the scalar message to get the bytes.
    let scalar_pool =
        NativeDescriptorPool::from_file_descriptor_set(scalar_fds()).expect("scalar FDS");
    let scalar_native = build_native_scalar(&scalar_pool);
    let encoded_bytes = scalar_native.encode_to_vec().expect("encode");

    let prost_scalar = ProstScalarMsg {
        f_int32: 42,
        f_int64: i64::MAX / 2,
        f_uint32: 100_000,
        f_uint64: u64::MAX / 3,
        f_bool: true,
        f_float: std::f32::consts::PI,
        f_double: std::f64::consts::E,
        f_string: "hello world".to_owned(),
    };
    use prost::Message as _;
    let prost_bytes = prost_scalar.encode_to_vec();

    let scalar_desc = scalar_pool
        .get_message_by_name("bench.ScalarMsg")
        .expect("ScalarMsg");

    let mut group = c.benchmark_group("dynamic_decode");

    group.bench_function("native/scalar", |b| {
        b.iter(|| {
            black_box(
                NativeDynamicMessage::decode(
                    // MessageDescriptor is Clone+Arc-backed; clone is cheap.
                    black_box(scalar_desc.clone()),
                    black_box(encoded_bytes.as_slice()),
                )
                .expect("decode"),
            )
        })
    });

    group.bench_function("prost_static/scalar", |b| {
        b.iter(|| {
            use prost::Message as _;
            black_box(ProstScalarMsg::decode(black_box(prost_bytes.as_slice())).expect("decode"))
        })
    });

    group.finish();
}

// ── Round-trip benchmarks ─────────────────────────────────────────────────────

fn bench_roundtrip(c: &mut Criterion) {
    let scalar_pool =
        NativeDescriptorPool::from_file_descriptor_set(scalar_fds()).expect("scalar FDS");
    let scalar_native = build_native_scalar(&scalar_pool);
    let scalar_desc = scalar_pool
        .get_message_by_name("bench.ScalarMsg")
        .expect("ScalarMsg");

    let prost_scalar = ProstScalarMsg {
        f_int32: 42,
        f_int64: i64::MAX / 2,
        f_uint32: 100_000,
        f_uint64: u64::MAX / 3,
        f_bool: true,
        f_float: std::f32::consts::PI,
        f_double: std::f64::consts::E,
        f_string: "hello world".to_owned(),
    };

    let mut group = c.benchmark_group("dynamic_roundtrip");

    group.bench_function("native/scalar", |b| {
        b.iter(|| {
            let bytes = black_box(&scalar_native).encode_to_vec().expect("encode");
            black_box(
                NativeDynamicMessage::decode(black_box(scalar_desc.clone()), black_box(&bytes))
                    .expect("decode"),
            )
        })
    });

    group.bench_function("prost_static/scalar", |b| {
        use prost::Message as _;
        b.iter(|| {
            let bytes = prost_scalar.encode_to_vec();
            black_box(ProstScalarMsg::decode(black_box(bytes.as_slice())).expect("decode"))
        })
    });

    group.finish();
}

criterion_group!(benches, bench_encode, bench_decode, bench_roundtrip);
criterion_main!(benches);
