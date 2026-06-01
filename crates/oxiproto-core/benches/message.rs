//! Criterion benchmarks comparing `OxiMessage` encode/decode vs `prost::Message`
//! on a representative 4-field message.
//!
//! An `assert_byte_equal_once` guard fires before any timing to verify that
//! both implementations produce identical wire bytes.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use oxiproto_core::wire::{
    encoded_len_length_delimited, varint::encoded_len_varint, DecodeBuffer, EncodeBuffer, WireType,
};
use oxiproto_core::{OxiMessage, OxiProtoResult};

// ── Native OxiMessage implementation ─────────────────────────────────────────
//
// Proto schema (field ordering matches tag numbers):
//
//   message BenchUser {
//     int32          id     = 1;
//     string         name   = 2;
//     repeated string tags  = 3;
//     bool           active = 4;
//   }

#[derive(Debug, Default, Clone, PartialEq)]
struct BenchUser {
    id: i32,
    name: String,
    tags: Vec<String>,
    active: bool,
}

impl OxiMessage for BenchUser {
    fn encoded_len(&self) -> usize {
        let mut len = 0usize;

        // Field 1: int32 id — omit when 0 (proto3 default)
        if self.id != 0 {
            // tag = (1 << 3) | 0 = 8
            let tag_val = (1u64 << 3) | u64::from(WireType::Varint.value());
            len += encoded_len_varint(tag_val);
            len += encoded_len_varint(self.id as i64 as u64);
        }

        // Field 2: string name — omit when empty
        if !self.name.is_empty() {
            // tag = (2 << 3) | 2 = 18
            let tag_val = (2u64 << 3) | u64::from(WireType::Len.value());
            len += encoded_len_varint(tag_val);
            len += encoded_len_length_delimited(self.name.len());
        }

        // Field 3: repeated string tags — each element has its own tag
        for tag_str in &self.tags {
            // tag = (3 << 3) | 2 = 26
            let tag_val = (3u64 << 3) | u64::from(WireType::Len.value());
            len += encoded_len_varint(tag_val);
            len += encoded_len_length_delimited(tag_str.len());
        }

        // Field 4: bool active — omit when false (proto3 default)
        if self.active {
            // tag = (4 << 3) | 0 = 32
            let tag_val = (4u64 << 3) | u64::from(WireType::Varint.value());
            len += encoded_len_varint(tag_val);
            len += 1; // bool true → varint(1) = 1 byte
        }

        len
    }

    fn encode_raw(&self, buf: &mut EncodeBuffer) {
        // Emit in ascending tag order (1, 2, 3, 4) to match prost's output.

        // Field 1: int32 id
        if self.id != 0 {
            let _ = buf.write_tag(1, WireType::Varint);
            buf.write_varint_i32(self.id);
        }

        // Field 2: string name
        if !self.name.is_empty() {
            let _ = buf.write_tag(2, WireType::Len);
            buf.write_string(&self.name);
        }

        // Field 3: repeated string tags
        for tag_str in &self.tags {
            let _ = buf.write_tag(3, WireType::Len);
            buf.write_string(tag_str);
        }

        // Field 4: bool active
        if self.active {
            let _ = buf.write_tag(4, WireType::Varint);
            buf.write_bool(true);
        }
    }

    fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = buf.read_tag()?;
            match tag.field_number {
                1 => {
                    self.id = buf.read_varint_i32()?;
                }
                2 => {
                    let s = buf.read_string()?;
                    self.name = s.to_owned();
                }
                3 => {
                    let s = buf.read_string()?;
                    self.tags.push(s.to_owned());
                }
                4 => {
                    self.active = buf.read_bool()?;
                }
                _ => {
                    buf.skip_field(tag.wire_type)?;
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        self.id = 0;
        self.name.clear();
        self.tags.clear();
        self.active = false;
    }
}

// ── Prost equivalent ──────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, prost::Message)]
struct ProstUser {
    #[prost(int32, tag = "1")]
    id: i32,
    #[prost(string, tag = "2")]
    name: String,
    #[prost(string, repeated, tag = "3")]
    tags: Vec<String>,
    #[prost(bool, tag = "4")]
    active: bool,
}

// ── Byte-equality guard ───────────────────────────────────────────────────────

fn assert_byte_equal_once() {
    use prost::Message as ProstMsg;

    let native = BenchUser {
        id: 42,
        name: "Alice".to_owned(),
        tags: vec!["admin".to_owned(), "user".to_owned()],
        active: true,
    };
    let prost_msg = ProstUser {
        id: 42,
        name: "Alice".to_owned(),
        tags: vec!["admin".to_owned(), "user".to_owned()],
        active: true,
    };

    let native_bytes = native.encode_to_vec();
    let prost_bytes = prost_msg.encode_to_vec();

    assert_eq!(
        native_bytes, prost_bytes,
        "Native OxiMessage and prost wire bytes differ!\n\
         native = {:?}\n\
         prost  = {:?}",
        native_bytes, prost_bytes
    );

    // Also verify the all-defaults case (empty → 0 bytes on both sides).
    let native_empty = BenchUser::default();
    let prost_empty = ProstUser::default();
    assert_eq!(
        native_empty.encode_to_vec(),
        prost_empty.encode_to_vec(),
        "Default BenchUser must encode to 0 bytes on both sides"
    );
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

fn bench_encode(c: &mut Criterion) {
    assert_byte_equal_once();

    let user = BenchUser {
        id: 42,
        name: "Alice".to_owned(),
        tags: vec!["admin".to_owned(), "user".to_owned()],
        active: true,
    };
    let prost_user = ProstUser {
        id: 42,
        name: "Alice".to_owned(),
        tags: vec!["admin".to_owned(), "user".to_owned()],
        active: true,
    };

    let mut group = c.benchmark_group("encode");

    group.bench_function("native_oxi", |b| {
        b.iter(|| black_box(black_box(&user).encode_to_vec()))
    });

    group.bench_function("prost", |b| {
        use prost::Message as ProstMsg;
        b.iter(|| black_box(prost_user.encode_to_vec()))
    });

    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let user = BenchUser {
        id: 42,
        name: "Alice".to_owned(),
        tags: vec!["admin".to_owned(), "user".to_owned()],
        active: true,
    };
    let bytes = user.encode_to_vec();

    let mut group = c.benchmark_group("decode");

    group.bench_function("native_oxi", |b| {
        b.iter(|| black_box(BenchUser::decode(black_box(bytes.as_slice()))).ok())
    });

    group.bench_function("prost", |b| {
        use prost::Message as ProstMsg;
        b.iter(|| black_box(ProstUser::decode(black_box(bytes.as_slice()))).ok())
    });

    group.finish();
}

fn bench_encode_decode_roundtrip(c: &mut Criterion) {
    let user = BenchUser {
        id: 42,
        name: "Alice".to_owned(),
        tags: vec!["admin".to_owned(), "user".to_owned()],
        active: true,
    };
    let prost_user = ProstUser {
        id: 42,
        name: "Alice".to_owned(),
        tags: vec!["admin".to_owned(), "user".to_owned()],
        active: true,
    };

    let mut group = c.benchmark_group("roundtrip");

    group.bench_function("native_oxi", |b| {
        b.iter(|| {
            let bytes = black_box(&user).encode_to_vec();
            black_box(BenchUser::decode(black_box(&bytes))).ok()
        })
    });

    group.bench_function("prost", |b| {
        use prost::Message as ProstMsg;
        b.iter(|| {
            let bytes = prost_user.encode_to_vec();
            black_box(ProstUser::decode(black_box(bytes.as_slice()))).ok()
        })
    });

    group.finish();
}

// ── Criterion harness ─────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_encode,
    bench_decode,
    bench_encode_decode_roundtrip
);
criterion_main!(benches);
