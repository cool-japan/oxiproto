# oxiproto-core TODO

## Status
Re-export facade over `prost` (`Message`, `Name`, `prost_types`) PLUS a complete
native wire format module (`oxiproto_core::wire`). The wire module provides
varint/zigzag/tag/fixed/length-delimited codecs, `DecodeBuffer`/`EncodeBuffer`,
and `UnknownFields` — ~1900 SLOC including tests. Error type now has
`WireFormatError`, `From<std::io::Error>`, `#[non_exhaustive]`, and
`OxiProtoResult<T>`. Goal: build a native `Message` trait on top of the wire
module to fully replace `prost`.

## Core Implementation
- [x] Implement native `WireType` enum: Varint(0), I64(1), Len(2), SGroup(3), EGroup(4), I32(5)
- [x] Implement varint encoding (LEB128) for u32/u64/i32/i64 with overflow detection
- [x] Implement zigzag encoding for sint32/sint64
- [x] Implement field tag encoding/decoding: (field_number << 3) | wire_type
- [x] Implement length-delimited field encoding/decoding
- [x] Implement fixed32/fixed64 encoding/decoding (little-endian) + float/double/sfixed
- [x] Implement `DecodeBuffer` for zero-copy wire format reading from `&[u8]` (was: BufReader)
- [x] Implement `EncodeBuffer` for wire format writing to `Vec<u8>` (was: BufWriter)
- [x] Implement `UnknownFields` storage for preserving unrecognized fields during decode
- [x] Implement native `OxiMessage` trait: `encode_raw`, `decode`, `encoded_len`, `merge`, `clear`, `encode_to_vec` (200-220 SLOC) (done 2026-05-29)
  - **Goal:** Define `oxiproto_core::OxiMessage` trait on top of the existing `wire` module. Named `OxiMessage` to avoid collision with existing `pub use prost::Message`. KEEP `pub use prost::Message` re-export UNCHANGED.
  - **Design:** Trait in `src/message.rs`. Methods: `encoded_len() -> usize`, `encode_raw(&self, buf: &mut wire::EncodeBuffer)`, `merge(&mut self, buf: &mut wire::DecodeBuffer) -> OxiProtoResult<()>`, `clear(&mut self)`. Default impls: `decode_raw`, `encode_to_vec`, `decode`. Add `pub mod message; pub use message::OxiMessage;` to lib.rs (KEEP existing prost re-exports!).
  - **Files:** crates/oxiproto-core/src/message.rs (new); crates/oxiproto-core/src/lib.rs (modified: add module + re-export, kept prost re-exports)
  - **Tests:** Hand-written TestFoo {id: i32, name: String, tags: Vec<String>} impl OxiMessage; round-trip through encode_to_vec → decode; byte cross-validation against a prost-derived equivalent — bytes are identical.
- [x] Implement native `OxiName` trait: `full_name`, `type_url` (50 SLOC) (done 2026-05-29)
  - **Goal:** `oxiproto_core::OxiName` with `const NAME: &'static str`, `const PACKAGE: &'static str`, `fn full_name() -> String`, `fn type_url() -> String`. Distinct from `prost::Name` re-export.
  - **Design:** Trait in `src/name.rs`. Defaults: full_name concatenates PACKAGE + "." + NAME (skips "." if PACKAGE is empty). type_url = "type.googleapis.com/" + full_name().
  - **Files:** crates/oxiproto-core/src/name.rs (new); crates/oxiproto-core/src/lib.rs (modified)
- [x] Implement `Extensions` registry for proto2 extension field support (160 SLOC) (done 2026-05-29)
  - **Goal:** `oxiproto_core::Extensions` struct backed by `BTreeMap<u32, Vec<u8>>` for proto2 extension storage.
  - **Design:** Struct in `src/extensions.rs`. Methods: `get_extension<T: OxiMessage>`, `set_extension<T: OxiMessage>`, `has_extension`, `clear_extension`, `is_empty`, `len`, `merge_raw`, `encode_raw`, `encoded_len`.
  - **Files:** crates/oxiproto-core/src/extensions.rs (new); crates/oxiproto-core/src/lib.rs (modified)
  - **Tests:** set/get round-trip, has/clear, is_empty/len, overwrite, encode_raw + merge_raw round-trip, encoded_len matches actual.
