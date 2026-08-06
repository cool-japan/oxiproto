#![forbid(unsafe_code)]

//! Protobuf *Editions* feature resolution.
//!
//! Starting with `edition = "2023";` a `.proto` file no longer selects its
//! semantics through the `syntax` statement.  Instead every behaviour that used
//! to differ between proto2 and proto3 is a named **feature** that can be set at
//! file, message, field, oneof, enum, enum-value, service or method scope and is
//! *inherited* by everything nested inside that scope:
//!
//! ```proto
//! edition = "2023";
//!
//! option features.field_presence = IMPLICIT;   // whole file
//!
//! message M {
//!   option features.field_presence = EXPLICIT; // this message only
//!
//!   int32 a = 1;                                       // EXPLICIT (message)
//!   int32 b = 2 [features.field_presence = IMPLICIT];  // IMPLICIT (field)
//!   repeated int32 c = 3 [features.repeated_field_encoding = EXPANDED];
//! }
//! ```
//!
//! This module implements the resolution algorithm:
//!
//! * [`FeatureSet`] — a *complete* set of resolved feature values.
//! * [`FeatureOverrides`] — a *partial* set parsed out of `option` statements.
//! * [`FeatureSet::with_overrides`] — the inheritance step (child overrides
//!   parent, unset values inherit).
//! * [`FeatureSet::defaults_for_edition`] / [`FeatureSet::defaults_for_syntax`]
//!   — the edition/syntax baselines that seed the chain.
//!
//! The proto2 and proto3 baselines are included because Editions define them as
//! the "legacy editions" `EDITION_PROTO2` / `EDITION_PROTO3`; expressing the old
//! syntaxes in feature terms is what lets the rest of the compiler share one
//! code path instead of branching on `syntax` everywhere.

use crate::parser::ast::{
    Edition, Enum, Field, FieldLabel, FieldType, Message, OptionValue, ProtoFile, ProtoOption,
    Service,
};
use crate::parser::error::ParseError;

/// The `features.` prefix that marks an option as an Editions feature.
pub const FEATURE_PREFIX: &str = "features.";

// ---------------------------------------------------------------------------
// Individual feature enums
// ---------------------------------------------------------------------------

/// `features.field_presence` — how a singular field tracks whether it is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldPresence {
    /// The field tracks presence explicitly (proto2 `optional` semantics).
    Explicit,
    /// The field has no presence; the zero value is indistinguishable from
    /// unset and is never serialized (proto3 singular semantics).
    Implicit,
    /// The field is required to be present (proto2 `required` semantics).
    LegacyRequired,
}

/// `features.enum_type` — whether an enum accepts values outside its
/// declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumType {
    /// Unknown values are preserved in the field (proto3 semantics).
    Open,
    /// Unknown values are placed in the unknown-field set (proto2 semantics).
    Closed,
}

/// `features.repeated_field_encoding` — packed vs. one-tag-per-element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatedFieldEncoding {
    /// A single length-delimited run of elements (proto3 default).
    Packed,
    /// One tag per element (proto2 default).
    Expanded,
}

/// `features.utf8_validation` — whether `string` payloads are validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Utf8Validation {
    /// Reject non-UTF-8 `string` payloads (proto3 default).
    Verify,
    /// Accept arbitrary bytes in `string` fields (proto2 default).
    None,
}

/// `features.message_encoding` — length-prefixed vs. delimited (group) framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageEncoding {
    /// Wire type 2: a varint length followed by the payload.
    LengthPrefixed,
    /// Wire types 3/4: a start-group tag, the fields inline, an end-group tag.
    /// This is the encoding proto2 `group` used.
    Delimited,
}

/// `features.json_format` — how strictly the JSON mapping is enforced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonFormat {
    /// The full canonical JSON mapping is required to work.
    Allow,
    /// JSON support is best-effort (proto2 default).
    LegacyBestEffort,
}

macro_rules! feature_enum_impl {
    ($ty:ident, $feature:literal, { $($variant:ident => $text:literal),+ $(,)? }) => {
        impl $ty {
            /// The feature name this enum belongs to, without the `features.`
            /// prefix.
            pub const FEATURE_NAME: &'static str = $feature;

            /// Parse the identifier that appeared on the right-hand side of the
            /// option.
            pub fn from_ident(ident: &str) -> Option<Self> {
                match ident {
                    $($text => Some($ty::$variant),)+
                    _ => None,
                }
            }

            /// The canonical identifier for this value, as it appears in
            /// `.proto` source.
            pub fn as_ident(self) -> &'static str {
                match self {
                    $($ty::$variant => $text,)+
                }
            }
        }
    };
}

