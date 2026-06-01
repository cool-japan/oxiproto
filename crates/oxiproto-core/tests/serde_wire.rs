//! Tests for serde::Serialize/Deserialize on wire-format types.
//!
//! Exercises round-trips through serde_json for all public wire types.
//! Only compiled when the `serde` feature is enabled.
#![cfg(feature = "serde")]

use oxiproto_core::wire::{UnknownField, UnknownFields, UnknownValue, WireType};

/// All WireType variants round-trip through serde_json.
#[test]
fn wire_type_all_variants_round_trip() {
    let variants = [
        WireType::Varint,
        WireType::I64,
        WireType::Len,
        WireType::SGroup,
        WireType::EGroup,
        WireType::I32,
    ];
    for wt in &variants {
        let json = serde_json::to_string(wt).expect("serialize WireType");
        let back: WireType = serde_json::from_str(&json).expect("deserialize WireType");
        assert_eq!(*wt, back, "round-trip failed for {wt:?}");
    }
}

/// Empty UnknownFields serializes and deserializes back to empty.
#[test]
fn unknown_fields_empty_round_trip() {
    let uf = UnknownFields::default();
    let json = serde_json::to_string(&uf).expect("serialize empty UnknownFields");
    let back: UnknownFields = serde_json::from_str(&json).expect("deserialize empty UnknownFields");
    assert!(
        back.is_empty(),
        "deserialized UnknownFields should be empty"
    );
    assert_eq!(uf, back);
}

/// UnknownFields with one of each UnknownValue variant round-trips exactly.
#[test]
fn unknown_fields_all_value_variants_round_trip() {
    let mut uf = UnknownFields::new();
    uf.push_varint(1, 12345);
    uf.push_fixed64(2, 0xDEAD_BEEF_CAFE_BABE);
    uf.push_length_delimited(3, vec![0xAA, 0xBB, 0xCC]);
    uf.push_fixed32(4, 0x1234_5678);
    // Group variant: add directly via push()
    uf.push(UnknownField {
        field_number: 5,
        value: UnknownValue::Group(vec![0x01, 0x02]),
    });

    let json = serde_json::to_string(&uf).expect("serialize UnknownFields");
    let back: UnknownFields = serde_json::from_str(&json).expect("deserialize UnknownFields");

    assert_eq!(
        uf, back,
        "full round-trip failed for UnknownFields with all variants"
    );
    assert_eq!(back.len(), 5);
}

/// UnknownValue variants each serialize and deserialize in isolation.
#[test]
fn unknown_value_variants_round_trip() {
    let values: &[UnknownValue] = &[
        UnknownValue::Varint(0),
        UnknownValue::Varint(u64::MAX),
        UnknownValue::Fixed64(0),
        UnknownValue::Fixed64(u64::MAX),
        UnknownValue::LengthDelimited(vec![]),
        UnknownValue::LengthDelimited(vec![1, 2, 3, 255]),
        UnknownValue::Fixed32(0),
        UnknownValue::Fixed32(u32::MAX),
        UnknownValue::Group(vec![]),
        UnknownValue::Group(vec![0xFF, 0x00]),
    ];
    for uv in values {
        let json = serde_json::to_string(uv).expect("serialize UnknownValue");
        let back: UnknownValue = serde_json::from_str(&json).expect("deserialize UnknownValue");
        assert_eq!(*uv, back, "round-trip failed for {uv:?}");
    }
}

/// UnknownField (the struct containing field_number + UnknownValue) round-trips.
#[test]
fn unknown_field_struct_round_trip() {
    let field = UnknownField {
        field_number: 42,
        value: UnknownValue::LengthDelimited(b"hello proto".to_vec()),
    };
    let json = serde_json::to_string(&field).expect("serialize UnknownField");
    let back: UnknownField = serde_json::from_str(&json).expect("deserialize UnknownField");
    assert_eq!(field, back);
}
