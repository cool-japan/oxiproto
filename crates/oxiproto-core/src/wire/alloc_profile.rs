#![forbid(unsafe_code)]
//! Allocation profiling for the wire format decode path.
//!
//! This module provides `DecodeStats` — a lightweight counter struct that
//! tracks the number and total byte cost of heap allocations made during
//! protobuf decoding. It is designed as a *passive* observer: callers must
//! explicitly notify it when they allocate, which makes it zero-cost when
//! profiling is not active.
//!
//! The companion `ProfiledDecodeBuffer` wraps a [`DecodeBuffer`] and
//! automatically records allocation events for every `read_string`,
//! `read_length_delimited`, and `read_varint` call that results in owned data
//! being produced.
//!
//! # Allocation categories
//!
//! The profiler distinguishes between three allocation sites in the decode path:
//!
//! | Category             | Protobuf field types             |
//! |----------------------|----------------------------------|
//! | `string_alloc`       | `string` fields                  |
//! | `bytes_alloc`        | `bytes` / embedded messages      |
//! | `varint_alloc`       | `unknown` varints stored in heap |
//!
//! # Example
//!
//! ```rust
//! use oxiproto_core::wire::{EncodeBuffer, WireType};
//! use oxiproto_core::wire::alloc_profile::{DecodeStats, ProfiledDecodeBuffer};
//!
//! // Build a tiny wire payload.
//! let mut enc = EncodeBuffer::new();
//! enc.write_tag(1, WireType::Len).unwrap();
//! enc.write_string("hello");
//! enc.write_tag(2, WireType::Len).unwrap();
//! enc.write_string("world");
//!
//! let bytes = enc.into_vec();
//!
//! let mut stats = DecodeStats::new();
//! let mut prof = ProfiledDecodeBuffer::new(&bytes, &mut stats);
//!
//! let _t1 = prof.read_tag().unwrap();
//! let s1 = prof.inner_mut().read_string().unwrap().to_owned(); // consume "hello"
//! prof.record_string_alloc(s1.len());   // "hello" → 5 bytes owned
//! let _t2 = prof.read_tag().unwrap();
//! let s2 = prof.inner_mut().read_string().unwrap().to_owned(); // consume "world"
//! prof.record_string_alloc(s2.len());   // "world" → 5 bytes owned
//!
//! assert_eq!(stats.string_alloc_count, 2);
//! assert_eq!(stats.string_alloc_bytes, 10);
//! ```

use super::buf::{DecodeBuffer, EncodeBuffer};
use super::{Tag, WireError, WireType};

// ── DecodeStats ───────────────────────────────────────────────────────────────

/// Allocation statistics gathered while decoding a protobuf message.
///
/// All counters are plain `usize` fields; they overflow silently on 32-bit
/// platforms decoding exceptionally large messages (>4 GiB). For production
/// use this is acceptable — the struct is a diagnostic tool, not a correctness
/// guard.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DecodeStats {
    /// Number of `String` allocations (one per decoded `string` field value).
    pub string_alloc_count: usize,
    /// Total bytes allocated for `String` fields.
    pub string_alloc_bytes: usize,
    /// Number of `Vec<u8>` allocations (one per decoded `bytes` / embedded
    /// message payload that is stored as owned bytes).
    pub bytes_alloc_count: usize,
    /// Total bytes allocated for `bytes` / embedded message fields.
    pub bytes_alloc_bytes: usize,
    /// Number of `Vec<T>` resize events for repeated scalar fields.
    ///
    /// Each `push` that triggers a capacity doubling is counted as one resize.
    /// The profiler cannot observe these automatically; callers must report
    /// them via [`ProfiledDecodeBuffer::record_repeated_resize`].
    pub repeated_resize_count: usize,
    /// Number of individual elements appended to repeated fields.
    pub repeated_element_count: usize,
    /// Total bytes for all allocation events combined.
    pub total_alloc_bytes: usize,
    /// Total number of allocation events (sum of all `_count` fields).
    pub total_alloc_count: usize,
}

impl DecodeStats {
    /// Create zeroed statistics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a string allocation event of `bytes` bytes.
    #[inline]
    pub fn record_string(&mut self, bytes: usize) {
        self.string_alloc_count += 1;
        self.string_alloc_bytes += bytes;
        self.total_alloc_count += 1;
        self.total_alloc_bytes += bytes;
    }

