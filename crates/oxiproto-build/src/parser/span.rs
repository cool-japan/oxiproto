#![forbid(unsafe_code)]

/// A byte-range span in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// Create a new span from `start` (inclusive) to `end` (exclusive).
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Number of bytes covered by this span.
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// `true` if the span covers no bytes.
    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// A value with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Wrap `value` with the given `span`.
    pub fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }
}

// ---------------------------------------------------------------------------
// LineTable — precomputed line-start offsets for O(log n) span conversion
// ---------------------------------------------------------------------------

/// Precomputed table of byte offsets at which each line begins.
///
/// `line_starts[0] == 0` (line 0 starts at byte 0).
/// `line_starts[i]` is the offset of the first byte on line i.
///
/// Required by `source_code_info` generation (`native-parser` feature).
#[cfg(feature = "native-parser")]
pub(crate) struct LineTable {
    line_starts: Vec<usize>,
}

#[cfg(feature = "native-parser")]
impl LineTable {
    /// Build a `LineTable` from the complete source string.
    pub fn build(src: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    /// Convert a byte offset to (0-based line, 0-based col).
    pub fn offset_to_line_col_0based(&self, offset: usize) -> (u32, u32) {
        let line = self
            .line_starts
            .partition_point(|&s| s <= offset)
            .saturating_sub(1);
        let col = offset.saturating_sub(self.line_starts.get(line).copied().unwrap_or(0));
        (line as u32, col as u32)
    }

    /// Build a protobuf `Location::span` vector from a byte range.
    ///
    /// Returns `[sl, sc, ec]` for same-line spans, or `[sl, sc, el, ec]` for
    /// multi-line spans, where all values are 0-based as required by protobuf.
    pub fn proto_span(&self, start: usize, end: usize) -> Vec<i32> {
        let (sl, sc) = self.offset_to_line_col_0based(start);
        let (el, ec) = self.offset_to_line_col_0based(end);
        if sl == el {
            vec![sl as i32, sc as i32, ec as i32]
        } else {
            vec![sl as i32, sc as i32, el as i32, ec as i32]
        }
    }
}

// ---------------------------------------------------------------------------
// offset_to_line_col — 1-based, used for error messages only
// ---------------------------------------------------------------------------

/// Map a 0-based byte offset to 1-based (line, col) for error reporting.
#[cfg(feature = "native-parser")]
pub(crate) fn offset_to_line_col(src: &str, offset: usize) -> (u32, u32) {
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, b) in src.bytes().enumerate() {
        if i >= offset {
            break;
        }
        if b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
