//! Criterion benchmarks for the native wire codec in `oxiproto-core`.
//!
//! Compares varint/zigzag/fixed encoding against the prost equivalent to
//! establish a performance baseline and catch regressions.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use oxiproto_core::wire::zigzag::{zigzag_encode32, zigzag_encode64};
use oxiproto_core::wire::{DecodeBuffer, EncodeBuffer};

// ── Varint encode ─────────────────────────────────────────────────────────────

fn bench_varint_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("varint_encode");

    group.bench_function("u64_small", |b| {
        b.iter(|| {
            let mut buf = EncodeBuffer::new();
            buf.write_varint(black_box(42u64));
            black_box(buf);
        })
    });

    group.bench_function("u64_large", |b| {
        b.iter(|| {
            let mut buf = EncodeBuffer::new();
            buf.write_varint(black_box(u64::MAX));
            black_box(buf);
        })
    });

    group.bench_function("u32_mid", |b| {
        b.iter(|| {
            let mut buf = EncodeBuffer::new();
            buf.write_varint32(black_box(300u32));
            black_box(buf);
        })
    });

    group.finish();
}

// ── Varint decode ─────────────────────────────────────────────────────────────

fn bench_varint_decode(c: &mut Criterion) {
    // Pre-encode values so we benchmark pure decode.
    let bytes_small = {
        let mut buf = EncodeBuffer::new();
        buf.write_varint(42u64);
        buf.into_vec()
    };
    let bytes_large = {
        let mut buf = EncodeBuffer::new();
        buf.write_varint(u64::MAX);
        buf.into_vec()
    };
    let bytes_mid = {
        let mut buf = EncodeBuffer::new();
        buf.write_varint32(300u32);
        buf.into_vec()
    };

    let mut group = c.benchmark_group("varint_decode");

    group.bench_function("u64_small", |b| {
        b.iter(|| {
            let mut dec = DecodeBuffer::new(black_box(bytes_small.as_slice()));
            black_box(dec.read_varint().unwrap_or(0))
        })
    });

    group.bench_function("u64_large", |b| {
        b.iter(|| {
            let mut dec = DecodeBuffer::new(black_box(bytes_large.as_slice()));
            black_box(dec.read_varint().unwrap_or(0))
        })
    });

    group.bench_function("u32_mid", |b| {
        b.iter(|| {
            let mut dec = DecodeBuffer::new(black_box(bytes_mid.as_slice()));
            black_box(dec.read_varint32().unwrap_or(0))
        })
    });

    group.finish();
}

// ── Prost varint comparison ───────────────────────────────────────────────────

fn bench_prost_varint_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("prost_varint_encode");

    group.bench_function("u64_small", |b| {
        b.iter(|| {
            let mut buf = Vec::with_capacity(10);
            prost::encoding::encode_varint(black_box(42u64), &mut buf);
            black_box(buf);
        })
    });

    group.bench_function("u64_large", |b| {
        b.iter(|| {
            let mut buf = Vec::with_capacity(10);
            prost::encoding::encode_varint(black_box(u64::MAX), &mut buf);
            black_box(buf);
        })
    });

    group.finish();
}

fn bench_prost_varint_decode(c: &mut Criterion) {
    let bytes_small: Vec<u8> = {
        let mut v = Vec::with_capacity(10);
        prost::encoding::encode_varint(42u64, &mut v);
        v
    };
    let bytes_large: Vec<u8> = {
        let mut v = Vec::with_capacity(10);
        prost::encoding::encode_varint(u64::MAX, &mut v);
        v
    };

    let mut group = c.benchmark_group("prost_varint_decode");

    group.bench_function("u64_small", |b| {
        b.iter(|| {
            // prost::encoding::decode_varint takes &mut impl Buf;
            // &[u8] implements Buf.
            let mut slice: &[u8] = black_box(bytes_small.as_slice());
            black_box(prost::encoding::decode_varint(&mut slice).unwrap_or(0))
        })
    });

    group.bench_function("u64_large", |b| {
        b.iter(|| {
            let mut slice: &[u8] = black_box(bytes_large.as_slice());
            black_box(prost::encoding::decode_varint(&mut slice).unwrap_or(0))
        })
    });

    group.finish();
}

// ── ZigZag encode ─────────────────────────────────────────────────────────────

fn bench_zigzag(c: &mut Criterion) {
    let mut group = c.benchmark_group("zigzag");

    group.bench_function("encode_i32_negative", |b| {
        b.iter(|| {
            let mut buf = EncodeBuffer::new();
            let encoded = zigzag_encode32(black_box(-12345i32));
            buf.write_varint(u64::from(encoded));
            black_box(buf);
        })
    });

    group.bench_function("encode_i64_negative", |b| {
        b.iter(|| {
            let mut buf = EncodeBuffer::new();
            let encoded = zigzag_encode64(black_box(-987654321i64));
            buf.write_varint(encoded);
            black_box(buf);
        })
    });

    group.bench_function("encode_i32_positive", |b| {
        b.iter(|| {
            let mut buf = EncodeBuffer::new();
            let encoded = zigzag_encode32(black_box(12345i32));
            buf.write_varint(u64::from(encoded));
            black_box(buf);
        })
    });

    group.finish();
}

// ── Fixed-width encode ────────────────────────────────────────────────────────

fn bench_fixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("fixed");

    group.bench_function("fixed64_encode", |b| {
        b.iter(|| {
            let mut buf = EncodeBuffer::new();
            buf.write_fixed64(black_box(0x1234_5678_90ab_cdefu64));
            black_box(buf);
        })
    });

    group.bench_function("fixed32_encode", |b| {
        b.iter(|| {
            let mut buf = EncodeBuffer::new();
            buf.write_fixed32(black_box(0x1234_5678u32));
            black_box(buf);
        })
    });

    group.bench_function("float_encode", |b| {
        b.iter(|| {
            let mut buf = EncodeBuffer::new();
            buf.write_float(black_box(1.234_567f32));
            black_box(buf);
        })
    });

    group.bench_function("double_encode", |b| {
        b.iter(|| {
            let mut buf = EncodeBuffer::new();
            buf.write_double(black_box(9.876_543_210_987f64));
            black_box(buf);
        })
    });

    group.finish();
}

// ── Criterion harness ─────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_varint_encode,
    bench_varint_decode,
    bench_prost_varint_encode,
    bench_prost_varint_decode,
    bench_zigzag,
    bench_fixed,
);
criterion_main!(benches);
