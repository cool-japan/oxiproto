// Feature flag contract tests.
//
// Verifies that:
//   1. Each opt-in feature exposes exactly the expected module at the facade level.
//   2. Core types are always available regardless of which features are enabled.
//   3. No feature bleeds into another's namespace.
//
// Each test is gated on the specific feature it exercises.  The tests that run
// with *no* features confirm the baseline guarantee.  The `--all-features` run
// exercises all feature-gated blocks simultaneously.
//
// Run per-feature:
//   cargo test -p oxiproto --no-default-features --features build
//   cargo test -p oxiproto --no-default-features --features reflect
//   cargo test -p oxiproto --no-default-features --features wkt
//   cargo test -p oxiproto --no-default-features --features codegen
//   cargo test -p oxiproto --no-default-features --features json

// ─── Baseline: core types always available ─────────────────────────────────────

/// Core types must be accessible with the empty feature set (default = []).
#[test]
fn default_features_core_types_accessible() {
    // These types are unconditionally re-exported from oxiproto-core.
    // If any of these fail to compile, the facade contract is broken.
    fn _check_oxi_message_trait_bound<T: oxiproto::OxiMessage>() {}
    fn _check_oxi_name_trait_bound<T: oxiproto::OxiName>() {}
    fn _check_oxi_oneof_trait_bound<T: oxiproto::OxiOneof>() {}
    // Extensions is a struct (not a trait); verify it's accessible by type reference
    fn _check_extensions_struct_accessible(_: oxiproto::Extensions) {}
    let _ = _check_extensions_struct_accessible;

    // version() is always available
    let v = oxiproto::version();
    assert!(!v.is_empty(), "version() must always be available");
}

/// The `wire` module is unconditionally re-exported.
#[test]
fn wire_module_always_available() {
    // Use a type from each major sub-module of wire to confirm they're accessible.
    let _: oxiproto::wire::WireType = oxiproto::wire::WireType::Varint;
    let buf = oxiproto::wire::EncodeBuffer::new();
    drop(buf);
    let _: oxiproto::wire::UnknownFields = Default::default();
}

/// The `prelude` module is unconditionally exported.
#[test]
fn prelude_always_available() {
    use oxiproto::prelude::*;
    // These must compile — if prelude is removed or gated this fails.
    fn _check_wire_type(_: WireType) {}
    fn _check_encode_buffer(_: EncodeBuffer) {}
    fn _check_oxi_error(_: OxiProtoError) {}
    fn _check_oxi_result(_: OxiProtoResult<()>) {}
    let _ = _check_wire_type;
    let _ = _check_encode_buffer;
    let _ = _check_oxi_error;
    let _ = _check_oxi_result;
}

/// `encode` and `decode` top-level helpers are unconditionally available.
#[test]
fn encode_decode_helpers_always_available() {
    use oxiproto::{OxiMessage, OxiName, OxiProtoError, OxiProtoResult};

    /// Minimal OxiMessage + OxiName implementation for this test.
    #[derive(Debug, Default, Clone, PartialEq)]
    struct Ping {
        value: i32,
    }
    impl OxiName for Ping {
        const NAME: &'static str = "Ping";
        const PACKAGE: &'static str = "test";
    }
    impl OxiMessage for Ping {
        fn encoded_len(&self) -> usize {
            if self.value != 0 {
                // tag (field 1, varint) = 1 byte; value encoded as varint
                1 + oxiproto::wire::varint::encoded_len_varint(self.value as u64)
            } else {
                0
            }
        }
        fn encode_raw(&self, buf: &mut oxiproto::wire::EncodeBuffer) {
            if self.value != 0 {
                buf.write_tag(1, oxiproto::wire::WireType::Varint)
                    .expect("write_tag");
                buf.write_varint(self.value as u64);
            }
        }
        fn merge(&mut self, buf: &mut oxiproto::wire::DecodeBuffer) -> OxiProtoResult<()> {
            while !buf.is_empty() {
                let tag = match buf.read_tag() {
                    Ok(t) => t,
                    Err(oxiproto::wire::WireError::UnexpectedEof) => break,
                    Err(e) => return Err(OxiProtoError::WireFormatError(e)),
                };
                match (tag.field_number, tag.wire_type) {
                    (1, oxiproto::wire::WireType::Varint) => {
                        self.value =
                            buf.read_varint().map_err(OxiProtoError::WireFormatError)? as i32;
                    }
                    (_, wt) => {
                        buf.skip_field(wt).map_err(OxiProtoError::WireFormatError)?;
                    }
                }
            }
            Ok(())
        }
        fn clear(&mut self) {
            self.value = 0;
        }
    }

    let msg = Ping { value: 7 };
    let bytes = oxiproto::encode(&msg);
    assert!(
        !bytes.is_empty(),
        "encode must produce non-empty bytes for non-default value"
    );
    let back: Ping = oxiproto::decode(&bytes).expect("decode must succeed");
    assert_eq!(back, msg, "encode/decode round-trip must preserve value");

    // Empty encode
    let empty = Ping::default();
    let empty_bytes = oxiproto::encode(&empty);
    assert!(
        empty_bytes.is_empty(),
        "default value must encode to zero bytes (proto3)"
    );
}

// ─── Feature: build ────────────────────────────────────────────────────────────

/// When the `build` feature is enabled, `oxiproto::build` module must be present
/// and `oxiproto::compile_protos` must be accessible as a top-level re-export.
#[cfg(feature = "build")]
#[test]
fn build_feature_exposes_build_module() {
    // Verify the module is accessible by referencing a known type from it.
    // Builder is a concrete struct, so we can confirm type-name equality.
    let type_name = std::any::type_name::<oxiproto::build::Builder>();
    assert!(
        type_name.contains("Builder"),
        "oxiproto::build::Builder must be accessible: {type_name}"
    );
}

