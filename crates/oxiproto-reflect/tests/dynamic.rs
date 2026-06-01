use oxiproto_reflect::{
    clear_field, dynamic_message, get_field_by_name, has_field, pool_from_fds, set_field_by_name,
    DynamicMessage, ReflectError, ReflectValue,
};
use prost_reflect::MapKey;
use prost_types::{
    field_descriptor_proto::{Label, Type},
    DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet, MessageOptions,
    OneofDescriptorProto,
};

/// Build a `FileDescriptorSet` with a single message that has three scalar fields:
///   - `name` (string, field number 1)
///   - `count` (int32, field number 2)
///   - `active` (bool, field number 3)
fn make_scalar_fds() -> FileDescriptorSet {
    let file = FileDescriptorProto {
        name: Some("scalars.proto".to_owned()),
        syntax: Some("proto3".to_owned()),
        message_type: vec![DescriptorProto {
            name: Some("ScalarMsg".to_owned()),
            field: vec![
                FieldDescriptorProto {
                    name: Some("name".to_owned()),
                    number: Some(1),
                    label: Some(Label::Optional as i32),
                    r#type: Some(Type::String as i32),
                    json_name: Some("name".to_owned()),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("count".to_owned()),
                    number: Some(2),
                    label: Some(Label::Optional as i32),
                    r#type: Some(Type::Int32 as i32),
                    json_name: Some("count".to_owned()),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("active".to_owned()),
                    number: Some(3),
                    label: Some(Label::Optional as i32),
                    r#type: Some(Type::Bool as i32),
                    json_name: Some("active".to_owned()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    FileDescriptorSet { file: vec![file] }
}

// ---- set / get round-trips --------------------------------------------------

#[test]
fn set_and_get_string_field_round_trip() {
    let pool = pool_from_fds(make_scalar_fds()).expect("pool should build");
    let mut msg = dynamic_message(&pool, "ScalarMsg").expect("message should exist");

    set_field_by_name(&mut msg, "name", ReflectValue::String("hello".to_owned()))
        .expect("set_field_by_name should succeed");

    let value = get_field_by_name(&msg, "name")
        .expect("get_field_by_name should not error")
        .expect("field should be set");

    assert_eq!(value, ReflectValue::String("hello".to_owned()));
}

#[test]
fn set_and_get_int32_field_round_trip() {
    let pool = pool_from_fds(make_scalar_fds()).expect("pool should build");
    let mut msg = dynamic_message(&pool, "ScalarMsg").expect("message should exist");

    set_field_by_name(&mut msg, "count", ReflectValue::I32(42))
        .expect("set_field_by_name should succeed");

    let value = get_field_by_name(&msg, "count")
        .expect("get_field_by_name should not error")
        .expect("field should be set");

    assert_eq!(value, ReflectValue::I32(42));
}

#[test]
fn set_and_get_bool_field_round_trip() {
    let pool = pool_from_fds(make_scalar_fds()).expect("pool should build");
    let mut msg = dynamic_message(&pool, "ScalarMsg").expect("message should exist");

    set_field_by_name(&mut msg, "active", ReflectValue::Bool(true))
        .expect("set_field_by_name should succeed");

    let value = get_field_by_name(&msg, "active")
        .expect("get_field_by_name should not error")
        .expect("field should be set");

    assert_eq!(value, ReflectValue::Bool(true));
}

// ---- has_field --------------------------------------------------------------

#[test]
fn has_field_false_before_set() {
    let pool = pool_from_fds(make_scalar_fds()).expect("pool should build");
    let msg = dynamic_message(&pool, "ScalarMsg").expect("message should exist");

    // Proto3 scalar: unset means default (zero/empty), so has_field is false.
    let set = has_field(&msg, "count").expect("has_field should not error");
    assert!(!set, "count should not be set on a fresh message");
}

#[test]
fn has_field_true_after_nonzero_set() {
    let pool = pool_from_fds(make_scalar_fds()).expect("pool should build");
    let mut msg = dynamic_message(&pool, "ScalarMsg").expect("message should exist");

    set_field_by_name(&mut msg, "count", ReflectValue::I32(7)).expect("set should succeed");

    let set = has_field(&msg, "count").expect("has_field should not error");
    assert!(set, "count should be set after assigning 7");
}

// ---- clear_field ------------------------------------------------------------

#[test]
fn clear_field_resets_to_default() {
    let pool = pool_from_fds(make_scalar_fds()).expect("pool should build");
    let mut msg = dynamic_message(&pool, "ScalarMsg").expect("message should exist");

    set_field_by_name(&mut msg, "count", ReflectValue::I32(99)).expect("set should succeed");
    assert!(
        has_field(&msg, "count").expect("has_field ok"),
        "should be set"
    );

    clear_field(&mut msg, "count").expect("clear_field should succeed");
    let set = has_field(&msg, "count").expect("has_field after clear");
    assert!(!set, "count should not be set after clear_field");
}

// ---- error on unknown field names ------------------------------------------

#[test]
fn set_field_by_name_unknown_field_returns_not_found() {
    let pool = pool_from_fds(make_scalar_fds()).expect("pool should build");
    let mut msg = dynamic_message(&pool, "ScalarMsg").expect("message should exist");

    let result = set_field_by_name(&mut msg, "nonexistent", ReflectValue::I32(1));
    assert!(
        matches!(result, Err(ReflectError::NotFound(ref n)) if n == "nonexistent"),
        "expected NotFound for unknown field; got: {result:?}"
    );
}

#[test]
fn get_field_by_name_unknown_field_returns_not_found() {
    let pool = pool_from_fds(make_scalar_fds()).expect("pool should build");
    let msg = dynamic_message(&pool, "ScalarMsg").expect("message should exist");

    let result = get_field_by_name(&msg, "nonexistent");
    assert!(
        matches!(result, Err(ReflectError::NotFound(ref n)) if n == "nonexistent"),
        "expected NotFound for unknown field; got: {result:?}"
    );
}

#[test]
fn has_field_unknown_field_returns_not_found() {
    let pool = pool_from_fds(make_scalar_fds()).expect("pool should build");
    let msg = dynamic_message(&pool, "ScalarMsg").expect("message should exist");

    let result = has_field(&msg, "nonexistent");
    assert!(
        matches!(result, Err(ReflectError::NotFound(ref n)) if n == "nonexistent"),
        "expected NotFound for unknown field; got: {result:?}"
    );
}

#[test]
fn clear_field_unknown_field_returns_not_found() {
    let pool = pool_from_fds(make_scalar_fds()).expect("pool should build");
    let mut msg = dynamic_message(&pool, "ScalarMsg").expect("message should exist");

    let result = clear_field(&mut msg, "nonexistent");
    assert!(
        matches!(result, Err(ReflectError::NotFound(ref n)) if n == "nonexistent"),
        "expected NotFound for unknown field; got: {result:?}"
    );
}

// ---- get_field_by_name returns None for unset field -------------------------

#[test]
fn get_field_by_name_returns_none_for_unset_field() {
    let pool = pool_from_fds(make_scalar_fds()).expect("pool should build");
    let msg = dynamic_message(&pool, "ScalarMsg").expect("message should exist");

    // String field starts at default ""; proto3 scalar, so has_field is false.
    let result = get_field_by_name(&msg, "name")
        .expect("get_field_by_name should not error for existing field");
    assert!(
        result.is_none(),
        "expected None for unset string field; got: {result:?}"
    );
}

// ---- repeated field ---------------------------------------------------------

/// Build a `FileDescriptorSet` with a message that has a `repeated int32 values = 1` field.
fn make_repeated_fds() -> FileDescriptorSet {
    let file = FileDescriptorProto {
        name: Some("repeated.proto".to_owned()),
        syntax: Some("proto3".to_owned()),
        message_type: vec![DescriptorProto {
            name: Some("RepeatedMsg".to_owned()),
            field: vec![FieldDescriptorProto {
                name: Some("values".to_owned()),
                number: Some(1),
                label: Some(Label::Repeated as i32),
                r#type: Some(Type::Int32 as i32),
                json_name: Some("values".to_owned()),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    FileDescriptorSet { file: vec![file] }
}

#[test]
fn dynamic_message_repeated_field() {
    let pool = pool_from_fds(make_repeated_fds()).expect("pool should build");
    let mut msg = dynamic_message(&pool, "RepeatedMsg").expect("message should exist");

    let items = vec![
        ReflectValue::I32(10),
        ReflectValue::I32(20),
        ReflectValue::I32(30),
    ];
    set_field_by_name(&mut msg, "values", ReflectValue::List(items.clone()))
        .expect("set_field_by_name for repeated field should succeed");

    let got = get_field_by_name(&msg, "values")
        .expect("get_field_by_name should not error")
        .expect("repeated field should be set after assignment");

    let list = got
        .as_list()
        .expect("repeated field value should be a List");
    assert_eq!(list.len(), 3, "expected 3 elements");
    assert_eq!(list[0], ReflectValue::I32(10));
    assert_eq!(list[1], ReflectValue::I32(20));
    assert_eq!(list[2], ReflectValue::I32(30));
}

// ---- map field --------------------------------------------------------------

/// Build a `FileDescriptorSet` with a message containing `map<string, int32> scores = 1`.
///
/// In the protobuf wire format, a map field is represented as a repeated field
/// whose element type is a synthetic map-entry message.  We must construct the
/// map-entry nested message explicitly when building a `FileDescriptorSet` from
/// scratch (protoc normally does this automatically).
///
/// The entry message must have `MessageOptions.map_entry = true` and contain
/// exactly two fields: key (field 1) and value (field 2).
fn make_map_fds() -> FileDescriptorSet {
    // Synthetic map-entry message: message ScoresEntry { string key = 1; int32 value = 2; }
    // with options { map_entry = true }
    let map_entry = DescriptorProto {
        name: Some("ScoresEntry".to_owned()),
        options: Some(MessageOptions {
            map_entry: Some(true),
            ..Default::default()
        }),
        field: vec![
            FieldDescriptorProto {
                name: Some("key".to_owned()),
                number: Some(1),
                label: Some(Label::Optional as i32),
                r#type: Some(Type::String as i32),
                json_name: Some("key".to_owned()),
                ..Default::default()
            },
            FieldDescriptorProto {
                name: Some("value".to_owned()),
                number: Some(2),
                label: Some(Label::Optional as i32),
                r#type: Some(Type::Int32 as i32),
                json_name: Some("value".to_owned()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    // The outer message has the map field as a repeated message field pointing
    // at the synthetic entry type.
    let file = FileDescriptorProto {
        name: Some("map.proto".to_owned()),
        syntax: Some("proto3".to_owned()),
        message_type: vec![DescriptorProto {
            name: Some("MapMsg".to_owned()),
            nested_type: vec![map_entry],
            field: vec![FieldDescriptorProto {
                name: Some("scores".to_owned()),
                number: Some(1),
                label: Some(Label::Repeated as i32),
                r#type: Some(Type::Message as i32),
                type_name: Some(".MapMsg.ScoresEntry".to_owned()),
                json_name: Some("scores".to_owned()),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    FileDescriptorSet { file: vec![file] }
}

#[test]
fn dynamic_message_map_field() {
    let pool = pool_from_fds(make_map_fds()).expect("pool should build");
    let mut msg = dynamic_message(&pool, "MapMsg").expect("message should exist");

    // Build a Value::Map with two entries.
    let mut map = std::collections::HashMap::new();
    map.insert(MapKey::String("alpha".to_owned()), ReflectValue::I32(1));
    map.insert(MapKey::String("beta".to_owned()), ReflectValue::I32(2));

    set_field_by_name(&mut msg, "scores", ReflectValue::Map(map))
        .expect("set_field_by_name for map field should succeed");

    let got = get_field_by_name(&msg, "scores")
        .expect("get_field_by_name should not error")
        .expect("map field should be set after assignment");

    let map = got.as_map().expect("map field value should be a Map");
    assert_eq!(map.len(), 2, "expected 2 map entries");
    assert_eq!(
        map.get(&MapKey::String("alpha".to_owned())),
        Some(&ReflectValue::I32(1))
    );
    assert_eq!(
        map.get(&MapKey::String("beta".to_owned())),
        Some(&ReflectValue::I32(2))
    );
}

// ---- oneof field ------------------------------------------------------------

/// Build a `FileDescriptorSet` with a message that has a oneof with two alternatives:
///   oneof payload { int32 int_val = 1; string str_val = 2; }
fn make_oneof_fds() -> FileDescriptorSet {
    let file = FileDescriptorProto {
        name: Some("oneof.proto".to_owned()),
        syntax: Some("proto3".to_owned()),
        message_type: vec![DescriptorProto {
            name: Some("OneofMsg".to_owned()),
            oneof_decl: vec![OneofDescriptorProto {
                name: Some("payload".to_owned()),
                ..Default::default()
            }],
            field: vec![
                FieldDescriptorProto {
                    name: Some("int_val".to_owned()),
                    number: Some(1),
                    label: Some(Label::Optional as i32),
                    r#type: Some(Type::Int32 as i32),
                    // oneof_index = 0 links this field to the first oneof_decl.
                    oneof_index: Some(0),
                    json_name: Some("intVal".to_owned()),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("str_val".to_owned()),
                    number: Some(2),
                    label: Some(Label::Optional as i32),
                    r#type: Some(Type::String as i32),
                    oneof_index: Some(0),
                    json_name: Some("strVal".to_owned()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    FileDescriptorSet { file: vec![file] }
}

#[test]
fn dynamic_message_oneof_field() {
    let pool = pool_from_fds(make_oneof_fds()).expect("pool should build");
    let mut msg: DynamicMessage = dynamic_message(&pool, "OneofMsg").expect("message should exist");

    // Set the int_val arm and confirm str_val is absent.
    set_field_by_name(&mut msg, "int_val", ReflectValue::I32(42))
        .expect("setting int_val should succeed");

    assert!(
        has_field(&msg, "int_val").expect("has_field int_val"),
        "int_val should be set"
    );
    assert!(
        !has_field(&msg, "str_val").expect("has_field str_val"),
        "str_val should be absent when int_val is set (oneof semantics)"
    );

    // Switch to str_val; prost-reflect clears the previously-set arm.
    set_field_by_name(
        &mut msg,
        "str_val",
        ReflectValue::String("hello".to_owned()),
    )
    .expect("setting str_val should succeed");

    assert!(
        has_field(&msg, "str_val").expect("has_field str_val after set"),
        "str_val should now be set"
    );
    assert!(
        !has_field(&msg, "int_val").expect("has_field int_val after str_val set"),
        "int_val should be cleared when str_val is set (oneof semantics)"
    );
}
