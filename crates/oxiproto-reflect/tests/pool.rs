use oxiproto_reflect::{
    all_messages, all_services, dynamic_message, get_enum_by_name, get_service_by_name,
    pool_from_fds, pool_from_fds_bytes, ReflectError,
};
use prost::Message;
use prost_reflect::ReflectMessage;
use prost_types::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FileDescriptorProto,
    FileDescriptorSet, MethodDescriptorProto, ServiceDescriptorProto,
};

/// Build a minimal `FileDescriptorSet` bytes containing one message type.
fn make_fds_bytes(message_name: &str) -> Vec<u8> {
    let file = FileDescriptorProto {
        name: Some("test.proto".to_owned()),
        syntax: Some("proto3".to_owned()),
        message_type: vec![DescriptorProto {
            name: Some(message_name.to_owned()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let fds = FileDescriptorSet { file: vec![file] };
    fds.encode_to_vec()
}

/// Build a `FileDescriptorSet` with a message, a service, and an enum.
fn make_full_fds() -> FileDescriptorSet {
    let file = FileDescriptorProto {
        name: Some("full.proto".to_owned()),
        syntax: Some("proto3".to_owned()),
        package: Some("testpkg".to_owned()),
        message_type: vec![
            DescriptorProto {
                name: Some("RequestMsg".to_owned()),
                ..Default::default()
            },
            DescriptorProto {
                name: Some("ResponseMsg".to_owned()),
                ..Default::default()
            },
        ],
        service: vec![ServiceDescriptorProto {
            name: Some("MyService".to_owned()),
            method: vec![MethodDescriptorProto {
                name: Some("DoStuff".to_owned()),
                input_type: Some(".testpkg.RequestMsg".to_owned()),
                output_type: Some(".testpkg.ResponseMsg".to_owned()),
                ..Default::default()
            }],
            ..Default::default()
        }],
        enum_type: vec![EnumDescriptorProto {
            name: Some("MyEnum".to_owned()),
            value: vec![EnumValueDescriptorProto {
                name: Some("MY_ENUM_ZERO".to_owned()),
                number: Some(0),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    FileDescriptorSet { file: vec![file] }
}

// ---- existing tests (kept verbatim) ----------------------------------------

#[test]
fn pool_from_valid_fds_succeeds() {
    let bytes = make_fds_bytes("MyMessage");
    let pool = pool_from_fds_bytes(&bytes).expect("pool construction should succeed");
    assert!(pool.get_message_by_name("MyMessage").is_some());
}

#[test]
fn pool_from_empty_bytes_decodes_to_empty_pool() {
    let fds = FileDescriptorSet { file: vec![] };
    let bytes = fds.encode_to_vec();
    let pool = pool_from_fds_bytes(&bytes).expect("empty FDS should produce an empty pool");
    assert!(pool.get_message_by_name("AnyMessage").is_none());
}

#[test]
fn pool_from_garbage_bytes_returns_decode_error() {
    let bad_bytes = b"\xff\xfe\x00garbage";
    let result = pool_from_fds_bytes(bad_bytes);
    assert!(
        matches!(result, Err(ReflectError::Decode(_))),
        "expected Decode error, got: {result:?}"
    );
}

#[test]
fn dynamic_message_from_valid_pool_succeeds() {
    let bytes = make_fds_bytes("MyMessage");
    let pool = pool_from_fds_bytes(&bytes).expect("pool construction should succeed");
    let msg = dynamic_message(&pool, "MyMessage").expect("dynamic message should be constructed");
    assert_eq!(msg.descriptor().name(), "MyMessage");
}

#[test]
fn dynamic_message_missing_name_returns_not_found() {
    let bytes = make_fds_bytes("MyMessage");
    let pool = pool_from_fds_bytes(&bytes).expect("pool construction should succeed");
    let result = dynamic_message(&pool, "NoSuchMessage");
    assert!(
        matches!(result, Err(ReflectError::NotFound(ref name)) if name == "NoSuchMessage"),
        "expected NotFound error, got: {result:?}"
    );
}

// ---- new tests for Slice R -------------------------------------------------

#[test]
fn pool_from_fds_matches_pool_from_fds_bytes() {
    let fds = make_full_fds();
    let bytes = fds.encode_to_vec();

    let pool_bytes = pool_from_fds_bytes(&bytes).expect("pool_from_fds_bytes should succeed");
    let pool_struct = pool_from_fds(fds).expect("pool_from_fds should succeed");

    // Both pools should contain the same message names.
    let names_bytes: std::collections::BTreeSet<String> = pool_bytes
        .all_messages()
        .map(|m| m.full_name().to_owned())
        .collect();
    let names_struct: std::collections::BTreeSet<String> = pool_struct
        .all_messages()
        .map(|m| m.full_name().to_owned())
        .collect();
    assert_eq!(names_bytes, names_struct);
}

#[test]
fn get_service_by_name_returns_some_for_known_service() {
    let pool = pool_from_fds(make_full_fds()).expect("pool should build");
    let svc = get_service_by_name(&pool, "testpkg.MyService");
    assert!(svc.is_some(), "expected Some for testpkg.MyService");
    assert_eq!(svc.unwrap().name(), "MyService");
}

#[test]
fn get_service_by_name_returns_none_for_unknown_service() {
    let pool = pool_from_fds(make_full_fds()).expect("pool should build");
    let svc = get_service_by_name(&pool, "testpkg.NoSuchService");
    assert!(svc.is_none(), "expected None for unknown service");
}

#[test]
fn get_enum_by_name_returns_some_for_known_enum() {
    let pool = pool_from_fds(make_full_fds()).expect("pool should build");
    let enm = get_enum_by_name(&pool, "testpkg.MyEnum");
    assert!(enm.is_some(), "expected Some for testpkg.MyEnum");
    assert_eq!(enm.unwrap().name(), "MyEnum");
}

#[test]
fn get_enum_by_name_returns_none_for_unknown_enum() {
    let pool = pool_from_fds(make_full_fds()).expect("pool should build");
    let enm = get_enum_by_name(&pool, "testpkg.NoSuchEnum");
    assert!(enm.is_none(), "expected None for unknown enum");
}

#[test]
fn all_messages_returns_expected_descriptors() {
    let pool = pool_from_fds(make_full_fds()).expect("pool should build");
    let names: std::collections::BTreeSet<String> = all_messages(&pool)
        .map(|m| m.full_name().to_owned())
        .collect();
    assert!(names.contains("testpkg.RequestMsg"), "missing RequestMsg");
    assert!(names.contains("testpkg.ResponseMsg"), "missing ResponseMsg");
}

#[test]
fn all_services_returns_expected_descriptors() {
    let pool = pool_from_fds(make_full_fds()).expect("pool should build");
    let names: std::collections::BTreeSet<String> = all_services(&pool)
        .map(|s| s.full_name().to_owned())
        .collect();
    assert!(names.contains("testpkg.MyService"), "missing MyService");
}

#[test]
fn reflect_error_field_variant_display() {
    let err = ReflectError::Field("type mismatch for field 'count': expected i32".to_owned());
    let msg = err.to_string();
    assert!(
        msg.contains("type mismatch"),
        "Display for Field variant should contain message body; got: {msg}"
    );
    assert!(
        msg.contains("field error"),
        "Display for Field variant should start with 'field error'; got: {msg}"
    );
}

// ---- multi-file FDS with imports --------------------------------------------

#[test]
fn pool_from_multi_file_fds_with_imports() {
    use prost_types::field_descriptor_proto::{Label, Type};
    use prost_types::{
        DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
        FileDescriptorProto, FileDescriptorSet,
    };

    // File A (events.proto): defines enum "Status" in package "events".
    // File B (request.proto): defines message "Request" in package "api",
    //   importing events.proto and referencing events.Status.
    // events.proto must appear before request.proto in the file list so that
    // prost-reflect resolves the import during construction.
    let fds = FileDescriptorSet {
        file: vec![
            FileDescriptorProto {
                name: Some("events.proto".to_string()),
                syntax: Some("proto3".to_string()),
                package: Some("events".to_string()),
                enum_type: vec![EnumDescriptorProto {
                    name: Some("Status".to_string()),
                    value: vec![
                        EnumValueDescriptorProto {
                            name: Some("STATUS_UNKNOWN".to_string()),
                            number: Some(0),
                            ..Default::default()
                        },
                        EnumValueDescriptorProto {
                            name: Some("STATUS_OK".to_string()),
                            number: Some(1),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }],
                ..Default::default()
            },
            FileDescriptorProto {
                name: Some("request.proto".to_string()),
                syntax: Some("proto3".to_string()),
                package: Some("api".to_string()),
                dependency: vec!["events.proto".to_string()],
                message_type: vec![DescriptorProto {
                    name: Some("Request".to_string()),
                    field: vec![FieldDescriptorProto {
                        name: Some("status".to_string()),
                        number: Some(1),
                        label: Some(Label::Optional as i32),
                        r#type: Some(Type::Enum as i32),
                        type_name: Some(".events.Status".to_string()),
                        json_name: Some("status".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
    };

    let pool = pool_from_fds(fds).expect("pool creation from multi-file FDS");

    // Both descriptors must be resolvable by their fully-qualified names.
    assert!(
        pool.get_message_by_name("api.Request").is_some(),
        "api.Request message not found in pool"
    );
    assert!(
        pool.get_enum_by_name("events.Status").is_some(),
        "events.Status enum not found in pool"
    );

    // The message field must reference the correct enum type.
    let msg_desc = pool.get_message_by_name("api.Request").unwrap();
    let field = msg_desc
        .get_field_by_name("status")
        .expect("status field not found on api.Request");
    assert_eq!(
        field.kind().as_enum().map(|e| e.full_name()),
        Some("events.Status"),
        "status field should reference events.Status enum"
    );
}
