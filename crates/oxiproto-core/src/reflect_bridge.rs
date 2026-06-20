/*!
Native → reflection bridge.

This module provides the [`OxiReflect`] trait that connects types
implementing [`OxiMessage`] + [`OxiName`] to the native protobuf
reflection layer.

## Design

The bridge is deliberately a thin glue layer.  A type that implements both
`OxiMessage` (native wire encode/decode) and `OxiName` (proto type metadata)
can be introspected reflectively by:

1. **Type-URL extraction** — `OxiName::type_url()` gives the fully-qualified
   type URL used to locate the message in a descriptor pool.

2. **Wire round-trip** — the message is encoded to bytes via
   `OxiMessage::encode_to_vec` and those bytes plus the full name are handed
   to any reflection system that accepts `(full_name, &[u8])`.

3. **Opaque handle** — `OxiReflectHandle` packages a full name and encoded
   bytes so the reflect layer can decode the message dynamically without
   knowing the concrete type.

## Example

```rust
use oxiproto_core::reflect_bridge::{OxiReflect, OxiReflectHandle};
use oxiproto_core::{OxiMessage, OxiName};
use oxiproto_core::wire::{DecodeBuffer, EncodeBuffer};
use oxiproto_core::OxiProtoResult;

#[derive(Debug, Default, Clone)]
struct Point {
    x: i32,
    y: i32,
}

impl OxiName for Point {
    const NAME: &'static str = "Point";
    const PACKAGE: &'static str = "geometry";
}

impl OxiMessage for Point {
    fn encoded_len(&self) -> usize {
        let mut n = 0;
        if self.x != 0 {
            n += 1 + ::oxiproto_core::wire::varint::encoded_len_varint(self.x as i64 as u64);
        }
        if self.y != 0 {
            n += 1 + ::oxiproto_core::wire::varint::encoded_len_varint(self.y as i64 as u64);
        }
        n
    }
    fn encode_raw(&self, buf: &mut EncodeBuffer) {
        if self.x != 0 {
            let _ = buf.write_tag(1, ::oxiproto_core::wire::WireType::Varint);
            buf.write_varint_i32(self.x);
        }
        if self.y != 0 {
            let _ = buf.write_tag(2, ::oxiproto_core::wire::WireType::Varint);
            buf.write_varint_i32(self.y);
        }
    }
    fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = buf.read_tag().map_err(oxiproto_core::OxiProtoError::WireFormatError)?;
            match tag.field_number {
                1 => self.x = buf.read_varint_i32().map_err(oxiproto_core::OxiProtoError::WireFormatError)?,
                2 => self.y = buf.read_varint_i32().map_err(oxiproto_core::OxiProtoError::WireFormatError)?,
                _ => buf.skip_field(tag.wire_type).map_err(oxiproto_core::OxiProtoError::WireFormatError)?,
            }
        }
        Ok(())
    }
    fn clear(&mut self) {
        *self = Self::default();
    }
}

let point = Point { x: 3, y: -7 };
let handle = point.reflect_handle();
assert_eq!(handle.full_name(), "geometry.Point");
assert_eq!(handle.type_url(), "type.googleapis.com/geometry.Point");
assert!(!handle.encoded_bytes().is_empty());
```
*/

use prost::alloc::{string::String, vec::Vec};

use crate::{OxiMessage, OxiName};

// ---------------------------------------------------------------------------
// OxiReflectHandle — a type-erased snapshot of a reflected message
// ---------------------------------------------------------------------------

/// A type-erased snapshot of an [`OxiMessage`] suitable for passing to
/// reflection or introspection APIs.
///
/// The handle owns the message's fully-qualified proto name and a
/// wire-encoded byte snapshot. It is intentionally *not* tied to the
/// original concrete type — callers can pass it to any API that works
/// with `(full_name, &[u8])` without carrying generic type parameters.
///
/// Create one via [`OxiReflect::reflect_handle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OxiReflectHandle {
    full_name: String,
    type_url: String,
    encoded: Vec<u8>,
}

impl OxiReflectHandle {
    /// The fully-qualified proto name, e.g. `"my.package.MyMessage"`.
    pub fn full_name(&self) -> &str {
        &self.full_name
    }

    /// The type URL, e.g. `"type.googleapis.com/my.package.MyMessage"`.
    pub fn type_url(&self) -> &str {
        &self.type_url
    }

    /// The wire-encoded bytes of the message snapshot.
    ///
    /// These bytes are a valid protobuf binary encoding and can be fed
    /// into any conformant decoder, including prost and the native
    /// `oxiproto_core::wire` codecs.
    pub fn encoded_bytes(&self) -> &[u8] {
        &self.encoded
    }

