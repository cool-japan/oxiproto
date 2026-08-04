#![forbid(unsafe_code)]

//! Protobuf *Editions* (`edition = "2023";`) end-to-end tests.
//!
//! Each test drives the whole native pipeline — lexer → parser → feature
//! resolution → `FileDescriptorProto` — and asserts on the descriptor that a
//! consumer actually sees, not on an intermediate AST.

#[cfg(feature = "native-parser")]
mod editions_tests {
    use oxiproto_build::compile_str_native;
    use oxiproto_build::parser::features::{
        EnumType, FeatureOverrides, FeatureSet, FieldPresence, JsonFormat, MessageEncoding,
        RepeatedFieldEncoding, Utf8Validation,
    };
    use oxiproto_build::parser::{parse_file, ParseError};
    use prost_types::field_descriptor_proto::{Label, Type};
    use prost_types::{FieldDescriptorProto, FileDescriptorProto, UninterpretedOption};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn file_of(src: &str) -> FileDescriptorProto {
        let fds = compile_str_native(src).expect("edition file must compile");
        fds.file.into_iter().next().expect("one file")
    }

    fn field_named<'a>(
        file: &'a FileDescriptorProto,
        msg: &str,
        name: &str,
    ) -> &'a FieldDescriptorProto {
        file.message_type
            .iter()
            .find(|m| m.name.as_deref() == Some(msg))
            .unwrap_or_else(|| panic!("message {msg} not found"))
            .field
            .iter()
            .find(|f| f.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("field {name} not found in {msg}"))
    }

    /// Read a resolved feature back out of an option list, exactly as a
    /// descriptor consumer would.
    fn feature(options: &[UninterpretedOption], name: &str) -> Option<String> {
        options.iter().find_map(|u| {
            let mut parts = u.name.iter();
            let first = parts.next()?;
            let second = parts.next()?;
            if parts.next().is_some()
                || first.is_extension
                || second.is_extension
                || first.name_part != "features"
                || second.name_part != name
            {
                return None;
            }
            u.identifier_value.clone()
        })
    }

    fn field_feature(file: &FileDescriptorProto, msg: &str, fname: &str, feat: &str) -> String {
        let f = field_named(file, msg, fname);
        let opts = f.options.as_ref().expect("edition fields carry options");
        feature(&opts.uninterpreted_option, feat)
            .unwrap_or_else(|| panic!("feature {feat} not materialised on {msg}.{fname}"))
    }

    // -----------------------------------------------------------------------
    // Baselines
    // -----------------------------------------------------------------------

    /// The Edition 2023 defaults, asserted against the spec table (they are
    /// proto2's presence with proto3's enums / packing / UTF-8 / JSON).
    #[test]
    fn edition_2023_defaults_are_materialised_on_every_field() {
        let file = file_of(
            r#"edition = "2023";
message M {
  int32 a = 1;
}
"#,
        );
        assert_eq!(file.syntax.as_deref(), Some("editions"));
        assert_eq!(field_feature(&file, "M", "a", "field_presence"), "EXPLICIT");
        assert_eq!(field_feature(&file, "M", "a", "enum_type"), "OPEN");
        assert_eq!(
            field_feature(&file, "M", "a", "repeated_field_encoding"),
            "PACKED"
        );
        assert_eq!(field_feature(&file, "M", "a", "utf8_validation"), "VERIFY");
        assert_eq!(
            field_feature(&file, "M", "a", "message_encoding"),
            "LENGTH_PREFIXED"
        );
        assert_eq!(field_feature(&file, "M", "a", "json_format"), "ALLOW");
    }

    /// Nothing about a `syntax` file changes: features are not materialised and
    /// the old label rules still apply.
    #[test]
    fn proto2_and_proto3_files_are_untouched() {
        let p2 = file_of(
            r#"syntax = "proto2";
message M {
  optional int32 a = 1;
  required string b = 2;
}
"#,
        );
        assert_eq!(p2.syntax.as_deref(), Some("proto2"));
        assert_eq!(
            field_named(&p2, "M", "a").label,
            Some(Label::Optional as i32)
        );
        assert_eq!(
            field_named(&p2, "M", "b").label,
            Some(Label::Required as i32)
        );
        assert!(field_named(&p2, "M", "a").options.is_none());

        let p3 = file_of(
            r#"syntax = "proto3";
message M {
  optional int32 a = 1;
}
"#,
        );
        // proto3 `optional` still uses the synthetic-oneof encoding.
        assert_eq!(field_named(&p3, "M", "a").proto3_optional, Some(true));
    }

    // -----------------------------------------------------------------------
    // field_presence
    // -----------------------------------------------------------------------

    #[test]
    fn field_presence_legacy_required_becomes_label_required() {
        let file = file_of(
            r#"edition = "2023";
message M {
  int32 must = 1 [features.field_presence = LEGACY_REQUIRED];
  int32 may  = 2;
}
"#,
        );
        assert_eq!(
            field_named(&file, "M", "must").label,
            Some(Label::Required as i32)
        );
        assert_eq!(
            field_named(&file, "M", "may").label,
            Some(Label::Optional as i32)
        );
        assert_eq!(
            field_feature(&file, "M", "must", "field_presence"),
            "LEGACY_REQUIRED"
        );
    }

    #[test]
    fn field_presence_inherits_file_then_message_then_field() {
        let file = file_of(
            r#"edition = "2023";
option features.field_presence = IMPLICIT;

message Outer {
  int32 from_file = 1;

  message Inner {
    option features.field_presence = EXPLICIT;
    int32 from_message = 1;
    int32 from_field = 2 [features.field_presence = IMPLICIT];
  }
}
"#,
        );
        assert_eq!(
            field_feature(&file, "Outer", "from_file", "field_presence"),
            "IMPLICIT"
        );
        let inner = &file.message_type[0].nested_type[0];
        let by = |name: &str| {
            inner
                .field
                .iter()
                .find(|f| f.name.as_deref() == Some(name))
                .and_then(|f| f.options.as_ref())
                .map(|o| feature(&o.uninterpreted_option, "field_presence").expect("presence"))
                .expect("field")
        };
        assert_eq!(by("from_message"), "EXPLICIT");
        assert_eq!(by("from_field"), "IMPLICIT");
    }

    // -----------------------------------------------------------------------
    // repeated_field_encoding
    // -----------------------------------------------------------------------

    #[test]
    fn repeated_field_encoding_materialises_an_explicit_packed_flag() {
        let file = file_of(
            r#"edition = "2023";
message M {
  repeated int32 def = 1;
  repeated int32 exp = 2 [features.repeated_field_encoding = EXPANDED];
  repeated string names = 3;
}
"#,
        );
        assert_eq!(
            field_named(&file, "M", "def")
                .options
                .as_ref()
                .and_then(|o| o.packed),
            Some(true),
            "the edition default PACKED must be written out explicitly"
        );
        assert_eq!(
            field_named(&file, "M", "exp")
                .options
                .as_ref()
                .and_then(|o| o.packed),
            Some(false)
        );
        assert_eq!(
            field_named(&file, "M", "names")
                .options
                .as_ref()
                .and_then(|o| o.packed),
            None,
            "string is not packable, so no packed flag is invented"
        );
    }

    #[test]
    fn repeated_field_encoding_can_be_set_file_wide() {
        let file = file_of(
            r#"edition = "2023";
option features.repeated_field_encoding = EXPANDED;
message M {
  repeated int64 xs = 1;
}
"#,
        );
        assert_eq!(
            field_named(&file, "M", "xs")
                .options
                .as_ref()
                .and_then(|o| o.packed),
            Some(false)
        );
    }

    // -----------------------------------------------------------------------
    // message_encoding
    // -----------------------------------------------------------------------

    #[test]
    fn message_encoding_delimited_becomes_type_group() {
        let file = file_of(
            r#"edition = "2023";
message Inner {
  int32 x = 1;
}
message M {
  Inner len  = 1;
  Inner delim = 2 [features.message_encoding = DELIMITED];
}
"#,
        );
        assert_eq!(
            field_named(&file, "M", "len").r#type,
            Some(Type::Message as i32)
        );
        assert_eq!(
            field_named(&file, "M", "delim").r#type,
            Some(Type::Group as i32),
            "DELIMITED is the group wire framing, spelled TYPE_GROUP in the descriptor"
        );
        // The type reference is unchanged: only the framing differs.
        assert_eq!(
            field_named(&file, "M", "delim").type_name.as_deref(),
            Some(".Inner")
        );
    }

    // -----------------------------------------------------------------------
    // Enum / oneof scopes
    // -----------------------------------------------------------------------

    #[test]
    fn enum_scope_resolves_and_materialises_enum_type() {
        let file = file_of(
            r#"edition = "2023";
enum Closed {
  option features.enum_type = CLOSED;
  CLOSED_ZERO = 0;
}
enum Default {
  DEFAULT_ZERO = 0;
}
"#,
        );
        let read = |name: &str| {
            file.enum_type
                .iter()
                .find(|e| e.name.as_deref() == Some(name))
                .and_then(|e| e.options.as_ref())
                .map(|o| feature(&o.uninterpreted_option, "enum_type").expect("enum_type"))
                .expect("enum")
        };
        assert_eq!(read("Closed"), "CLOSED");
        assert_eq!(read("Default"), "OPEN");
    }

    #[test]
    fn oneof_scope_is_inherited_by_its_members() {
        let file = file_of(
            r#"edition = "2023";
message M {
  oneof choice {
    option features.json_format = LEGACY_BEST_EFFORT;
    int32 a = 1;
    string b = 2;
  }
}
"#,
        );
        assert_eq!(
            field_feature(&file, "M", "a", "json_format"),
            "LEGACY_BEST_EFFORT"
        );
        assert_eq!(
            field_feature(&file, "M", "b", "json_format"),
            "LEGACY_BEST_EFFORT"
        );
    }

    // -----------------------------------------------------------------------
    // Rejections
    // -----------------------------------------------------------------------

    #[test]
    fn removed_constructs_are_rejected() {
        for (src, needle) in [
            (
                r#"edition = "2023";
message M { optional int32 a = 1; }
"#,
                "'optional' label",
            ),
            (
                r#"edition = "2023";
message M { required int32 a = 1; }
"#,
                "'required' label",
            ),
            (
                r#"edition = "2023";
message M { group Sub = 1 { int32 x = 1; } }
"#,
                "'group' field",
            ),
        ] {
            let err = parse_file(src).expect_err("removed construct must be rejected");
            let rendered = err.to_string();
            assert!(
                rendered.contains(needle),
                "expected {needle:?} in error, got {rendered}"
            );
        }
    }

    #[test]
    fn features_are_rejected_outside_an_edition_file() {
        for syntax in ["proto2", "proto3"] {
            let src = format!(
                "syntax = \"{syntax}\";\nmessage M {{\n  int32 a = 1 [features.field_presence = IMPLICIT];\n}}\n"
            );
            let err = parse_file(&src).expect_err("features need an edition");
            assert!(matches!(err, ParseError::FeaturesRequireEdition { .. }));
        }
    }

    #[test]
    fn unknown_feature_name_and_value_are_rejected() {
        let err = parse_file(
            r#"edition = "2023";
message M { int32 a = 1 [features.no_such_feature = X]; }
"#,
        )
        .expect_err("unknown feature");
        assert!(matches!(err, ParseError::UnknownFeature { .. }));

        let err = parse_file(
            r#"edition = "2023";
message M { int32 a = 1 [features.field_presence = SOMETIMES]; }
"#,
        )
        .expect_err("unknown value");
        assert!(matches!(err, ParseError::InvalidFeatureValue { .. }));
    }

    #[test]
    fn features_are_rejected_where_they_cannot_apply() {
        let cases = [
            // presence on a repeated field
            r#"edition = "2023";
message M { repeated int32 a = 1 [features.field_presence = EXPLICIT]; }
"#,
            // packing on a non-repeated field
            r#"edition = "2023";
message M { int32 a = 1 [features.repeated_field_encoding = PACKED]; }
"#,
            // delimited framing on a scalar
            r#"edition = "2023";
message M { int32 a = 1 [features.message_encoding = DELIMITED]; }
"#,
            // LEGACY_REQUIRED inside a oneof
            r#"edition = "2023";
message M { oneof c { int32 a = 1 [features.field_presence = LEGACY_REQUIRED]; } }
"#,
        ];
        for src in cases {
            let err = parse_file(src).expect_err("inapplicable feature must be rejected");
            assert!(
                matches!(err, ParseError::FeatureNotApplicable { .. }),
                "unexpected error: {err}"
            );
        }
    }

    /// `DELIMITED` on a field whose named type turns out to be an *enum* cannot
    /// be caught at parse time (the parser cannot tell an enum reference from a
    /// message reference), so the descriptor builder's post-pass must reject it
    /// rather than emit a descriptor whose materialised feature contradicts its
    /// own `type`.
    #[test]
    fn delimited_on_an_enum_typed_field_is_rejected_after_resolution() {
        let err = compile_str_native(
            r#"edition = "2023";
enum E { E_ZERO = 0; }
message M {
  E e = 1 [features.message_encoding = DELIMITED];
}
"#,
        )
        .expect_err("DELIMITED must not apply to an enum field");
        let rendered = err.to_string();
        assert!(
            rendered.contains("DELIMITED") && rendered.contains("message-typed"),
            "unexpected error: {rendered}"
        );
    }

    /// Enum *values* are a feature scope too; their resolution must survive into
    /// the descriptor rather than being validated and dropped.
    #[test]
    fn enum_value_scope_is_materialised() {
        let file = file_of(
            r#"edition = "2023";
enum E {
  option features.enum_type = CLOSED;
  E_ZERO = 0;
  E_ONE = 1 [features.json_format = LEGACY_BEST_EFFORT];
}
"#,
        );
        let values = &file.enum_type[0].value;
        let read = |idx: usize, feat: &str| {
            values[idx]
                .options
                .as_ref()
                .and_then(|o| feature(&o.uninterpreted_option, feat))
                .unwrap_or_else(|| panic!("feature {feat} missing on value {idx}"))
        };
        // Inherited from the enum scope.
        assert_eq!(read(0, "enum_type"), "CLOSED");
        assert_eq!(read(1, "enum_type"), "CLOSED");
        // Set on the value itself.
        assert_eq!(read(0, "json_format"), "ALLOW");
        assert_eq!(read(1, "json_format"), "LEGACY_BEST_EFFORT");
    }

    #[test]
    fn unsupported_edition_string_is_rejected() {
        let err = parse_file("edition = \"1999\";\n").expect_err("unknown edition");
        assert!(matches!(err, ParseError::UnsupportedEdition(ref s) if s == "1999"));
    }

    /// Edition 2024 is rejected *by name*, not silently approximated as 2023.
    ///
    /// An edition is defined by the feature defaults it changes, so accepting a
    /// 2024 file under the 2023 table would emit a descriptor set that diverges
    /// from `protoc` on the wire while reporting success. Edition 2024 also adds
    /// the `export` / `local` symbol-visibility modifiers, which this grammar
    /// does not parse at all. Both are lifted together, not one at a time.
    #[test]
    fn edition_2024_is_rejected_rather_than_approximated() {
        let err = parse_file("edition = \"2024\";\nmessage M { int32 x = 1; }\n")
            .expect_err("edition 2024 is not implemented");
        assert!(
            matches!(err, ParseError::UnsupportedEdition(ref s) if s == "2024"),
            "expected a typed rejection naming the edition, got: {err:?}"
        );
        assert!(
            err.to_string().contains("2024"),
            "the message must name the offending edition: {err}"
        );
    }

    #[test]
    fn syntax_and_edition_cannot_coexist() {
        let err = parse_file("syntax = \"proto3\";\nedition = \"2023\";\n")
            .expect_err("mutually exclusive");
        assert!(matches!(err, ParseError::SyntaxAndEditionConflict));
    }

    // -----------------------------------------------------------------------
    // The resolution engine itself
    // -----------------------------------------------------------------------

    #[test]
    fn legacy_editions_are_expressible_as_feature_sets() {
        let p2 = FeatureSet::proto2();
        assert_eq!(p2.field_presence, FieldPresence::Explicit);
        assert_eq!(p2.enum_type, EnumType::Closed);
        assert_eq!(p2.repeated_field_encoding, RepeatedFieldEncoding::Expanded);
        assert_eq!(p2.utf8_validation, Utf8Validation::None);
        assert_eq!(p2.message_encoding, MessageEncoding::LengthPrefixed);
        assert_eq!(p2.json_format, JsonFormat::LegacyBestEffort);

        let ed = FeatureSet::edition_2023();
        assert_eq!(ed.field_presence, FieldPresence::Explicit);
        assert_eq!(ed.enum_type, EnumType::Open);
        assert_eq!(ed.repeated_field_encoding, RepeatedFieldEncoding::Packed);
    }

    #[test]
    fn an_empty_override_set_changes_nothing() {
        let base = FeatureSet::edition_2023();
        assert_eq!(base.with_overrides(&FeatureOverrides::default()), base);
    }
}
