// SPDX-License-Identifier: Apache-2.0
// Copyright COOLJAPAN OU (Team Kitasan)

//! Integration tests for `AnyExt::unpack_dynamic` (requires `reflect` feature).

#[cfg(feature = "reflect")]
mod dynamic_unpack {
    use oxiproto_reflect::NativeDescriptorPool;
    use oxiproto_wkt::AnyExt;
    use prost_types::{
        field_descriptor_proto::{Label, Type},
        Any, DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
    };

    /// Build a minimal `FileDescriptorSet` containing a single message:
    ///
    /// ```proto
    /// syntax = "proto3";
    /// package wkt_test;
    /// message Foo { int32 id = 1; string name = 2; }
    /// ```
    fn build_foo_fds() -> FileDescriptorSet {
        FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("foo.proto".to_string()),
                package: Some("wkt_test".to_string()),
                syntax: Some("proto3".to_string()),
                message_type: vec![DescriptorProto {
                    name: Some("Foo".to_string()),
                    field: vec![
                        FieldDescriptorProto {
                            name: Some("id".to_string()),
                            number: Some(1),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::Int32 as i32),
                            json_name: Some("id".to_string()),
                            ..Default::default()
                        },
                        FieldDescriptorProto {
                            name: Some("name".to_string()),
                            number: Some(2),
                            label: Some(Label::Optional as i32),
                            r#type: Some(Type::String as i32),
                            json_name: Some("name".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        }
    }

    /// Encode `Foo { id: 42, name: "hello" }` manually into wire bytes.
    ///
    /// Field 1 (int32 42):  tag=0x08, value=0x2A
    /// Field 2 (string "hello"): tag=0x12, len=0x05, b"hello"
    fn encode_foo() -> Vec<u8> {
        let mut buf = oxiproto_core::wire::EncodeBuffer::new();
        // field 1: int32
        buf.write_tag(1, oxiproto_core::wire::WireType::Varint)
            .expect("write_tag field 1");
        buf.write_varint32(42u32);
        // field 2: string
        buf.write_tag(2, oxiproto_core::wire::WireType::Len)
            .expect("write_tag field 2");
        buf.write_string("hello");
        buf.into_vec()
    }

    #[test]
    fn unpack_dynamic_roundtrip() {
        let fds = build_foo_fds();
        let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("build pool");

        let wire = encode_foo();
        let any = Any {
            type_url: "type.googleapis.com/wkt_test.Foo".to_string(),
            value: wire.clone(),
        };

        let result = any.unpack_dynamic(&pool);
        assert!(result.is_some(), "type should be found in pool");

        let msg = result.unwrap().expect("decode should succeed");
        // Verify the descriptor name
        assert_eq!(msg.descriptor().name(), "Foo");

        // Re-encode and check wire bytes are identical (lossless round-trip).
        let re_encoded = msg.encode_to_vec().expect("re-encode");
        assert_eq!(re_encoded, wire, "re-encoded bytes must match original");
    }

    #[test]
    fn unpack_dynamic_wrong_type_url_returns_none() {
        let fds = build_foo_fds();
        let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("build pool");

        let any = Any {
            type_url: "type.googleapis.com/wkt_test.Bar".to_string(), // Bar not in pool
            value: vec![],
        };

        let result = any.unpack_dynamic(&pool);
        assert!(result.is_none(), "unknown type should return None");
    }

    #[test]
    fn unpack_dynamic_malformed_bytes_returns_err() {
        let fds = build_foo_fds();
        let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("build pool");

        // Wire type 7 is invalid; a tag byte 0x0F = field 1 | wire_type 7.
        let bad_bytes = vec![0x0Fu8, 0x01];
        let any = Any {
            type_url: "type.googleapis.com/wkt_test.Foo".to_string(),
            value: bad_bytes,
        };

        let result = any.unpack_dynamic(&pool);
        assert!(result.is_some(), "type found but bytes are bad");
        assert!(
            result.unwrap().is_err(),
            "malformed bytes should produce ReflectError"
        );
    }

    #[test]
    fn unpack_dynamic_empty_value_returns_empty_message() {
        let fds = build_foo_fds();
        let pool = NativeDescriptorPool::from_file_descriptor_set(fds).expect("build pool");

        let any = Any {
            type_url: "type.googleapis.com/wkt_test.Foo".to_string(),
            value: vec![], // empty = all-default proto3 message
        };

        let result = any.unpack_dynamic(&pool);
        assert!(result.is_some());
        let msg = result.unwrap().expect("empty bytes decode OK");
        // All fields at default: re-encode produces empty bytes.
        let re_encoded = msg.encode_to_vec().expect("re-encode");
        assert!(
            re_encoded.is_empty(),
            "empty message re-encodes to zero bytes"
        );
    }

    #[test]
    fn type_name_extraction_for_unpack() {
        // Ensure type_name() handles the URL correctly.
        let any = Any {
            type_url: "type.googleapis.com/wkt_test.Foo".to_string(),
            value: vec![],
        };
        assert_eq!(any.type_name(), "wkt_test.Foo");

        // Also test bare name (no slash).
        let any2 = Any {
            type_url: "wkt_test.Foo".to_string(),
            value: vec![],
        };
        assert_eq!(any2.type_name(), "wkt_test.Foo");
    }
}