    /// Add a bytes / embedded-message allocation event of `bytes` bytes.
    #[inline]
    pub fn record_bytes(&mut self, bytes: usize) {
        self.bytes_alloc_count += 1;
        self.bytes_alloc_bytes += bytes;
        self.total_alloc_count += 1;
        self.total_alloc_bytes += bytes;
    }

    /// Record a repeated-field capacity resize.
    ///
    /// `element_bytes` is the byte cost of the *new element* that triggered or
    /// accompanied the resize (used to update `total_alloc_bytes`).
    #[inline]
    pub fn record_repeated_resize(&mut self, element_bytes: usize) {
        self.repeated_resize_count += 1;
        self.repeated_element_count += 1;
        self.total_alloc_count += 1;
        self.total_alloc_bytes += element_bytes;
    }

    /// Record a repeated-field element append that did *not* trigger a resize.
    #[inline]
    pub fn record_repeated_element(&mut self, element_bytes: usize) {
        self.repeated_element_count += 1;
        self.total_alloc_bytes += element_bytes;
    }

    /// Merge another `DecodeStats` into `self` (add all counters).
    pub fn merge(&mut self, other: &DecodeStats) {
        self.string_alloc_count += other.string_alloc_count;
        self.string_alloc_bytes += other.string_alloc_bytes;
        self.bytes_alloc_count += other.bytes_alloc_count;
        self.bytes_alloc_bytes += other.bytes_alloc_bytes;
        self.repeated_resize_count += other.repeated_resize_count;
        self.repeated_element_count += other.repeated_element_count;
        self.total_alloc_count += other.total_alloc_count;
        self.total_alloc_bytes += other.total_alloc_bytes;
    }

    /// Reset all counters to zero.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Returns `true` if no allocation events have been recorded.
    pub fn is_zero(&self) -> bool {
        self.total_alloc_count == 0 && self.total_alloc_bytes == 0
    }

    /// Human-readable one-line summary.
    ///
    /// # Example output
    ///
    /// ```text
    /// allocs: 7 (strings=3/24B, bytes=2/128B, repeated=2/elem=16B, total=168B)
    /// ```
    pub fn summary(&self) -> prost::alloc::string::String {
        let total = self.total_alloc_count;
        let sc = self.string_alloc_count;
        let sb = self.string_alloc_bytes;
        let bc = self.bytes_alloc_count;
        let bb = self.bytes_alloc_bytes;
        let rc = self.repeated_resize_count;
        let re = self.repeated_element_count;
        let tb = self.total_alloc_bytes;
        prost::alloc::format!(
            "allocs: {total} (strings={sc}/{sb}B, bytes={bc}/{bb}B, repeated={rc}/elem={re} total_bytes={tb}B)",
        )
    }
}

impl core::fmt::Display for DecodeStats {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.summary())
    }
}

// ── ProfiledDecodeBuffer ──────────────────────────────────────────────────────

/// A [`DecodeBuffer`] wrapper that records allocation events into a
/// [`DecodeStats`] reference.
///
/// The buffer does **not** own the stats; it holds a `&mut DecodeStats` so
/// multiple buffers can report into a single stats object (e.g. a top-level
/// message and its embedded sub-messages).
///
/// The wrapper is `#[repr(transparent)]`-transparent over the inner buffer for
/// all read operations — it only adds recording calls around the operations that
/// produce owned heap data.
pub struct ProfiledDecodeBuffer<'buf, 'stats> {
    inner: DecodeBuffer<'buf>,
    stats: &'stats mut DecodeStats,
}

impl<'buf, 'stats> ProfiledDecodeBuffer<'buf, 'stats> {
    /// Create a new `ProfiledDecodeBuffer` wrapping `bytes`, recording into
    /// `stats`.
    pub fn new(bytes: &'buf [u8], stats: &'stats mut DecodeStats) -> Self {
        Self {
            inner: DecodeBuffer::new(bytes),
            stats,
        }
    }