feature_enum_impl!(FieldPresence, "field_presence", {
    Explicit => "EXPLICIT",
    Implicit => "IMPLICIT",
    LegacyRequired => "LEGACY_REQUIRED",
});
feature_enum_impl!(EnumType, "enum_type", {
    Open => "OPEN",
    Closed => "CLOSED",
});
feature_enum_impl!(RepeatedFieldEncoding, "repeated_field_encoding", {
    Packed => "PACKED",
    Expanded => "EXPANDED",
});
feature_enum_impl!(Utf8Validation, "utf8_validation", {
    Verify => "VERIFY",
    None => "NONE",
});
feature_enum_impl!(MessageEncoding, "message_encoding", {
    LengthPrefixed => "LENGTH_PREFIXED",
    Delimited => "DELIMITED",
});
feature_enum_impl!(JsonFormat, "json_format", {
    Allow => "ALLOW",
    LegacyBestEffort => "LEGACY_BEST_EFFORT",
});

/// Every feature name this implementation understands, in declaration order.
pub const KNOWN_FEATURES: [&str; 6] = [
    FieldPresence::FEATURE_NAME,
    EnumType::FEATURE_NAME,
    RepeatedFieldEncoding::FEATURE_NAME,
    Utf8Validation::FEATURE_NAME,
    MessageEncoding::FEATURE_NAME,
    JsonFormat::FEATURE_NAME,
];

// ---------------------------------------------------------------------------
// FeatureSet
// ---------------------------------------------------------------------------

/// A fully resolved set of Editions features.
///
/// Every field is populated: resolution always starts from an edition (or
/// legacy syntax) baseline, so there is no "unset" state once resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureSet {
    /// Resolved `features.field_presence`.
    pub field_presence: FieldPresence,
    /// Resolved `features.enum_type`.
    pub enum_type: EnumType,
    /// Resolved `features.repeated_field_encoding`.
    pub repeated_field_encoding: RepeatedFieldEncoding,
    /// Resolved `features.utf8_validation`.
    pub utf8_validation: Utf8Validation,
    /// Resolved `features.message_encoding`.
    pub message_encoding: MessageEncoding,
    /// Resolved `features.json_format`.
    pub json_format: JsonFormat,
}

impl FeatureSet {
    /// The baseline for `edition = "2023"`.
    ///
    /// Edition 2023 keeps proto2's explicit presence but adopts proto3's open
    /// enums, packed repeated encoding, UTF-8 validation and JSON support.
    pub fn edition_2023() -> Self {
        FeatureSet {
            field_presence: FieldPresence::Explicit,
            enum_type: EnumType::Open,
            repeated_field_encoding: RepeatedFieldEncoding::Packed,
            utf8_validation: Utf8Validation::Verify,
            message_encoding: MessageEncoding::LengthPrefixed,
            json_format: JsonFormat::Allow,
        }
    }

    /// The baseline for the legacy `EDITION_PROTO2`.
    pub fn proto2() -> Self {
        FeatureSet {
            field_presence: FieldPresence::Explicit,
            enum_type: EnumType::Closed,
            repeated_field_encoding: RepeatedFieldEncoding::Expanded,
            utf8_validation: Utf8Validation::None,
            message_encoding: MessageEncoding::LengthPrefixed,
            json_format: JsonFormat::LegacyBestEffort,
        }
    }

    /// The baseline for the legacy `EDITION_PROTO3`.
    pub fn proto3() -> Self {
        FeatureSet {
            field_presence: FieldPresence::Implicit,
            enum_type: EnumType::Open,
            repeated_field_encoding: RepeatedFieldEncoding::Packed,
            utf8_validation: Utf8Validation::Verify,
            message_encoding: MessageEncoding::LengthPrefixed,
            json_format: JsonFormat::Allow,
        }
    }

    /// The baseline for an [`Edition`].
    ///
    /// Only [`Edition::Edition2023`] has a defined baseline; an
    /// [`Edition::Unknown`] value can never reach here because
    /// `parse_edition_statement` rejects it, so it falls back to the 2023
    /// table rather than inventing semantics.
    pub fn defaults_for_edition(edition: &Edition) -> Self {
        match edition {
            Edition::Edition2023 | Edition::Unknown(_) => FeatureSet::edition_2023(),
        }
    }

