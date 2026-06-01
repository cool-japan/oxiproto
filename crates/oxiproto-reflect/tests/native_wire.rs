//! Tests for the native reflection path: descriptor pool, dynamic message
//! field access, and byte-exact protobuf wire encode/decode.
//!
//! The primary oracle is byte-exact wire vectors derived from the protobuf
//! specification's canonical examples (e.g. `int32 a = 1; a = 150` encodes to
//! `08 96 01`). Round-trip and descriptor-lookup tests complement them.

use std::collections::HashMap;

use oxiproto_reflect::native::{Cardinality, DescriptorPool, DynamicMessage, Kind, MapKey, Value};
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FieldOptions, FileDescriptorProto, FileDescriptorSet, MessageOptions, MethodDescriptorProto,
    OneofDescriptorProto, ServiceDescriptorProto,
};

// ---------------------------------------------------------------------------
// FDS construction helpers
// ---------------------------------------------------------------------------

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

fn proto3_file(messages: Vec<DescriptorProto>) -> FileDescriptorSet {
    proto3_file_pkg("", messages, Vec::new(), Vec::new())
}

fn proto3_file_pkg(
    package: &str,
    messages: Vec<DescriptorProto>,
    enums: Vec<EnumDescriptorProto>,
    services: Vec<ServiceDescriptorProto>,
) -> FileDescriptorSet {
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("test.proto".to_owned()),
            syntax: Some("proto3".to_owned()),
            package: if package.is_empty() {
                None
            } else {
                Some(package.to_owned())
            },
            message_type: messages,
            enum_type: enums,
            service: services,
            ..Default::default()
        }],
    }
}

/// `message M { int32 a = 1; }`
fn single_int32_pool() -> DescriptorPool {
    let fds = proto3_file(vec![DescriptorProto {
        name: Some("M".to_owned()),
        field: vec![field("a", 1, Label::Optional, Type::Int32, None)],
        ..Default::default()
    }]);
    DescriptorPool::from_file_descriptor_set(fds).expect("pool builds")
}

// ---------------------------------------------------------------------------
// Byte-exact wire vectors (the real oracle)
// ---------------------------------------------------------------------------

#[test]
fn byte_exact_int32_150() {
    let pool = single_int32_pool();
    let m = pool.get_message_by_name("M").expect("M exists");
    let f = m.get_field(1).expect("field 1");

    let mut msg = DynamicMessage::new(m.clone());
    msg.set_field(&f, Value::I32(150));

    // Canonical protobuf example: { a: 150 } -> 08 96 01
    let bytes = msg.encode_to_vec().expect("encode");
    assert_eq!(bytes, vec![0x08, 0x96, 0x01]);

    // Decode recovers the value.
    let decoded = DynamicMessage::decode(m, &bytes).expect("decode");
    assert_eq!(decoded.get_field(&f).into_owned(), Value::I32(150));
}

