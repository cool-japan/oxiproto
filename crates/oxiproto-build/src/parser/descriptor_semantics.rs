#![forbid(unsafe_code)]

//! Edition-aware semantics for the descriptor builder.
//!
//! A `syntax = "proto2"` / `"proto3"` file gets its behaviour from a single
//! boolean; an `edition = "2023"` file gets it from a resolved
//! [`FeatureSet`](crate::parser::features::FeatureSet) that is re-resolved at
//! every nesting level. [`Semantics`] carries both so the builder can stay one
//! code path, and the helpers here translate a resolved feature set into the
//! legacy descriptor fields (`label`, `packed`, `TYPE_GROUP`) plus
//! `uninterpreted_option` entries that preserve the full resolution result.

use prost_types::{uninterpreted_option::NamePart, UninterpretedOption};

use prost_types::field_descriptor_proto::{Label, Type};

use crate::parser::ast::{Field, FieldLabel, ProtoFile, ProtoOption};
use crate::parser::descriptor::{
    build_enum_options, build_field_options, build_file_options, build_message_options,
    file_is_proto2,
};
use crate::parser::features::{
    FeatureSet, FieldPresence, MessageEncoding, RepeatedFieldEncoding, FEATURE_PREFIX,
};

/// The resolved semantics in force at one scope of the descriptor build.
///
/// A `syntax`-based file keeps its historical code path (`is_proto2` decides
/// whether `optional` needs a synthetic oneof).  An `edition`-based file has no
/// label keywords at all, so every decision — presence, packing, message
/// framing — is read out of [`Semantics::features`], which is re-resolved at
/// each nesting level from that level's `option features.*` statements.
#[derive(Debug, Clone, Copy)]
pub(super) struct Semantics {
    /// The file declared `syntax = "proto2"`.
    pub(super) is_proto2: bool,
    /// The file declared an `edition` instead of a `syntax`.
    pub(super) is_edition: bool,
    /// Features resolved down to and including the current scope.
    pub(super) features: FeatureSet,
}

impl Semantics {
    /// Seed the chain from a file's edition/syntax baseline plus its
    /// file-level `option features.*` statements.
    pub(super) fn for_file(proto_file: &ProtoFile) -> Self {
        Semantics {
            is_proto2: file_is_proto2(proto_file),
            is_edition: proto_file.edition.is_some(),
            features: FeatureSet::defaults_for_file(proto_file).resolve_lossy(&proto_file.options),
        }
    }

    /// Descend into a nested scope carrying its own `option features.*` list.
    pub(super) fn child(self, options: &[ProtoOption]) -> Self {
        Semantics {
            features: self.features.resolve_lossy(options),
            ..self
        }
    }
}

/// Render a resolved [`FeatureSet`] as `uninterpreted_option` entries named
/// `features.<name>`.
///
/// `prost-types` 0.14 still models the pre-Editions `descriptor.proto`, so it
/// has neither `FileDescriptorProto.edition` nor an `options.features` field to
/// hold a `FeatureSet` message.  Rather than drop the resolution result on the
/// floor, edition files carry the *fully resolved* value of every feature in the
/// standard `uninterpreted_option` slot — the same place a compiler front-end
/// puts options it has not interpreted.  Consumers that understand Editions can
/// read the effective semantics back out of the descriptor set; consumers that
/// do not simply ignore an option list they were always allowed to ignore.
pub(super) fn resolved_feature_options(features: &FeatureSet) -> Vec<UninterpretedOption> {
    features
        .as_name_value_pairs()
        .iter()
        .map(|(name, value)| UninterpretedOption {
            name: vec![
                NamePart {
                    name_part: "features".to_owned(),
                    is_extension: false,
                },
                NamePart {
                    name_part: (*name).to_owned(),
                    is_extension: false,
                },
            ],
            identifier_value: Some((*value).to_owned()),
            ..Default::default()
        })
        .collect()
}

/// `true` when this `UninterpretedOption` is one of the raw `features.<x>`
/// entries produced by the generic option path.
pub(super) fn is_raw_feature_option(u: &UninterpretedOption) -> bool {
    matches!(u.name.first(), Some(first)
        if !first.is_extension && first.name_part == "features")
}