- [x] Implement `OxiOneof` trait for oneof field group representation (90 SLOC) (done 2026-05-29)
  - **Goal:** `oxiproto_core::OxiOneof` trait for generated oneof enums. Enables field-number-dispatch during merge().
  - **Design:** Trait in `src/oneof.rs`. Methods: `discriminant(&self) -> u32`, `encoded_len(&self) -> usize`, `encode(&self, buf: &mut wire::EncodeBuffer)`, `merge_field(field_number, wire_type, buf, slot) -> OxiProtoResult<bool>`.
  - **Files:** crates/oxiproto-core/src/oneof.rs (new); crates/oxiproto-core/src/lib.rs (modified)
  - **Tests:** 3-variant enum (int, str, bool); each round-trips; last-write-wins; unknown field_number → Ok(false); encoded_len matches actual.
- [x] Add `WireFormatError` variant to `OxiProtoError` for decode failures
- [x] Add `UnexpectedEof`, `InvalidWireType`, `InvalidFieldNumber`, `Overflow` error variants (in `WireError`)

## API Improvements
- [x] Make `OxiProtoError` implement `From<std::io::Error>` instead of storing `ErrorKind`
- [x] Add `#[non_exhaustive]` to `OxiProtoError`
- [x] Add `OxiProtoResult<T>` type alias
- [x] Implement `serde::Serialize` / `Deserialize` for wire-format types behind feature (done 2026-05-29)
  - **Goal:** Optional `serde` feature deriving `Serialize`/`Deserialize` on public wire-format data types (`WireType`, `UnknownFields`/unknown-field entries, and other public wire structs). NOT on transient `EncodeBuffer`/`DecodeBuffer`.
  - **Design:** `Cargo.toml`: `serde = { workspace = true, optional = true, default-features = false, features = ["derive", "alloc"] }` + feature `serde = ["dep:serde"]`. Gated via `#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]` on public types. **Dual-build gate:** both `--no-default-features --features alloc` (serde off, no_std preserved) AND `--features serde` must build.
  - **Files:** `Cargo.toml` (serde optional dep + feature), `src/wire/*.rs` (cfg_attr derives), `src/lib.rs` (feature plumbing if needed), `tests/serde_wire.rs` (new — serde_json round-trip for WireType/UnknownFields).
  - **Tests:** `cargo nextest -p oxiproto-core --all-features` green; `cargo build -p oxiproto-core --no-default-features --features alloc` still passes; `cargo build -p oxiproto-core --features serde` passes; serde_json round-trip test for WireType/UnknownFields.
  - **Risk:** Low. Main risk = serde feature accidentally breaking no_std+alloc build → mitigated by dual-build gate.
- [x] Add `no_std` support with `alloc` feature for embedded use (done 2026-05-29)
  - **Goal:** `oxiproto-core` builds under `#![no_std]` + `alloc`. Default stays `std`. Proven by `cargo build -p oxiproto-core --no-default-features --features alloc`.
  - **Design:** Added `[features]` `default=["std"]`, `std=["prost/std","prost-types/std"]`, `alloc=[]`. Mechanical swaps: `std::fmt`->`core::fmt`, `std::str`->`core::str`, `std::slice`->`core::slice`, `std::error::Error`->`core::error::Error`, `std::collections::BTreeMap`->`prost::alloc::collections::BTreeMap`. `String`/`Vec`/`format!`/`vec!` all use `prost::alloc::*`. Gated `OxiProtoError::IoError` + `From<std::io::Error>` behind `#[cfg(feature="std")]`. prost and prost-types support no_std natively (their `std` feature propagated from ours). Added `WireError::InvalidUtf8(core::str::Utf8Error)` variant for alloc-free UTF-8 error reporting.
  - **Files:** `Cargo.toml` (features), `src/lib.rs`, `src/wire/mod.rs`, `src/wire/buf.rs`, `src/wire/fixed.rs`, `src/wire/length_delimited.rs`, `src/wire/tag.rs`, `src/wire/unknown.rs`, `src/wire/varint.rs`, `src/wire/wire_type.rs`, `src/message.rs`, `src/name.rs`, `src/extensions.rs` (std->core/alloc), `tests/no_std_smoke.rs` (new, 8 tests)
  - **Tests:** All 118 tests pass under `--all-features`. `cargo build -p oxiproto-core --no-default-features --features alloc` succeeds. Clippy clean under both configurations.

