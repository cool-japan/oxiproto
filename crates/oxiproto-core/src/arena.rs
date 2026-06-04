#![forbid(unsafe_code)]

//! Arena (bump) allocator for repeated protobuf message fields.
//!
//! Decoding `repeated` fields in protobuf triggers many small heap allocations
//! — one `Vec` push per element, each element potentially containing several
//! `String` / `Vec<u8>` sub-allocations. In hot paths (e.g. a repeated
//! submessage with 1000+ entries) the default Rust allocator can become a
//! bottleneck because of fragmentation and per-alloc overhead.
//!
//! This module provides an *arena allocator* strategy built entirely on safe
//! Rust using `prost::alloc` (so it works under `no_std + alloc`). The design:
//!
//! 1. **[`ArenaVec<T>`]** — a `Vec<T>` wrapper that pre-allocates in
//!    configurable *slab* sizes instead of Rust's default 2× doubling.
//!    For highly predictable repeated fields (e.g. "always ~1000 elements")
//!    you can call `ArenaVec::with_slab(capacity_hint)` to avoid any resize.
//!
//! 2. **[`StringPool`]** — a compact string intern/dedup pool for repeated
//!    `string` fields where many elements share the same value (enum-as-string
//!    or tag fields with high cardinality overlap). Stores each unique string
//!    once and returns shared references, cutting allocation costs from
//!    O(n × string_len) down to O(unique_strings × string_len + n × ptr_size).
//!
//! 3. **[`BytesArena`]** — a single large `Vec<u8>` into which repeated
//!    `bytes` payloads are appended contiguously, with an index table of
//!    (start, len) pairs. Eliminates per-entry heap allocation entirely for
//!    `bytes` fields. Trade-off: the arena owns all bytes and cannot release
//!    individual entries.
//!
//! 4. **[`ArenaDecoder`]** — a convenience wrapper combining an [`ArenaVec`]
//!    for elements and a [`BytesArena`] for sub-slices, used by generated code
//!    when the `arena` feature is active.
//!
//! # Example — ArenaVec
//!
//! ```rust
//! use oxiproto_core::arena::ArenaVec;
//!
//! // Pre-allocate a slab of 256 entries (no resize until 256 elements pushed).
//! let mut ids: ArenaVec<i32> = ArenaVec::with_slab(256);
//! for i in 0..200i32 {
//!     ids.push(i);
//! }
//! assert_eq!(ids.len(), 200);
//! assert_eq!(ids[0], 0);
//! assert_eq!(ids[199], 199);
//! ```
//!
//! # Example — StringPool
//!
//! ```rust
//! use oxiproto_core::arena::StringPool;
//!
//! let mut pool = StringPool::new();
//! let idx_a = pool.intern("admin");
//! let idx_b = pool.intern("user");
//! let idx_c = pool.intern("admin"); // same as idx_a
//!
//! assert_eq!(idx_a, idx_c);
//! assert_ne!(idx_a, idx_b);
//! assert_eq!(pool.get(idx_a), "admin");
//! assert_eq!(pool.unique_count(), 2);
//! ```

use prost::alloc::{borrow::ToOwned, collections::BTreeMap, string::String, vec::Vec};

// ── ArenaVec ──────────────────────────────────────────────────────────────────

/// A `Vec<T>` variant with slab-based pre-allocation for repeated fields.
///
/// Unlike the standard `Vec<T>` which doubles capacity on each reallocation,
/// `ArenaVec` grows by fixed `slab_size` steps. This reduces peak memory usage
/// when the number of elements is predictable (you can set `slab_size` equal to
/// or larger than the expected element count).
///
/// The public API mirrors `Vec<T>` so code can switch between the two.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArenaVec<T> {
    inner: Vec<T>,
    /// Number of elements to allocate per growth step.
    slab_size: usize,
}

impl<T> ArenaVec<T> {
    /// Create an empty `ArenaVec` with the default slab size (64 elements).
    pub fn new() -> Self {
        Self::with_slab(64)
    }