    /// The baseline implied by a `syntax` statement (`None` means the implicit
    /// proto2 default that protoc assumes for a file with no `syntax`).
    pub fn defaults_for_syntax(syntax: Option<&str>) -> Self {
        match syntax {
            Some("proto3") => FeatureSet::proto3(),
            _ => FeatureSet::proto2(),
        }
    }

    /// The baseline for a whole [`ProtoFile`], before its own file-level
    /// `option features.*` statements are applied.
    pub fn defaults_for_file(file: &ProtoFile) -> Self {
        match file.edition {
            Some(ref ed) => FeatureSet::defaults_for_edition(ed),
            None => FeatureSet::defaults_for_syntax(file.syntax.as_deref()),
        }
    }

    /// Apply a partial override set, returning the resolved child scope.
    #[must_use]
    pub fn with_overrides(self, ov: &FeatureOverrides) -> Self {
        FeatureSet {
            field_presence: ov.field_presence.unwrap_or(self.field_presence),
            enum_type: ov.enum_type.unwrap_or(self.enum_type),
            repeated_field_encoding: ov
                .repeated_field_encoding
                .unwrap_or(self.repeated_field_encoding),
            utf8_validation: ov.utf8_validation.unwrap_or(self.utf8_validation),
            message_encoding: ov.message_encoding.unwrap_or(self.message_encoding),
            json_format: ov.json_format.unwrap_or(self.json_format),
        }
    }

    /// Resolve this scope against a list of `option` statements, ignoring
    /// entries that are not features and tolerating malformed ones.
    ///
    /// Malformed features are impossible after [`validate_file`] has run, which
    /// is why this variant is infallible: it exists for the descriptor builder,
    /// which runs strictly after validation.
    #[must_use]
    pub fn resolve_lossy(self, options: &[ProtoOption]) -> Self {
        self.with_overrides(&FeatureOverrides::from_options_lossy(options))
    }

    /// The resolved values as `(feature_name, value_ident)` pairs, in
    /// [`KNOWN_FEATURES`] order.
    pub fn as_name_value_pairs(&self) -> [(&'static str, &'static str); 6] {
        [
            (FieldPresence::FEATURE_NAME, self.field_presence.as_ident()),
            (EnumType::FEATURE_NAME, self.enum_type.as_ident()),
            (
                RepeatedFieldEncoding::FEATURE_NAME,
                self.repeated_field_encoding.as_ident(),
            ),
            (
                Utf8Validation::FEATURE_NAME,
                self.utf8_validation.as_ident(),
            ),
            (
                MessageEncoding::FEATURE_NAME,
                self.message_encoding.as_ident(),
            ),
            (JsonFormat::FEATURE_NAME, self.json_format.as_ident()),
        ]
    }
}

// ---------------------------------------------------------------------------
// FeatureOverrides
// ---------------------------------------------------------------------------

/// A *partial* feature set: only the features explicitly written at one scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeatureOverrides {
    /// `features.field_presence`, if written at this scope.
    pub field_presence: Option<FieldPresence>,
    /// `features.enum_type`, if written at this scope.
    pub enum_type: Option<EnumType>,
    /// `features.repeated_field_encoding`, if written at this scope.
    pub repeated_field_encoding: Option<RepeatedFieldEncoding>,
    /// `features.utf8_validation`, if written at this scope.
    pub utf8_validation: Option<Utf8Validation>,
    /// `features.message_encoding`, if written at this scope.
    pub message_encoding: Option<MessageEncoding>,
    /// `features.json_format`, if written at this scope.
    pub json_format: Option<JsonFormat>,
}

impl FeatureOverrides {
    /// `true` when no feature was written at this scope.
    pub fn is_empty(&self) -> bool {
        *self == FeatureOverrides::default()
    }

    /// Parse every `features.*` option out of `options`, rejecting unknown
    /// feature names and unknown feature values.
    ///
    /// # Errors
    ///
    /// * [`ParseError::UnknownFeature`] — the option is `features.<x>` but `<x>`
    ///   is not a feature this implementation knows.
    /// * [`ParseError::InvalidFeatureValue`] — the value is not one of the
    ///   feature's enumerators.
    pub fn from_options(options: &[ProtoOption]) -> Result<Self, ParseError> {
        let mut out = FeatureOverrides::default();
        for opt in options {
            let Some(name) = feature_name(&opt.name) else {
                continue;
            };
            out.set_from_option(name, opt)?;
        }
        Ok(out)
    }

