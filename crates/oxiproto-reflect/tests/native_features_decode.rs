//! Decode-time enforcement of `features.utf8_validation` and
//! `features.enum_type`.
//!
//! Both features were resolved by `oxiproto-build` and materialised into the
//! descriptor set, but the native decoder ignored them: every `string` was
//! UTF-8 validated and every enum behaved as OPEN. The assertions here are on
//! observable decode behaviour for all three syntaxes, because a feature that
//! resolves correctly but never reaches the decoder is indistinguishable from
//! one that was never implemented.

use oxiproto_reflect::native::{DescriptorPool, DynamicMessage, MessageDescriptor, Value};

fn pool_of(src: &str) -> DescriptorPool {
    let fds = oxiproto_build::compile_str_native(src).expect("source must compile");
    DescriptorPool::from_file_descriptor_set(fds).expect("pool")
}

fn message(pool: &DescriptorPool, name: &str) -> MessageDescriptor {
    pool.get_message_by_name(name)
        .unwrap_or_else(|| panic!("message {name} not found"))
}

/// field 1, length-delimited, holding a lone 0xFF byte — never valid UTF-8.
const INVALID_UTF8_FIELD_1: [u8; 3] = [0x0a, 0x01, 0xff];

// ---------------------------------------------------------------------------
// features.utf8_validation
// ---------------------------------------------------------------------------

/// proto3 resolves `utf8_validation` to VERIFY, so an invalid payload is a
/// decode error rather than something the caller has to notice later.
#[test]
fn proto3_string_rejects_invalid_utf8() {
    let pool = pool_of(
        r#"syntax = "proto3";
message M { string s = 1; }
"#,
    );
    let err = DynamicMessage::decode(message(&pool, "M"), &INVALID_UTF8_FIELD_1)
        .expect_err("VERIFY must reject invalid UTF-8");
    assert!(
        err.to_string().contains("utf"),
        "expected a UTF-8 error, got: {err}"
    );
}

/// Edition 2023 defaults `utf8_validation` to VERIFY.
#[test]
fn editions_string_defaults_to_verify() {
    let pool = pool_of(
        r#"edition = "2023";
message M { string s = 1; }
"#,
    );
    DynamicMessage::decode(message(&pool, "M"), &INVALID_UTF8_FIELD_1)
        .expect_err("the Editions default is VERIFY");
}

/// `features.utf8_validation = NONE` accepts the payload and preserves it
/// verbatim in a typed variant — not as `bytes`, and not lossily converted.
#[test]
fn editions_none_accepts_invalid_utf8_into_a_typed_variant() {
    let pool = pool_of(
        r#"edition = "2023";
message M { string s = 1 [features.utf8_validation = NONE]; }
"#,
    );
    let desc = message(&pool, "M");
    let msg = DynamicMessage::decode(desc.clone(), &INVALID_UTF8_FIELD_1)
        .expect("NONE must skip validation");
    let field = desc.get_field_by_name("s").expect("field s");

    assert!(!field.validates_utf8());
    assert_eq!(
        *msg.get_field(&field),
        Value::UnvalidatedString(vec![0xff]),
        "invalid payload must land in the typed unvalidated variant"
    );
    // It is a string, not bytes: `as_bytes` (the `bytes` accessor) says no.
    let value = msg.get_field(&field);
    assert!(value.as_bytes().is_none());
    assert!(value.as_str().is_none(), "it is not valid UTF-8");
    assert_eq!(value.as_string_bytes(), Some(&[0xff_u8][..]));
    assert_eq!(value.as_str_lossy().as_deref(), Some("\u{fffd}"));

    // And the bytes survive a re-encode exactly.
    assert_eq!(
        msg.encode_to_vec().expect("re-encode"),
        INVALID_UTF8_FIELD_1.to_vec()
    );
}