#[test]
fn byte_exact_string_field() {
    let fds = proto3_file(vec![DescriptorProto {
        name: Some("S".to_owned()),
        field: vec![field("s", 2, Label::Optional, Type::String, None)],
        ..Default::default()
    }]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let m = pool.get_message_by_name("S").expect("S");
    let f = m.get_field(2).expect("field 2");

    let mut msg = DynamicMessage::new(m.clone());
    msg.set_field(&f, Value::String("testing".to_owned()));

    // tag(2,LEN)=0x12, len=7, "testing"
    let bytes = msg.encode_to_vec().expect("encode");
    assert_eq!(
        bytes,
        vec![0x12, 0x07, 0x74, 0x65, 0x73, 0x74, 0x69, 0x6e, 0x67]
    );

    let decoded = DynamicMessage::decode(m, &bytes).expect("decode");
    assert_eq!(
        decoded.get_field(&f).into_owned(),
        Value::String("testing".to_owned())
    );
}

#[test]
fn byte_exact_packed_repeated_int32() {
    // proto3 repeated scalar is packed by default.
    let fds = proto3_file(vec![DescriptorProto {
        name: Some("R".to_owned()),
        field: vec![field("vals", 4, Label::Repeated, Type::Int32, None)],
        ..Default::default()
    }]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let m = pool.get_message_by_name("R").expect("R");
    let f = m.get_field(4).expect("field 4");
    assert!(f.is_packed(), "proto3 repeated int32 should be packed");
    assert!(f.is_list());

    let mut msg = DynamicMessage::new(m.clone());
    msg.set_field(
        &f,
        Value::List(vec![Value::I32(1), Value::I32(2), Value::I32(3)]),
    );

    // tag(4,LEN)=0x22, len=3, [01 02 03]
    let bytes = msg.encode_to_vec().expect("encode");
    assert_eq!(bytes, vec![0x22, 0x03, 0x01, 0x02, 0x03]);

    let decoded = DynamicMessage::decode(m, &bytes).expect("decode");
    assert_eq!(
        decoded.get_field(&f).into_owned(),
        Value::List(vec![Value::I32(1), Value::I32(2), Value::I32(3)])
    );
}

#[test]
fn byte_exact_packed_repeated_canonical_doc_example() {
    // From protobuf docs: repeated int32 f = 6 = [3, 270, 86942]
    // -> 32 06 03 8e 02 9e a7 05
    let fds = proto3_file(vec![DescriptorProto {
        name: Some("R".to_owned()),
        field: vec![field("f", 6, Label::Repeated, Type::Int32, None)],
        ..Default::default()
    }]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let m = pool.get_message_by_name("R").expect("R");
    let f = m.get_field(6).expect("field 6");

    let mut msg = DynamicMessage::new(m.clone());
    msg.set_field(
        &f,
        Value::List(vec![Value::I32(3), Value::I32(270), Value::I32(86942)]),
    );

    let bytes = msg.encode_to_vec().expect("encode");
    assert_eq!(bytes, vec![0x32, 0x06, 0x03, 0x8e, 0x02, 0x9e, 0xa7, 0x05]);

    let decoded = DynamicMessage::decode(m, &bytes).expect("decode");
    assert_eq!(
        decoded.get_field(&f).into_owned(),
        Value::List(vec![Value::I32(3), Value::I32(270), Value::I32(86942)])
    );
}

#[test]
fn byte_exact_nested_message() {
    // message Inner { int32 a = 1; }
    // message Outer { Inner inner = 3; }
    let fds = proto3_file(vec![
        DescriptorProto {
            name: Some("Inner".to_owned()),
            field: vec![field("a", 1, Label::Optional, Type::Int32, None)],
            ..Default::default()
        },
        DescriptorProto {
            name: Some("Outer".to_owned()),
            field: vec![field(
                "inner",
                3,
                Label::Optional,
                Type::Message,
                Some(".Inner"),
            )],
            ..Default::default()
        },
    ]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let inner_desc = pool.get_message_by_name("Inner").expect("Inner");
    let outer_desc = pool.get_message_by_name("Outer").expect("Outer");
    let inner_a = inner_desc.get_field(1).expect("Inner.a");
    let outer_inner = outer_desc.get_field(3).expect("Outer.inner");

    let mut inner = DynamicMessage::new(inner_desc);
    inner.set_field(&inner_a, Value::I32(150));

    let mut outer = DynamicMessage::new(outer_desc.clone());
    outer.set_field(&outer_inner, Value::Message(Box::new(inner)));

    // tag(3,LEN)=0x1a, len=3, [08 96 01]
    let bytes = outer.encode_to_vec().expect("encode");
    assert_eq!(bytes, vec![0x1a, 0x03, 0x08, 0x96, 0x01]);

    let decoded = DynamicMessage::decode(outer_desc, &bytes).expect("decode");
    let got = decoded.get_field(&outer_inner).into_owned();
    match got {
        Value::Message(inner_msg) => {
            let a = inner_msg.descriptor().get_field(1).expect("a");
            assert_eq!(inner_msg.get_field(&a).into_owned(), Value::I32(150));
        }
        other => panic!("expected message, got {other:?}"),
    }
}

#[test]
fn byte_exact_map_string_int32() {
    let pool = map_pool();
    let m = pool.get_message_by_name("WithMap").expect("WithMap");
    let f = m.get_field(7).expect("map field 7");
    assert!(f.is_map());

    let mut msg = DynamicMessage::new(m.clone());
    let mut map = HashMap::new();
    map.insert(MapKey::String("k".to_owned()), Value::I32(5));
    msg.set_field(&f, Value::Map(map));

    // tag(7,LEN)=0x3a, len=5, entry = [tag(1,LEN)=0a len1 'k' tag(2,VARINT)=10 05]
    let bytes = msg.encode_to_vec().expect("encode");
    assert_eq!(bytes, vec![0x3a, 0x05, 0x0a, 0x01, 0x6b, 0x10, 0x05]);

    let decoded = DynamicMessage::decode(m, &bytes).expect("decode");
    let got = decoded.get_field(&f).into_owned();
    match got {
        Value::Map(map) => {
            assert_eq!(map.len(), 1);
            assert_eq!(
                map.get(&MapKey::String("k".to_owned())),
                Some(&Value::I32(5))
            );
        }
        other => panic!("expected map, got {other:?}"),
    }
}

/// `message WithMap { map<string, int32> m = 7; }` — requires a synthetic
/// `MapEntry` nested message with `options.map_entry = true`.
fn map_pool() -> DescriptorPool {
    let entry = DescriptorProto {
        name: Some("MEntry".to_owned()),
        field: vec![
            field("key", 1, Label::Optional, Type::String, None),
            field("value", 2, Label::Optional, Type::Int32, None),
        ],
        options: Some(MessageOptions {
            map_entry: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };
    let with_map = DescriptorProto {
        name: Some("WithMap".to_owned()),
        field: vec![field(
            "m",
            7,
            Label::Repeated,
            Type::Message,
            Some(".WithMap.MEntry"),
        )],
        nested_type: vec![entry],
        ..Default::default()
    };
    let fds = proto3_file(vec![with_map]);
    DescriptorPool::from_file_descriptor_set(fds).expect("map pool builds")
}

// ---------------------------------------------------------------------------
// Decode accepts unpacked repeated scalars (compat with proto2 / older encoders)
// ---------------------------------------------------------------------------

#[test]
fn decode_accepts_unpacked_repeated() {
    let fds = proto3_file(vec![DescriptorProto {
        name: Some("R".to_owned()),
        field: vec![field("vals", 4, Label::Repeated, Type::Int32, None)],
        ..Default::default()
    }]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let m = pool.get_message_by_name("R").expect("R");
    let f = m.get_field(4).expect("field 4");

    // Unpacked wire bytes: tag(4,VARINT)=0x20 per element.
    let unpacked = vec![0x20, 0x01, 0x20, 0x02, 0x20, 0x03];
    let decoded = DynamicMessage::decode(m, &unpacked).expect("decode");
    assert_eq!(
        decoded.get_field(&f).into_owned(),
        Value::List(vec![Value::I32(1), Value::I32(2), Value::I32(3)])
    );
}

// ---------------------------------------------------------------------------
// Round-trip: scalars / sint zigzag / fixed / enum
// ---------------------------------------------------------------------------

#[test]
fn round_trip_all_scalar_kinds() {
    // One message with every scalar type, each at a non-default value.
    let fds = proto3_file(vec![DescriptorProto {
        name: Some("All".to_owned()),
        field: vec![
            field("f_double", 1, Label::Optional, Type::Double, None),
            field("f_float", 2, Label::Optional, Type::Float, None),
            field("f_int32", 3, Label::Optional, Type::Int32, None),
            field("f_int64", 4, Label::Optional, Type::Int64, None),
            field("f_uint32", 5, Label::Optional, Type::Uint32, None),
            field("f_uint64", 6, Label::Optional, Type::Uint64, None),
            field("f_sint32", 7, Label::Optional, Type::Sint32, None),
            field("f_sint64", 8, Label::Optional, Type::Sint64, None),
            field("f_fixed32", 9, Label::Optional, Type::Fixed32, None),
            field("f_fixed64", 10, Label::Optional, Type::Fixed64, None),
            field("f_sfixed32", 11, Label::Optional, Type::Sfixed32, None),
            field("f_sfixed64", 12, Label::Optional, Type::Sfixed64, None),
            field("f_bool", 13, Label::Optional, Type::Bool, None),
            field("f_string", 14, Label::Optional, Type::String, None),
            field("f_bytes", 15, Label::Optional, Type::Bytes, None),
        ],
        ..Default::default()
    }]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let m = pool.get_message_by_name("All").expect("All");

    let values: Vec<(u32, Value)> = vec![
        (1, Value::F64(3.5)),
        (2, Value::F32(-1.25)),
        (3, Value::I32(-7)),
        (4, Value::I64(-9_000_000_000)),
        (5, Value::U32(42)),
        (6, Value::U64(18_000_000_000)),
        (7, Value::I32(-123_456)),
        (8, Value::I64(-987_654_321)),
        (9, Value::U32(0xDEAD_BEEF)),
        (10, Value::U64(0xCAFE_BABE_DEAD_BEEF)),
        (11, Value::I32(-2_000_000)),
        (12, Value::I64(-5_000_000_000)),
        (13, Value::Bool(true)),
        (14, Value::String("héllo".to_owned())),
        (15, Value::Bytes(vec![0, 1, 2, 254, 255])),
    ];

    let mut msg = DynamicMessage::new(m.clone());
    for (num, val) in &values {
        let f = m.get_field(*num).expect("field");
        msg.set_field(&f, val.clone());
    }

    let bytes = msg.encode_to_vec().expect("encode");
    let decoded = DynamicMessage::decode(m.clone(), &bytes).expect("decode");

    for (num, val) in &values {
        let f = m.get_field(*num).expect("field");
        assert_eq!(&decoded.get_field(&f).into_owned(), val, "field {num}");
    }
}

#[test]
fn round_trip_enum_field() {
    let en = EnumDescriptorProto {
        name: Some("Color".to_owned()),
        value: vec![
            EnumValueDescriptorProto {
                name: Some("RED".to_owned()),
                number: Some(0),
                ..Default::default()
            },
            EnumValueDescriptorProto {
                name: Some("GREEN".to_owned()),
                number: Some(1),
                ..Default::default()
            },
            EnumValueDescriptorProto {
                name: Some("BLUE".to_owned()),
                number: Some(2),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let msg_proto = DescriptorProto {
        name: Some("Paint".to_owned()),
        field: vec![field(
            "color",
            1,
            Label::Optional,
            Type::Enum,
            Some(".Color"),
        )],
        ..Default::default()
    };
    let fds = proto3_file_pkg("", vec![msg_proto], vec![en], Vec::new());
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let m = pool.get_message_by_name("Paint").expect("Paint");
    let f = m.get_field(1).expect("color");

    assert!(matches!(f.kind(), Kind::Enum(_)));
    let enum_desc = f.enum_type().expect("enum type");
    assert_eq!(enum_desc.full_name(), "Color");
    assert_eq!(enum_desc.get_value(2).expect("BLUE").name(), "BLUE");
    assert_eq!(
        enum_desc
            .get_value_by_name("GREEN")
            .expect("GREEN")
            .number(),
        1
    );

    let mut msg = DynamicMessage::new(m.clone());
    msg.set_field(&f, Value::EnumNumber(2));
    // tag(1,VARINT)=08, value 2
    let bytes = msg.encode_to_vec().expect("encode");
    assert_eq!(bytes, vec![0x08, 0x02]);

    let decoded = DynamicMessage::decode(m, &bytes).expect("decode");
    assert_eq!(decoded.get_field(&f).into_owned(), Value::EnumNumber(2));
}

#[test]
fn round_trip_unpacked_repeated_when_packed_disabled() {
    // Explicitly disable packing; encoding must then be unpacked.
    let mut f = field("vals", 4, Label::Repeated, Type::Int32, None);
    f.options = Some(FieldOptions {
        packed: Some(false),
        ..Default::default()
    });
    let fds = proto3_file(vec![DescriptorProto {
        name: Some("R".to_owned()),
        field: vec![f],
        ..Default::default()
    }]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let m = pool.get_message_by_name("R").expect("R");
    let fd = m.get_field(4).expect("field 4");
    assert!(!fd.is_packed());

    let mut msg = DynamicMessage::new(m.clone());
    msg.set_field(
        &fd,
        Value::List(vec![Value::I32(1), Value::I32(2), Value::I32(3)]),
    );
    // Unpacked: tag(4,VARINT)=0x20 repeated.
    let bytes = msg.encode_to_vec().expect("encode");
    assert_eq!(bytes, vec![0x20, 0x01, 0x20, 0x02, 0x20, 0x03]);

    let decoded = DynamicMessage::decode(m, &bytes).expect("decode");
    assert_eq!(
        decoded.get_field(&fd).into_owned(),
        Value::List(vec![Value::I32(1), Value::I32(2), Value::I32(3)])
    );
}

// ---------------------------------------------------------------------------
// proto3 default omission
// ---------------------------------------------------------------------------

#[test]
fn proto3_default_values_are_omitted() {
    let pool = single_int32_pool();
    let m = pool.get_message_by_name("M").expect("M");
    let f = m.get_field(1).expect("field 1");

    // Setting the field to its default (0) must produce empty output.
    let mut msg = DynamicMessage::new(m.clone());
    msg.set_field(&f, Value::I32(0));
    assert!(msg.encode_to_vec().expect("encode").is_empty());
    assert!(!msg.has_field(&f), "default-valued field is not present");

    // An unset field is also empty.
    let empty = DynamicMessage::new(m);
    assert!(empty.encode_to_vec().expect("encode").is_empty());
}

// ---------------------------------------------------------------------------
// Descriptor lookups by name and number
// ---------------------------------------------------------------------------

#[test]
fn descriptor_lookups_by_name_and_number() {
    let pool = single_int32_pool();
    // By name.
    let m = pool.get_message_by_name("M").expect("M by name");
    assert_eq!(m.name(), "M");
    assert_eq!(m.full_name(), "M");
    assert!(pool.get_message_by_name("Nope").is_none());

    // Field by number and by name.
    let by_num = m.get_field(1).expect("by number");
    let by_name = m.get_field_by_name("a").expect("by name");
    assert_eq!(by_num, by_name);
    assert_eq!(by_num.number(), 1);
    assert_eq!(by_num.name(), "a");
    assert_eq!(by_num.kind(), Kind::Int32);
    assert_eq!(by_num.cardinality(), Cardinality::Optional);
    assert!(m.get_field(99).is_none());

    // Iterators.
    assert_eq!(m.fields().count(), 1);
    assert_eq!(pool.all_messages().count(), 1);
}

#[test]
fn descriptor_lookups_with_package_and_service() {
    let msgs = vec![
        DescriptorProto {
            name: Some("Req".to_owned()),
            ..Default::default()
        },
        DescriptorProto {
            name: Some("Resp".to_owned()),
            ..Default::default()
        },
    ];
    let svc = ServiceDescriptorProto {
        name: Some("Greeter".to_owned()),
        method: vec![
            MethodDescriptorProto {
                name: Some("Unary".to_owned()),
                input_type: Some(".pkg.Req".to_owned()),
                output_type: Some(".pkg.Resp".to_owned()),
                ..Default::default()
            },
            MethodDescriptorProto {
                name: Some("BidiStream".to_owned()),
                input_type: Some(".pkg.Req".to_owned()),
                output_type: Some(".pkg.Resp".to_owned()),
                client_streaming: Some(true),
                server_streaming: Some(true),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let fds = proto3_file_pkg("pkg", msgs, Vec::new(), vec![svc]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");

    // Fully-qualified message names include the package.
    assert!(pool.get_message_by_name("pkg.Req").is_some());
    assert!(pool.get_message_by_name("Req").is_none());

    let service = pool.get_service_by_name("pkg.Greeter").expect("service");
    assert_eq!(service.name(), "Greeter");
    assert_eq!(service.full_name(), "pkg.Greeter");
    assert_eq!(service.methods().count(), 2);

    let mut methods = service.methods();
    let unary = methods.next().expect("unary");
    assert_eq!(unary.name(), "Unary");
    assert_eq!(unary.full_name(), "pkg.Greeter.Unary");
    assert_eq!(unary.input().full_name(), "pkg.Req");
    assert_eq!(unary.output().full_name(), "pkg.Resp");
    assert!(!unary.is_client_streaming());
    assert!(!unary.is_server_streaming());

    let bidi = methods.next().expect("bidi");
    assert_eq!(bidi.name(), "BidiStream");
    assert!(bidi.is_client_streaming());
    assert!(bidi.is_server_streaming());

    assert_eq!(pool.services().count(), 1);
}

// ---------------------------------------------------------------------------
// Oneof exclusivity
// ---------------------------------------------------------------------------

#[test]
fn oneof_exclusivity_clears_sibling() {
    // message O { oneof choice { int32 a = 1; string b = 2; } }
    let fds = proto3_file(vec![DescriptorProto {
        name: Some("O".to_owned()),
        field: vec![
            {
                let mut f = field("a", 1, Label::Optional, Type::Int32, None);
                f.oneof_index = Some(0);
                f
            },
            {
                let mut f = field("b", 2, Label::Optional, Type::String, None);
                f.oneof_index = Some(0);
                f
            },
        ],
        oneof_decl: vec![OneofDescriptorProto {
            name: Some("choice".to_owned()),
            ..Default::default()
        }],
        ..Default::default()
    }]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let m = pool.get_message_by_name("O").expect("O");
    let fa = m.get_field(1).expect("a");
    let fb = m.get_field(2).expect("b");

    // The oneof is visible on the descriptor and groups both fields.
    let oneof = fa.containing_oneof().expect("a in oneof");
    assert_eq!(oneof.name(), "choice");
    assert_eq!(oneof.fields().count(), 2);
    assert!(!oneof.is_synthetic());

    let mut msg = DynamicMessage::new(m.clone());
    // Set arm A.
    msg.set_field(&fa, Value::I32(10));
    assert!(msg.has_field(&fa));
    assert!(!msg.has_field(&fb));
    assert_eq!(msg.which_oneof("choice").as_ref(), Some(&fa));

    // Setting arm B clears arm A.
    msg.set_field(&fb, Value::String("x".to_owned()));
    assert!(!msg.has_field(&fa), "arm A should be cleared");
    assert!(msg.has_field(&fb));
    assert_eq!(msg.which_oneof("choice").as_ref(), Some(&fb));

    // Encode reflects only arm B: tag(2,LEN)=0x12 len1 'x'.
    let bytes = msg.encode_to_vec().expect("encode");
    assert_eq!(bytes, vec![0x12, 0x01, 0x78]);

    // Decoding a message that (illegally) has both arms keeps the last seen.
    let both = vec![0x08, 0x0a, 0x12, 0x01, 0x78];
    let decoded = DynamicMessage::decode(m, &both).expect("decode");
    assert!(!decoded.has_field(&fa), "later arm wins");
    assert_eq!(
        decoded.get_field(&fb).into_owned(),
        Value::String("x".to_owned())
    );
}

// ---------------------------------------------------------------------------
// Unknown-field preservation
// ---------------------------------------------------------------------------

#[test]
fn unknown_fields_survive_round_trip() {
    // Descriptor only knows field 1 (int32). The wire bytes additionally carry
    // field 2 (varint) and field 3 (length-delimited "extra"), which must be
    // preserved and re-emitted.
    let pool = single_int32_pool();
    let m = pool.get_message_by_name("M").expect("M");
    let f1 = m.get_field(1).expect("field 1");

    // Build bytes: field1=150, field2(varint)=99, field3(len)="extra".
    let mut bytes = vec![0x08, 0x96, 0x01]; // f1 = 150
    bytes.extend_from_slice(&[0x10, 0x63]); // f2 = 99 (tag 0x10 = field2/varint)
    bytes.extend_from_slice(&[0x1a, 0x05]); // f3 len-delimited, len 5
    bytes.extend_from_slice(b"extra");

    let decoded = DynamicMessage::decode(m.clone(), &bytes).expect("decode");
    assert_eq!(decoded.get_field(&f1).into_owned(), Value::I32(150));
    // Two unknown fields preserved.
    assert_eq!(decoded.unknown_fields().len(), 2);

    // Re-encode: known field first (ascending number), then unknown fields in
    // encounter order.
    let reencoded = decoded.encode_to_vec().expect("re-encode");
    assert_eq!(reencoded, bytes, "unknown bytes survive the round-trip");
}

#[test]
fn unknown_field_only_message_preserves_bytes() {
    // A message whose descriptor has NO fields still preserves arbitrary bytes.
    let fds = proto3_file(vec![DescriptorProto {
        name: Some("Empty".to_owned()),
        ..Default::default()
    }]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let m = pool.get_message_by_name("Empty").expect("Empty");

    let bytes = vec![0x08, 0x2a, 0x15, 0xef, 0xbe, 0xad, 0xde]; // f1 varint 42; f2 fixed32
    let decoded = DynamicMessage::decode(m, &bytes).expect("decode");
    assert_eq!(decoded.unknown_fields().len(), 2);
    assert_eq!(decoded.encode_to_vec().expect("encode"), bytes);
}

// ---------------------------------------------------------------------------
// Groups are rejected
// ---------------------------------------------------------------------------

#[test]
fn group_wire_type_in_unknown_is_rejected() {
    let fds = proto3_file(vec![DescriptorProto {
        name: Some("Empty".to_owned()),
        ..Default::default()
    }]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let m = pool.get_message_by_name("Empty").expect("Empty");

    // tag(1, SGroup=3) = (1<<3)|3 = 0x0b — a start-group tag.
    let bytes = vec![0x0b];
    let err = DynamicMessage::decode(m, &bytes).expect_err("group should be rejected");
    let msg = err.to_string();
    assert!(msg.contains("group"), "error should mention groups: {msg}");
}

// ---------------------------------------------------------------------------
// Nested + repeated messages round-trip
// ---------------------------------------------------------------------------

#[test]
fn round_trip_repeated_message() {
    let fds = proto3_file(vec![
        DescriptorProto {
            name: Some("Item".to_owned()),
            field: vec![field("id", 1, Label::Optional, Type::Int32, None)],
            ..Default::default()
        },
        DescriptorProto {
            name: Some("Bag".to_owned()),
            field: vec![field(
                "items",
                1,
                Label::Repeated,
                Type::Message,
                Some(".Item"),
            )],
            ..Default::default()
        },
    ]);
    let pool = DescriptorPool::from_file_descriptor_set(fds).expect("pool");
    let item_desc = pool.get_message_by_name("Item").expect("Item");
    let bag_desc = pool.get_message_by_name("Bag").expect("Bag");
    let item_id = item_desc.get_field(1).expect("id");
    let bag_items = bag_desc.get_field(1).expect("items");
    assert!(bag_items.is_list());

    let mut item1 = DynamicMessage::new(item_desc.clone());
    item1.set_field(&item_id, Value::I32(11));
    let mut item2 = DynamicMessage::new(item_desc);
    item2.set_field(&item_id, Value::I32(22));

    let mut bag = DynamicMessage::new(bag_desc.clone());
    bag.set_field(
        &bag_items,
        Value::List(vec![
            Value::Message(Box::new(item1)),
            Value::Message(Box::new(item2)),
        ]),
    );

    let bytes = bag.encode_to_vec().expect("encode");
    let decoded = DynamicMessage::decode(bag_desc, &bytes).expect("decode");
    let got = decoded.get_field(&bag_items).into_owned();
    match got {
        Value::List(list) => {
            assert_eq!(list.len(), 2);
            let ids: Vec<i32> = list
                .iter()
                .filter_map(|v| match v {
                    Value::Message(m) => {
                        let f = m.descriptor().get_field(1).expect("id");
                        m.get_field(&f).into_owned().as_i32()
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(ids, vec![11, 22]);
        }
        other => panic!("expected list, got {other:?}"),
    }
}
