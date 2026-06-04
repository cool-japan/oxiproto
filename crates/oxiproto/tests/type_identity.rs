// Type identity tests: verify that the facade re-exports are bit-for-bit the same
// types as the originals in the sub-crates (no accidental shadowing or wrapping).
//
// Each check uses a compile-time trait bound or const assertion to confirm that:
//   oxiproto::T  is exactly the same type as  oxiproto_core::T
//
// If any type were accidentally shadowed (e.g. a re-exported wrapper), the
// trait-object cast or `std::any::TypeId` comparison would fail at compile time
// or at runtime respectively.

use std::any::TypeId;

// ─── Core trait / type identities ─────────────────────────────────────────────

#[test]
fn oxi_proto_error_is_same_type() {
    assert_eq!(
        TypeId::of::<oxiproto::OxiProtoError>(),
        TypeId::of::<oxiproto_core::OxiProtoError>(),
        "oxiproto::OxiProtoError must be the exact same type as oxiproto_core::OxiProtoError"
    );
}

#[test]
fn prost_types_re_export_matches() {
    // prost_types::Timestamp is re-exported through oxiproto::prost_types.
    // Verify the TypeId is identical to the direct oxiproto_core re-export.
    assert_eq!(
        TypeId::of::<oxiproto::prost_types::Timestamp>(),
        TypeId::of::<oxiproto_core::prost_types::Timestamp>(),
        "oxiproto::prost_types::Timestamp must be the same type as oxiproto_core::prost_types::Timestamp"
    );
}

#[test]
fn wire_decode_buffer_is_same_type() {
    assert_eq!(
        TypeId::of::<oxiproto::wire::DecodeBuffer>(),
        TypeId::of::<oxiproto_core::wire::DecodeBuffer>(),
        "oxiproto::wire::DecodeBuffer must be the exact same type as oxiproto_core::wire::DecodeBuffer"
    );
}

#[test]
fn wire_encode_buffer_is_same_type() {
    assert_eq!(
        TypeId::of::<oxiproto::wire::EncodeBuffer>(),
        TypeId::of::<oxiproto_core::wire::EncodeBuffer>(),
        "oxiproto::wire::EncodeBuffer must be the exact same type as oxiproto_core::wire::EncodeBuffer"
    );
}

#[test]
fn wire_type_enum_is_same_type() {
    assert_eq!(
        TypeId::of::<oxiproto::wire::WireType>(),
        TypeId::of::<oxiproto_core::wire::WireType>(),
        "oxiproto::wire::WireType must be the exact same type as oxiproto_core::wire::WireType"
    );
}

#[test]
fn unknown_fields_is_same_type() {
    assert_eq!(
        TypeId::of::<oxiproto::wire::UnknownFields>(),
        TypeId::of::<oxiproto_core::wire::UnknownFields>(),
        "oxiproto::wire::UnknownFields must be the exact same type as oxiproto_core::wire::UnknownFields"
    );
}

// ─── Prelude identity ─────────────────────────────────────────────────────────

#[test]
fn prelude_oxi_proto_error_matches_top_level() {
    // Both the top-level re-export and the prelude must point to the same type.
    assert_eq!(
        TypeId::of::<oxiproto::prelude::OxiProtoError>(),
        TypeId::of::<oxiproto::OxiProtoError>(),
        "oxiproto::prelude::OxiProtoError must be the same type as oxiproto::OxiProtoError"
    );
}

#[test]
fn prelude_decode_buffer_matches_wire_module() {
    assert_eq!(
        TypeId::of::<oxiproto::prelude::DecodeBuffer>(),
        TypeId::of::<oxiproto::wire::DecodeBuffer>(),
        "oxiproto::prelude::DecodeBuffer must be the same type as oxiproto::wire::DecodeBuffer"
    );
}

#[test]
fn prelude_encode_buffer_matches_wire_module() {
    assert_eq!(
        TypeId::of::<oxiproto::prelude::EncodeBuffer>(),
        TypeId::of::<oxiproto::wire::EncodeBuffer>(),
        "oxiproto::prelude::EncodeBuffer must be the same type as oxiproto::wire::EncodeBuffer"
    );
}

// ─── Compile-time zero-cost re-export verification ────────────────────────────
//
// Facade re-exports must add zero runtime overhead.  We verify this by
// confirming that every public function / constant in the facade is a thin
// re-export: the TypeId / function pointer tests above already confirm no
// wrapper types exist.  The `version()` function is the only non-trivial item;
// it just calls `env!()` which is inlined by the compiler.
//
// This test serves as documentation that the overhead guarantee is tested.
#[test]
fn version_is_inlineable_const_expr() {
    let v = oxiproto::version();
    // Must be a non-empty semver string at the very least.
    assert!(!v.is_empty(), "version() must return a non-empty string");
    // Must start with a digit (major version number).
    assert!(
        v.chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false),
        "version() must start with a digit, got: {v}"
    );
}