    /// Whether the encoded snapshot is empty (default/zero-value message).
    pub fn is_empty_message(&self) -> bool {
        self.encoded.is_empty()
    }

    /// Consume the handle and return the raw encoded bytes.
    pub fn into_encoded(self) -> Vec<u8> {
        self.encoded
    }
}

// ---------------------------------------------------------------------------
// OxiReflect trait
// ---------------------------------------------------------------------------

/// Bridge trait that connects [`OxiMessage`] + [`OxiName`] types to the
/// native reflection layer.
///
/// Every type that implements both `OxiMessage` and `OxiName` automatically
/// gets a blanket `OxiReflect` implementation — there is nothing to implement
/// manually.
///
/// # Usage
///
/// ```rust
/// use oxiproto_core::reflect_bridge::OxiReflect;
/// use oxiproto_core::{OxiMessage, OxiName};
/// # use oxiproto_core::wire::{DecodeBuffer, EncodeBuffer};
/// # use oxiproto_core::OxiProtoResult;
/// #
/// # #[derive(Debug, Default, Clone)]
/// # struct Ping { seq: u32 }
/// # impl OxiName for Ping {
/// #     const NAME: &'static str = "Ping";
/// #     const PACKAGE: &'static str = "net";
/// # }
/// # impl OxiMessage for Ping {
/// #     fn encoded_len(&self) -> usize { if self.seq != 0 { 2 } else { 0 } }
/// #     fn encode_raw(&self, buf: &mut EncodeBuffer) {
/// #         if self.seq != 0 {
/// #             let _ = buf.write_tag(1, ::oxiproto_core::wire::WireType::Varint);
/// #             buf.write_varint32(self.seq);
/// #         }
/// #     }
/// #     fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
/// #         while !buf.is_empty() {
/// #             let tag = buf.read_tag().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;
/// #             match tag.field_number {
/// #                 1 => self.seq = buf.read_varint32().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?,
/// #                 _ => buf.skip_field(tag.wire_type).map_err(::oxiproto_core::OxiProtoError::WireFormatError)?,
/// #             }
/// #         }
/// #         Ok(())
/// #     }
/// #     fn clear(&mut self) { *self = Self::default(); }
/// # }
///
/// let ping = Ping { seq: 7 };
/// // Via OxiReflect (auto-implemented):
/// let handle = ping.reflect_handle();
/// assert_eq!(handle.full_name(), "net.Ping");
/// let bytes = handle.encoded_bytes();
/// // Re-decode with the native codec:
/// let decoded = Ping::decode(bytes).unwrap();
/// assert_eq!(decoded.seq, 7);
/// ```
pub trait OxiReflect: OxiMessage + OxiName {
    /// The fully-qualified proto name of this message.
    ///
    /// Delegates to [`OxiName::full_name`].
    fn proto_full_name() -> String
    where
        Self: Sized,
    {
        Self::full_name()
    }

    /// The type URL for this message.
    ///
    /// Delegates to [`OxiName::type_url`].
    fn proto_type_url() -> String
    where
        Self: Sized,
    {
        Self::type_url()
    }

    /// Encode this message and package it as an [`OxiReflectHandle`].
    ///
    /// The handle is a cheaply shareable, type-erased snapshot that carries
    /// the full proto name, type URL, and wire bytes.
    fn reflect_handle(&self) -> OxiReflectHandle {
        OxiReflectHandle {
            full_name: Self::full_name(),
            type_url: Self::type_url(),
            encoded: self.encode_to_vec(),
        }
    }
}

/// Blanket implementation: every type that is both `OxiMessage` and `OxiName`
/// automatically gains `OxiReflect`.
impl<T: OxiMessage + OxiName> OxiReflect for T {}

// ---------------------------------------------------------------------------
// BridgeDecoder — decode an OxiReflectHandle back to a concrete type
// ---------------------------------------------------------------------------