    /// Borrow the inner [`DecodeBuffer`] for direct access.
    pub fn inner(&self) -> &DecodeBuffer<'buf> {
        &self.inner
    }

    /// Mutably borrow the inner [`DecodeBuffer`].
    pub fn inner_mut(&mut self) -> &mut DecodeBuffer<'buf> {
        &mut self.inner
    }

    /// Returns a reference to the stats accumulator.
    pub fn stats(&self) -> &DecodeStats {
        self.stats
    }

    // ── Delegated read operations ─────────────────────────────────────────────

    /// Read a tag, delegating to the inner buffer (no allocation).
    pub fn read_tag(&mut self) -> Result<Tag, WireError> {
        self.inner.read_tag()
    }

    /// Read a varint `u64` (no allocation).
    pub fn read_varint(&mut self) -> Result<u64, WireError> {
        self.inner.read_varint()
    }

    /// Read a varint `u32` (no allocation).
    pub fn read_varint32(&mut self) -> Result<u32, WireError> {
        self.inner.read_varint32()
    }

    /// Read a varint `i64` (no allocation).
    pub fn read_varint_i64(&mut self) -> Result<i64, WireError> {
        self.inner.read_varint_i64()
    }

    /// Read a varint `i32` (no allocation).
    pub fn read_varint_i32(&mut self) -> Result<i32, WireError> {
        self.inner.read_varint_i32()
    }

    /// Read a varint `bool` (no allocation).
    pub fn read_bool(&mut self) -> Result<bool, WireError> {
        self.inner.read_bool()
    }

    /// Read a fixed 32-bit value (no allocation).
    pub fn read_fixed32(&mut self) -> Result<u32, WireError> {
        self.inner.read_fixed32()
    }

    /// Read a fixed 64-bit value (no allocation).
    pub fn read_fixed64(&mut self) -> Result<u64, WireError> {
        self.inner.read_fixed64()
    }

    /// Read a float (no allocation).
    pub fn read_float(&mut self) -> Result<f32, WireError> {
        self.inner.read_float()
    }

    /// Read a double (no allocation).
    pub fn read_double(&mut self) -> Result<f64, WireError> {
        self.inner.read_double()
    }

    /// Read a zero-copy byte slice (no allocation).
    ///
    /// If you subsequently call `.to_vec()` on the returned slice, record the
    /// allocation manually via [`record_bytes_alloc`](Self::record_bytes_alloc).
    pub fn read_length_delimited(&mut self) -> Result<&'buf [u8], WireError> {
        self.inner.read_length_delimited()
    }

    /// Read a zero-copy string slice (no allocation).
    ///
    /// If you subsequently call `.to_owned()` on the returned `&str`, record the
    /// allocation manually via [`record_string_alloc`](Self::record_string_alloc).
    pub fn read_string(&mut self) -> Result<&'buf str, WireError> {
        self.inner.read_string()
    }

    /// Skip a field (no allocation for non-group fields).
    pub fn skip_field(&mut self, wire_type: WireType) -> Result<(), WireError> {
        self.inner.skip_field(wire_type)
    }

    /// Returns `true` if all bytes have been consumed.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns the number of remaining bytes.
    pub fn remaining(&self) -> usize {
        self.inner.remaining()
    }

    // ── Allocation recording helpers ──────────────────────────────────────────

    /// Record that a `String` of `bytes` bytes was allocated.
    ///
    /// Call this immediately after calling `.to_owned()` / `.to_string()` on a
    /// string slice obtained from [`read_string`](Self::read_string).
    #[inline]
    pub fn record_string_alloc(&mut self, bytes: usize) {
        self.stats.record_string(bytes);
    }

    /// Record that a `Vec<u8>` of `bytes` bytes was allocated.
    ///
    /// Call this immediately after calling `.to_vec()` on a byte slice obtained
    /// from [`read_length_delimited`](Self::read_length_delimited).
    #[inline]
    pub fn record_bytes_alloc(&mut self, bytes: usize) {
        self.stats.record_bytes(bytes);
    }

    /// Record a repeated-field capacity resize event for an element of
    /// `element_bytes` bytes.
    #[inline]
    pub fn record_repeated_resize(&mut self, element_bytes: usize) {
        self.stats.record_repeated_resize(element_bytes);
    }

    /// Record a repeated-field element append that did not trigger a resize.
    #[inline]
    pub fn record_repeated_element(&mut self, element_bytes: usize) {
        self.stats.record_repeated_element(element_bytes);
    }
}

// ── AllocReport ──────────────────────────────────────────────────────────────