    /// Create an empty `ArenaVec` that pre-allocates `slab_size` elements at
    /// a time on each growth.
    ///
    /// Setting `slab_size` to the expected element count avoids all reallocations.
    pub fn with_slab(slab_size: usize) -> Self {
        let slab = slab_size.max(1);
        Self {
            inner: Vec::with_capacity(slab),
            slab_size: slab,
        }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if there are no elements.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns the current allocated capacity.
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Returns the configured slab size.
    pub fn slab_size(&self) -> usize {
        self.slab_size
    }

    /// Push an element, growing by `slab_size` if necessary.
    pub fn push(&mut self, value: T) {
        if self.inner.len() == self.inner.capacity() {
            self.inner.reserve_exact(self.slab_size);
        }
        self.inner.push(value);
    }

    /// Returns an iterator over the elements.
    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.inner.iter()
    }

    /// Returns a mutable iterator over the elements.
    pub fn iter_mut(&mut self) -> core::slice::IterMut<'_, T> {
        self.inner.iter_mut()
    }

    /// Returns a slice of all elements.
    pub fn as_slice(&self) -> &[T] {
        self.inner.as_slice()
    }

    /// Returns a mutable slice of all elements.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.inner.as_mut_slice()
    }

    /// Clear all elements (retains capacity).
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Consume the `ArenaVec` and return the underlying `Vec<T>`.
    pub fn into_vec(self) -> Vec<T> {
        self.inner
    }

    /// Consume a `Vec<T>` and wrap it in an `ArenaVec` with the given slab size.
    pub fn from_vec(v: Vec<T>, slab_size: usize) -> Self {
        Self {
            inner: v,
            slab_size: slab_size.max(1),
        }
    }

    /// Extend from an iterator.
    pub fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            self.push(item);
        }
    }

    /// Pre-allocate capacity for `additional` more elements.
    pub fn reserve(&mut self, additional: usize) {
        // Round up to the nearest slab boundary.
        let slabs_needed = additional.div_ceil(self.slab_size);
        self.inner.reserve_exact(slabs_needed * self.slab_size);
    }

    /// Estimate the number of reallocation events that have occurred.
    ///
    /// Since capacity grows in `slab_size` steps starting from `slab_size`,
    /// the number of reallocations is approximately `(len / slab_size)`.
    pub fn estimated_resizes(&self) -> usize {
        if self.inner.is_empty() || self.slab_size == 0 {
            return 0;
        }
        self.inner.len().saturating_sub(1) / self.slab_size
    }
}

impl<T> core::ops::Index<usize> for ArenaVec<T> {
    type Output = T;
    fn index(&self, idx: usize) -> &T {
        &self.inner[idx]
    }
}

impl<T> core::ops::IndexMut<usize> for ArenaVec<T> {
    fn index_mut(&mut self, idx: usize) -> &mut T {
        &mut self.inner[idx]
    }
}

impl<T> IntoIterator for ArenaVec<T> {
    type Item = T;
    type IntoIter = prost::alloc::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a ArenaVec<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<T> From<ArenaVec<T>> for Vec<T> {
    fn from(av: ArenaVec<T>) -> Vec<T> {
        av.inner
    }
}

// ── StringPool ────────────────────────────────────────────────────────────────

/// An index into a [`StringPool`].
///
/// `StringIndex` is a `u32` newtype so it fits in 4 bytes while supporting
/// pools of up to ~4 billion unique strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StringIndex(pub u32);

impl StringIndex {
    /// The raw index value.
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl core::fmt::Display for StringIndex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "StringIndex({})", self.0)
    }
}

