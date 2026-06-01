#![forbid(unsafe_code)]

//! Comment extraction and association for source_code_info generation.
//!
//! A separate re-lex pass over the source string extracts all comment tokens
//! into a `CommentMap`.  This avoids touching the 1300-line `parse.rs` and its
//! 30+ functions that take `&mut PeekLexer`.

#[cfg(feature = "native-parser")]
use crate::parser::{lexer::Lexer, span::Span, token::Token};

// ---------------------------------------------------------------------------
// CommentEntry
// ---------------------------------------------------------------------------

/// A single comment token extracted from source.
#[cfg(feature = "native-parser")]
pub(crate) struct CommentEntry {
    /// Byte span of the entire comment token (including `//` / `/* */`).
    pub span: Span,
    /// Comment text formatted in protobuf style: post-`//` text with trailing
    /// `\n` for line comments; inner text with trailing `\n` for block comments.
    pub text: String,
}

// ---------------------------------------------------------------------------
// CommentMap
// ---------------------------------------------------------------------------

/// A sorted collection of `CommentEntry` values, supporting leading/trailing
/// comment queries for any source position.
#[cfg(feature = "native-parser")]
pub(crate) struct CommentMap {
    /// All comments, sorted ascending by `span.start`.
    entries: Vec<CommentEntry>,
}

#[cfg(feature = "native-parser")]
impl CommentMap {
    /// Build a `CommentMap` from the given entries (any order).
    pub fn build(mut entries: Vec<CommentEntry>) -> Self {
        entries.sort_by_key(|e| e.span.start);
        Self { entries }
    }

    /// Extract all comment tokens from `src` by re-lexing.  Lex errors and
    /// non-comment tokens are silently skipped — the source already passed a
    /// successful parse, so this cannot produce new syntax errors.
    pub fn extract(src: &str) -> Self {
        let mut entries = Vec::new();
        for result in Lexer::new(src) {
            let spanned = match result {
                Ok(s) => s,
                Err(_) => continue,
            };
            match spanned.value {
                Token::LineComment(text) => {
                    // Protobuf format: the text after `//` with a trailing `\n`.
                    let formatted = format!("{text}\n");
                    entries.push(CommentEntry {
                        span: spanned.span,
                        text: formatted,
                    });
                }
                Token::BlockComment(text) => {
                    // Protobuf format: inner text with trailing `\n`.
                    let formatted = format!("{text}\n");
                    entries.push(CommentEntry {
                        span: spanned.span,
                        text: formatted,
                    });
                }
                Token::Eof => break,
                _ => {}
            }
        }
        Self::build(entries)
    }

    /// Find the index of the last comment whose `span.end <= pos`.
    fn last_before(&self, pos: usize) -> Option<usize> {
        // entries are sorted by start; all entries with end <= pos qualify.
        // We want the last such entry.
        let mut result = None;
        for (i, e) in self.entries.iter().enumerate() {
            if e.span.end <= pos {
                result = Some(i);
            } else {
                break;
            }
        }
        result
    }

    /// Leading and detached comments for the declaration starting at byte
    /// offset `decl_start` in `src`.
    ///
    /// Returns `(leading_comments, leading_detached_comments)`.
    ///
    /// **Leading** comments are the group of comments that immediately precede
    /// the declaration with no blank line between the last comment and
    /// `decl_start`.
    ///
    /// **Detached** comments are earlier groups separated from `decl_start`
    /// (and from each other) by blank lines.  They are returned in source
    /// order, outermost group first.
    pub fn leading_for(&self, decl_start: usize, src: &[u8]) -> (Option<String>, Vec<String>) {
        // All comment entries whose span.end <= decl_start, in reverse order.
        let last_idx = match self.last_before(decl_start) {
            None => return (None, Vec::new()),
            Some(i) => i,
        };

        // Walk backwards from last_idx, grouping comments by blank-line
        // boundaries.  A "blank line" between two positions means >=2 `\n`
        // bytes in src[a..b].
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut current_group: Vec<String> = Vec::new();

        // The "next boundary" starts at decl_start and moves backward as we
        // accumulate each comment.
        let mut boundary = decl_start;

        let mut idx = last_idx as isize;
        while idx >= 0 {
            let entry = &self.entries[idx as usize];

            // Gap between this comment's end and the current boundary.
            let gap_start = entry.span.end;
            let gap_end = boundary.min(src.len());
            let gap = if gap_end > gap_start {
                &src[gap_start..gap_end]
            } else {
                &[]
            };
            let newlines = gap.iter().filter(|&&b| b == b'\n').count();

            if newlines >= 2 {
                // Blank line found — close current group and start a new one.
                if !current_group.is_empty() {
                    // current_group is in reverse order; fix it.
                    current_group.reverse();
                    groups.push(current_group);
                    current_group = Vec::new();
                }
            }

            current_group.push(entry.text.clone());
            boundary = entry.span.start;
            idx -= 1;
        }

        // Flush the last (innermost) group.
        if !current_group.is_empty() {
            current_group.reverse();
            groups.push(current_group);
        }

        // `groups` is in innermost-first order; reverse to get source order.
        groups.reverse();

        if groups.is_empty() {
            return (None, Vec::new());
        }

        // The last group (after reversal) is the immediately leading group.
        let leading_group = groups.pop().expect("non-empty groups");
        let leading = if leading_group.is_empty() {
            None
        } else {
            Some(leading_group.concat())
        };

        // Remaining groups are detached, in source order; flatten each group.
        let detached: Vec<String> = groups.into_iter().map(|g| g.concat()).collect();

        (leading, detached)
    }

    /// Trailing comment: the first comment on the same line as `decl_end`.
    ///
    /// A trailing comment is a `//` or `/* */` comment whose start byte is
    /// >= `decl_end` and is on the same source line as `decl_end`.
    pub fn trailing_for(&self, decl_end: usize, src: &[u8]) -> Option<String> {
        // Compute the source line index (0-based) containing `decl_end`.
        let line_of_end = src[..decl_end.min(src.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count();

        // Find the first comment with span.start >= decl_end on the same line.
        for entry in &self.entries {
            if entry.span.start < decl_end {
                continue;
            }
            let line_of_comment = src[..entry.span.start.min(src.len())]
                .iter()
                .filter(|&&b| b == b'\n')
                .count();
            if line_of_comment == line_of_end {
                return Some(entry.text.clone());
            }
            // Comments are sorted by start; once past the line, stop.
            if line_of_comment > line_of_end {
                break;
            }
        }
        None
    }
}
