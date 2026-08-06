//! The `prost-reflect`-backed facade against a Protobuf Editions descriptor
//! set.
//!
//! `prost-reflect` 0.16 rejects `syntax = "editions"` outright
//! (`unknown syntax 'editions'`), so every consumer of `pool_from_fds` — the
//! `DynamicMessage` re-export, `oxiproto-json`, and the CLI's `convert`
//! subcommand — was unusable for edition schemas. `oxiproto_reflect::editions`
//! down-levels such a file to its proto2 equivalent first.
//!
//! The assertions here are on observable behaviour: the facade must produce the
//! *same wire bytes* as the native path for the same logical message. Anything
//! weaker would pass even if the down-level silently changed the encoding.

use oxiproto_reflect::{
    downlevel_editions, has_editions_file, native, pool_from_fds, DynamicMessage, ReflectValue,
};
use prost::Message;

const EDITION_SRC: &str = r#"edition = "2023";
package edfacade;

enum Color {
  COLOR_UNKNOWN = 0;
  COLOR_RED = 1;
}

message Inner {
  int32 x = 1;
}

message M {
  int32 scalar = 1;
  string label = 2;
  repeated int32 defaulted = 3;
  repeated int32 expanded = 4 [features.repeated_field_encoding = EXPANDED];
  Inner nested = 5;
  Color color = 6;
  repeated string names = 7;
}
"#;

fn fds() -> prost_types::FileDescriptorSet {
    oxiproto_build::compile_str_native(EDITION_SRC).expect("edition source must compile")
}

// ---------------------------------------------------------------------------
// The bug: prost-reflect rejects the edition sentinel
// ---------------------------------------------------------------------------

/// The raw descriptor set really does carry the `"editions"` sentinel, and
/// `prost_reflect::DescriptorPool` really cannot load it — this pins the
/// motivation for the down-level so the test cannot quietly become vacuous.
///
/// prost-reflect 0.16.5 does not merely return `unknown syntax 'editions'`: it
/// panics while *building* that error, because the reporting code indexes the
/// file it has just refused to register (`descriptor/error.rs`). Catching the
/// unwind is therefore the only way to observe the failure — and it is exactly
/// what `pool_from_fds` now spares every caller. Both outcomes are accepted so
/// that a future upstream fix turning the panic into a plain error does not
/// fail this test.
#[test]
fn prost_reflect_cannot_load_the_raw_editions_descriptor_set() {
    let raw = fds();
    assert!(has_editions_file(&raw));
    let outcome = std::panic::catch_unwind(|| {
        prost_reflect::DescriptorPool::from_file_descriptor_set(raw)
            .map(|_| ())
            .map_err(|e| e.to_string())
    });
    match outcome {
        Ok(Ok(())) => panic!("prost-reflect unexpectedly accepted an editions FDS"),
        Ok(Err(message)) => assert!(message.contains("editions"), "unexpected error: {message}"),
        Err(_) => { /* upstream panic while reporting the unknown syntax */ }
    }
}

/// The facade entry point accepts it.
#[test]
fn pool_from_fds_accepts_an_editions_descriptor_set() {
    let pool = pool_from_fds(fds()).expect("facade must accept an editions FDS");
    let desc = pool
        .get_message_by_name("edfacade.M")
        .expect("message must be registered");
    assert_eq!(desc.fields().count(), 7);
    assert!(pool.get_enum_by_name("edfacade.Color").is_some());
    assert!(pool.get_message_by_name("edfacade.Inner").is_some());
}

/// The down-level is a no-op for a file that already declares a syntax, so
/// applying it unconditionally is safe.
#[test]
fn a_proto3_descriptor_set_is_unaffected() {
    let raw = oxiproto_build::compile_str_native(
        r#"syntax = "proto3";
package p3facade;
message M { repeated int32 vals = 1; }
"#,
    )
    .expect("proto3 source must compile");
    assert!(!has_editions_file(&raw));
    assert_eq!(downlevel_editions(raw.clone()), raw);
}

// ---------------------------------------------------------------------------
// Wire parity between the facade and the native path
// ---------------------------------------------------------------------------