/// Replace the raw `features.<x>` entries in `uninterpreted` with the resolved
/// feature set for this scope.
pub(super) fn splice_resolved_features(
    uninterpreted: &mut Vec<UninterpretedOption>,
    features: &FeatureSet,
) {
    uninterpreted.retain(|u| !is_raw_feature_option(u));
    uninterpreted.extend(resolved_feature_options(features));
}

/// Strip the raw `features.*` options out of a scope's option list.
///
/// Used where the option list is re-scanned by a generic builder that would
/// otherwise emit both the raw and the resolved entry.
pub(super) fn without_feature_options(options: &[ProtoOption]) -> Vec<ProtoOption> {
    options
        .iter()
        .filter(|o| !o.name.starts_with(FEATURE_PREFIX))
        .cloned()
        .collect()
}

/// The `label` an edition file's field carries in the legacy descriptor.
///
/// Editions removed the `optional` and `required` keywords; `repeated` is the
/// only surviving modifier.  Presence is decided by `features.field_presence`,
/// and `LEGACY_REQUIRED` means precisely what `LABEL_REQUIRED` always meant.
pub(super) fn edition_label(field: &Field, features: FeatureSet) -> i32 {
    if matches!(field.label, FieldLabel::Repeated) {
        return Label::Repeated as i32;
    }
    match features.field_presence {
        FieldPresence::LegacyRequired => Label::Required as i32,
        FieldPresence::Explicit | FieldPresence::Implicit => Label::Optional as i32,
    }
}

/// Apply `features.message_encoding` to a field's descriptor type.
///
/// `DELIMITED` turns a message field into the start-group/end-group framing,
/// which the legacy descriptor spells `TYPE_GROUP`.  Non-message fields and
/// non-edition files are returned unchanged — a field that named an *enum* is
/// therefore left as `TYPE_ENUM` here and rejected afterwards by
/// [`validate_descriptor_features`], which runs once the resolved type is known
/// and can tell an enum reference from a message reference.
pub(super) fn edition_message_encoding_type(proto_type: Type, field_sem: Semantics) -> Type {
    if field_sem.is_edition
        && proto_type == Type::Message
        && field_sem.features.message_encoding == MessageEncoding::Delimited
    {
        Type::Group
    } else {
        proto_type
    }
}

/// Whether a descriptor type is eligible for packed repeated encoding.
pub(super) fn is_packable_proto_type(ty: Type) -> bool {
    !matches!(ty, Type::String | Type::Bytes | Type::Message | Type::Group)
}

/// Build `FieldOptions`, materialising the Editions-derived bits.
///
/// For an edition file two things are written into the legacy descriptor:
///
/// * `features.repeated_field_encoding` becomes an explicit `packed` flag, so a
///   consumer that only knows the proto2/proto3 packing defaults still encodes
///   the field the way the edition asked for; and
/// * the fully resolved feature set replaces the raw `features.*` entries in
///   `uninterpreted_option`.
pub(super) fn build_field_options_for(
    field: &Field,
    field_sem: Semantics,
    proto_type: Type,
) -> Option<prost_types::FieldOptions> {
    if !field_sem.is_edition {
        return build_field_options(&field.options);
    }
    let mut opts =
        build_field_options(&without_feature_options(&field.options)).unwrap_or_default();
    if matches!(field.label, FieldLabel::Repeated)
        && is_packable_proto_type(proto_type)
        && opts.packed.is_none()
    {
        opts.packed = Some(matches!(
            field_sem.features.repeated_field_encoding,
            RepeatedFieldEncoding::Packed
        ));
    }
    splice_resolved_features(&mut opts.uninterpreted_option, &field_sem.features);
    Some(opts)
}

/// [`build_message_options`] plus the resolved feature set on edition files.
pub(super) fn build_message_options_with_features(
    options: &[ProtoOption],
    sem: Semantics,
) -> Option<prost_types::MessageOptions> {
    if !sem.is_edition {
        return build_message_options(options);
    }
    let mut opts = build_message_options(&without_feature_options(options)).unwrap_or_default();
    splice_resolved_features(&mut opts.uninterpreted_option, &sem.features);
    Some(opts)
}