/// Decode an [`OxiReflectHandle`] back into a concrete `T: OxiMessage`.
///
/// This performs the inverse of [`OxiReflect::reflect_handle`]: it takes
/// the wire bytes stored in the handle and decodes them into `T`.
///
/// # Type Safety
///
/// The caller is responsible for ensuring `T` matches the message type that
/// was originally reflected.  If the bytes were produced by a different
/// message type the decode will either succeed with garbage values, or fail
/// with a wire error.  This mirrors standard protobuf dynamic-typing behaviour
/// (the wire format carries no type annotations).
///
/// # Errors
///
/// Returns an error if the wire bytes are malformed.
///
/// # Example
///
/// ```rust
/// use oxiproto_core::reflect_bridge::{decode_handle, OxiReflect};
/// use oxiproto_core::{OxiMessage, OxiName};
/// # use oxiproto_core::wire::{DecodeBuffer, EncodeBuffer};
/// # use oxiproto_core::OxiProtoResult;
/// #
/// # #[derive(Debug, Default, Clone, PartialEq)]
/// # struct Score { value: u64 }
/// # impl OxiName for Score {
/// #     const NAME: &'static str = "Score";
/// #     const PACKAGE: &'static str = "";
/// # }
/// # impl OxiMessage for Score {
/// #     fn encoded_len(&self) -> usize {
/// #         if self.value != 0 { 1 + ::oxiproto_core::wire::varint::encoded_len_varint(self.value) } else { 0 }
/// #     }
/// #     fn encode_raw(&self, buf: &mut EncodeBuffer) {
/// #         if self.value != 0 {
/// #             let _ = buf.write_tag(1, ::oxiproto_core::wire::WireType::Varint);
/// #             buf.write_varint(self.value);
/// #         }
/// #     }
/// #     fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
/// #         while !buf.is_empty() {
/// #             let tag = buf.read_tag().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;
/// #             match tag.field_number {
/// #                 1 => self.value = buf.read_varint().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?,
/// #                 _ => buf.skip_field(tag.wire_type).map_err(::oxiproto_core::OxiProtoError::WireFormatError)?,
/// #             }
/// #         }
/// #         Ok(())
/// #     }
/// #     fn clear(&mut self) { *self = Self::default(); }
/// # }
///
/// let original = Score { value: 9999 };
/// let handle = original.reflect_handle();
/// let decoded: Score = decode_handle(&handle).unwrap();
/// assert_eq!(decoded, original);
/// ```
pub fn decode_handle<T: OxiMessage>(handle: &OxiReflectHandle) -> crate::OxiProtoResult<T> {
    T::decode(handle.encoded_bytes())
}

// ---------------------------------------------------------------------------
// ReflectMetadata — static type information without an instance
// ---------------------------------------------------------------------------

/// Static reflection metadata for a type that implements [`OxiName`].
///
/// Unlike [`OxiReflect`] (which requires a *value*), `ReflectMetadata` can
/// be constructed without an instance, making it useful for building
/// schema-level registries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReflectMetadata {
    /// Simple proto message name.
    pub name: &'static str,
    /// Proto package path (empty for top-level types).
    pub package: &'static str,
    /// Cached fully-qualified name.
    pub full_name: String,
    /// Cached type URL.
    pub type_url: String,
}

impl ReflectMetadata {
    /// Build a `ReflectMetadata` from a type `T: OxiName`.
    ///
    /// ```rust
    /// use oxiproto_core::reflect_bridge::ReflectMetadata;
    /// use oxiproto_core::OxiName;
    ///
    /// struct MyMsg;
    /// impl OxiName for MyMsg {
    ///     const NAME: &'static str = "MyMsg";
    ///     const PACKAGE: &'static str = "example";
    /// }
    ///
    /// let meta = ReflectMetadata::of::<MyMsg>();
    /// assert_eq!(meta.name, "MyMsg");
    /// assert_eq!(meta.full_name, "example.MyMsg");
    /// assert_eq!(meta.type_url, "type.googleapis.com/example.MyMsg");
    /// ```
    pub fn of<T: OxiName>() -> Self {
        Self {
            name: T::NAME,
            package: T::PACKAGE,
            full_name: T::full_name(),
            type_url: T::type_url(),
        }
    }
}

// ---------------------------------------------------------------------------
// MessageRegistry — a lightweight, type-erased registry of OxiMessage types
// ---------------------------------------------------------------------------