/// A post-decode allocation analysis report derived from [`DecodeStats`].
///
/// Use [`AllocReport::from_stats`] to convert a completed [`DecodeStats`] into
/// a richer view with derived metrics (bytes-per-alloc, alloc density, etc.).
#[derive(Debug, Clone)]
pub struct AllocReport {
    /// The raw stats this report was built from.
    pub stats: DecodeStats,
    /// Average bytes per allocation event (or 0 when no allocations).
    pub avg_bytes_per_alloc: usize,
    /// Fraction of total decoded bytes that were heap-allocated (0..=100).
    ///
    /// A value of 100 means every byte was copied to the heap; 0 means no
    /// heap copies occurred (all reads were zero-copy slices).
    pub heap_fraction_pct: u8,
    /// Total bytes that were read from the wire during the profiled decode.
    pub wire_bytes_read: usize,
}

impl AllocReport {
    /// Build a report from accumulated stats and the total wire bytes read.
    ///
    /// `wire_bytes_read` should be the total size of the encoded message(s)
    /// processed during profiling.
    pub fn from_stats(stats: DecodeStats, wire_bytes_read: usize) -> Self {
        let avg_bytes_per_alloc = stats
            .total_alloc_bytes
            .checked_div(stats.total_alloc_count)
            .unwrap_or(0);
        let heap_fraction_pct = (stats.total_alloc_bytes * 100)
            .checked_div(wire_bytes_read)
            .map(|frac| frac.min(100) as u8)
            .unwrap_or(0u8);
        AllocReport {
            stats,
            avg_bytes_per_alloc,
            heap_fraction_pct,
            wire_bytes_read,
        }
    }

    /// Encode the report as a compact wire-format blob for storage/transmission.
    ///
    /// The format is an informal protobuf-compatible struct with eight varint
    /// fields (field numbers 1–8 correspond to the stats fields, field 9 =
    /// wire_bytes_read). This is *not* a generated proto definition — it is
    /// a hand-written convenience encoder for diagnostic tooling.
    pub fn to_wire_bytes(&self) -> prost::alloc::vec::Vec<u8> {
        use super::encode_varint;
        let s = &self.stats;
        let mut out = prost::alloc::vec::Vec::with_capacity(64);
        // Field 1: string_alloc_count
        out.push(0x08); // tag = (1 << 3) | 0
        encode_varint(s.string_alloc_count as u64, &mut out);
        // Field 2: string_alloc_bytes
        out.push(0x10); // tag = (2 << 3) | 0
        encode_varint(s.string_alloc_bytes as u64, &mut out);
        // Field 3: bytes_alloc_count
        out.push(0x18); // tag = (3 << 3) | 0
        encode_varint(s.bytes_alloc_count as u64, &mut out);
        // Field 4: bytes_alloc_bytes
        out.push(0x20); // tag = (4 << 3) | 0
        encode_varint(s.bytes_alloc_bytes as u64, &mut out);
        // Field 5: repeated_resize_count
        out.push(0x28); // tag = (5 << 3) | 0
        encode_varint(s.repeated_resize_count as u64, &mut out);
        // Field 6: repeated_element_count
        out.push(0x30); // tag = (6 << 3) | 0
        encode_varint(s.repeated_element_count as u64, &mut out);
        // Field 7: total_alloc_count
        out.push(0x38); // tag = (7 << 3) | 0
        encode_varint(s.total_alloc_count as u64, &mut out);
        // Field 8: total_alloc_bytes
        out.push(0x40); // tag = (8 << 3) | 0
        encode_varint(s.total_alloc_bytes as u64, &mut out);
        // Field 9: wire_bytes_read
        out.push(0x48); // tag = (9 << 3) | 0
        encode_varint(self.wire_bytes_read as u64, &mut out);
        out
    }

    /// Decode a report from the compact wire bytes produced by
    /// [`to_wire_bytes`](Self::to_wire_bytes).
    ///
    /// # Errors
    ///
    /// Returns a [`WireError`] if `bytes` is malformed.
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self, WireError> {
        let mut dec = DecodeBuffer::new(bytes);
        let mut s = DecodeStats::new();
        let mut wire_bytes_read = 0usize;

        while !dec.is_empty() {
            let tag = dec.read_tag()?;
            match tag.field_number {
                1 => s.string_alloc_count = dec.read_varint()? as usize,
                2 => s.string_alloc_bytes = dec.read_varint()? as usize,
                3 => s.bytes_alloc_count = dec.read_varint()? as usize,
                4 => s.bytes_alloc_bytes = dec.read_varint()? as usize,
                5 => s.repeated_resize_count = dec.read_varint()? as usize,
                6 => s.repeated_element_count = dec.read_varint()? as usize,
                7 => s.total_alloc_count = dec.read_varint()? as usize,
                8 => s.total_alloc_bytes = dec.read_varint()? as usize,
                9 => wire_bytes_read = dec.read_varint()? as usize,
                _ => dec.skip_field(tag.wire_type)?,
            }
        }

        Ok(Self::from_stats(s, wire_bytes_read))
    }
}