/// [`build_enum_options`] plus the resolved feature set on edition files.
pub(super) fn build_enum_options_with_features(
    options: &[ProtoOption],
    sem: Semantics,
) -> Option<prost_types::EnumOptions> {
    if !sem.is_edition {
        return build_enum_options(options);
    }
    let mut opts = build_enum_options(&without_feature_options(options)).unwrap_or_default();
    splice_resolved_features(&mut opts.uninterpreted_option, &sem.features);
    Some(opts)
}

/// The resolved feature set on an edition file's enum value.
///
/// A `syntax` file's enum values carry no options here (the parser has no
/// interpreted enum-value option), so `None` is returned unchanged.
pub(super) fn build_enum_value_options_with_features(
    options: &[ProtoOption],
    sem: Semantics,
) -> Option<prost_types::EnumValueOptions> {
    if !sem.is_edition {
        return None;
    }
    let mut opts = prost_types::EnumValueOptions {
        deprecated: options
            .iter()
            .find(|o| o.name == "deprecated")
            .and_then(|o| match o.value {
                crate::parser::ast::OptionValue::Bool(b) => Some(b),
                _ => None,
            }),
        ..Default::default()
    };
    splice_resolved_features(&mut opts.uninterpreted_option, &sem.features);
    Some(opts)
}

/// [`build_file_options`] plus the resolved feature set on edition files.
pub(super) fn build_file_options_with_features(
    options: &[ProtoOption],
    sem: Semantics,
) -> Option<prost_types::FileOptions> {
    if !sem.is_edition {
        return build_file_options(options);
    }
    let mut opts = build_file_options(&without_feature_options(options)).unwrap_or_default();
    splice_resolved_features(&mut opts.uninterpreted_option, &sem.features);
    Some(opts)
}

// ---------------------------------------------------------------------------
// Post-build validation
// ---------------------------------------------------------------------------

/// Check that every resolved feature actually took effect in the built
/// descriptor set.
///
/// Only `message_encoding` can silently fail to apply: `DELIMITED` is legal on
/// a message-typed field but meaningless on an enum-typed one, and the parser
/// cannot tell the two apart (both are `FieldType::Named` until name
/// resolution). Rather than leave a descriptor whose materialised feature
/// contradicts its own `type`, this rejects the file.
///
/// # Errors
///
/// Returns a human-readable description of the first contradiction found.
pub(crate) fn validate_descriptor_features(
    fds: &prost_types::FileDescriptorSet,
) -> Result<(), String> {
    for file in &fds.file {
        for msg in &file.message_type {
            validate_message_features(msg)?;
        }
        for field in &file.extension {
            validate_field_features(field)?;
        }
    }
    Ok(())
}

fn validate_message_features(msg: &prost_types::DescriptorProto) -> Result<(), String> {
    for field in &msg.field {
        validate_field_features(field)?;
    }
    for nested in &msg.nested_type {
        validate_message_features(nested)?;
    }
    Ok(())
}

fn validate_field_features(field: &prost_types::FieldDescriptorProto) -> Result<(), String> {
    let Some(opts) = field.options.as_ref() else {
        return Ok(());
    };
    let delimited = opts.uninterpreted_option.iter().any(|u| {
        let mut parts = u.name.iter();
        matches!((parts.next(), parts.next(), parts.next()), (Some(a), Some(b), None)
            if !a.is_extension
                && !b.is_extension
                && a.name_part == "features"
                && b.name_part == MessageEncoding::FEATURE_NAME)
            && u.identifier_value.as_deref() == Some(MessageEncoding::Delimited.as_ident())
    });
    if delimited && field.r#type != Some(Type::Group as i32) {
        return Err(format!(
            "field '{}' sets features.message_encoding = DELIMITED but is not a message field; \
             DELIMITED only applies to a message-typed field",
            field.name.as_deref().unwrap_or("<unnamed>")
        ));
    }
    Ok(())
}
