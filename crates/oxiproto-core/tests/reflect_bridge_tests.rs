//! Tests for `oxiproto_core::reflect_bridge`.
//!
//! Verifies that `OxiReflect`, `OxiReflectHandle`, `decode_handle`,
//! `ReflectMetadata`, and `MessageRegistry` all function correctly for
//! concrete `OxiMessage + OxiName` types.

use oxiproto_core::{
    reflect_bridge::{
        decode_handle, MessageRegistry, OxiReflect, OxiReflectHandle, ReflectMetadata,
    },
    wire::{DecodeBuffer, EncodeBuffer, WireType},
    OxiMessage, OxiName, OxiProtoResult,
};

// ---------------------------------------------------------------------------
// Test message types
// ---------------------------------------------------------------------------

/// A minimal message: `{ int32 id = 1; string label = 2; }`.
#[derive(Debug, Default, Clone, PartialEq)]
struct SimpleMsg {
    id: i32,
    label: String,
}

impl OxiName for SimpleMsg {
    const NAME: &'static str = "SimpleMsg";
    const PACKAGE: &'static str = "test.bridge";
}

impl OxiMessage for SimpleMsg {
    fn encoded_len(&self) -> usize {
        use oxiproto_core::wire::varint::encoded_len_varint;
        let mut n = 0;
        if self.id != 0 {
            n += 1 + encoded_len_varint(self.id as i64 as u64);
        }
        if !self.label.is_empty() {
            n += 1 + encoded_len_varint(self.label.len() as u64) + self.label.len();
        }
        n
    }

    fn encode_raw(&self, buf: &mut EncodeBuffer) {
        if self.id != 0 {
            let _ = buf.write_tag(1, WireType::Varint);
            buf.write_varint_i32(self.id);
        }
        if !self.label.is_empty() {
            let _ = buf.write_tag(2, WireType::Len);
            buf.write_string(&self.label);
        }
    }

    fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = buf
                .read_tag()
                .map_err(oxiproto_core::OxiProtoError::WireFormatError)?;
            match tag.field_number {
                1 => {
                    self.id = buf
                        .read_varint_i32()
                        .map_err(oxiproto_core::OxiProtoError::WireFormatError)?;
                }
                2 => {
                    self.label = buf
                        .read_string()
                        .map_err(oxiproto_core::OxiProtoError::WireFormatError)?
                        .to_owned();
                }
                _ => {
                    buf.skip_field(tag.wire_type)
                        .map_err(oxiproto_core::OxiProtoError::WireFormatError)?;
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

/// A second message type for registry tests.
#[derive(Debug, Default, Clone, PartialEq)]
struct CountMsg {
    count: u64,
}

impl OxiName for CountMsg {
    const NAME: &'static str = "CountMsg";
    const PACKAGE: &'static str = "test.bridge";
}

impl OxiMessage for CountMsg {
    fn encoded_len(&self) -> usize {
        use oxiproto_core::wire::varint::encoded_len_varint;
        if self.count != 0 {
            1 + encoded_len_varint(self.count)
        } else {
            0
        }
    }

    fn encode_raw(&self, buf: &mut EncodeBuffer) {
        if self.count != 0 {
            let _ = buf.write_tag(1, WireType::Varint);
            buf.write_varint(self.count);
        }
    }

    fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = buf
                .read_tag()
                .map_err(oxiproto_core::OxiProtoError::WireFormatError)?;
            match tag.field_number {
                1 => {
                    self.count = buf
                        .read_varint()
                        .map_err(oxiproto_core::OxiProtoError::WireFormatError)?;
                }
                _ => {
                    buf.skip_field(tag.wire_type)
                        .map_err(oxiproto_core::OxiProtoError::WireFormatError)?;
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

/// A message in the root package (PACKAGE = "").
#[derive(Debug, Default, Clone, PartialEq)]
struct RootMsg {
    flag: bool,
}

impl OxiName for RootMsg {
    const NAME: &'static str = "RootMsg";
    const PACKAGE: &'static str = "";
}

impl OxiMessage for RootMsg {
    fn encoded_len(&self) -> usize {
        if self.flag {
            2
        } else {
            0
        }
    }

    fn encode_raw(&self, buf: &mut EncodeBuffer) {
        if self.flag {
            let _ = buf.write_tag(1, WireType::Varint);
            buf.write_bool(true);
        }
    }

    fn merge(&mut self, buf: &mut DecodeBuffer) -> OxiProtoResult<()> {
        while !buf.is_empty() {
            let tag = buf
                .read_tag()
                .map_err(oxiproto_core::OxiProtoError::WireFormatError)?;
            match tag.field_number {
                1 => {
                    self.flag = buf
                        .read_bool()
                        .map_err(oxiproto_core::OxiProtoError::WireFormatError)?;
                }
                _ => {
                    buf.skip_field(tag.wire_type)
                        .map_err(oxiproto_core::OxiProtoError::WireFormatError)?;
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

// ---------------------------------------------------------------------------
// OxiReflect / OxiReflectHandle tests
// ---------------------------------------------------------------------------

#[test]
fn reflect_handle_full_name() {
    let msg = SimpleMsg {
        id: 1,
        label: "x".to_owned(),
    };
    let handle = msg.reflect_handle();
    assert_eq!(handle.full_name(), "test.bridge.SimpleMsg");
}

#[test]
fn reflect_handle_type_url() {
    let msg = SimpleMsg::default();
    let handle = msg.reflect_handle();
    assert_eq!(
        handle.type_url(),
        "type.googleapis.com/test.bridge.SimpleMsg"
    );
}

#[test]
fn reflect_handle_empty_message_is_empty() {
    let handle = SimpleMsg::default().reflect_handle();
    // Default SimpleMsg (id=0, label="") → no fields → 0 bytes
    assert!(handle.is_empty_message());
    assert_eq!(handle.encoded_bytes(), &[]);
}

#[test]
fn reflect_handle_non_default_not_empty() {
    let msg = SimpleMsg {
        id: 42,
        label: "hello".to_owned(),
    };
    let handle = msg.reflect_handle();
    assert!(!handle.is_empty_message());
    assert!(!handle.encoded_bytes().is_empty());
}

#[test]
fn reflect_handle_encoded_bytes_match_encode_to_vec() {
    let msg = SimpleMsg {
        id: 7,
        label: "world".to_owned(),
    };
    let handle = msg.reflect_handle();
    assert_eq!(handle.encoded_bytes(), msg.encode_to_vec().as_slice());
}

#[test]
fn reflect_handle_into_encoded_consumes() {
    let msg = SimpleMsg {
        id: 3,
        label: "abc".to_owned(),
    };
    let expected = msg.encode_to_vec();
    let handle = msg.reflect_handle();
    let bytes = handle.into_encoded();
    assert_eq!(bytes, expected);
}

#[test]
fn reflect_handle_clone_equality() {
    let msg = SimpleMsg {
        id: 5,
        label: "dup".to_owned(),
    };
    let h1 = msg.reflect_handle();
    let h2 = h1.clone();
    assert_eq!(h1, h2);
}

#[test]
fn proto_full_name_static_method() {
    let name = SimpleMsg::proto_full_name();
    assert_eq!(name, "test.bridge.SimpleMsg");
}

#[test]
fn proto_type_url_static_method() {
    let url = SimpleMsg::proto_type_url();
    assert_eq!(url, "type.googleapis.com/test.bridge.SimpleMsg");
}

#[test]
fn reflect_handle_root_package_no_leading_dot() {
    let msg = RootMsg { flag: true };
    let handle = msg.reflect_handle();
    assert_eq!(handle.full_name(), "RootMsg");
    assert_eq!(handle.type_url(), "type.googleapis.com/RootMsg");
}

// ---------------------------------------------------------------------------
// decode_handle tests
// ---------------------------------------------------------------------------

#[test]
fn decode_handle_round_trips_simple_message() {
    let original = SimpleMsg {
        id: 100,
        label: "bridge test".to_owned(),
    };
    let handle = original.reflect_handle();
    let decoded: SimpleMsg = decode_handle(&handle).expect("decode_handle");
    assert_eq!(decoded, original);
}

#[test]
fn decode_handle_default_message_round_trips() {
    let original = SimpleMsg::default();
    let handle = original.reflect_handle();
    let decoded: SimpleMsg = decode_handle(&handle).expect("decode_handle");
    assert_eq!(decoded, original);
}

#[test]
fn decode_handle_root_package_round_trips() {
    let original = RootMsg { flag: true };
    let handle = original.reflect_handle();
    let decoded: RootMsg = decode_handle(&handle).expect("decode_handle");
    assert_eq!(decoded, original);
}

#[test]
fn decode_handle_count_msg_round_trips() {
    let original = CountMsg {
        count: u64::MAX / 2,
    };
    let handle = original.reflect_handle();
    let decoded: CountMsg = decode_handle(&handle).expect("decode_handle");
    assert_eq!(decoded, original);
}

// ---------------------------------------------------------------------------
// ReflectMetadata tests
// ---------------------------------------------------------------------------

#[test]
fn reflect_metadata_of_simple_msg() {
    let meta = ReflectMetadata::of::<SimpleMsg>();
    assert_eq!(meta.name, "SimpleMsg");
    assert_eq!(meta.package, "test.bridge");
    assert_eq!(meta.full_name, "test.bridge.SimpleMsg");
    assert_eq!(meta.type_url, "type.googleapis.com/test.bridge.SimpleMsg");
}

#[test]
fn reflect_metadata_of_root_msg() {
    let meta = ReflectMetadata::of::<RootMsg>();
    assert_eq!(meta.name, "RootMsg");
    assert_eq!(meta.package, "");
    assert_eq!(meta.full_name, "RootMsg");
    assert_eq!(meta.type_url, "type.googleapis.com/RootMsg");
}

#[test]
fn reflect_metadata_equality() {
    let m1 = ReflectMetadata::of::<SimpleMsg>();
    let m2 = ReflectMetadata::of::<SimpleMsg>();
    assert_eq!(m1, m2);
}

// ---------------------------------------------------------------------------
// MessageRegistry tests
// ---------------------------------------------------------------------------

#[test]
fn registry_starts_empty() {
    let reg = MessageRegistry::new();
    assert!(reg.is_empty());
    assert_eq!(reg.len(), 0);
}

#[test]
fn registry_default_is_empty() {
    let reg: MessageRegistry = MessageRegistry::default();
    assert!(reg.is_empty());
}

#[test]
fn registry_register_and_contains() {
    let mut reg = MessageRegistry::new();
    reg.register::<SimpleMsg>();
    assert!(reg.contains("test.bridge.SimpleMsg"));
    assert!(!reg.contains("test.bridge.CountMsg"));
}

#[test]
fn registry_len_after_register() {
    let mut reg = MessageRegistry::new();
    reg.register::<SimpleMsg>();
    assert_eq!(reg.len(), 1);
    reg.register::<CountMsg>();
    assert_eq!(reg.len(), 2);
}

#[test]
fn registry_not_contains_unregistered() {
    let reg = MessageRegistry::new();
    assert!(!reg.contains("does.not.Exist"));
}

#[test]
fn registry_metadata_returns_correct_data() {
    let mut reg = MessageRegistry::new();
    reg.register::<SimpleMsg>();
    let meta = reg.metadata("test.bridge.SimpleMsg").expect("metadata");
    assert_eq!(meta.name, "SimpleMsg");
    assert_eq!(meta.package, "test.bridge");
}

#[test]
fn registry_metadata_none_for_unregistered() {
    let reg = MessageRegistry::new();
    assert!(reg.metadata("no.such.Type").is_none());
}

#[test]
fn registry_validate_valid_bytes_returns_ok() {
    let mut reg = MessageRegistry::new();
    reg.register::<SimpleMsg>();
    let msg = SimpleMsg {
        id: 42,
        label: "ok".to_owned(),
    };
    let bytes = msg.encode_to_vec();
    let result = reg.validate_by_name("test.bridge.SimpleMsg", &bytes);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
}

#[test]
fn registry_validate_empty_bytes_returns_ok() {
    let mut reg = MessageRegistry::new();
    reg.register::<SimpleMsg>();
    // Empty bytes = default message, which is always a valid decode
    let result = reg.validate_by_name("test.bridge.SimpleMsg", &[]);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
}

#[test]
fn registry_validate_unregistered_returns_none() {
    let reg = MessageRegistry::new();
    assert!(reg
        .validate_by_name("no.such.Type", &[0x08, 0x01])
        .is_none());
}

#[test]
fn registry_encode_by_name_roundtrip() {
    let mut reg = MessageRegistry::new();
    reg.register::<CountMsg>();
    let msg = CountMsg { count: 9999 };
    let bytes = msg.encode_to_vec();
    let result = reg
        .encode_by_name("test.bridge.CountMsg", &bytes)
        .expect("entry exists")
        .expect("valid bytes");
    assert_eq!(result, bytes);
}

#[test]
fn registry_encode_by_name_unregistered_returns_none() {
    let reg = MessageRegistry::new();
    assert!(reg.encode_by_name("test.bridge.SimpleMsg", &[]).is_none());
}

#[test]
fn registry_names_iterator() {
    let mut reg = MessageRegistry::new();
    reg.register::<SimpleMsg>();
    reg.register::<CountMsg>();
    let mut names: Vec<&str> = reg.names().collect();
    names.sort();
    // Both have package "test.bridge"
    assert!(names.contains(&"test.bridge.SimpleMsg"));
    assert!(names.contains(&"test.bridge.CountMsg"));
    assert_eq!(names.len(), 2);
}

#[test]
fn registry_overwrite_registration() {
    let mut reg = MessageRegistry::new();
    reg.register::<SimpleMsg>();
    reg.register::<SimpleMsg>(); // second register replaces the first
    assert_eq!(
        reg.len(),
        1,
        "duplicate registration should not grow the registry"
    );
}

#[test]
fn registry_multiple_types_independent() {
    let mut reg = MessageRegistry::new();
    reg.register::<SimpleMsg>();
    reg.register::<CountMsg>();
    reg.register::<RootMsg>();

    // Validate each with its own type's bytes
    let sm = SimpleMsg {
        id: 1,
        label: "a".to_owned(),
    }
    .encode_to_vec();
    assert!(reg
        .validate_by_name("test.bridge.SimpleMsg", &sm)
        .unwrap()
        .is_ok());

    let cm = CountMsg { count: 7 }.encode_to_vec();
    assert!(reg
        .validate_by_name("test.bridge.CountMsg", &cm)
        .unwrap()
        .is_ok());

    let rm = RootMsg { flag: true }.encode_to_vec();
    assert!(reg.validate_by_name("RootMsg", &rm).unwrap().is_ok());
}

// ---------------------------------------------------------------------------
// Blanket OxiReflect impl: verify all OxiMessage+OxiName types get it free
// ---------------------------------------------------------------------------

#[test]
fn blanket_impl_works_for_all_test_types() {
    fn check_reflect<T: OxiReflect + Default>() -> OxiReflectHandle {
        T::default().reflect_handle()
    }

    let _h1: OxiReflectHandle = check_reflect::<SimpleMsg>();
    let _h2: OxiReflectHandle = check_reflect::<CountMsg>();
    let _h3: OxiReflectHandle = check_reflect::<RootMsg>();
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn handle_debug_is_non_empty() {
    let handle = SimpleMsg {
        id: 1,
        label: "dbg".to_owned(),
    }
    .reflect_handle();
    let debug_str = format!("{handle:?}");
    assert!(!debug_str.is_empty());
}

#[test]
fn metadata_clone() {
    let meta = ReflectMetadata::of::<SimpleMsg>();
    let cloned = meta.clone();
    assert_eq!(meta, cloned);
}
