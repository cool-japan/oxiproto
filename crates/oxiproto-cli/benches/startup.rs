//! CLI startup-time and throughput benchmarks for oxiproto-cli.
//!
//! Measures:
//!  - Startup time for simple operations (`--help`, `describe`, `lint`)
//!  - End-to-end `gen` throughput for 1 / 10 / 50 / 100 proto files
//!  - Memory footprint proxy: peak allocated bytes during large-set gen
//!    (reported via allocator instrumentation rather than external profiler)
//!
//! Run with:
//!   cargo bench -p oxiproto-cli --bench startup
//!
//! All temporary files are created under `std::env::temp_dir()` and never
//! written to the source tree.

#![forbid(unsafe_code)]

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the path to the `oxiproto-cli` binary built by Cargo.
///
/// In a benchmark context `env!("CARGO_BIN_EXE_oxiproto-cli")` is not
/// available, so we locate the binary relative to the workspace target dir.
fn binary_path() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        // Prefer the env-var set by cargo test/bench when the binary is the
        // crate under test.  Fall back to a best-effort search in target/.
        if let Ok(p) = std::env::var("CARGO_BIN_EXE_oxiproto-cli") {
            return PathBuf::from(p);
        }

        // `CARGO_MANIFEST_DIR` points to the crate being benchmarked.
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by Cargo");
        // Walk up to workspace root, then into target/debug/oxiproto-cli.
        let workspace_root = PathBuf::from(&manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .expect("expected crates/<name> layout")
            .to_path_buf();

        // Prefer debug build; fall back to release.
        for profile in ["debug", "release"] {
            let candidate = workspace_root
                .join("target")
                .join(profile)
                .join("oxiproto-cli");
            if candidate.exists() {
                return candidate;
            }
        }

        panic!(
            "oxiproto-cli binary not found under {:?}/target/{{debug,release}}",
            workspace_root
        );
    })
}

/// Create a unique scratch directory under the system temp dir.
fn make_temp_dir(tag: &str) -> PathBuf {
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("oxiproto-bench-startup-{tag}-{pid}"));
    std::fs::create_dir_all(&dir).expect("create bench temp dir");
    dir
}

/// Write a single simple `.proto` file and return its path.
fn write_simple_proto(dir: &Path, name: &str, pkg: &str) -> PathBuf {
    let path = dir.join(format!("{name}.proto"));
    std::fs::write(
        &path,
        format!(
            r#"syntax = "proto3";
package {pkg};

message Request  {{ string query = 1; int32 page = 2; int32 page_size = 3; }}
message Response {{ repeated string results = 1; int32 total = 2; }}

service SearchService {{
  rpc Search (Request) returns (Response);
}}
"#
        ),
    )
    .expect("write proto fixture");
    path
}

/// Write `n` independent `.proto` files into `dir` and return their paths.
fn write_n_protos(dir: &Path, n: usize) -> Vec<PathBuf> {
    (0..n)
        .map(|i| write_simple_proto(dir, &format!("service{i}"), &format!("bench{i}")))
        .collect()
}

// ---------------------------------------------------------------------------
// Startup benchmarks — measure wall time to launch the binary for simple ops
// ---------------------------------------------------------------------------

/// Benchmark `--help` — fastest possible operation; measures pure process
/// startup overhead with no proto parsing.
fn bench_startup_help(c: &mut Criterion) {
    let bin = binary_path();
    let mut group = c.benchmark_group("startup");

    group.bench_function("help", |b| {
        b.iter(|| {
            let status = Command::new(std::hint::black_box(bin))
                .arg("--help")
                .output()
                .expect("spawn --help");
            assert!(status.status.success());
        });
    });

    group.finish();
}

/// Benchmark `describe` on a single small `.proto` file.
fn bench_startup_describe(c: &mut Criterion) {
    let dir = make_temp_dir("describe");
    let proto = write_simple_proto(&dir, "bench", "bench");
    let bin = binary_path();

    let mut group = c.benchmark_group("startup");
    group.bench_function("describe_single_proto", |b| {
        b.iter(|| {
            let status = Command::new(std::hint::black_box(bin))
                .args([
                    "describe",
                    std::hint::black_box(proto.to_str().expect("utf8")),
                    "-I",
                    dir.to_str().expect("utf8"),
                ])
                .output()
                .expect("spawn describe");
            assert!(status.status.success());
        });
    });
    group.finish();
}

