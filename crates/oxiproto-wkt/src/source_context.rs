#![forbid(unsafe_code)]
//! Extension trait for `prost_types::SourceContext`.
//!
//! Provides ergonomic construction and access methods for the well-known
//! `google.protobuf.SourceContext` type.

use prost_types::SourceContext;

/// Extension methods for [`prost_types::SourceContext`].
pub trait SourceContextExt {
    /// Create a `SourceContext` with the given file name.
    #[allow(clippy::new_ret_no_self)]
    fn new(file_name: impl Into<String>) -> SourceContext;

    /// Return the file name stored in this `SourceContext`.
    fn file_name(&self) -> &str;
}

impl SourceContextExt for SourceContext {
    fn new(file_name: impl Into<String>) -> SourceContext {
        SourceContext {
            file_name: file_name.into(),
        }
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_file_name() {
        let sc = SourceContext::new("google/protobuf/timestamp.proto");
        assert_eq!(sc.file_name(), "google/protobuf/timestamp.proto");
    }

    #[test]
    fn new_empty_string() {
        let sc = SourceContext::new("");
        assert_eq!(sc.file_name(), "");
    }
}