// ── AllocBudget ───────────────────────────────────────────────────────────────

/// A guard that enforces an upper bound on allocations during decode.
///
/// Used in resource-constrained environments (embedded, request-bounded
/// servers) to prevent a malicious or corrupt message from triggering
/// unlimited heap growth.
///
/// The budget is *advisory* — it does not hook the allocator. Instead, callers
/// must check [`AllocBudget::check`] after each `record_*` call and abort
/// decoding if it returns `Err`.
#[derive(Debug, Clone)]
pub struct AllocBudget {
    /// Maximum total bytes that may be allocated.
    pub max_bytes: usize,
    /// Maximum total number of allocation events.
    pub max_allocs: usize,
}

impl AllocBudget {
    /// Create a budget that allows `max_bytes` bytes across `max_allocs`
    /// allocation events.
    pub fn new(max_bytes: usize, max_allocs: usize) -> Self {
        Self {
            max_bytes,
            max_allocs,
        }
    }

    /// Check `stats` against this budget.
    ///
    /// Returns `Ok(())` if both limits are respected, or `Err(exceeded)` with
    /// a description of the first exceeded limit.
    pub fn check(&self, stats: &DecodeStats) -> Result<(), BudgetExceeded> {
        if stats.total_alloc_bytes > self.max_bytes {
            return Err(BudgetExceeded::Bytes {
                used: stats.total_alloc_bytes,
                limit: self.max_bytes,
            });
        }
        if stats.total_alloc_count > self.max_allocs {
            return Err(BudgetExceeded::Count {
                used: stats.total_alloc_count,
                limit: self.max_allocs,
            });
        }
        Ok(())
    }
}

/// Returned by [`AllocBudget::check`] when a limit is exceeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetExceeded {
    /// The byte budget was exceeded.
    Bytes {
        /// Bytes used so far.
        used: usize,
        /// Byte limit.
        limit: usize,
    },
    /// The allocation count budget was exceeded.
    Count {
        /// Allocations so far.
        used: usize,
        /// Allocation limit.
        limit: usize,
    },
}

impl core::fmt::Display for BudgetExceeded {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BudgetExceeded::Bytes { used, limit } => {
                write!(f, "allocation byte budget exceeded: {used} > {limit}")
            }
            BudgetExceeded::Count { used, limit } => {
                write!(f, "allocation count budget exceeded: {used} > {limit}")
            }
        }
    }
}

impl core::error::Error for BudgetExceeded {}

// ── EncodeBuffer stats helper ─────────────────────────────────────────────────

/// Extension trait for [`EncodeBuffer`] that reports allocation cost.
///
/// The encode path allocates exactly once (the output `Vec<u8>`) plus any
/// intermediate temp buffers for nested messages. This trait surfaces the
/// final output size as an allocation event for symmetric profiling.
pub trait EncodeAllocProfile {
    /// Record the final encoded size into `stats` as a single bytes-alloc
    /// event.
    fn record_alloc(&self, stats: &mut DecodeStats);
}