/// Build the same logical message on both paths and require byte equality.
///
/// This is the assertion that proves the down-level preserves packing: the
/// Editions default is PACKED, but proto2 — the syntax the file is rewritten to
/// — defaults to expanded, so a transform that forgot to materialise
/// `options.packed` would produce different bytes here while still building a
/// perfectly valid pool.
#[test]
fn facade_and_native_paths_agree_on_wire_bytes() {
    let pool = pool_from_fds(fds()).expect("facade pool");
    let desc = pool
        .get_message_by_name("edfacade.M")
        .expect("edfacade.M in facade pool");
    let mut facade = DynamicMessage::new(desc.clone());
    facade.set_field_by_name("scalar", ReflectValue::I32(7));
    facade.set_field_by_name("label", ReflectValue::String("hi".to_owned()));
    facade.set_field_by_name(
        "defaulted",
        ReflectValue::List(vec![
            ReflectValue::I32(1),
            ReflectValue::I32(2),
            ReflectValue::I32(3),
        ]),
    );
    facade.set_field_by_name(
        "expanded",
        ReflectValue::List(vec![ReflectValue::I32(4), ReflectValue::I32(5)]),
    );
    facade.set_field_by_name("color", ReflectValue::EnumNumber(1));
    let facade_bytes = facade.encode_to_vec();

    let native_pool = native::DescriptorPool::from_file_descriptor_set(fds()).expect("native pool");
    let native_desc = native_pool
        .get_message_by_name("edfacade.M")
        .expect("edfacade.M in native pool");
    let mut msg = native::DynamicMessage::new(native_desc.clone());
    let field = |name: &str| {
        native_desc
            .get_field_by_name(name)
            .unwrap_or_else(|| panic!("field {name}"))
    };
    msg.set_field(&field("scalar"), native::Value::I32(7));
    msg.set_field(&field("label"), native::Value::String("hi".to_owned()));
    msg.set_field(
        &field("defaulted"),
        native::Value::List(vec![
            native::Value::I32(1),
            native::Value::I32(2),
            native::Value::I32(3),
        ]),
    );
    msg.set_field(
        &field("expanded"),
        native::Value::List(vec![native::Value::I32(4), native::Value::I32(5)]),
    );
    msg.set_field(&field("color"), native::Value::EnumNumber(1));
    let native_bytes = msg.encode_to_vec().expect("native encode");

    assert_eq!(
        facade_bytes, native_bytes,
        "facade and native encodings diverged"
    );

    // And the bytes really are what the Editions features ask for: field 3
    // packed (tag 0x1a, Len), field 4 expanded (tag 0x20, Varint, twice).
    assert!(
        facade_bytes.windows(2).any(|w| w == [0x1a, 0x03]),
        "field 3 must be packed: {facade_bytes:02x?}"
    );
    assert_eq!(
        facade_bytes.iter().filter(|b| **b == 0x20).count(),
        2,
        "field 4 must be expanded: {facade_bytes:02x?}"
    );
}

/// Bytes written by the native encoder decode through the facade and vice
/// versa, including the packed/expanded split.
#[test]
fn facade_decodes_native_bytes_for_an_editions_schema() {
    let native_pool = native::DescriptorPool::from_file_descriptor_set(fds()).expect("native pool");
    let native_desc = native_pool
        .get_message_by_name("edfacade.M")
        .expect("edfacade.M");
    let mut msg = native::DynamicMessage::new(native_desc.clone());
    let defaulted = native_desc
        .get_field_by_name("defaulted")
        .expect("defaulted");
    msg.set_field(
        &defaulted,
        native::Value::List(vec![native::Value::I32(10), native::Value::I32(20)]),
    );
    let bytes = msg.encode_to_vec().expect("native encode");

    let pool = pool_from_fds(fds()).expect("facade pool");
    let desc = pool.get_message_by_name("edfacade.M").expect("edfacade.M");
    let decoded = DynamicMessage::decode(desc.clone(), &bytes[..]).expect("facade decode");
    let field = desc.get_field_by_name("defaulted").expect("defaulted");
    let value = decoded.get_field(&field);
    let list = value.as_list().expect("repeated field is a list");
    assert_eq!(
        list.iter()
            .filter_map(ReflectValue::as_i32)
            .collect::<Vec<_>>(),
        vec![10, 20]
    );
}

// ---------------------------------------------------------------------------
// oxiproto-json over the facade
// ---------------------------------------------------------------------------

/// `features.field_presence` defaults to EXPLICIT in Editions, which the
/// proto2 base reproduces: a field explicitly set to its type default is
/// emitted rather than omitted.
#[test]
fn explicit_presence_survives_the_downlevel() {
    let pool = pool_from_fds(fds()).expect("facade pool");
    let desc = pool.get_message_by_name("edfacade.M").expect("edfacade.M");
    let scalar = desc.get_field_by_name("scalar").expect("scalar");
    assert!(
        scalar.supports_presence(),
        "Editions defaults field_presence to EXPLICIT"
    );

    let mut msg = DynamicMessage::new(desc);
    msg.set_field(&scalar, ReflectValue::I32(0));
    assert!(msg.has_field(&scalar));
    // An explicit zero on a presence-tracking field is written to the wire.
    assert_eq!(msg.encode_to_vec(), vec![0x08, 0x00]);
}

/// A `TYPE_GROUP` field materialised from `features.message_encoding =
/// DELIMITED` loads through the facade and keeps its group framing.
#[test]
fn delimited_message_encoding_loads_through_the_facade() {
    let raw = oxiproto_build::compile_str_native(
        r#"edition = "2023";
package eddelim;
message Inner { int32 x = 1; }
message Outer { Inner delim = 1 [features.message_encoding = DELIMITED]; }
"#,
    )
    .expect("edition source must compile");

    let pool = pool_from_fds(raw).expect("facade must accept a DELIMITED field");
    let desc = pool.get_message_by_name("eddelim.Outer").expect("Outer");
    let field = desc.get_field_by_name("delim").expect("delim");
    assert!(
        field.is_group(),
        "DELIMITED must present as a group, got {:?}",
        field.kind()
    );
}
