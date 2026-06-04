//! Criterion benchmarks for `oxiproto-codegen` code generation speed.
//!
//! Measures:
//! - `generate_small`  — 10 messages, default options
//! - `generate_medium` — 50 messages, default options
//! - `generate_large`  — 100 messages, default options
//! - `generate_with_oxi_impl` — 20 messages, `emit_oxi_message_impl = true`
//! - `generate_with_json`     — 20 messages, `emit_json = true`
//! - `generate_with_format`   — 20 messages, `format_output = true` (pretty-printing)
//! - `generate_module_tree`   — 20 messages across 4 packages, `generate_module()`
//!
//! Run with:
//! ```text
//! cargo bench -p oxiproto-codegen --no-run   # compile check
//! cargo bench -p oxiproto-codegen            # full benchmark run
//! ```

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet};

// ── FDS builders ─────────────────────────────────────────────────────────────

fn make_field(name: &str, number: i32, r#type: Type, label: Label) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_string()),
        number: Some(number),
        label: Some(label as i32),
        r#type: Some(r#type as i32),
        ..Default::default()
    }
}

/// Build a `DescriptorProto` with a mix of scalar, repeated, and string fields.
fn make_message(name: &str) -> DescriptorProto {
    DescriptorProto {
        name: Some(name.to_string()),
        field: vec![
            make_field("id", 1, Type::Int64, Label::Optional),
            make_field("name", 2, Type::String, Label::Optional),
            make_field("value", 3, Type::Double, Label::Optional),
            make_field("enabled", 4, Type::Bool, Label::Optional),
            make_field("tags", 5, Type::String, Label::Repeated),
            make_field("count", 6, Type::Uint32, Label::Optional),
            make_field("data", 7, Type::Bytes, Label::Optional),
        ],
        ..Default::default()
    }
}

/// Build a `FileDescriptorSet` with `n` messages in a single package.
fn build_flat_fds(n: usize) -> FileDescriptorSet {
    let messages: Vec<DescriptorProto> = (0..n)
        .map(|i| make_message(&format!("Message{i}")))
        .collect();

    let file = FileDescriptorProto {
        name: Some("bench.proto".to_string()),
        package: Some("bench".to_string()),
        message_type: messages,
        ..Default::default()
    };

    FileDescriptorSet { file: vec![file] }
}

/// Build a `FileDescriptorSet` spreading `n` messages across `pkg_count` packages.
///
/// Messages are round-robin assigned to packages `"pkg0"`, `"pkg1"`, …
fn build_namespaced_fds(n: usize, pkg_count: usize) -> FileDescriptorSet {
    let mut pkg_messages: Vec<Vec<DescriptorProto>> = (0..pkg_count).map(|_| vec![]).collect();

    for i in 0..n {
        pkg_messages[i % pkg_count].push(make_message(&format!("Msg{i}")));
    }

    let files: Vec<FileDescriptorProto> = (0..pkg_count)
        .map(|p| FileDescriptorProto {
            name: Some(format!("pkg{p}.proto")),
            package: Some(format!("bench.pkg{p}")),
            message_type: pkg_messages[p].clone(),
            ..Default::default()
        })
        .collect();

    FileDescriptorSet { file: files }
}

// ── Benchmark functions ───────────────────────────────────────────────────────

/// Benchmark default-options codegen at three descriptor-set sizes.
fn bench_generate_flat(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate_flat");

    let fds_small = build_flat_fds(10);
    let fds_medium = build_flat_fds(50);
    let fds_large = build_flat_fds(100);

    let opts = oxiproto_codegen::CodegenOptions::default();

    group.bench_function("10_messages", |b| {
        b.iter(|| {
            let result = oxiproto_codegen::generate_with_options(black_box(&fds_small), &opts);
            black_box(result.unwrap().len())
        })
    });

    group.bench_function("50_messages", |b| {
        b.iter(|| {
            let result = oxiproto_codegen::generate_with_options(black_box(&fds_medium), &opts);
            black_box(result.unwrap().len())
        })
    });

    group.bench_function("100_messages", |b| {
        b.iter(|| {
            let result = oxiproto_codegen::generate_with_options(black_box(&fds_large), &opts);
            black_box(result.unwrap().len())
        })
    });

    group.finish();
}

