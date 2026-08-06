//! Descriptor-driven checks that generated code picks the *file's* default
//! repeated-scalar encoding.
//!
//! `protoc` leaves a proto2 repeated packable scalar **unpacked** unless the
//! schema writes `[packed = true]`; proto3 and Protobuf Editions pack it unless
//! the schema (or `features.repeated_field_encoding = EXPANDED`) says otherwise.
//! Generating the wrong default is a silent wire-compatibility divergence, so
//! it is asserted here at the emitted-source level for all three syntaxes, with
//! and without an explicit option.
//!
//! The descriptors are hand-built rather than parsed so the matrix is exercised
//! independently of the `.proto` front end.

use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{
    DescriptorProto, FieldDescriptorProto, FieldOptions, FileDescriptorProto, FileDescriptorSet,
};

/// `repeated int32 <name> = <number>;` with an optional explicit `packed`.
fn repeated_int32(name: &str, number: i32, packed: Option<bool>) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_owned()),
        number: Some(number),
        label: Some(Label::Repeated as i32),
        r#type: Some(Type::Int32 as i32),
        options: packed.map(|p| FieldOptions {
            packed: Some(p),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Generate the `OxiMessage` impl for a one-message file with the given syntax.
///
/// `syntax` is `None` for a file that carries no `syntax` statement at all —
/// which `protoc` records as proto2.
fn generate(syntax: Option<&str>, fields: Vec<FieldDescriptorProto>) -> String {
    let fds = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("packing.proto".to_owned()),
            package: Some("packing".to_owned()),
            syntax: syntax.map(str::to_owned),
            message_type: vec![DescriptorProto {
                name: Some("Packing".to_owned()),
                field: fields,
                ..Default::default()
            }],
            ..Default::default()
        }],
    };
    let mut options = oxiproto_codegen::CodegenOptions::new();
    options.emit_oxi_message_impl = true;
    oxiproto_codegen::generate_with_options(&fds, &options).expect("codegen must succeed")
}

/// The packed emission — in both `encoded_len` and `encode_raw` — sums the run
/// into a `_payload_len` local before writing a single length-delimited tag.
/// The expanded emission never computes one.
fn packs(code: &str) -> bool {
    code.contains("_payload_len")
}

/// The expanded emission writes one element tag per value and computes no
/// packed payload length. Exactly one of the two branches is emitted per field.
fn expands(code: &str) -> bool {
    !packs(code) && code.contains("for _v in &self.vals")
}

#[test]
fn proto2_defaults_to_expanded() {
    let code = generate(Some("proto2"), vec![repeated_int32("vals", 1, None)]);
    assert!(expands(&code), "proto2 default must be expanded:\n{code}");
    assert!(!packs(&code), "proto2 default must not pack:\n{code}");
}

#[test]
fn a_file_with_no_syntax_statement_is_proto2() {
    let code = generate(None, vec![repeated_int32("vals", 1, None)]);
    assert!(
        expands(&code),
        "an absent syntax statement means proto2:\n{code}"
    );
    assert!(!packs(&code));
}

#[test]
fn proto3_defaults_to_packed() {
    let code = generate(Some("proto3"), vec![repeated_int32("vals", 1, None)]);
    assert!(packs(&code), "proto3 default must pack:\n{code}");
}

#[test]
fn editions_defaults_to_packed() {
    // `oxiproto-build` records an `edition = "20XX";` file with the
    // `syntax = "editions"` sentinel, because `prost-types` still models the
    // pre-Editions `descriptor.proto`.
    let code = generate(Some("editions"), vec![repeated_int32("vals", 1, None)]);
    assert!(
        packs(&code),
        "Editions defaults features.repeated_field_encoding to PACKED:\n{code}"
    );
}

#[test]
fn an_explicit_packed_true_wins_in_proto2() {
    let code = generate(Some("proto2"), vec![repeated_int32("vals", 1, Some(true))]);
    assert!(packs(&code), "[packed = true] must win in proto2:\n{code}");
}

#[test]
fn an_explicit_packed_false_wins_in_proto3() {
    let code = generate(Some("proto3"), vec![repeated_int32("vals", 1, Some(false))]);
    assert!(
        expands(&code),
        "[packed = false] must win in proto3:\n{code}"
    );
    assert!(!packs(&code));
}

#[test]
fn an_explicit_packed_false_wins_in_editions() {
    // This is how `features.repeated_field_encoding = EXPANDED` reaches the
    // code generator: `oxiproto-build` materialises the resolved feature into
    // `FieldOptions.packed`.
    let code = generate(
        Some("editions"),
        vec![repeated_int32("vals", 1, Some(false))],
    );
    assert!(
        expands(&code),
        "a materialised EXPANDED feature must win:\n{code}"
    );
    assert!(!packs(&code));
}

/// `encoded_len` and `encode_raw` must agree on the encoding, otherwise a
/// nested message's length prefix is computed from bytes that are never
/// written. Both bodies are emitted from the same `FieldInfo`, so the check is
/// that the *expanded* size formula (per-element tag) appears rather than the
/// packed one (single length-delimited run).
#[test]
fn encoded_len_matches_encode_raw_for_an_expanded_field() {
    let code = generate(Some("proto2"), vec![repeated_int32("vals", 1, None)]);
    // Packed sizing sums a payload then adds a length varint; expanded sizing
    // adds a tag per element.
    assert!(
        !code.contains("_payload_len"),
        "expanded field must not be sized with the packed formula:\n{code}"
    );
}

/// Strings and bytes are never packable, in any syntax.
#[test]
fn non_packable_repeated_fields_are_unaffected() {
    for syntax in [Some("proto2"), Some("proto3"), Some("editions")] {
        let code = generate(
            syntax,
            vec![FieldDescriptorProto {
                name: Some("vals".to_owned()),
                number: Some(1),
                label: Some(Label::Repeated as i32),
                r#type: Some(Type::String as i32),
                ..Default::default()
            }],
        );
        assert!(
            !packs(&code),
            "repeated string is never packed ({syntax:?}):\n{code}"
        );
    }
}
