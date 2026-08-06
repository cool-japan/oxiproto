//! Tests for proto2 group support in the native reflection codec.
//!
//! A `group` is a proto2 construct encoded as a start-group tag (wire type 3),
//! the group's fields inline, then a matching end-group tag (wire type 4). The
//! descriptor pool models it as a synthetic message referenced by a
//! [`Kind::Group`] field, so encode/decode/JSON/text all treat it structurally
//! like a nested message.

use oxiproto_reflect::native::{DescriptorPool, DynamicMessage, Kind, Value};
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet};

fn field(
    name: &str,
    number: i32,
    label: Label,
    ty: Type,
    type_name: Option<&str>,
) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_owned()),
        number: Some(number),
        label: Some(label as i32),
        r#type: Some(ty as i32),
        type_name: type_name.map(str::to_owned),
        ..Default::default()
    }
}

fn proto2_file(messages: Vec<DescriptorProto>) -> FileDescriptorSet {
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("group_test.proto".to_owned()),
            syntax: Some("proto2".to_owned()),
            message_type: messages,
            ..Default::default()
        }],
    }
}

/// `MyGroup { optional int32 x = 1; optional string s = 2; }` referenced from
/// `Outer` by a singular group field.
fn outer_with_group(group_label: Label) -> FileDescriptorSet {
    proto2_file(vec![
        DescriptorProto {
            name: Some("MyGroup".to_owned()),
            field: vec![
                field("x", 1, Label::Optional, Type::Int32, None),
                field("s", 2, Label::Optional, Type::String, None),
            ],
            ..Default::default()
        },
        DescriptorProto {
            name: Some("Outer".to_owned()),
            field: vec![
                field("lead", 1, Label::Optional, Type::Int32, None),
                field("mygroup", 2, group_label, Type::Group, Some(".MyGroup")),
                field("trail", 3, Label::Optional, Type::Int32, None),
            ],
            ..Default::default()
        },
    ])
}

#[test]
fn singular_group_round_trips_through_wire() {
    let pool =
        DescriptorPool::from_file_descriptor_set(outer_with_group(Label::Optional)).expect("pool");
    let outer_desc = pool.get_message_by_name("Outer").expect("Outer");
    let group_desc = pool.get_message_by_name("MyGroup").expect("MyGroup");
    let group_field = outer_desc.get_field(2).expect("mygroup");
    assert!(
        matches!(group_field.kind(), Kind::Group(_)),
        "field must be a group kind"
    );

    let mut group = DynamicMessage::new(group_desc);
    group.set_field(&group.descriptor().get_field(1).expect("x"), Value::I32(42));
    group.set_field(
        &group.descriptor().get_field(2).expect("s"),
        Value::String("hi".to_owned()),
    );

    let mut outer = DynamicMessage::new(outer_desc.clone());
    outer.set_field(&outer_desc.get_field(1).expect("lead"), Value::I32(7));
    outer.set_field(&group_field, Value::Message(Box::new(group)));
    outer.set_field(&outer_desc.get_field(3).expect("trail"), Value::I32(99));

    let bytes = outer.encode_to_vec().expect("encode");

    // The group must be framed by start-group / end-group tags for field 2:
    // tag(2, SGroup=3) = (2<<3)|3 = 0x13 ; tag(2, EGroup=4) = (2<<3)|4 = 0x14.
    assert!(bytes.contains(&0x13), "start-group tag present: {bytes:?}");
    assert!(bytes.contains(&0x14), "end-group tag present: {bytes:?}");

    let decoded = DynamicMessage::decode(outer_desc, &bytes).expect("decode");
    assert_eq!(
        decoded
            .get_field(&decoded.descriptor().get_field(1).expect("lead"))
            .into_owned()
            .as_i32(),
        Some(7)
    );
    assert_eq!(
        decoded
            .get_field(&decoded.descriptor().get_field(3).expect("trail"))
            .into_owned()
            .as_i32(),
        Some(99)
    );
    let got_group = decoded
        .get_field(&decoded.descriptor().get_field(2).expect("mygroup"))
        .into_owned();
    match got_group {
        Value::Message(g) => {
            assert_eq!(
                g.get_field(&g.descriptor().get_field(1).expect("x"))
                    .into_owned()
                    .as_i32(),
                Some(42)
            );
            match g
                .get_field(&g.descriptor().get_field(2).expect("s"))
                .into_owned()
            {
                Value::String(s) => assert_eq!(s, "hi"),
                other => panic!("expected string, got {other:?}"),
            }
        }
        other => panic!("expected group message, got {other:?}"),
    }

    // Full byte-for-byte re-encode stability.
    assert_eq!(decoded.encode_to_vec().expect("re-encode"), bytes);
}

