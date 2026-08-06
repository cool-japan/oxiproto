//! Down-levelling a Protobuf Editions `FileDescriptorSet` for the
//! `prost-reflect` facade.
//!
//! `prost-reflect` 0.16 recognises exactly two values in
//! `FileDescriptorProto.syntax` — `"proto2"` and `"proto3"` — and rejects the
//! whole descriptor set with `unknown syntax 'editions'` otherwise. A
//! descriptor set produced from an `edition = "2023";` source therefore cannot
//! be loaded through [`crate::pool_from_fds`] as-is, which takes the entire
//! prost-reflect-backed surface (`DynamicMessage`, `oxiproto-json`, the CLI's
//! `convert` subcommand) offline for edition schemas.
//!
//! [`downlevel_editions`] rewrites such a file into the equivalent **proto2**
//! descriptor. proto2 is the right base:
//!
//! * Editions' default `features.field_presence` is `EXPLICIT`, which is
//!   precisely proto2 singular-field presence;
//! * `field_presence = LEGACY_REQUIRED` materialises as `LABEL_REQUIRED`, which
//!   only proto2 accepts;
//! * `message_encoding = DELIMITED` materialises as `TYPE_GROUP`, which only
//!   proto2 has;
//! * an edition enum may start at a non-zero value, which prost-reflect rejects
//!   for proto3 (`InvalidProto3EnumDefault`);
//! * prost-reflect skips its lowerCamelCase field-name check for proto2.
//!
//! The one thing proto2 gets *wrong* by default is packing: proto2 leaves a
//! repeated packable scalar expanded, whereas Editions defaults
//! `features.repeated_field_encoding` to `PACKED`. The transform therefore
//! writes an explicit `FieldOptions.packed` on every repeated packable field
//! that does not already carry one, taken from the resolved feature (defaulting
//! to `PACKED`). Without that step the facade would encode expanded where the
//! native path packs — a wire divergence between two code paths in the same
//! crate.
//!
//! # Known divergence
//!
//! `features.field_presence = IMPLICIT` has no proto2 descriptor expression, so
//! a field that opts into implicit presence is seen by the facade as having
//! presence. The single observable consequence is that an *explicitly encoded*
//! zero for such a field appears in the facade's JSON/text output, where
//! implicit-presence semantics would omit it. A conformant encoder never writes
//! that byte sequence in the first place, and the native path
//! ([`crate::native`]) models the feature exactly; nothing else in the pipeline
//! is affected.

use prost_types::{
    field_descriptor_proto::{Label, Type},
    DescriptorProto, FieldDescriptorProto, FieldOptions, FileDescriptorProto, FileDescriptorSet,
};

use crate::native::pool::{feature_value, EDITIONS_SYNTAX};

/// The `syntax` value an Editions file is rewritten to.
const PROTO2_SYNTAX: &str = "proto2";

/// The resolved-feature name carrying a repeated field's encoding.
const REPEATED_FIELD_ENCODING: &str = "repeated_field_encoding";

/// The enumerator meaning "pack this repeated field".
const PACKED_IDENT: &str = "PACKED";

/// Returns `true` if `file` was produced from an `edition = "20XX";` source.
///
/// Such a file carries the `"editions"` sentinel in its `syntax` field, because
/// `prost-types` 0.14 still models the pre-Editions `descriptor.proto` and has
/// no `edition` field of its own.
pub fn is_editions_file(file: &FileDescriptorProto) -> bool {
    file.syntax.as_deref() == Some(EDITIONS_SYNTAX)
}

/// Returns `true` if any file in `fds` uses Protobuf Editions.
pub fn has_editions_file(fds: &FileDescriptorSet) -> bool {
    fds.file.iter().any(is_editions_file)
}

/// Rewrite every `syntax = "editions"` file in `fds` into the equivalent proto2
/// descriptor, so that `prost-reflect` can load it.
///
/// Files that already declare `proto2` or `proto3` are returned untouched, so
/// this is safe to apply unconditionally. The resolved `features.*`
/// `uninterpreted_option` entries are deliberately left in place: prost-reflect
/// ignores them, and they remain available to any consumer that wants the
/// original feature set (including [`crate::native`], which reads them).
///
/// See the module documentation for the mapping rules and the one known
/// divergence (`features.field_presence = IMPLICIT`).
pub fn downlevel_editions(mut fds: FileDescriptorSet) -> FileDescriptorSet {
    for file in &mut fds.file {
        if !is_editions_file(file) {
            continue;
        }
        file.syntax = Some(PROTO2_SYNTAX.to_owned());
        for msg in &mut file.message_type {
            downlevel_message(msg);
        }
        for ext in &mut file.extension {
            materialise_packed(ext);
        }
    }
    fds
}

