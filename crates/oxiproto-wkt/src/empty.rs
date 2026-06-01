#![forbid(unsafe_code)]
//! A local equivalent of `google.protobuf.Empty` and its extension trait.
//!
//! `prost_types` (as of 0.14.x) does not export an `Empty` type, so we define
//! one here. It is a zero-field message that carries no information and is
//! mainly used as a placeholder input/output type in RPCs.

/// A zero-field message equivalent to `google.protobuf.Empty`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Empty {}

/// A pre-constructed `Empty` value for use as a constant.
pub const EMPTY: Empty = Empty {};

/// Convenience methods for [`Empty`].
pub trait EmptyExt {
    /// Create a new `Empty` value.
    #[allow(clippy::new_ret_no_self)]
    fn new() -> Empty;
}

impl EmptyExt for Empty {
    fn new() -> Empty {
        Empty {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_const_accessible() {
        let _ = EMPTY;
    }

    #[test]
    fn new_equals_const() {
        assert_eq!(Empty::new(), EMPTY);
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(Empty::default(), EMPTY);
    }
}