/// An intern pool for `String` values in repeated `string` fields.
///
/// Each unique string is stored once in a backing `Vec<String>`. A `BTreeMap`
/// index maps each string's content to its position, enabling O(log n) dedup.
/// The pool itself is O(unique_count) in memory instead of O(total_count).
///
/// # Thread safety
///
/// `StringPool` is `Send + Sync` (the `BTreeMap` and `Vec` are both). However,
/// `&mut StringPool` is required for `intern` — shared access is read-only.
#[derive(Debug, Default, Clone)]
pub struct StringPool {
    /// The deduplicated strings, in insertion order.
    strings: Vec<String>,
    /// Map from string content to index in `strings`.
    index: BTreeMap<String, u32>,
}

impl StringPool {
    /// Create an empty pool.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a pool with pre-allocated capacity for `capacity` unique strings.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            strings: Vec::with_capacity(capacity),
            index: BTreeMap::new(),
        }
    }

    /// Intern a string, returning its [`StringIndex`].
    ///
    /// If `s` already exists in the pool, returns its existing index without
    /// allocation. Otherwise, inserts `s` and returns the new index.
    pub fn intern(&mut self, s: &str) -> StringIndex {
        if let Some(&idx) = self.index.get(s) {
            return StringIndex(idx);
        }
        let idx = self.strings.len() as u32;
        let owned = s.to_owned();
        self.index.insert(owned.clone(), idx);
        self.strings.push(owned);
        StringIndex(idx)
    }

    /// Look up the string at `idx`.
    ///
    /// # Panics
    ///
    /// Panics if `idx` is out of range (i.e. was not returned by this pool).
    pub fn get(&self, idx: StringIndex) -> &str {
        &self.strings[idx.as_usize()]
    }

    /// Look up the string at `idx`, returning `None` if out of range.
    pub fn get_checked(&self, idx: StringIndex) -> Option<&str> {
        self.strings.get(idx.as_usize()).map(|s| s.as_str())
    }

    /// Returns the number of unique strings stored.
    pub fn unique_count(&self) -> usize {
        self.strings.len()
    }

    /// Returns `true` if the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Returns an iterator over all interned strings in insertion order.
    pub fn iter(&self) -> core::slice::Iter<'_, String> {
        self.strings.iter()
    }

    /// Total heap bytes used by all interned string contents (not including
    /// `String` struct overhead or the index map).
    pub fn total_bytes(&self) -> usize {
        self.strings.iter().map(|s| s.len()).sum()
    }

    /// Clear the pool, releasing all interned strings.
    pub fn clear(&mut self) {
        self.strings.clear();
        self.index.clear();
    }
}

// ── BytesArena ────────────────────────────────────────────────────────────────

/// An index + length pair pointing into a [`BytesArena`] backing buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BytesHandle {
    /// Start offset in the backing buffer.
    pub start: usize,
    /// Length of the slice in bytes.
    pub len: usize,
}