/// Benchmark `emit_oxi_message_impl = true` — adds encode/decode dispatch code.
fn bench_generate_oxi_impl(c: &mut Criterion) {
    let fds = build_flat_fds(20);
    let opts = oxiproto_codegen::CodegenOptions {
        emit_oxi_message_impl: true,
        ..Default::default()
    };

    c.bench_function("generate_oxi_impl_20", |b| {
        b.iter(|| {
            let result = oxiproto_codegen::generate_with_options(black_box(&fds), &opts);
            black_box(result.unwrap().len())
        })
    });
}

/// Benchmark `emit_json = true` — adds camelCase JSON methods per message.
fn bench_generate_json(c: &mut Criterion) {
    let fds = build_flat_fds(20);
    let opts = oxiproto_codegen::CodegenOptions {
        emit_json: true,
        ..Default::default()
    };

    c.bench_function("generate_json_20", |b| {
        b.iter(|| {
            let result = oxiproto_codegen::generate_with_options(black_box(&fds), &opts);
            black_box(result.unwrap().len())
        })
    });
}

/// Benchmark `format_output = true` — adds prettyplease formatting.
#[cfg(feature = "format")]
fn bench_generate_formatted(c: &mut Criterion) {
    let fds = build_flat_fds(20);
    let opts = oxiproto_codegen::CodegenOptions {
        format_output: true,
        ..Default::default()
    };

    c.bench_function("generate_formatted_20", |b| {
        b.iter(|| {
            let result = oxiproto_codegen::generate_with_options(black_box(&fds), &opts);
            black_box(result.unwrap().len())
        })
    });
}

/// Benchmark `generate_module()` with multiple packages.
fn bench_generate_module_tree(c: &mut Criterion) {
    let fds = build_namespaced_fds(20, 4);
    let opts = oxiproto_codegen::CodegenOptions {
        package_namespacing: true,
        ..Default::default()
    };

    c.bench_function("generate_module_tree_20x4pkg", |b| {
        b.iter(|| {
            let result = oxiproto_codegen::generate_module(black_box(&fds), &opts);
            black_box(result.unwrap().render().len())
        })
    });
}

/// Benchmark streaming output via `generate_to_writer` against `generate_with_options`.
///
/// This measures whether the `Write`-based path (which avoids an extra copy)
/// is faster than building a `String` first.
fn bench_streaming_vs_string(c: &mut Criterion) {
    let fds = build_flat_fds(50);
    let opts = oxiproto_codegen::CodegenOptions::default();

    let mut group = c.benchmark_group("streaming_vs_string_50");

    group.bench_function("generate_to_string", |b| {
        b.iter(|| {
            let result = oxiproto_codegen::generate_with_options(black_box(&fds), &opts);
            black_box(result.unwrap().len())
        })
    });

    group.bench_function("generate_to_writer_vec", |b| {
        b.iter(|| {
            let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);
            oxiproto_codegen::generate_to_writer(black_box(&fds), &opts, &mut buf).unwrap();
            black_box(buf.len())
        })
    });

    group.finish();
}

// ── Group registration ────────────────────────────────────────────────────────

#[cfg(feature = "format")]
criterion_group!(
    benches,
    bench_generate_flat,
    bench_generate_oxi_impl,
    bench_generate_json,
    bench_generate_formatted,
    bench_generate_module_tree,
    bench_streaming_vs_string,
);

#[cfg(not(feature = "format"))]
criterion_group!(
    benches,
    bench_generate_flat,
    bench_generate_oxi_impl,
    bench_generate_json,
    bench_generate_module_tree,
    bench_streaming_vs_string,
);

criterion_main!(benches);
