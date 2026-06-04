//! Criterion benchmarks for [`NativeDescriptorPool`] construction.
//!
//! Measures the cost of building a descriptor pool from a
//! [`prost_types::FileDescriptorSet`] across three representative sizes:
//!
//! - **small**: single file, one message, two scalar fields, one service with
//!   one method.
//! - **medium**: 10 files, 5 messages per file (50 total), 4 fields each, 5 enums,
//!   3 services.
//! - **large**: 50 files, 20 messages per file (1 000 total), 8 fields each,
//!   20 enums, 10 services — covers the "many registered files" memory-growth
//!   and lookup-index build time.
//!
//! The `prost-reflect`-backed [`DescriptorPool`] is included as a reference
//! oracle so we can compare the two paths side-by-side.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use oxiproto_reflect::{pool_from_fds, NativeDescriptorPool};
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FileDescriptorProto, FileDescriptorSet, MethodDescriptorProto, ServiceDescriptorProto,
};

// ── FDS builders ─────────────────────────────────────────────────────────────

fn scalar_field(name: &str, number: i32, ty: Type) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_owned()),
        number: Some(number),
        label: Some(Label::Optional as i32),
        r#type: Some(ty as i32),
        json_name: Some(name.to_owned()),
        ..Default::default()
    }
}

fn simple_message(name: &str, field_count: usize) -> DescriptorProto {
    let field_types = [
        Type::Int32,
        Type::String,
        Type::Bool,
        Type::Int64,
        Type::Uint32,
        Type::Float,
        Type::Double,
        Type::Bytes,
    ];
    let fields: Vec<FieldDescriptorProto> = (0..field_count)
        .map(|i| {
            let ty = field_types[i % field_types.len()];
            scalar_field(&format!("field_{i}"), (i + 1) as i32, ty)
        })
        .collect();
    DescriptorProto {
        name: Some(name.to_owned()),
        field: fields,
        ..Default::default()
    }
}

fn simple_enum(name: &str, value_count: usize) -> EnumDescriptorProto {
    let values: Vec<EnumValueDescriptorProto> = (0..value_count)
        .map(|i| EnumValueDescriptorProto {
            name: Some(format!("{name}_VALUE_{i}")),
            number: Some(i as i32),
            ..Default::default()
        })
        .collect();
    EnumDescriptorProto {
        name: Some(name.to_owned()),
        value: values,
        ..Default::default()
    }
}

fn simple_service(name: &str, method_count: usize) -> ServiceDescriptorProto {
    let methods: Vec<MethodDescriptorProto> = (0..method_count)
        .map(|i| MethodDescriptorProto {
            name: Some(format!("Method{i}")),
            input_type: Some(".benchpkg.RequestMsg".to_owned()),
            output_type: Some(".benchpkg.ResponseMsg".to_owned()),
            client_streaming: Some(false),
            server_streaming: Some(false),
            ..Default::default()
        })
        .collect();
    ServiceDescriptorProto {
        name: Some(name.to_owned()),
        method: methods,
        ..Default::default()
    }
}

/// Build the "anchor" file that all services reference for input/output types.
fn anchor_file() -> FileDescriptorProto {
    FileDescriptorProto {
        name: Some("anchor.proto".to_owned()),
        syntax: Some("proto3".to_owned()),
        package: Some("benchpkg".to_owned()),
        message_type: vec![
            simple_message("RequestMsg", 2),
            simple_message("ResponseMsg", 2),
        ],
        ..Default::default()
    }
}

/// Build a small single-file FDS: 1 message + 1 service.
fn small_fds() -> FileDescriptorSet {
    FileDescriptorSet {
        file: vec![
            anchor_file(),
            FileDescriptorProto {
                name: Some("small.proto".to_owned()),
                syntax: Some("proto3".to_owned()),
                package: Some("benchpkg".to_owned()),
                message_type: vec![simple_message("SmallMsg", 2)],
                service: vec![simple_service("SmallSvc", 1)],
                dependency: vec!["anchor.proto".to_owned()],
                ..Default::default()
            },
        ],
    }
}