## Testing
- [x] Test varint encoding/decoding round-trip for edge values (0, 1, 127, 128, u64::MAX)
- [x] Test zigzag encoding: 0->0, -1->1, 1->2, -2->3, i32::MIN, i32::MAX
- [x] Test field tag encoding/decoding for all wire types
- [x] Test unknown field preservation: encode unknown fields, decode, verify preserved
- [x] Test OxiMessage round-trip: encode_to_vec → decode, field preservation, empty message = 0 bytes
- [x] Test OxiMessage byte cross-validation: OxiMessage bytes == prost::Message bytes for TestFoo
- [x] Test OxiMessage encoded_len matches actual encoded byte count
- [x] Test OxiOneof: 3-variant round-trip, last-write-wins, unknown field → Ok(false), encoded_len matches actual
- [x] Test Extensions: set/get round-trip, has/clear, is_empty/len, overwrite, encode_raw+merge_raw round-trip, encoded_len
- [x] Add property-based round-trip tests (proptest) for varint, zigzag, length-delimited, and tag codecs (done 2026-05-29)
- [x] Fuzz varint decoder with arbitrary byte sequences (done 2026-05-29)
  - **Goal:** Proptest no-panic harness feeding arbitrary/malformed bytes into the decoder, asserting graceful `Err` (never panic). The existing `proptest_wire.rs` only feeds *valid* encodings.
  - **Design:** `tests/fuzz_decode.rs`: proptest strategies generating arbitrary `Vec<u8>` (and structured-but-adversarial: valid header + truncated body, oversized lengths, etc.) fed to `DecodeBuffer::read_varint`, tag decode, length-delimited, full-message decode paths. Assert `Ok|Err` without panic. Pure Rust (no cargo-fuzz/libFuzzer which is C++, violating Pure-Rust Policy).
  - **Files:** `tests/fuzz_decode.rs` (new).
  - **Tests:** All proptest cases pass; clippy clean; no `should_panic` (use `Result` assertion).
  - **Risk:** Low. Proptest already a dev-dep.

## Performance
- [x] Benchmark varint encoding/decoding against prost's implementation (done 2026-05-29)
  - **Goal:** Criterion harness comparing native varint/zigzag/fixed vs prost equivalents.
  - **Design:** `benches/wire.rs` — criterion benchmarks for varint encode/decode (vs `prost::encoding::encode_varint`/`decode_varint`), zigzag (i32/i64), fixed32/64, length-delimited. Representative value distributions.
  - **Files:** `crates/oxiproto-core/benches/wire.rs` (new ~140 SLOC), `Cargo.toml` (criterion dev-dep + `[[bench]]` entries)
  - **Tests:** `cargo bench -p oxiproto-core --no-run` compiles. clippy clean on bench targets.
- [x] Benchmark full message encode/decode against prost (done 2026-05-29)
  - **Goal:** Compare OxiMessage encode/decode vs prost::Message on a representative message.
  - **Design:** `benches/message.rs` — hand-written benchmark message (scalars+repeated+string) implementing OxiMessage + a `#[derive(prost::Message)]` equivalent; benchmark `encode_to_vec` + `decode` both ways; assert byte-equal payloads once before timing.
  - **Files:** `crates/oxiproto-core/benches/message.rs` (new ~160 SLOC), `Cargo.toml` (same criterion dev-dep)
  - **Tests:** Covered by `cargo bench --no-run` gate.
- [ ] Profile allocation patterns in decode path
- [ ] Consider arena allocation for repeated message fields

## Integration
- [ ] Ensure oxiproto-build generates code that uses native Message trait instead of prost::Message
- [ ] Ensure oxiproto-reflect can work with both prost-backed and native messages
- [ ] Ensure wire format compatibility with canonical protobuf implementations (Go, C++, Java)