#[test]
fn repeated_group_round_trips() {
    let pool =
        DescriptorPool::from_file_descriptor_set(outer_with_group(Label::Repeated)).expect("pool");
    let outer_desc = pool.get_message_by_name("Outer").expect("Outer");
    let group_desc = pool.get_message_by_name("MyGroup").expect("MyGroup");
    let group_field = outer_desc.get_field(2).expect("mygroup");

    let mk = |v: i32| {
        let mut g = DynamicMessage::new(group_desc.clone());
        g.set_field(&g.descriptor().get_field(1).expect("x"), Value::I32(v));
        Value::Message(Box::new(g))
    };

    let mut outer = DynamicMessage::new(outer_desc.clone());
    outer.set_field(&group_field, Value::List(vec![mk(1), mk(2), mk(3)]));

    let bytes = outer.encode_to_vec().expect("encode");
    let decoded = DynamicMessage::decode(outer_desc, &bytes).expect("decode");
    match decoded.get_field(&group_field).into_owned() {
        Value::List(list) => {
            let xs: Vec<i32> = list
                .iter()
                .filter_map(|v| match v {
                    Value::Message(g) => g
                        .get_field(&g.descriptor().get_field(1).expect("x"))
                        .into_owned()
                        .as_i32(),
                    _ => None,
                })
                .collect();
            assert_eq!(xs, vec![1, 2, 3]);
        }
        other => panic!("expected list of groups, got {other:?}"),
    }
}

#[test]
fn unknown_group_is_preserved_verbatim() {
    // Encode an `Outer` that HAS the group, then decode it with a schema whose
    // `Outer` has no field 2 — the group becomes an unknown field and must
    // survive a decode → encode round-trip byte-identically.
    let full = DescriptorPool::from_file_descriptor_set(outer_with_group(Label::Optional))
        .expect("full pool");
    let full_outer = full.get_message_by_name("Outer").expect("Outer");
    let group_desc = full.get_message_by_name("MyGroup").expect("MyGroup");
    let group_field = full_outer.get_field(2).expect("mygroup");
    let mut group = DynamicMessage::new(group_desc);
    group.set_field(&group.descriptor().get_field(1).expect("x"), Value::I32(5));
    let mut outer = DynamicMessage::new(full_outer.clone());
    outer.set_field(&full_outer.get_field(1).expect("lead"), Value::I32(1));
    outer.set_field(&group_field, Value::Message(Box::new(group)));
    let bytes = outer.encode_to_vec().expect("encode");

    // Slim schema: `Outer` keeps only field 1 (lead); field 2 is now unknown.
    let slim = DescriptorPool::from_file_descriptor_set(proto2_file(vec![DescriptorProto {
        name: Some("Outer".to_owned()),
        field: vec![field("lead", 1, Label::Optional, Type::Int32, None)],
        ..Default::default()
    }]))
    .expect("slim pool");
    let slim_outer = slim.get_message_by_name("Outer").expect("Outer");

    let decoded = DynamicMessage::decode(slim_outer, &bytes).expect("decode with unknown group");
    assert_eq!(
        decoded.unknown_fields().len(),
        1,
        "group preserved as unknown"
    );
    assert_eq!(
        decoded.encode_to_vec().expect("re-encode"),
        bytes,
        "unknown group must round-trip byte-identically"
    );
}