/// Build a medium FDS: 10 files × 5 messages, 5 enums, 3 services.
fn medium_fds() -> FileDescriptorSet {
    let mut files = vec![anchor_file()];
    for file_idx in 0..10usize {
        let messages: Vec<DescriptorProto> = (0..5)
            .map(|m| simple_message(&format!("MediumMsg_{file_idx}_{m}"), 4))
            .collect();
        let enums: Vec<EnumDescriptorProto> = (0..1)
            .map(|e| simple_enum(&format!("MediumEnum_{file_idx}_{e}"), 3))
            .collect();
        let services: Vec<ServiceDescriptorProto> = if file_idx < 3 {
            vec![simple_service(&format!("MediumSvc_{file_idx}"), 2)]
        } else {
            vec![]
        };
        files.push(FileDescriptorProto {
            name: Some(format!("medium_{file_idx}.proto")),
            syntax: Some("proto3".to_owned()),
            package: Some("benchpkg".to_owned()),
            message_type: messages,
            enum_type: enums,
            service: services,
            dependency: vec!["anchor.proto".to_owned()],
            ..Default::default()
        });
    }
    FileDescriptorSet { file: files }
}

/// Build a large FDS: 50 files × 20 messages (1 000 total), 8 fields each,
/// 20 enums, 10 services. Exercises index-build time and memory growth.
fn large_fds() -> FileDescriptorSet {
    let mut files = vec![anchor_file()];
    for file_idx in 0..50usize {
        let messages: Vec<DescriptorProto> = (0..20)
            .map(|m| simple_message(&format!("LargeMsg_{file_idx}_{m}"), 8))
            .collect();
        let enums: Vec<EnumDescriptorProto> = (0..2)
            .map(|e| simple_enum(&format!("LargeEnum_{file_idx}_{e}"), 5))
            .collect();
        let services: Vec<ServiceDescriptorProto> = if file_idx < 10 {
            vec![simple_service(&format!("LargeSvc_{file_idx}"), 3)]
        } else {
            vec![]
        };
        files.push(FileDescriptorProto {
            name: Some(format!("large_{file_idx}.proto")),
            syntax: Some("proto3".to_owned()),
            package: Some("benchpkg".to_owned()),
            message_type: messages,
            enum_type: enums,
            service: services,
            dependency: vec!["anchor.proto".to_owned()],
            ..Default::default()
        });
    }
    FileDescriptorSet { file: files }
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

fn bench_pool_construction_native(c: &mut Criterion) {
    let small = small_fds();
    let medium = medium_fds();
    let large = large_fds();

    let mut group = c.benchmark_group("pool_construction/native");

    for (label, fds) in [("small", &small), ("medium", &medium), ("large", &large)] {
        group.bench_with_input(BenchmarkId::new("from_fds", label), fds, |b, fds| {
            b.iter(|| {
                black_box(
                    NativeDescriptorPool::from_file_descriptor_set(black_box(fds.clone()))
                        .expect("valid FDS"),
                )
            })
        });
    }

    group.finish();
}

fn bench_pool_construction_prost_reflect(c: &mut Criterion) {
    let small = small_fds();
    let medium = medium_fds();
    let large = large_fds();

    let mut group = c.benchmark_group("pool_construction/prost_reflect");

    for (label, fds) in [("small", &small), ("medium", &medium), ("large", &large)] {
        group.bench_with_input(BenchmarkId::new("from_fds", label), fds, |b, fds| {
            b.iter(|| black_box(pool_from_fds(black_box(fds.clone())).expect("valid FDS")))
        });
    }

    group.finish();
}

/// Benchmark name-lookup performance after pool construction.
fn bench_pool_lookup(c: &mut Criterion) {
    let large = large_fds();
    let native_pool =
        NativeDescriptorPool::from_file_descriptor_set(large.clone()).expect("valid FDS");
    let prost_pool = pool_from_fds(large).expect("valid FDS");

    let mut group = c.benchmark_group("pool_lookup");

    group.bench_function("native/get_message_by_name", |b| {
        b.iter(|| black_box(native_pool.get_message_by_name(black_box("benchpkg.LargeMsg_25_10"))))
    });

    group.bench_function("prost_reflect/get_message_by_name", |b| {
        b.iter(|| black_box(prost_pool.get_message_by_name(black_box("benchpkg.LargeMsg_25_10"))))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_pool_construction_native,
    bench_pool_construction_prost_reflect,
    bench_pool_lookup
);
criterion_main!(benches);