impl BytesHandle {
    /// Returns `true` if the slice is empty.
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// A contiguous slab allocator for repeated `bytes` fields.
///
/// All byte slices are stored end-to-end in a single `Vec<u8>` backing buffer.
/// Retrieval is O(1) via a [`BytesHandle`] (start + length). No per-entry heap
/// allocation occurs after the initial buffer capacity is established.
///
/// Ideal for use cases where:
///
/// - `repeated bytes` fields contain many small-to-medium payloads.
/// - The payloads are accessed sequentially (streaming decode).
/// - Individual entries do not need to be freed before the arena is dropped.
#[derive(Debug, Default, Clone)]
pub struct BytesArena {
    /// Contiguous backing buffer holding all appended slices.
    buf: Vec<u8>,
    /// Index of all stored slices: (start, len).
    handles: Vec<BytesHandle>,
}

impl BytesArena {
    /// Create a new empty arena with no pre-allocation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new arena with `capacity_bytes` pre-allocated in the backing
    /// buffer and `capacity_entries` slots in the handle table.
    pub fn with_capacity(capacity_bytes: usize, capacity_entries: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity_bytes),
            handles: Vec::with_capacity(capacity_entries),
        }
    }

    /// Append `data` to the arena and return a [`BytesHandle`] for retrieval.
    pub fn append(&mut self, data: &[u8]) -> BytesHandle {
        let start = self.buf.len();
        self.buf.extend_from_slice(data);
        let handle = BytesHandle {
            start,
            len: data.len(),
        };
        self.handles.push(handle);
        handle
    }

    /// Retrieve the byte slice for `handle`.
    ///
    /// Returns `None` if `handle.start + handle.len > self.buf.len()` (stale
    /// handle or arena was cleared).
    pub fn get(&self, handle: BytesHandle) -> Option<&[u8]> {
        let end = handle.start.checked_add(handle.len)?;
        self.buf.get(handle.start..end)
    }

    /// Retrieve the byte slice for `handle`.
    ///
    /// # Panics
    ///
    /// Panics if the handle is out of range.
    pub fn get_unchecked(&self, handle: BytesHandle) -> &[u8] {
        &self.buf[handle.start..handle.start + handle.len]
    }

    /// Returns the number of stored entries.
    pub fn entry_count(&self) -> usize {
        self.handles.len()
    }

    /// Returns the total number of bytes stored in the backing buffer.
    pub fn total_bytes(&self) -> usize {
        self.buf.len()
    }

    /// Returns `true` if no data has been appended.
    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// Returns an iterator over all [`BytesHandle`]s in insertion order.
    pub fn handles(&self) -> core::slice::Iter<'_, BytesHandle> {
        self.handles.iter()
    }

    /// Returns an iterator over all byte slices in insertion order.
    pub fn iter(&self) -> BytesArenaIter<'_> {
        BytesArenaIter {
            arena: self,
            pos: 0,
        }
    }

    /// Consume the arena and return the raw backing buffer and handle table.
    pub fn into_parts(self) -> (Vec<u8>, Vec<BytesHandle>) {
        (self.buf, self.handles)
    }

    /// Clear all entries, releasing backing memory.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.handles.clear();
    }

    /// Reserve at least `additional_bytes` more bytes in the backing buffer
    /// and `additional_entries` more slots in the handle table.
    pub fn reserve(&mut self, additional_bytes: usize, additional_entries: usize) {
        self.buf.reserve(additional_bytes);
        self.handles.reserve(additional_entries);
    }
}

/// Iterator over byte slices in a [`BytesArena`].
pub struct BytesArenaIter<'a> {
    arena: &'a BytesArena,
    pos: usize,
}

impl<'a> Iterator for BytesArenaIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        if self.pos >= self.arena.handles.len() {
            return None;
        }
        let handle = self.arena.handles[self.pos];
        self.pos += 1;
        self.arena.get(handle)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.arena.handles.len() - self.pos;
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for BytesArenaIter<'a> {}

// ── ArenaDecoder ──────────────────────────────────────────────────────────────

/// A combined decode context pairing an element slab ([`ArenaVec`]) with a
/// bytes store ([`BytesArena`]).
///
/// Generated code for `repeated bytes` / `repeated message` fields can use
/// this instead of a plain `Vec`, significantly reducing per-element allocation
/// overhead in hot decode paths.
///
/// `ArenaDecoder<T>` is generic over the element type `T`. For `repeated bytes`
/// fields, `T` would be `BytesHandle`; for `repeated message` fields, `T` would
/// be the generated message struct.
#[derive(Debug, Default)]
pub struct ArenaDecoder<T> {
    /// Elements decoded so far.
    pub elements: ArenaVec<T>,
    /// Contiguous store for byte-payload sub-slices.
    pub bytes_store: BytesArena,
}

impl<T> ArenaDecoder<T> {
    /// Create an `ArenaDecoder` with the given element slab size and bytes
    /// capacity hint.
    pub fn new(slab_size: usize, bytes_capacity: usize) -> Self {
        Self {
            elements: ArenaVec::with_slab(slab_size),
            bytes_store: BytesArena::with_capacity(bytes_capacity, slab_size),
        }
    }