    /// Like [`from_options`](Self::from_options) but silently skipping anything
    /// that does not parse.
    pub fn from_options_lossy(options: &[ProtoOption]) -> Self {
        let mut out = FeatureOverrides::default();
        for opt in options {
            let Some(name) = feature_name(&opt.name) else {
                continue;
            };
            let _ = out.set_from_option(name, opt);
        }
        out
    }

    fn set_from_option(&mut self, name: &str, opt: &ProtoOption) -> Result<(), ParseError> {
        let ident = option_ident(&opt.value).ok_or_else(|| ParseError::InvalidFeatureValue {
            feature: name.to_owned(),
            value: describe_value(&opt.value),
            span: opt.span,
        })?;
        let bad = || ParseError::InvalidFeatureValue {
            feature: name.to_owned(),
            value: ident.to_owned(),
            span: opt.span,
        };
        match name {
            FieldPresence::FEATURE_NAME => {
                self.field_presence = Some(FieldPresence::from_ident(ident).ok_or_else(bad)?);
            }
            EnumType::FEATURE_NAME => {
                self.enum_type = Some(EnumType::from_ident(ident).ok_or_else(bad)?);
            }
            RepeatedFieldEncoding::FEATURE_NAME => {
                self.repeated_field_encoding =
                    Some(RepeatedFieldEncoding::from_ident(ident).ok_or_else(bad)?);
            }
            Utf8Validation::FEATURE_NAME => {
                self.utf8_validation = Some(Utf8Validation::from_ident(ident).ok_or_else(bad)?);
            }
            MessageEncoding::FEATURE_NAME => {
                self.message_encoding = Some(MessageEncoding::from_ident(ident).ok_or_else(bad)?);
            }
            JsonFormat::FEATURE_NAME => {
                self.json_format = Some(JsonFormat::from_ident(ident).ok_or_else(bad)?);
            }
            other => {
                return Err(ParseError::UnknownFeature {
                    name: other.to_owned(),
                    span: opt.span,
                })
            }
        }
        Ok(())
    }
}

/// Return the feature name (without the `features.` prefix) for an option name,
/// or `None` if the option is not a feature.
///
/// A *custom* option always starts with `(`, so `(features).x` — an extension
/// whose name merely happens to contain the word — is correctly not treated as
/// a feature.
fn feature_name(option_name: &str) -> Option<&str> {
    option_name.strip_prefix(FEATURE_PREFIX)
}

/// Extract the identifier from a feature value.
///
/// Feature values are always enum identifiers (`EXPLICIT`, `PACKED`, ...).
/// `true`/`false` lex as booleans, so accept them as identifiers too in order to
/// produce an "invalid value" error rather than a confusing "not an identifier".
fn option_ident(value: &OptionValue) -> Option<&str> {
    match value {
        OptionValue::Ident(s) => Some(s.as_str()),
        OptionValue::Bool(true) => Some("true"),
        OptionValue::Bool(false) => Some("false"),
        _ => None,
    }
}