/// `build::Builder` is accessible when the `build` feature is enabled.
#[cfg(feature = "build")]
#[test]
fn build_feature_exposes_builder_type() {
    // Constructing a Builder validates the type is properly re-exported.
    let builder = oxiproto::build::Builder::new();
    drop(builder);
}

/// `build::compile_to_fds` is accessible (needed for codegen integration).
#[cfg(feature = "build")]
#[test]
fn build_feature_exposes_compile_to_fds() {
    // Verify compile_to_fds is accessible by calling it with an empty proto list.
    // An empty list will fail (no files), but the error confirms the fn exists.
    let result =
        oxiproto::build::compile_to_fds(&[] as &[&std::path::Path], &[] as &[&std::path::Path]);
    // Empty input returns an error or empty FDS — either way the function is accessible.
    let _ = result;
}

// ─── Feature: reflect ──────────────────────────────────────────────────────────

/// When the `reflect` feature is enabled, `oxiproto::reflect` must be present.
#[cfg(feature = "reflect")]
#[test]
fn reflect_feature_exposes_reflect_module() {
    // DescriptorPool is the primary entry point in oxiproto-reflect.
    fn _accepts_pool(_: &oxiproto::reflect::DescriptorPool) {}
    let _ = _accepts_pool;
}

// ─── Feature: wkt ─────────────────────────────────────────────────────────────

/// When the `wkt` feature is enabled, `oxiproto::wkt` must expose extension traits
/// for Well-Known Types (Timestamp, Duration, etc.).
#[cfg(feature = "wkt")]
#[test]
fn wkt_feature_exposes_wkt_module() {
    // TimestampExt is the primary trait in oxiproto-wkt.
    fn _uses_timestamp_ext<T: oxiproto::wkt::TimestampExt>(_: &T) {}
    let _ = _uses_timestamp_ext::<oxiproto::prost_types::Timestamp>;
}

/// When `wkt` is enabled the well-known type extension traits are in scope.
#[cfg(feature = "wkt")]
#[test]
fn wkt_duration_ext_accessible() {
    fn _uses_duration_ext<T: oxiproto::wkt::DurationExt>(_: &T) {}
    let _ = _uses_duration_ext::<oxiproto::prost_types::Duration>;
}

// ─── Feature: codegen ─────────────────────────────────────────────────────────

/// When the `codegen` feature is enabled, `oxiproto::codegen` must be present
/// and expose `CodegenOptions` and `generate_with_options`.
#[cfg(feature = "codegen")]
#[test]
fn codegen_feature_exposes_codegen_module() {
    let opts = oxiproto::codegen::CodegenOptions::new();
    // emit_json and emit_oxi_message_impl must be settable
    assert!(!opts.emit_json, "emit_json default must be false");
    assert!(
        !opts.emit_oxi_message_impl,
        "emit_oxi_message_impl default must be false"
    );
}

/// `generate_with_options` is accessible via the codegen feature.
#[cfg(feature = "codegen")]
#[test]
fn codegen_generate_with_options_accessible() {
    use prost_types::{FileDescriptorProto, FileDescriptorSet};

    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("empty.proto".to_string()),
            ..Default::default()
        }],
    };
    let opts = oxiproto::codegen::CodegenOptions::new();
    let result = oxiproto::codegen::generate_with_options(&fds, &opts);
    assert!(
        result.is_ok(),
        "generate_with_options on empty FDS must not fail"
    );
}

// ─── Feature: json ─────────────────────────────────────────────────────────────

/// When the `json` feature is enabled, `oxiproto::json` module must be present
/// and expose `JsonCodec`, `to_json`, and `from_json`.
#[cfg(feature = "json")]
#[test]
fn json_feature_exposes_json_module() {
    // JsonCodec is the primary integration point in oxiproto-json.
    fn _accepts_codec(_: &oxiproto::json::JsonCodec) {}
    let _ = _accepts_codec;
}

// ─── Feature isolation: modules absent without their feature ───────────────────
//
// The cfg(not(feature = "...")) tests below confirm that modules are NOT
// accessible when the corresponding feature is disabled.  These tests always
// compile — they just check that we haven't accidentally un-gated a module.
// They are deliberately "documentation tests": the real enforcement is the
// compile error you would get if the module were unconditionally available.
//
// Since we cannot assert "this fails to compile" at runtime, we document the
// invariant here and rely on the compile-time gating in lib.rs being correct.

/// Documents that `build` module isolation is enforced at compile time.
///
/// If you see `oxiproto::build` accessible without the `build` feature, that is
/// a regression in lib.rs gating.
#[cfg(not(feature = "build"))]
#[test]
fn build_module_absent_without_feature_documented() {
    // This test passes trivially.  Its presence documents the invariant.
    // Actual enforcement: `#[cfg(feature = "build")] pub mod build { ... }` in lib.rs.
    let _ = true;
}

/// Documents that `reflect` module isolation is enforced at compile time.
#[cfg(not(feature = "reflect"))]
#[test]
fn reflect_module_absent_without_feature_documented() {
    let _ = true;
}

/// Documents that `wkt` module isolation is enforced at compile time.
#[cfg(not(feature = "wkt"))]
#[test]
fn wkt_module_absent_without_feature_documented() {
    let _ = true;
}

/// Documents that `codegen` module isolation is enforced at compile time.
#[cfg(not(feature = "codegen"))]
#[test]
fn codegen_module_absent_without_feature_documented() {
    let _ = true;
}

/// Documents that `json` module isolation is enforced at compile time.
#[cfg(not(feature = "json"))]
#[test]
fn json_module_absent_without_feature_documented() {
    let _ = true;
}
