#![forbid(unsafe_code)]

//! Native OxiProto message trait.
//!
//! Every generated protobuf message type implements [`OxiMessage`].
//! Unlike `prost::Message`, this trait works directly with the
//! native [`wire::EncodeBuffer`] / [`wire::DecodeBuffer`] types.

use crate::{wire, OxiProtoResult};
use prost::alloc::vec::Vec;

/// The native OxiProto message trait.
///
/// Every generated protobuf message type implements this trait.
/// Unlike `prost::Message`, this trait works directly with the
/// native `wire::EncodeBuffer` / `wire::DecodeBuffer` types.
///
/// # Note
/// The existing `pub use prost::Message` re-export in `oxiproto-core` remains
/// unchanged. This is a NEW trait with a distinct name.
pub trait OxiMessage: Sized + core::fmt::Debug + Default + Send + Sync {
    /// Total encoded size of this message in bytes.
    fn encoded_len(&self) -> usize;

    /// Encode this message into `buf`, appending field by field.
    ///
    /// Fields with proto3 default values (0, false, empty string/bytes)
    /// **must be omitted** to maintain wire-format compatibility.
    fn encode_raw(&self, buf: &mut wire::EncodeBuffer);

    /// Merge the wire-format bytes from `buf` into `self`.
    ///
    /// For repeated fields, appends. For singular fields, last write wins
    /// (protobuf spec). For unknown field numbers, skip over them.
    fn merge(&mut self, buf: &mut wire::DecodeBuffer) -> OxiProtoResult<()>;

    /// Reset all fields to their default values.
    fn clear(&mut self);

    // Provided default implementations:

    /// Decode a complete message from `buf` (creates a fresh Default instance, then merges).
    fn decode_raw(buf: &mut wire::DecodeBuffer) -> OxiProtoResult<Self> {
        let mut me = Self::default();
        me.merge(buf)?;
        Ok(me)
    }

    /// Encode this message to a `Vec<u8>`.
    fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = wire::EncodeBuffer::new();
        self.encode_raw(&mut buf);
        buf.into_vec()
    }

    /// Decode a message from a byte slice.
    fn decode(bytes: &[u8]) -> OxiProtoResult<Self> {
        let mut buf = wire::DecodeBuffer::new(bytes);
        Self::decode_raw(&mut buf)
    }
}