/// A short human-readable description of a non-identifier option value.
fn describe_value(value: &OptionValue) -> String {
    match value {
        OptionValue::Ident(s) => s.clone(),
        OptionValue::Str(s) => format!("{s:?}"),
        OptionValue::Int(i) => i.to_string(),
        OptionValue::Float(f) => f.to_string(),
        OptionValue::Bool(b) => b.to_string(),
        OptionValue::MessageLiteral(_) => "a message literal".to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Whole-file validation
// ---------------------------------------------------------------------------

/// Validate the Editions rules for a freshly parsed file.
///
/// For an edition file this checks that
///
/// * every `features.*` option names a known feature with a legal value,
/// * the removed proto2 keywords (`optional`, `required`, `group`) are absent,
/// * features that only make sense on some fields are not applied to others.
///
/// For a `syntax`-based file it checks that no `features.*` option appears at
/// all, because feature resolution does not exist outside Editions.
///
/// # Errors
///
/// Returns the first violation as a [`ParseError`].
pub fn validate_file(file: &ProtoFile) -> Result<(), ParseError> {
    let is_edition = file.edition.is_some();
    validate_option_list(&file.options, is_edition)?;
    for msg in &file.messages {
        validate_message(msg, is_edition)?;
    }
    for en in &file.enums {
        validate_enum(en, is_edition)?;
    }
    for svc in &file.services {
        validate_service(svc, is_edition)?;
    }
    for ext in &file.extends {
        for field in &ext.fields {
            validate_field(field, is_edition, FieldScope::Extension)?;
        }
    }
    Ok(())
}

/// Where a field lives, which decides whether some features apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldScope {
    /// A plain message field.
    Message,
    /// A member of a real `oneof` block.
    Oneof,
    /// A field declared inside an `extend` block.
    Extension,
}

fn validate_message(msg: &Message, is_edition: bool) -> Result<(), ParseError> {
    validate_option_list(&msg.options, is_edition)?;
    for field in &msg.fields {
        validate_field(field, is_edition, FieldScope::Message)?;
    }
    for oneof in &msg.oneofs {
        validate_option_list(&oneof.options, is_edition)?;
        for field in &oneof.fields {
            validate_field(field, is_edition, FieldScope::Oneof)?;
        }
    }
    for nested in &msg.nested_messages {
        validate_message(nested, is_edition)?;
    }
    for en in &msg.nested_enums {
        validate_enum(en, is_edition)?;
    }
    Ok(())
}

fn validate_enum(en: &Enum, is_edition: bool) -> Result<(), ParseError> {
    validate_option_list(&en.options, is_edition)?;
    for value in &en.values {
        validate_option_list(&value.options, is_edition)?;
    }
    Ok(())
}

fn validate_service(svc: &Service, is_edition: bool) -> Result<(), ParseError> {
    validate_option_list(&svc.options, is_edition)?;
    for method in &svc.methods {
        validate_option_list(&method.options, is_edition)?;
    }
    Ok(())
}

fn validate_field(field: &Field, is_edition: bool, scope: FieldScope) -> Result<(), ParseError> {
    validate_option_list(&field.options, is_edition)?;
    if !is_edition {
        return Ok(());
    }

    // Checked before the label, because a bare `group` desugars to a field
    // that also carries an implicit `optional` label — reporting the label
    // would hide the real problem.
    if matches!(field.ty, FieldType::Group(_)) {
        return Err(ParseError::EditionSyntaxNotAllowed {
            construct: "a 'group' field",
            hint: "declare a nested message and use [features.message_encoding = DELIMITED]",
            span: field.span,
        });
    }

    // The proto2 label keywords were removed in Edition 2023: presence is a
    // feature now, so `optional`/`required` would be two ways of saying the
    // same thing (and `required` cannot be expressed by the syntax at all).
    match field.label {
        FieldLabel::Optional => {
            return Err(ParseError::EditionSyntaxNotAllowed {
                construct: "the 'optional' label",
                hint: "presence is controlled by features.field_presence in an edition file",
                span: field.span,
            })
        }
        FieldLabel::Required => {
            return Err(ParseError::EditionSyntaxNotAllowed {
                construct: "the 'required' label",
                hint: "use [features.field_presence = LEGACY_REQUIRED] in an edition file",
                span: field.span,
            })
        }
        FieldLabel::Repeated | FieldLabel::Singular => {}
    }

    let overrides = FeatureOverrides::from_options(&field.options)?;
    let is_repeated = matches!(field.label, FieldLabel::Repeated);
    let is_map = matches!(field.ty, FieldType::Map { .. });
    let is_message = matches!(field.ty, FieldType::Named(_));

    if let Some(presence) = overrides.field_presence {
        if is_repeated || is_map {
            return Err(ParseError::FeatureNotApplicable {
                feature: FieldPresence::FEATURE_NAME.to_owned(),
                reason: "a repeated or map field never tracks presence".to_owned(),
                span: field.span,
            });
        }
        if presence == FieldPresence::LegacyRequired
            && matches!(scope, FieldScope::Oneof | FieldScope::Extension)
        {
            return Err(ParseError::FeatureNotApplicable {
                feature: FieldPresence::FEATURE_NAME.to_owned(),
                reason: "LEGACY_REQUIRED is not allowed on a oneof member or an extension"
                    .to_owned(),
                span: field.span,
            });
        }
    }

    if overrides.repeated_field_encoding.is_some() && !is_repeated {
        return Err(ParseError::FeatureNotApplicable {
            feature: RepeatedFieldEncoding::FEATURE_NAME.to_owned(),
            reason: "the field is not repeated".to_owned(),
            span: field.span,
        });
    }

    if overrides.message_encoding == Some(MessageEncoding::Delimited) && !is_message {
        return Err(ParseError::FeatureNotApplicable {
            feature: MessageEncoding::FEATURE_NAME.to_owned(),
            reason: "DELIMITED only applies to a message-typed field".to_owned(),
            span: field.span,
        });
    }

    Ok(())
}

/// Check one `option` list: features must be well-formed, and must not appear
/// outside an edition file at all.
fn validate_option_list(options: &[ProtoOption], is_edition: bool) -> Result<(), ParseError> {
    if !is_edition {
        for opt in options {
            if let Some(name) = feature_name(&opt.name) {
                return Err(ParseError::FeaturesRequireEdition {
                    name: name.to_owned(),
                    span: opt.span,
                });
            }
        }
        return Ok(());
    }
    FeatureOverrides::from_options(options).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::span::Span;

    fn opt(name: &str, value: &str) -> ProtoOption {
        ProtoOption {
            name: name.to_owned(),
            value: OptionValue::Ident(value.to_owned()),
            span: Span { start: 0, end: 0 },
        }
    }

    #[test]
    fn edition_2023_baseline_matches_spec() {
        let f = FeatureSet::edition_2023();
        assert_eq!(f.field_presence, FieldPresence::Explicit);
        assert_eq!(f.enum_type, EnumType::Open);
        assert_eq!(f.repeated_field_encoding, RepeatedFieldEncoding::Packed);
        assert_eq!(f.utf8_validation, Utf8Validation::Verify);
        assert_eq!(f.message_encoding, MessageEncoding::LengthPrefixed);
        assert_eq!(f.json_format, JsonFormat::Allow);
    }

    #[test]
    fn legacy_baselines_match_spec() {
        let p2 = FeatureSet::proto2();
        assert_eq!(p2.field_presence, FieldPresence::Explicit);
        assert_eq!(p2.enum_type, EnumType::Closed);
        assert_eq!(p2.repeated_field_encoding, RepeatedFieldEncoding::Expanded);
        assert_eq!(p2.utf8_validation, Utf8Validation::None);
        assert_eq!(p2.json_format, JsonFormat::LegacyBestEffort);

        let p3 = FeatureSet::proto3();
        assert_eq!(p3.field_presence, FieldPresence::Implicit);
        assert_eq!(p3.enum_type, EnumType::Open);
        assert_eq!(p3.repeated_field_encoding, RepeatedFieldEncoding::Packed);
        assert_eq!(p3.utf8_validation, Utf8Validation::Verify);
        assert_eq!(p3.json_format, JsonFormat::Allow);
    }

    #[test]
    fn overrides_replace_only_named_features() {
        let base = FeatureSet::edition_2023();
        let ov = FeatureOverrides::from_options(&[opt("features.field_presence", "IMPLICIT")])
            .expect("valid override");
        let resolved = base.with_overrides(&ov);
        assert_eq!(resolved.field_presence, FieldPresence::Implicit);
        // Everything else is inherited untouched.
        assert_eq!(resolved.enum_type, base.enum_type);
        assert_eq!(
            resolved.repeated_field_encoding,
            base.repeated_field_encoding
        );
    }

    #[test]
    fn inheritance_is_three_levels_deep() {
        let file =
            FeatureSet::edition_2023().resolve_lossy(&[opt("features.field_presence", "IMPLICIT")]);
        let message = file.resolve_lossy(&[opt("features.enum_type", "CLOSED")]);
        let field = message.resolve_lossy(&[opt("features.field_presence", "EXPLICIT")]);
        assert_eq!(field.field_presence, FieldPresence::Explicit);
        assert_eq!(field.enum_type, EnumType::Closed);
        assert_eq!(message.field_presence, FieldPresence::Implicit);
    }

    #[test]
    fn unknown_feature_name_is_rejected() {
        let err = FeatureOverrides::from_options(&[opt("features.nonexistent", "X")])
            .expect_err("unknown feature must be rejected");
        assert!(
            matches!(err, ParseError::UnknownFeature { ref name, .. } if name == "nonexistent")
        );
    }

    #[test]
    fn unknown_feature_value_is_rejected() {
        let err = FeatureOverrides::from_options(&[opt("features.field_presence", "SOMETIMES")])
            .expect_err("unknown value must be rejected");
        match err {
            ParseError::InvalidFeatureValue { feature, value, .. } => {
                assert_eq!(feature, "field_presence");
                assert_eq!(value, "SOMETIMES");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn custom_options_are_not_mistaken_for_features() {
        let ov = FeatureOverrides::from_options(&[opt("(features).field_presence", "IMPLICIT")])
            .expect("custom option is not a feature");
        assert!(ov.is_empty());
    }

    #[test]
    fn resolved_pairs_cover_every_known_feature() {
        let pairs = FeatureSet::edition_2023().as_name_value_pairs();
        let names: Vec<&str> = pairs.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, KNOWN_FEATURES.to_vec());
    }
}