/// Apply the field rewrites to one message and everything nested inside it.
fn downlevel_message(msg: &mut DescriptorProto) {
    for field in &mut msg.field {
        materialise_packed(field);
    }
    for ext in &mut msg.extension {
        materialise_packed(ext);
    }
    for nested in &mut msg.nested_type {
        downlevel_message(nested);
    }
}

/// Write an explicit `packed` flag for a repeated packable field that has none.
///
/// An explicit flag already present always wins — that is how `oxiproto-build`
/// materialises `features.repeated_field_encoding` in the first place. When it
/// is absent (for instance in a descriptor set emitted by `protoc`, which
/// records the feature in `options.features` rather than `options.packed`), the
/// resolved feature is consulted, and Editions' default of `PACKED` applies if
/// even that is missing.
fn materialise_packed(field: &mut FieldDescriptorProto) {
    if field.label != Some(Label::Repeated as i32) || !is_packable(field.r#type) {
        return;
    }
    if field.options.as_ref().and_then(|o| o.packed).is_some() {
        return;
    }
    let packed = field
        .options
        .as_ref()
        .and_then(|o| feature_value(&o.uninterpreted_option, REPEATED_FIELD_ENCODING))
        .is_none_or(|encoding| encoding == PACKED_IDENT);
    field
        .options
        .get_or_insert_with(FieldOptions::default)
        .packed = Some(packed);
}

/// Whether a descriptor field type may be packed when repeated.
fn is_packable(proto_type: Option<i32>) -> bool {
    match proto_type.and_then(|t| Type::try_from(t).ok()) {
        Some(Type::String | Type::Bytes | Type::Message | Type::Group) | None => false,
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repeated_int32(packed: Option<bool>) -> FieldDescriptorProto {
        FieldDescriptorProto {
            name: Some("vals".to_owned()),
            number: Some(1),
            label: Some(Label::Repeated as i32),
            r#type: Some(Type::Int32 as i32),
            options: packed.map(|p| FieldOptions {
                packed: Some(p),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn editions_file(field: FieldDescriptorProto) -> FileDescriptorSet {
        FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("e.proto".to_owned()),
                syntax: Some(EDITIONS_SYNTAX.to_owned()),
                message_type: vec![DescriptorProto {
                    name: Some("M".to_owned()),
                    field: vec![field],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        }
    }

    #[test]
    fn editions_syntax_becomes_proto2() {
        let out = downlevel_editions(editions_file(repeated_int32(None)));
        assert_eq!(out.file[0].syntax.as_deref(), Some("proto2"));
    }

    #[test]
    fn an_unflagged_repeated_scalar_gains_an_explicit_packed_true() {
        let out = downlevel_editions(editions_file(repeated_int32(None)));
        let field = &out.file[0].message_type[0].field[0];
        assert_eq!(field.options.as_ref().and_then(|o| o.packed), Some(true));
    }

    #[test]
    fn an_explicit_packed_false_is_preserved() {
        let out = downlevel_editions(editions_file(repeated_int32(Some(false))));
        let field = &out.file[0].message_type[0].field[0];
        assert_eq!(field.options.as_ref().and_then(|o| o.packed), Some(false));
    }

    #[test]
    fn proto3_files_are_left_alone() {
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("p.proto".to_owned()),
                syntax: Some("proto3".to_owned()),
                message_type: vec![DescriptorProto {
                    name: Some("M".to_owned()),
                    field: vec![repeated_int32(None)],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };
        let out = downlevel_editions(fds);
        assert_eq!(out.file[0].syntax.as_deref(), Some("proto3"));
        assert!(out.file[0].message_type[0].field[0].options.is_none());
    }

    #[test]
    fn repeated_strings_are_never_flagged() {
        let field = FieldDescriptorProto {
            name: Some("vals".to_owned()),
            number: Some(1),
            label: Some(Label::Repeated as i32),
            r#type: Some(Type::String as i32),
            ..Default::default()
        };
        let out = downlevel_editions(editions_file(field));
        assert!(out.file[0].message_type[0].field[0].options.is_none());
    }
}