impl EncodeAllocProfile for EncodeBuffer {
    fn record_alloc(&self, stats: &mut DecodeStats) {
        stats.record_bytes(self.len());
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{EncodeBuffer, WireType};

    fn make_payload() -> prost::alloc::vec::Vec<u8> {
        let mut enc = EncodeBuffer::new();
        enc.write_tag(1, WireType::Len).expect("tag1");
        enc.write_string("hello");
        enc.write_tag(2, WireType::Len).expect("tag2");
        enc.write_string("world");
        enc.write_tag(3, WireType::Varint).expect("tag3");
        enc.write_varint(42);
        enc.into_vec()
    }

    #[test]
    fn decode_stats_default_is_zero() {
        let s = DecodeStats::new();
        assert!(s.is_zero());
    }

    #[test]
    fn record_string_increments_counters() {
        let mut s = DecodeStats::new();
        s.record_string(10);
        assert_eq!(s.string_alloc_count, 1);
        assert_eq!(s.string_alloc_bytes, 10);
        assert_eq!(s.total_alloc_count, 1);
        assert_eq!(s.total_alloc_bytes, 10);
    }

    #[test]
    fn record_bytes_increments_counters() {
        let mut s = DecodeStats::new();
        s.record_bytes(20);
        assert_eq!(s.bytes_alloc_count, 1);
        assert_eq!(s.bytes_alloc_bytes, 20);
        assert_eq!(s.total_alloc_count, 1);
        assert_eq!(s.total_alloc_bytes, 20);
    }

    #[test]
    fn record_repeated_resize_increments_counters() {
        let mut s = DecodeStats::new();
        s.record_repeated_resize(8);
        assert_eq!(s.repeated_resize_count, 1);
        assert_eq!(s.repeated_element_count, 1);
        assert_eq!(s.total_alloc_count, 1);
        assert_eq!(s.total_alloc_bytes, 8);
    }

    #[test]
    fn record_repeated_element_no_resize_no_count() {
        let mut s = DecodeStats::new();
        s.record_repeated_element(4);
        assert_eq!(s.repeated_element_count, 1);
        assert_eq!(s.repeated_resize_count, 0);
        assert_eq!(s.total_alloc_count, 0);
        assert_eq!(s.total_alloc_bytes, 4);
    }

    #[test]
    fn merge_adds_counters() {
        let mut a = DecodeStats::new();
        a.record_string(5);
        let mut b = DecodeStats::new();
        b.record_bytes(10);
        a.merge(&b);
        assert_eq!(a.string_alloc_count, 1);
        assert_eq!(a.bytes_alloc_count, 1);
        assert_eq!(a.total_alloc_count, 2);
        assert_eq!(a.total_alloc_bytes, 15);
    }

    #[test]
    fn reset_clears_all() {
        let mut s = DecodeStats::new();
        s.record_string(100);
        s.record_bytes(200);
        s.reset();
        assert!(s.is_zero());
    }

    #[test]
    fn profiled_buffer_delegates_reads() {
        let payload = make_payload();
        let mut stats = DecodeStats::new();
        let mut prof = ProfiledDecodeBuffer::new(&payload, &mut stats);

        // Read tag 1 + len-delimited string
        let t1 = prof.read_tag().expect("tag1");
        assert_eq!(t1.field_number, 1);
        let s1 = prof.read_string().expect("str1");
        assert_eq!(s1, "hello");
        prof.record_string_alloc(s1.len());

        // Read tag 2 + len-delimited string
        let t2 = prof.read_tag().expect("tag2");
        assert_eq!(t2.field_number, 2);
        let s2 = prof.read_string().expect("str2");
        assert_eq!(s2, "world");
        prof.record_string_alloc(s2.len());

        // Read tag 3 + varint
        let t3 = prof.read_tag().expect("tag3");
        assert_eq!(t3.field_number, 3);
        let v = prof.read_varint().expect("varint");
        assert_eq!(v, 42);

        assert!(prof.is_empty());
        assert_eq!(stats.string_alloc_count, 2);
        assert_eq!(stats.string_alloc_bytes, 10);
        assert_eq!(stats.total_alloc_count, 2);
        assert_eq!(stats.total_alloc_bytes, 10);
    }

    #[test]
    fn alloc_report_avg_and_fraction() {
        let mut s = DecodeStats::new();
        s.record_string(100);
        s.record_bytes(100);
        let report = AllocReport::from_stats(s, 400);
        assert_eq!(report.avg_bytes_per_alloc, 100);
        assert_eq!(report.heap_fraction_pct, 50);
    }

    #[test]
    fn alloc_report_zero_wire_bytes() {
        let s = DecodeStats::new();
        let report = AllocReport::from_stats(s, 0);
        assert_eq!(report.heap_fraction_pct, 0);
        assert_eq!(report.avg_bytes_per_alloc, 0);
    }

    #[test]
    fn alloc_report_wire_round_trip() {
        let mut s = DecodeStats::new();
        s.record_string(30);
        s.record_bytes(60);
        s.record_repeated_resize(4);
        s.record_repeated_element(4);
        let report = AllocReport::from_stats(s, 200);
        let wire = report.to_wire_bytes();
        let decoded = AllocReport::from_wire_bytes(&wire).expect("decode");
        assert_eq!(decoded.stats, report.stats);
        assert_eq!(decoded.wire_bytes_read, 200);
        assert_eq!(decoded.avg_bytes_per_alloc, report.avg_bytes_per_alloc);
    }

    #[test]
    fn budget_ok_within_limits() {
        let mut s = DecodeStats::new();
        s.record_string(100);
        let budget = AllocBudget::new(200, 10);
        assert!(budget.check(&s).is_ok());
    }

    #[test]
    fn budget_exceeded_bytes() {
        let mut s = DecodeStats::new();
        s.record_string(300);
        let budget = AllocBudget::new(200, 10);
        let err = budget.check(&s).unwrap_err();
        assert!(matches!(err, BudgetExceeded::Bytes { .. }));
    }

    #[test]
    fn budget_exceeded_count() {
        let mut s = DecodeStats::new();
        for _ in 0..5 {
            s.record_string(1);
        }
        let budget = AllocBudget::new(10000, 3);
        let err = budget.check(&s).unwrap_err();
        assert!(matches!(err, BudgetExceeded::Count { .. }));
    }

    #[test]
    fn encode_alloc_profile_records_len() {
        let mut enc = EncodeBuffer::new();
        enc.write_varint(42);
        let len = enc.len();
        let mut stats = DecodeStats::new();
        enc.record_alloc(&mut stats);
        assert_eq!(stats.bytes_alloc_bytes, len);
        assert_eq!(stats.bytes_alloc_count, 1);
    }

    #[test]
    fn decode_stats_summary_no_panic() {
        let mut s = DecodeStats::new();
        s.record_string(5);
        let summary = s.summary();
        assert!(!summary.is_empty());
        // summary implements Display too
        let display_str = prost::alloc::format!("{s}");
        assert!(!display_str.is_empty());
    }

    #[test]
    fn budget_exceeded_display() {
        let b = BudgetExceeded::Bytes {
            used: 300,
            limit: 200,
        };
        let s = prost::alloc::format!("{b}");
        assert!(s.contains("300"));
        assert!(s.contains("200"));
    }

    #[test]
    fn profiled_buffer_record_bytes_alloc() {
        let mut enc = EncodeBuffer::new();
        enc.write_tag(1, WireType::Len).expect("tag");
        enc.write_length_delimited(&[1, 2, 3]);
        let payload = enc.into_vec();

        let mut stats = DecodeStats::new();
        let mut prof = ProfiledDecodeBuffer::new(&payload, &mut stats);
        let t = prof.read_tag().expect("tag");
        assert_eq!(t.field_number, 1);
        let raw = prof.read_length_delimited().expect("bytes");
        let owned = raw.to_vec();
        assert_eq!(owned, [1, 2, 3]);
        prof.record_bytes_alloc(owned.len());

        assert_eq!(stats.bytes_alloc_count, 1);
        assert_eq!(stats.bytes_alloc_bytes, 3);
    }

    #[test]
    fn profiled_buffer_fixed_reads() {
        let mut enc = EncodeBuffer::new();
        enc.write_tag(1, WireType::I32).expect("tag");
        enc.write_fixed32(0xDEAD);
        enc.write_tag(2, WireType::I64).expect("tag");
        enc.write_fixed64(0xCAFE_BABE);
        let payload = enc.into_vec();

        let mut stats = DecodeStats::new();
        let mut prof = ProfiledDecodeBuffer::new(&payload, &mut stats);

        let _t1 = prof.read_tag().expect("t1");
        let _ = prof.read_fixed32().expect("f32");
        let _t2 = prof.read_tag().expect("t2");
        let _ = prof.read_fixed64().expect("f64");

        // Fixed reads do not allocate.
        assert!(stats.is_zero());
    }

    #[test]
    fn profiled_buffer_remaining_and_position() {
        let payload = make_payload();
        let total = payload.len();
        let mut stats = DecodeStats::new();
        let prof = ProfiledDecodeBuffer::new(&payload, &mut stats);
        assert_eq!(prof.remaining(), total);
        assert!(!prof.is_empty());
    }
}