    /// Push a decoded element.
    pub fn push_element(&mut self, elem: T) {
        self.elements.push(elem);
    }

    /// Append a bytes payload and return its handle.
    pub fn append_bytes(&mut self, data: &[u8]) -> BytesHandle {
        self.bytes_store.append(data)
    }

    /// Returns the number of decoded elements.
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Clear all state (retains capacity).
    pub fn clear(&mut self) {
        self.elements.clear();
        self.bytes_store.clear();
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ArenaVec tests ────────────────────────────────────────────────────────

    #[test]
    fn arena_vec_new_starts_empty() {
        let v: ArenaVec<i32> = ArenaVec::new();
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn arena_vec_push_and_index() {
        let mut v: ArenaVec<i32> = ArenaVec::with_slab(4);
        for i in 0..10i32 {
            v.push(i);
        }
        assert_eq!(v.len(), 10);
        assert_eq!(v[0], 0);
        assert_eq!(v[9], 9);
    }

    #[test]
    fn arena_vec_slab_growth() {
        // Slab size 4 means we should see capacity jump by 4 each time.
        let mut v: ArenaVec<i32> = ArenaVec::with_slab(4);
        assert_eq!(v.capacity(), 4);
        // Push 4 elements — fills first slab.
        for i in 0..4 {
            v.push(i);
        }
        assert_eq!(v.capacity(), 4); // no growth yet
                                     // 5th element triggers a slab allocation.
        v.push(4);
        assert_eq!(v.capacity(), 8); // grew by slab_size=4
    }

    #[test]
    fn arena_vec_clear_retains_capacity() {
        let mut v: ArenaVec<String> = ArenaVec::with_slab(8);
        for _ in 0..8 {
            v.push("hello".to_owned());
        }
        let cap = v.capacity();
        v.clear();
        assert_eq!(v.len(), 0);
        assert_eq!(v.capacity(), cap);
    }

    #[test]
    fn arena_vec_extend() {
        let mut v: ArenaVec<i32> = ArenaVec::with_slab(2);
        v.extend([1, 2, 3, 4, 5]);
        assert_eq!(v.len(), 5);
        assert_eq!(v.as_slice(), &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn arena_vec_into_vec() {
        let mut v: ArenaVec<i32> = ArenaVec::with_slab(4);
        v.push(1);
        v.push(2);
        let plain: Vec<i32> = v.into_vec();
        assert_eq!(plain, vec![1, 2]);
    }

    #[test]
    fn arena_vec_from_vec() {
        let plain = vec![10, 20, 30];
        let av = ArenaVec::from_vec(plain.clone(), 8);
        assert_eq!(av.len(), plain.len());
        assert_eq!(av.as_slice(), plain.as_slice());
    }

    #[test]
    fn arena_vec_iter() {
        let mut v: ArenaVec<i32> = ArenaVec::with_slab(4);
        v.push(7);
        v.push(8);
        let collected: Vec<i32> = v.iter().copied().collect();
        assert_eq!(collected, vec![7, 8]);
    }

    #[test]
    fn arena_vec_reserve() {
        let mut v: ArenaVec<i32> = ArenaVec::with_slab(4);
        v.reserve(12);
        // Should have reserved at least 12 elements worth.
        assert!(v.capacity() >= 12);
    }

    #[test]
    fn arena_vec_estimated_resizes_zero_when_empty() {
        let v: ArenaVec<i32> = ArenaVec::with_slab(4);
        assert_eq!(v.estimated_resizes(), 0);
    }

    #[test]
    fn arena_vec_into_iter() {
        let mut v: ArenaVec<i32> = ArenaVec::with_slab(4);
        v.push(1);
        v.push(2);
        v.push(3);
        let sum: i32 = v.into_iter().sum();
        assert_eq!(sum, 6);
    }

    // ── StringPool tests ──────────────────────────────────────────────────────

    #[test]
    fn string_pool_intern_unique() {
        let mut pool = StringPool::new();
        let a = pool.intern("hello");
        let b = pool.intern("world");
        assert_ne!(a, b);
        assert_eq!(pool.unique_count(), 2);
    }

    #[test]
    fn string_pool_intern_dedup() {
        let mut pool = StringPool::new();
        let a = pool.intern("admin");
        let b = pool.intern("admin");
        assert_eq!(a, b);
        assert_eq!(pool.unique_count(), 1);
    }

    #[test]
    fn string_pool_get_correct_value() {
        let mut pool = StringPool::new();
        let idx = pool.intern("oxiproto");
        assert_eq!(pool.get(idx), "oxiproto");
    }

    #[test]
    fn string_pool_get_checked_none_for_oob() {
        let pool = StringPool::new();
        assert!(pool.get_checked(StringIndex(99)).is_none());
    }

    #[test]
    fn string_pool_total_bytes() {
        let mut pool = StringPool::new();
        pool.intern("abc");
        pool.intern("de");
        assert_eq!(pool.total_bytes(), 5);
    }

    #[test]
    fn string_pool_clear() {
        let mut pool = StringPool::new();
        pool.intern("foo");
        pool.intern("bar");
        pool.clear();
        assert!(pool.is_empty());
        assert_eq!(pool.unique_count(), 0);
    }

    #[test]
    fn string_pool_iter_order() {
        let mut pool = StringPool::new();
        pool.intern("c");
        pool.intern("a");
        pool.intern("b");
        let words: Vec<&str> = pool.iter().map(|s| s.as_str()).collect();
        // Pool preserves insertion order.
        assert_eq!(words, vec!["c", "a", "b"]);
    }

    #[test]
    fn string_pool_with_capacity_no_panic() {
        let mut pool = StringPool::with_capacity(16);
        for i in 0..20u32 {
            pool.intern(&prost::alloc::format!("str_{i}"));
        }
        assert_eq!(pool.unique_count(), 20);
    }

    // ── BytesArena tests ──────────────────────────────────────────────────────

    #[test]
    fn bytes_arena_append_and_get() {
        let mut arena = BytesArena::new();
        let h1 = arena.append(b"hello");
        let h2 = arena.append(b"world");
        assert_eq!(arena.get(h1), Some(b"hello".as_ref()));
        assert_eq!(arena.get(h2), Some(b"world".as_ref()));
    }

    #[test]
    fn bytes_arena_entry_count_and_total() {
        let mut arena = BytesArena::new();
        arena.append(b"abc");
        arena.append(b"de");
        assert_eq!(arena.entry_count(), 2);
        assert_eq!(arena.total_bytes(), 5);
    }

    #[test]
    fn bytes_arena_iter() {
        let mut arena = BytesArena::new();
        arena.append(b"x");
        arena.append(b"yy");
        arena.append(b"zzz");
        let slices: Vec<&[u8]> = arena.iter().collect();
        assert_eq!(slices, vec![b"x".as_ref(), b"yy".as_ref(), b"zzz".as_ref()]);
    }

    #[test]
    fn bytes_arena_iter_exact_size() {
        let mut arena = BytesArena::new();
        arena.append(b"a");
        arena.append(b"b");
        let it = arena.iter();
        assert_eq!(it.len(), 2);
    }

    #[test]
    fn bytes_arena_get_stale_handle_returns_none() {
        let mut arena = BytesArena::new();
        let h = arena.append(b"abc");
        arena.clear();
        // After clear the buf is empty — the handle is out of range.
        assert!(arena.get(h).is_none());
    }

    #[test]
    fn bytes_arena_clear_resets() {
        let mut arena = BytesArena::new();
        arena.append(b"data");
        arena.clear();
        assert!(arena.is_empty());
        assert_eq!(arena.total_bytes(), 0);
        assert_eq!(arena.entry_count(), 0);
    }

    #[test]
    fn bytes_arena_handle_is_empty() {
        let mut arena = BytesArena::new();
        let h_empty = arena.append(&[]);
        let h_nonempty = arena.append(b"a");
        assert!(h_empty.is_empty());
        assert!(!h_nonempty.is_empty());
    }

    #[test]
    fn bytes_arena_get_unchecked() {
        let mut arena = BytesArena::new();
        let h = arena.append(b"test");
        assert_eq!(arena.get_unchecked(h), b"test");
    }

    #[test]
    fn bytes_arena_reserve() {
        let mut arena = BytesArena::new();
        arena.reserve(1024, 32);
        // Just ensure it doesn't panic; capacity is an impl detail.
        assert!(arena.is_empty());
    }

    #[test]
    fn bytes_arena_into_parts() {
        let mut arena = BytesArena::new();
        arena.append(b"abc");
        let (buf, handles) = arena.into_parts();
        assert_eq!(&buf[..3], b"abc");
        assert_eq!(handles.len(), 1);
        assert_eq!(handles[0].start, 0);
        assert_eq!(handles[0].len, 3);
    }

    #[test]
    fn bytes_arena_handles_iter() {
        let mut arena = BytesArena::new();
        arena.append(b"a");
        arena.append(b"bb");
        let hs: Vec<BytesHandle> = arena.handles().copied().collect();
        assert_eq!(hs.len(), 2);
        assert_eq!(hs[0].len, 1);
        assert_eq!(hs[1].len, 2);
    }

    // ── ArenaDecoder tests ────────────────────────────────────────────────────

    #[test]
    fn arena_decoder_push_and_count() {
        let mut dec: ArenaDecoder<i32> = ArenaDecoder::new(8, 64);
        dec.push_element(1);
        dec.push_element(2);
        dec.push_element(3);
        assert_eq!(dec.element_count(), 3);
    }

    #[test]
    fn arena_decoder_append_bytes() {
        let mut dec: ArenaDecoder<BytesHandle> = ArenaDecoder::new(8, 64);
        let h = dec.append_bytes(b"payload");
        dec.push_element(h);
        assert_eq!(dec.element_count(), 1);
        let stored = dec.bytes_store.get(h).expect("handle valid");
        assert_eq!(stored, b"payload");
    }

    #[test]
    fn arena_decoder_clear() {
        let mut dec: ArenaDecoder<i32> = ArenaDecoder::new(4, 32);
        dec.push_element(99);
        dec.clear();
        assert_eq!(dec.element_count(), 0);
    }

    // ── Cross-component ───────────────────────────────────────────────────────

    #[test]
    fn arena_vec_of_string_indices() {
        // Simulate decoding repeated string field with intern pool.
        let mut pool = StringPool::new();
        let mut indices: ArenaVec<StringIndex> = ArenaVec::with_slab(4);

        let data = ["alpha", "beta", "alpha", "gamma", "beta", "alpha"];
        for s in &data {
            let idx = pool.intern(s);
            indices.push(idx);
        }

        assert_eq!(indices.len(), data.len());
        assert_eq!(pool.unique_count(), 3); // alpha, beta, gamma

        // All "alpha" entries map to the same index.
        let alpha_idx = pool.intern("alpha");
        for (i, &idx) in indices.iter().enumerate() {
            if data[i] == "alpha" {
                assert_eq!(idx, alpha_idx);
            }
        }
    }

    #[test]
    fn bytes_arena_large_payload() {
        let mut arena = BytesArena::with_capacity(8192, 16);
        let payload: Vec<u8> = (0u8..=255).cycle().take(1000).collect();
        let h = arena.append(&payload);
        assert_eq!(arena.get(h), Some(payload.as_slice()));
    }

    #[test]
    fn arena_vec_min_slab_one() {
        // slab_size=0 is clamped to 1.
        let mut v: ArenaVec<u8> = ArenaVec::with_slab(0);
        assert_eq!(v.slab_size(), 1);
        v.push(42);
        assert_eq!(v[0], 42);
    }
}
