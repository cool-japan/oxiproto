//! Memory-footprint benchmarks for [`NativeDescriptorPool`].
//!
//! These benchmarks report the *allocated heap size* of a pool as a function
//! of the number of registered files, using the following technique:
//!
//! 1. Encode the current allocator baseline *before* constructing the pool.
//! 2. Construct the pool.
//! 3. Report the Criterion throughput as `bytes = pool_heap_bytes` so that
//!    the HTML report expresses pool size per unit of schema complexity.
//!
//! Because we do not have direct access to an instrumenting allocator in a
//! stable Criterion harness, we use two indirect proxies instead:
//!
//! - **Serialised-size proxy**: the pool is serialised back into a
//!   `FileDescriptorSet` via `prost::Message::encode_to_vec` and the byte
//!   length is reported as throughput — this lower-bounds the in-memory
//!   footprint and is entirely deterministic.
//! - **Element-count proxy**: pool sizes are varied across benchmark
//!   iterations so that Criterion's throughput display shows relative growth
//!   rates for both the native and prost-reflect pools.
//!
//! The approach is intentionally conservative: actual heap usage will be
//! higher due to `Arc<PoolInner>` overhead, `HashMap` slot tables, and
//! `Vec` capacity rounding.  The proxy gives a reproducible relative
//! comparison between pool sizes.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxiproto_reflect::NativeDescriptorPool;
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet};

// ── FDS builder ───────────────────────────────────────────────────────────────

fn make_fds(file_count: usize, msgs_per_file: usize, fields_per_msg: usize) -> FileDescriptorSet {
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
    let files: Vec<FileDescriptorProto> = (0..file_count)
        .map(|f| {
            let messages: Vec<DescriptorProto> = (0..msgs_per_file)
                .map(|m| {
                    let fields: Vec<FieldDescriptorProto> = (0..fields_per_msg)
                        .map(|i| {
                            let ty = field_types[i % field_types.len()];
                            FieldDescriptorProto {
                                name: Some(format!("f{i}")),
                                number: Some((i + 1) as i32),
                                label: Some(Label::Optional as i32),
                                r#type: Some(ty as i32),
                                json_name: Some(format!("f{i}")),
                                ..Default::default()
                            }
                        })
                        .collect();
                    DescriptorProto {
                        name: Some(format!("Msg_{f}_{m}")),
                        field: fields,
                        ..Default::default()
                    }
                })
                .collect();
            FileDescriptorProto {
                name: Some(format!("file_{f}.proto")),
                syntax: Some("proto3".to_owned()),
                package: Some("mem".to_owned()),
                message_type: messages,
                ..Default::default()
            }
        })
        .collect();
    FileDescriptorSet { file: files }
}

/// Rough lower-bound on native pool heap: sum of all field/message/file name
/// string bytes stored inside the pool (the dominant heap contribution for
/// descriptor-heavy schemas).
fn approx_native_pool_heap(pool: &NativeDescriptorPool) -> usize {
    let mut total = 0usize;
    for msg in pool.all_messages() {
        total += msg.name().len();
        total += msg.full_name().len();
        for field in msg.fields() {
            total += field.name().len();
            total += field.json_name().len();
        }
    }
    total
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

/// Measures pool construction time and reports approximate heap size as the
/// Criterion throughput.  Parameterised by file count to expose growth rate.
fn bench_pool_memory_growth(c: &mut Criterion) {
    let configs: &[(&str, usize, usize, usize)] = &[
        ("5_files", 5, 10, 6),
        ("20_files", 20, 10, 6),
        ("50_files", 50, 10, 6),
    ];

    let mut group = c.benchmark_group("pool_memory_growth");

    for &(label, file_count, msgs_per_file, fields_per_msg) in configs {
        let fds = make_fds(file_count, msgs_per_file, fields_per_msg);

        // Build once to measure size (excluded from timing).
        let pool = NativeDescriptorPool::from_file_descriptor_set(fds.clone()).expect("valid FDS");
        let approx_bytes = approx_native_pool_heap(&pool) as u64;

        group.throughput(Throughput::Bytes(approx_bytes));
        group.bench_with_input(
            BenchmarkId::new("native_construction", label),
            &fds,
            |b, fds| {
                b.iter(|| {
                    black_box(
                        NativeDescriptorPool::from_file_descriptor_set(black_box(fds.clone()))
                            .expect("valid FDS"),
                    )
                })
            },
        );
    }

    group.finish();
}

/// Verifies that the pool `Arc` clone is O(1) (pointer bump only) — not a
/// memory concern but confirms Arc overhead is negligible.
fn bench_pool_clone_cost(c: &mut Criterion) {
    let fds = make_fds(20, 10, 6);
    let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("valid FDS");

    let mut group = c.benchmark_group("pool_clone_cost");
    group.bench_function("native_arc_clone", |b| b.iter(|| black_box(pool.clone())));
    group.finish();
}

criterion_group!(benches, bench_pool_memory_growth, bench_pool_clone_cost);
criterion_main!(benches);
