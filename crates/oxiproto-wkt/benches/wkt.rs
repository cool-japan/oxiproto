//! Criterion benchmarks for `oxiproto-wkt`.
//!
//! Covers three areas from the performance TODO:
//! 1. `Timestamp` conversion throughput (SystemTime ↔ Timestamp, RFC 3339).
//! 2. `Any` pack/unpack vs manual encode/decode.
//! 3. Struct/Value construction and field access (allocation profile proxy).

use std::hint::black_box;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use criterion::{criterion_group, criterion_main, Criterion};
use prost::Message;
use prost_types::{Any, Struct, Timestamp, Value};

use oxiproto_wkt::{AnyExt, StructExt, TimestampExt, ValueExt};

// ── 1. Timestamp conversion benchmarks ────────────────────────────────────────

fn bench_timestamp_from_system_time(c: &mut Criterion) {
    let mut group = c.benchmark_group("timestamp_from_system_time");

    let now = SystemTime::now();
    let before_epoch = UNIX_EPOCH - Duration::from_secs(100_000);
    let far_future = UNIX_EPOCH + Duration::from_secs(4_000_000_000);

    group.bench_function("now", |b| {
        b.iter(|| Timestamp::from_system_time(black_box(now)))
    });

    group.bench_function("before_epoch", |b| {
        b.iter(|| Timestamp::from_system_time(black_box(before_epoch)))
    });

    group.bench_function("far_future", |b| {
        b.iter(|| Timestamp::from_system_time(black_box(far_future)))
    });

    group.finish();
}

fn bench_timestamp_to_system_time(c: &mut Criterion) {
    let mut group = c.benchmark_group("timestamp_to_system_time");

    let ts_positive = Timestamp {
        seconds: 1_700_000_000,
        nanos: 500_000_000,
    };
    let ts_negative = Timestamp {
        seconds: -100_000,
        nanos: 250_000_000,
    };
    let ts_epoch = Timestamp {
        seconds: 0,
        nanos: 0,
    };

    group.bench_function("positive", |b| {
        b.iter(|| black_box(&ts_positive).to_system_time())
    });

    group.bench_function("negative", |b| {
        b.iter(|| black_box(&ts_negative).to_system_time())
    });

    group.bench_function("epoch", |b| {
        b.iter(|| black_box(&ts_epoch).to_system_time())
    });

    group.finish();
}

fn bench_timestamp_round_trip(c: &mut Criterion) {
    let mut group = c.benchmark_group("timestamp_round_trip");

    let system_time = UNIX_EPOCH + Duration::from_secs(1_700_000_000);

    group.bench_function("system_time_to_timestamp_to_system_time", |b| {
        b.iter(|| {
            let ts = Timestamp::from_system_time(black_box(system_time));
            black_box(ts.to_system_time())
        })
    });

    group.finish();
}

fn bench_timestamp_rfc3339(c: &mut Criterion) {
    let mut group = c.benchmark_group("timestamp_rfc3339");

    let ts = Timestamp {
        seconds: 1_700_000_000,
        nanos: 123_456_789,
    };
    let ts_whole = Timestamp {
        seconds: 1_700_000_000,
        nanos: 0,
    };
    let ts_pre_epoch = Timestamp {
        seconds: -1_000_000,
        nanos: 500_000_000,
    };

    let rfc_str = "2023-11-14T22:13:20.123456789Z";
    let rfc_str_whole = "2023-11-14T22:13:20Z";
    let rfc_pre_epoch = "1969-11-14T22:13:20Z";

    group.bench_function("to_rfc3339_with_nanos", |b| {
        b.iter(|| black_box(&ts).to_rfc3339())
    });

    group.bench_function("to_rfc3339_whole_seconds", |b| {
        b.iter(|| black_box(&ts_whole).to_rfc3339())
    });

    group.bench_function("to_rfc3339_pre_epoch", |b| {
        b.iter(|| black_box(&ts_pre_epoch).to_rfc3339())
    });

    group.bench_function("from_rfc3339_with_nanos", |b| {
        b.iter(|| Timestamp::from_rfc3339(black_box(rfc_str)))
    });

    group.bench_function("from_rfc3339_whole_seconds", |b| {
        b.iter(|| Timestamp::from_rfc3339(black_box(rfc_str_whole)))
    });

    group.bench_function("from_rfc3339_pre_epoch", |b| {
        b.iter(|| Timestamp::from_rfc3339(black_box(rfc_pre_epoch)))
    });

    group.finish();
}