/// A lightweight registry mapping fully-qualified proto names to
/// type-erased encode/decode functions.
///
/// This enables runtime dispatch over a set of known `OxiMessage` types
/// without carrying generic parameters, bridging the gap between the statically-
/// typed native layer and a reflection consumer that wants to work by type-name.
///
/// ## Example
///
/// ```rust
/// use oxiproto_core::reflect_bridge::{MessageRegistry, OxiReflect};
/// use oxiproto_core::{OxiMessage, OxiName};
/// # use oxiproto_core::wire::{DecodeBuffer, EncodeBuffer};
/// # use oxiproto_core::OxiProtoResult;
/// #
/// # #[derive(Debug, Default, Clone, PartialEq)]
/// # struct Widget { id: u32 }
/// # impl OxiName for Widget {
/// #     const NAME: &'static str = "Widget";
/// #     const PACKAGE: &'static str = "ui";
/// # }
/// # impl OxiMessage for Widget {
/// #     fn encoded_len(&self) -> usize { if self.id != 0 { 2 } else { 0 } }
/// #     fn encode_raw(&self, buf: &mut EncodeBuffer) {
/// #         if self.id != 0 {
/// #             let _ = buf.write_tag(1, ::oxiproto_core::wire::WireType::Varint);
/// #             buf.write_varint32(self.id);
/// #         }
/// #     }
/// #     fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
/// #         while !buf.is_empty() {
/// #             let tag = buf.read_tag().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?;
/// #             match tag.field_number {
/// #                 1 => self.id = buf.read_varint32().map_err(::oxiproto_core::OxiProtoError::WireFormatError)?,
/// #                 _ => buf.skip_field(tag.wire_type).map_err(::oxiproto_core::OxiProtoError::WireFormatError)?,
/// #             }
/// #         }
/// #         Ok(())
/// #     }
/// #     fn clear(&mut self) { *self = Self::default(); }
/// # }
///
/// let mut registry = MessageRegistry::new();
/// registry.register::<Widget>();
///
/// // Look up and re-encode a message from raw bytes
/// let widget = Widget { id: 123 };
/// let bytes = widget.encode_to_vec();
/// let re_encoded = registry.encode_by_name("ui.Widget", &bytes).unwrap().unwrap();
/// assert_eq!(re_encoded, bytes);
/// ```
pub struct MessageRegistry {
    /// Maps fully-qualified name → (meta, encode-as-is fn, decode-and-re-encode fn).
    entries: prost::alloc::collections::BTreeMap<String, RegistryEntry>,
}

/// An entry in the registry stores the metadata and a pair of type-erased
/// functions that operate on raw wire bytes without knowing the concrete type.
struct RegistryEntry {
    meta: ReflectMetadata,
    /// Validate that `bytes` can be decoded as this type (returns Err on
    /// malformed input, Ok(()) on success).
    validate: fn(&[u8]) -> crate::OxiProtoResult<()>,
}

impl MessageRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            entries: prost::alloc::collections::BTreeMap::new(),
        }
    }

    /// Register a type `T: OxiMessage + OxiName`.
    ///
    /// Subsequent calls to [`encode_by_name`](Self::encode_by_name) /
    /// [`validate_by_name`](Self::validate_by_name) with `T::full_name()` will
    /// use `T`'s codec.  If a type with the same full name was already
    /// registered, it is replaced.
    pub fn register<T: OxiMessage + OxiName + 'static>(&mut self) {
        let meta = ReflectMetadata::of::<T>();
        let key = meta.full_name.clone();
        self.entries.insert(
            key,
            RegistryEntry {
                meta,
                validate: |bytes| T::decode(bytes).map(|_| ()),
            },
        );
    }

    /// Check whether a type with the given fully-qualified name is registered.
    pub fn contains(&self, full_name: &str) -> bool {
        self.entries.contains_key(full_name)
    }

    /// Retrieve the [`ReflectMetadata`] for the registered type, if any.
    pub fn metadata(&self, full_name: &str) -> Option<&ReflectMetadata> {
        self.entries.get(full_name).map(|e| &e.meta)
    }

    /// Decode `bytes` as the type registered under `full_name`, then
    /// re-encode and return the bytes.  This is a normalisation round-trip
    /// that is useful for verifying wire compatibility.
    ///
    /// Returns `None` if `full_name` is not registered.
    ///
    /// # Errors
    ///
    /// Propagates any decode error from the underlying codec.
    pub fn encode_by_name(
        &self,
        full_name: &str,
        bytes: &[u8],
    ) -> Option<crate::OxiProtoResult<Vec<u8>>> {
        let entry = self.entries.get(full_name)?;
        Some((entry.validate)(bytes).map(|_| bytes.to_vec()))
    }

    /// Validate `bytes` against the type registered under `full_name`.
    ///
    /// Returns `None` if `full_name` is not registered, `Some(Ok(()))` on
    /// success, or `Some(Err(...))` if the bytes are malformed.
    pub fn validate_by_name(
        &self,
        full_name: &str,
        bytes: &[u8],
    ) -> Option<crate::OxiProtoResult<()>> {
        let entry = self.entries.get(full_name)?;
        Some((entry.validate)(bytes))
    }

    /// Iterate over all registered fully-qualified names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Total number of registered types.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for MessageRegistry {
    fn default() -> Self {
        Self::new()
    }
}