#[test]
fn unterminated_group_is_rejected() {
    // A start-group tag for field 1 with no matching end-group.
    let pool = DescriptorPool::from_file_descriptor_set(proto2_file(vec![DescriptorProto {
        name: Some("Empty".to_owned()),
        ..Default::default()
    }]))
    .expect("pool");
    let m = pool.get_message_by_name("Empty").expect("Empty");
    // tag(1, SGroup=3) = 0x0b.
    let err = DynamicMessage::decode(m, &[0x0b]).expect_err("unterminated group rejected");
    assert!(
        err.to_string().contains("group"),
        "error should mention groups: {err}"
    );
}

#[test]
fn stray_end_group_is_rejected() {
    let pool = DescriptorPool::from_file_descriptor_set(proto2_file(vec![DescriptorProto {
        name: Some("Empty".to_owned()),
        ..Default::default()
    }]))
    .expect("pool");
    let m = pool.get_message_by_name("Empty").expect("Empty");
    // tag(1, EGroup=4) = (1<<3)|4 = 0x0c — an end-group with no start.
    let err = DynamicMessage::decode(m, &[0x0c]).expect_err("stray end-group rejected");
    assert!(
        err.to_string().contains("end-group"),
        "error should mention end-group: {err}"
    );
}

#[test]
fn group_round_trips_through_json() {
    let pool =
        DescriptorPool::from_file_descriptor_set(outer_with_group(Label::Optional)).expect("pool");
    let outer_desc = pool.get_message_by_name("Outer").expect("Outer");
    let group_desc = pool.get_message_by_name("MyGroup").expect("MyGroup");
    let group_field = outer_desc.get_field(2).expect("mygroup");
    let mut group = DynamicMessage::new(group_desc);
    group.set_field(&group.descriptor().get_field(1).expect("x"), Value::I32(77));
    let mut outer = DynamicMessage::new(outer_desc.clone());
    outer.set_field(&group_field, Value::Message(Box::new(group)));

    let json = outer.to_json().expect("to_json");
    let back = DynamicMessage::from_json(outer_desc, &json).expect("from_json");
    let got = back
        .get_field(&back.descriptor().get_field(2).expect("mygroup"))
        .into_owned();
    match got {
        Value::Message(g) => assert_eq!(
            g.get_field(&g.descriptor().get_field(1).expect("x"))
                .into_owned()
                .as_i32(),
            Some(77)
        ),
        other => panic!("expected group message, got {other:?}"),
    }
}

#[test]
fn group_round_trips_through_text() {
    let pool =
        DescriptorPool::from_file_descriptor_set(outer_with_group(Label::Optional)).expect("pool");
    let outer_desc = pool.get_message_by_name("Outer").expect("Outer");
    let group_desc = pool.get_message_by_name("MyGroup").expect("MyGroup");
    let group_field = outer_desc.get_field(2).expect("mygroup");
    let mut group = DynamicMessage::new(group_desc);
    group.set_field(
        &group.descriptor().get_field(2).expect("s"),
        Value::String("txt".to_owned()),
    );
    let mut outer = DynamicMessage::new(outer_desc.clone());
    outer.set_field(&group_field, Value::Message(Box::new(group)));

    let text = outer.to_text().expect("to_text");
    let back = DynamicMessage::from_text(outer_desc, &text).expect("from_text");
    let got = back
        .get_field(&back.descriptor().get_field(2).expect("mygroup"))
        .into_owned();
    match got {
        Value::Message(g) => match g
            .get_field(&g.descriptor().get_field(2).expect("s"))
            .into_owned()
        {
            Value::String(s) => assert_eq!(s, "txt"),
            other => panic!("expected string, got {other:?}"),
        },
        other => panic!("expected group message, got {other:?}"),
    }
}