/// Turning validation off must not change how *well-formed* text is
/// represented, otherwise every NONE field would silently lose its string
/// behaviour in JSON and text output.
#[test]
fn none_keeps_valid_text_on_the_string_variant() {
    let pool = pool_of(
        r#"edition = "2023";
message M { string s = 1 [features.utf8_validation = NONE]; }
"#,
    );
    let desc = message(&pool, "M");
    // field 1, length 2, "hi"
    let msg = DynamicMessage::decode(desc.clone(), &[0x0a, 0x02, b'h', b'i']).expect("decode");
    let field = desc.get_field_by_name("s").expect("field s");
    assert_eq!(*msg.get_field(&field), Value::String("hi".to_owned()));
}

/// The proto2 baseline for `utf8_validation` is NONE — `protoc` has never
/// required a proto2 `string` to be valid UTF-8.
#[test]
fn proto2_string_baseline_is_none() {
    let pool = pool_of(
        r#"syntax = "proto2";
message M { optional string s = 1; }
"#,
    );
    let desc = message(&pool, "M");
    let field = desc.get_field_by_name("s").expect("field s");
    assert!(!field.validates_utf8());
    let msg = DynamicMessage::decode(desc, &INVALID_UTF8_FIELD_1).expect("proto2 baseline is NONE");
    assert_eq!(
        msg.encode_to_vec().expect("re-encode"),
        INVALID_UTF8_FIELD_1
    );
}

/// `bytes` fields are unaffected in every syntax.
#[test]
fn bytes_fields_never_validate() {
    for src in [
        r#"syntax = "proto3";
message M { bytes b = 1; }
"#,
        r#"edition = "2023";
message M { bytes b = 1; }
"#,
    ] {
        let pool = pool_of(src);
        let desc = message(&pool, "M");
        let field = desc.get_field_by_name("b").expect("field b");
        assert!(!field.validates_utf8());
        let msg =
            DynamicMessage::decode(desc, &INVALID_UTF8_FIELD_1).expect("bytes never validate");
        assert_eq!(*msg.get_field(&field), Value::Bytes(vec![0xff]));
    }
}

/// The canonical JSON mapping has no representation for a non-UTF-8 string, so
/// the conversion is refused with a typed error instead of losing bytes.
#[test]
fn json_refuses_an_unvalidated_string() {
    let pool = pool_of(
        r#"edition = "2023";
message M { string s = 1 [features.utf8_validation = NONE]; }
"#,
    );
    let msg = DynamicMessage::decode(message(&pool, "M"), &INVALID_UTF8_FIELD_1).expect("decode");
    let err = msg.to_json().expect_err("no canonical JSON form exists");
    assert!(
        err.to_string().contains("not valid UTF-8"),
        "unexpected error: {err}"
    );
}

/// The protobuf text format *does* have a representation: `\xNN` escapes — and
/// they round-trip, so `to_text` → `from_text` recovers the exact bytes rather
/// than being a display-only rendering.
#[test]
fn text_format_round_trips_an_unvalidated_string() {
    let pool = pool_of(
        r#"edition = "2023";
message M { string s = 1 [features.utf8_validation = NONE]; }
"#,
    );
    let desc = message(&pool, "M");
    let msg = DynamicMessage::decode(desc.clone(), &INVALID_UTF8_FIELD_1).expect("decode");
    let text = msg.to_text().expect("text format has an escape for this");
    assert!(text.contains(r"\xff"), "text output: {text}");

    let reparsed = DynamicMessage::from_text(desc.clone(), &text).expect("text must re-parse");
    let field = desc.get_field_by_name("s").expect("field s");
    assert_eq!(
        *reparsed.get_field(&field),
        Value::UnvalidatedString(vec![0xff])
    );
    assert_eq!(
        reparsed.encode_to_vec().expect("re-encode"),
        INVALID_UTF8_FIELD_1.to_vec()
    );
}

