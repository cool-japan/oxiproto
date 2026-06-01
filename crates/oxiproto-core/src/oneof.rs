#![forbid(unsafe_code)]

//! Trait for oneof field group enums.
//!
//! A generated `FooOneof` enum (holding variants for each oneof field)
//! implements [`OxiOneof`] so the containing message's `encode_raw` and
//! `merge` impls can dispatch through it cleanly.

use crate::{wire, OxiProtoResult};

/// Trait for oneof field group enums.
///
/// A generated `FooOneof` enum (holding variants for each oneof field)
/// implements this trait so the containing message's `encode_raw` and
/// `merge` impls can dispatch through it cleanly.
pub trait OxiOneof: Sized {
    /// The proto field number of the currently-active variant.
    fn discriminant(&self) -> u32;

    /// The encoded size of the active variant (tag + value).
    fn encoded_len(&self) -> usize;

    /// Write the active variant (tag + value) into `buf`.
    fn encode(&self, buf: &mut wire::EncodeBuffer);

    /// Attempt to decode a field from `buf` into `slot`.
    ///
    /// Called from the containing message's `merge` loop when the
    /// `field_number` belongs to this oneof group. If `field_number`
    /// is not recognised by this oneof, return `Ok(false)` so the
    /// caller can forward to `UnknownFields`. On success, return `Ok(true)`.
    fn merge_field(
        field_number: u32,
        wire_type: wire::WireType,
        buf: &mut wire::DecodeBuffer,
        slot: &mut Option<Self>,
    ) -> OxiProtoResult<bool>;
}