// ── 2. Any pack/unpack vs manual encode/decode ─────────────────────────────────

fn bench_any_pack_unpack(c: &mut Criterion) {
    let mut group = c.benchmark_group("any_pack_unpack");

    let ts = Timestamp {
        seconds: 1_700_000_000,
        nanos: 123_456_789,
    };

    // Pre-packed Any for unpack benchmarks
    let packed_any = Any::pack(&ts);

    group.bench_function("pack_timestamp", |b| b.iter(|| Any::pack(black_box(&ts))));

    group.bench_function("unpack_timestamp", |b| {
        b.iter(|| {
            let result: Option<Timestamp> = black_box(&packed_any).unpack();
            black_box(result)
        })
    });

    group.bench_function("pack_unpack_round_trip", |b| {
        b.iter(|| {
            let any = Any::pack(black_box(&ts));
            let result: Option<Timestamp> = any.unpack();
            black_box(result)
        })
    });

    // Compare with manual encode/decode (no type URL check)
    group.bench_function("manual_encode", |b| {
        b.iter(|| {
            let bytes = black_box(&ts).encode_to_vec();
            black_box(bytes)
        })
    });

    group.bench_function("manual_decode", |b| {
        let bytes = ts.encode_to_vec();
        b.iter(|| {
            let result = Timestamp::decode(black_box(bytes.as_slice()));
            black_box(result)
        })
    });

    group.bench_function("type_url_check_is", |b| {
        b.iter(|| black_box(&packed_any).is::<Timestamp>())
    });

    group.finish();
}

// ── 3. Struct/Value allocation benchmarks ─────────────────────────────────────

fn bench_struct_value(c: &mut Criterion) {
    let mut group = c.benchmark_group("struct_value");

    // Build a struct with N fields
    group.bench_function("build_struct_10_fields", |b| {
        b.iter(|| {
            let mut s = Struct::empty();
            for i in 0..10u32 {
                s.insert(
                    format!("field_{i}"),
                    Value::from_f64(black_box(f64::from(i))),
                );
            }
            black_box(s)
        })
    });

    group.bench_function("build_struct_100_fields", |b| {
        b.iter(|| {
            let mut s = Struct::empty();
            for i in 0..100u32 {
                s.insert(
                    format!("field_{i:03}"),
                    Value::from_string(black_box(format!("value_{i}"))),
                );
            }
            black_box(s)
        })
    });

    // Value construction variants
    group.bench_function("value_from_f64", |b| {
        b.iter(|| Value::from_f64(black_box(42.0f64)))
    });

    group.bench_function("value_from_string", |b| {
        b.iter(|| Value::from_string(black_box("hello world")))
    });

    group.bench_function("value_from_bool", |b| {
        b.iter(|| Value::from_bool(black_box(true)))
    });

    group.bench_function("value_null", |b| b.iter(Value::null));

    // Struct field access
    let mut pre_built = Struct::empty();
    for i in 0..20u32 {
        pre_built.insert(format!("key_{i:02}"), Value::from_f64(f64::from(i)));
    }

    group.bench_function("struct_get_hit", |b| {
        b.iter(|| black_box(&pre_built).get(black_box("key_10")))
    });

    group.bench_function("struct_get_miss", |b| {
        b.iter(|| black_box(&pre_built).get(black_box("nonexistent_key")))
    });

    group.bench_function("struct_len", |b| b.iter(|| black_box(&pre_built).len()));

    group.finish();
}

// ── Criterion harness ──────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_timestamp_from_system_time,
    bench_timestamp_to_system_time,
    bench_timestamp_round_trip,
    bench_timestamp_rfc3339,
    bench_any_pack_unpack,
    bench_struct_value,
);
criterion_main!(benches);