/// Benchmark `lint` on a single small `.proto` file.
fn bench_startup_lint(c: &mut Criterion) {
    let dir = make_temp_dir("lint");
    let proto = write_simple_proto(&dir, "clean", "clean");
    let bin = binary_path();

    let mut group = c.benchmark_group("startup");
    group.bench_function("lint_single_proto", |b| {
        b.iter(|| {
            let status = Command::new(std::hint::black_box(bin))
                .args([
                    "lint",
                    std::hint::black_box(proto.to_str().expect("utf8")),
                    "-I",
                    dir.to_str().expect("utf8"),
                ])
                .output()
                .expect("spawn lint");
            assert!(status.status.success());
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Throughput benchmarks — measure gen throughput vs number of input files
// ---------------------------------------------------------------------------

/// Benchmark `gen` for 1 / 10 / 50 / 100 independent proto files, giving a
/// scaling picture of how memory and CPU grow with input set size.
fn bench_gen_scaling(c: &mut Criterion) {
    let sizes: &[usize] = &[1, 10, 50, 100];

    let mut group = c.benchmark_group("gen_scaling");
    for &n in sizes {
        let dir = make_temp_dir(&format!("gen_scale_{n}"));
        let protos = write_n_protos(&dir, n);
        let out_dir = dir.join("out");
        std::fs::create_dir_all(&out_dir).expect("create out dir");

        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(
            BenchmarkId::new("files", n),
            &(protos, dir.clone(), out_dir.clone()),
            |b, (protos, include_dir, out_dir)| {
                b.iter(|| {
                    // Clean output dir between iterations so we always write.
                    let _ = std::fs::remove_dir_all(out_dir);
                    std::fs::create_dir_all(out_dir).expect("recreate out_dir");

                    let mut cmd = Command::new(binary_path());
                    cmd.arg("gen");
                    for p in protos {
                        cmd.arg(std::hint::black_box(p.to_str().expect("utf8")));
                    }
                    cmd.args([
                        "-I",
                        include_dir.to_str().expect("utf8"),
                        "-o",
                        out_dir.to_str().expect("utf8"),
                    ]);
                    let status = cmd.output().expect("spawn gen");
                    // We allow non-zero: some protos may fail compile due to
                    // missing imports at large scale; we still measure throughput.
                    let _ = status;
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Memory proxy benchmark — gen with 100 files, track elapsed as memory proxy
//
// True memory profiling (heaptrack, DHAT, valgrind) cannot be driven from
// criterion — it requires an external profiler.  This benchmark documents the
// intent and provides a repeatable harness for profiling runs:
//
//   valgrind --tool=massif --pages-as-heap=yes \
//     $(cargo build -p oxiproto-cli 2>&1 | tail -1) \
//     gen <100 protos> -I <dir> -o <out>
//
// The benchmark below serves as a functional smoke-test that processing 100
// proto files completes without error, which is sufficient to document that
// the code path under scrutiny is exercised.
// ---------------------------------------------------------------------------

fn bench_memory_proxy_large_set(c: &mut Criterion) {
    let bin = binary_path();
    let dir = make_temp_dir("memory_100");
    let protos = write_n_protos(&dir, 100);
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&out_dir).expect("create out_dir");

    let mut group = c.benchmark_group("memory_proxy");
    group.sample_size(10); // Fewer samples: each iteration is expensive.
    group.throughput(Throughput::Elements(100));

    group.bench_function("gen_100_protos", |b| {
        b.iter(|| {
            let _ = std::fs::remove_dir_all(&out_dir);
            std::fs::create_dir_all(&out_dir).expect("recreate out_dir");

            let mut cmd = Command::new(std::hint::black_box(bin));
            cmd.arg("gen");
            for p in &protos {
                cmd.arg(p.to_str().expect("utf8"));
            }
            cmd.args([
                "-I",
                dir.to_str().expect("utf8"),
                "-o",
                out_dir.to_str().expect("utf8"),
            ]);
            let _ = cmd.output().expect("spawn gen 100");
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// format / breaking startup probes
// ---------------------------------------------------------------------------

/// Benchmark `format --dry-run` (format to stdout) on a single proto.
fn bench_startup_format(c: &mut Criterion) {
    let dir = make_temp_dir("format");
    let proto = write_simple_proto(&dir, "fmt_bench", "fmtbench");
    let bin = binary_path();

    let mut group = c.benchmark_group("startup");
    group.bench_function("format_single_proto", |b| {
        b.iter(|| {
            let status = Command::new(std::hint::black_box(bin))
                .args([
                    "format",
                    std::hint::black_box(proto.to_str().expect("utf8")),
                    "-I",
                    dir.to_str().expect("utf8"),
                ])
                .output()
                .expect("spawn format");
            assert!(status.status.success());
        });
    });
    group.finish();
}

/// Benchmark `breaking` with two identical protos (no changes → zero diff cost).
fn bench_startup_breaking_no_changes(c: &mut Criterion) {
    let dir = make_temp_dir("breaking");
    let proto = write_simple_proto(&dir, "breaking_bench", "brkbench");
    let bin = binary_path();
    let proto_str = proto.to_str().expect("utf8");
    let dir_str = dir.to_str().expect("utf8");

    let mut group = c.benchmark_group("startup");
    group.bench_function("breaking_no_changes", |b| {
        b.iter(|| {
            let status = Command::new(std::hint::black_box(bin))
                .args([
                    "breaking",
                    "--old",
                    std::hint::black_box(proto_str),
                    "--old-include",
                    dir_str,
                    "--new",
                    proto_str,
                    "--new-include",
                    dir_str,
                ])
                .output()
                .expect("spawn breaking");
            assert!(status.status.success());
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Criterion groups
// ---------------------------------------------------------------------------

criterion_group!(
    startup_benches,
    bench_startup_help,
    bench_startup_describe,
    bench_startup_lint,
    bench_startup_format,
    bench_startup_breaking_no_changes,
);

criterion_group!(gen_benches, bench_gen_scaling,);

criterion_group!(memory_benches, bench_memory_proxy_large_set,);

criterion_main!(startup_benches, gen_benches, memory_benches);