/// A VERIFY field still refuses non-UTF-8 text input — relaxing the parser for
/// NONE must not relax it everywhere.
#[test]
fn text_format_still_rejects_invalid_utf8_under_verify() {
    let pool = pool_of(
        r#"edition = "2023";
message M { string s = 1; }
"#,
    );
    DynamicMessage::from_text(message(&pool, "M"), r#"s: "\xff""#)
        .expect_err("VERIFY must reject invalid UTF-8 from text too");
}

/// Valid text under NONE still parses onto `Value::String`, matching the wire
/// decoder's classification.
#[test]
fn text_format_keeps_valid_text_on_the_string_variant() {
    let pool = pool_of(
        r#"edition = "2023";
message M { string s = 1 [features.utf8_validation = NONE]; }
"#,
    );
    let desc = message(&pool, "M");
    let msg = DynamicMessage::from_text(desc.clone(), r#"s: "hi""#).expect("parse");
    let field = desc.get_field_by_name("s").expect("field s");
    assert_eq!(*msg.get_field(&field), Value::String("hi".to_owned()));
}

// ---------------------------------------------------------------------------
// features.enum_type
// ---------------------------------------------------------------------------

const PROTO2_ENUM: &str = r#"syntax = "proto2";
enum E { E_A = 0; E_B = 1; }
message M { optional E e = 1; repeated E many = 2 [packed = true]; }
"#;

const PROTO3_ENUM: &str = r#"syntax = "proto3";
enum E { E_A = 0; E_B = 1; }
message M { E e = 1; repeated E many = 2; }
"#;

const EDITIONS_OPEN_ENUM: &str = r#"edition = "2023";
enum E { E_A = 0; E_B = 1; }
message M { E e = 1; repeated E many = 2; }
"#;

const EDITIONS_CLOSED_ENUM: &str = r#"edition = "2023";
enum E { option features.enum_type = CLOSED; E_A = 0; E_B = 1; }
message M { E e = 1; repeated E many = 2; }
"#;

/// Closedness is a property of the *enum type* and follows the file's
/// semantics: proto2 closed, proto3 open, Editions open by default and closed
/// when asked.
#[test]
fn enum_closedness_is_resolved_per_syntax() {
    for (src, expected) in [
        (PROTO2_ENUM, true),
        (PROTO3_ENUM, false),
        (EDITIONS_OPEN_ENUM, false),
        (EDITIONS_CLOSED_ENUM, true),
    ] {
        let pool = pool_of(src);
        let enum_desc = pool.get_enum_by_name("E").expect("enum E");
        assert_eq!(
            enum_desc.is_closed(),
            expected,
            "wrong closedness for:\n{src}"
        );
    }
}

/// A closed enum treats an unrecognised number as an unknown field: it is not
/// readable through the field, but it is preserved and re-emitted.
#[test]
fn a_closed_enum_routes_an_unknown_number_to_unknown_fields() {
    let pool = pool_of(PROTO2_ENUM);
    let desc = message(&pool, "M");
    // field 1, varint, value 7 — not a declared value of E.
    let bytes = [0x08u8, 0x07];
    let msg = DynamicMessage::decode(desc.clone(), &bytes).expect("decode");
    let field = desc.get_field_by_name("e").expect("field e");

    assert!(
        !msg.has_field(&field),
        "a closed enum must not accept an undeclared number"
    );
    assert_eq!(msg.unknown_fields().len(), 1);
    assert_eq!(
        msg.encode_to_vec().expect("re-encode"),
        bytes.to_vec(),
        "the raw bytes must survive"
    );
}

/// A declared value is unaffected.
#[test]
fn a_closed_enum_still_accepts_a_declared_number() {
    let pool = pool_of(PROTO2_ENUM);
    let desc = message(&pool, "M");
    let msg = DynamicMessage::decode(desc.clone(), &[0x08u8, 0x01]).expect("decode");
    let field = desc.get_field_by_name("e").expect("field e");
    assert_eq!(*msg.get_field(&field), Value::EnumNumber(1));
    assert_eq!(msg.unknown_fields().len(), 0);
}

/// An open enum keeps the raw number, which is what lets a reader built against
/// an older schema forward a value a newer writer produced.
#[test]
fn an_open_enum_keeps_an_unknown_number() {
    for src in [PROTO3_ENUM, EDITIONS_OPEN_ENUM] {
        let pool = pool_of(src);
        let desc = message(&pool, "M");
        let msg = DynamicMessage::decode(desc.clone(), &[0x08u8, 0x07]).expect("decode");
        let field = desc.get_field_by_name("e").expect("field e");
        assert_eq!(*msg.get_field(&field), Value::EnumNumber(7), "for:\n{src}");
        assert_eq!(msg.unknown_fields().len(), 0);
    }
}

/// `features.enum_type = CLOSED` in an edition file behaves exactly like a
/// proto2 enum.
#[test]
fn an_editions_closed_enum_rejects_an_unknown_number() {
    let pool = pool_of(EDITIONS_CLOSED_ENUM);
    let desc = message(&pool, "M");
    let msg = DynamicMessage::decode(desc.clone(), &[0x08u8, 0x07]).expect("decode");
    let field = desc.get_field_by_name("e").expect("field e");
    assert!(!msg.has_field(&field));
    assert_eq!(msg.unknown_fields().len(), 1);
}

/// A negative enum number is sign-extended to a full 10-byte varint on the
/// wire. Preserving the *raw* varint (rather than re-encoding the `i32`) keeps
/// the unknown-field bytes identical.
#[test]
fn a_negative_unknown_number_is_preserved_raw() {
    let pool = pool_of(PROTO2_ENUM);
    let desc = message(&pool, "M");
    // field 1, varint = -1 sign-extended to 64 bits.
    let bytes = [
        0x08u8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01,
    ];
    let msg = DynamicMessage::decode(desc, &bytes).expect("decode");
    assert_eq!(msg.encode_to_vec().expect("re-encode"), bytes.to_vec());
}

/// In a packed run, a closed enum keeps the values it recognises on the field
/// and moves the rest to the unknown set as individual varints — the placement
/// `protoc` produces. The round trip is therefore semantically faithful rather
/// than byte-identical, because the survivors are re-packed without the
/// rejected elements.
#[test]
fn a_packed_closed_enum_run_is_split() {
    let pool = pool_of(PROTO2_ENUM);
    let desc = message(&pool, "M");
    // field 2, Len, payload [0, 7, 1] — 7 is undeclared.
    let msg =
        DynamicMessage::decode(desc.clone(), &[0x12u8, 0x03, 0x00, 0x07, 0x01]).expect("decode");
    let field = desc.get_field_by_name("many").expect("field many");
    assert_eq!(
        *msg.get_field(&field),
        Value::List(vec![Value::EnumNumber(0), Value::EnumNumber(1)])
    );
    assert_eq!(msg.unknown_fields().len(), 1);

    // The rejected element re-emerges as an unpacked varint on field 2.
    let re_encoded = msg.encode_to_vec().expect("re-encode");
    assert_eq!(re_encoded, vec![0x12, 0x02, 0x00, 0x01, 0x10, 0x07]);
}

/// An open enum leaves a packed run untouched.
#[test]
fn a_packed_open_enum_run_is_kept_whole() {
    let pool = pool_of(PROTO3_ENUM);
    let desc = message(&pool, "M");
    let msg =
        DynamicMessage::decode(desc.clone(), &[0x12u8, 0x03, 0x00, 0x07, 0x01]).expect("decode");
    let field = desc.get_field_by_name("many").expect("field many");
    assert_eq!(
        *msg.get_field(&field),
        Value::List(vec![
            Value::EnumNumber(0),
            Value::EnumNumber(7),
            Value::EnumNumber(1)
        ])
    );
    assert_eq!(msg.unknown_fields().len(), 0);
}

/// A map entry whose *value* a closed enum rejects is moved wholesale into the
/// unknown-field set, rather than being inserted half-decoded. That is where
/// `protoc` puts it, and it keeps the entry's bytes recoverable.
#[test]
fn a_closed_enum_rejects_a_whole_map_entry() {
    let pool = pool_of(
        r#"syntax = "proto2";
enum E { E_A = 0; E_B = 1; }
message M { map<string, E> m = 1; }
"#,
    );
    let desc = message(&pool, "M");
    let field = desc.get_field_by_name("m").expect("field m");

    // field 1, Len 5: entry { key(1) = "k", value(2) = 7 }
    let bytes = [0x0au8, 0x05, 0x0a, 0x01, b'k', 0x10, 0x07];
    let msg = DynamicMessage::decode(desc.clone(), &bytes).expect("decode");

    let value = msg.get_field(&field);
    let map = value.as_map().expect("map field");
    assert!(map.is_empty(), "the rejected entry must not be inserted");
    assert_eq!(msg.unknown_fields().len(), 1);
    assert_eq!(
        msg.encode_to_vec().expect("re-encode"),
        bytes.to_vec(),
        "the entry's bytes must survive"
    );
}

/// The same entry with a *declared* value is accepted normally, so the
/// rejection is about closedness and not about map handling in general.
#[test]
fn a_closed_enum_accepts_a_declared_map_entry() {
    let pool = pool_of(
        r#"syntax = "proto2";
enum E { E_A = 0; E_B = 1; }
message M { map<string, E> m = 1; }
"#,
    );
    let desc = message(&pool, "M");
    let field = desc.get_field_by_name("m").expect("field m");
    let msg = DynamicMessage::decode(desc.clone(), &[0x0au8, 0x05, 0x0a, 0x01, b'k', 0x10, 0x01])
        .expect("decode");
    let value = msg.get_field(&field);
    let map = value.as_map().expect("map field");
    assert_eq!(
        map.get(&oxiproto_reflect::native::MapKey::String("k".to_owned())),
        Some(&Value::EnumNumber(1))
    );
    assert_eq!(msg.unknown_fields().len(), 0);
}

/// The entry is rejected regardless of field order inside it: an encoder that
/// writes `value` before `key` must land in the same place.
#[test]
fn a_closed_enum_rejects_a_map_entry_written_value_first() {
    let pool = pool_of(
        r#"syntax = "proto2";
enum E { E_A = 0; E_B = 1; }
message M { map<string, E> m = 1; }
"#,
    );
    let desc = message(&pool, "M");
    let field = desc.get_field_by_name("m").expect("field m");
    // value(2) = 7 first, then key(1) = "k".
    let bytes = [0x0au8, 0x05, 0x10, 0x07, 0x0a, 0x01, b'k'];
    let msg = DynamicMessage::decode(desc.clone(), &bytes).expect("decode");
    let value = msg.get_field(&field);
    assert!(value.as_map().expect("map field").is_empty());
    assert_eq!(msg.unknown_fields().len(), 1);
    assert_eq!(msg.encode_to_vec().expect("re-encode"), bytes.to_vec());
}

/// An unpacked (one tag per element) closed-enum run splits the same way a
/// packed one does.
#[test]
fn an_unpacked_closed_enum_run_is_split() {
    let pool = pool_of(PROTO2_ENUM);
    let desc = message(&pool, "M");
    // field 2, Varint, three times: 0, 7, 1.
    let msg = DynamicMessage::decode(desc.clone(), &[0x10u8, 0x00, 0x10, 0x07, 0x10, 0x01])
        .expect("decode");
    let field = desc.get_field_by_name("many").expect("field many");
    assert_eq!(
        *msg.get_field(&field),
        Value::List(vec![Value::EnumNumber(0), Value::EnumNumber(1)])
    );
    assert_eq!(msg.unknown_fields().len(), 1);
}
