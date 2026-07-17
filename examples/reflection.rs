//! Build and inspect a message at runtime via `oxiproto-reflect`'s
//! `DynamicMessage`, without generating (or even compiling) any Rust types
//! for the message schema.
//!
//! This is the workflow tools like `oxiproto-cli encode`/`decode` use: parse
//! `.proto` text into a `FileDescriptorSet`, build a `DescriptorPool` from
//! it, look up a message type by fully-qualified name, and read/write its
//! fields by name through `Value`.
//!
//! Run with:
//! ```text
//! cargo run --example reflection -p oxiproto-examples
//! ```

use oxiproto_reflect::{dynamic, pool_from_fds, DynamicMessage, ReflectMessage};
use prost::Message as _;
use prost_reflect::Value;

const PROTO_SOURCE: &str = r#"
syntax = "proto3";
package example;

message Order {
  string customer   = 1;
  uint32 item_count  = 2;
  bool   gift_wrap  = 3;
}
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Parse the inline .proto source into a FileDescriptorSet (pure Rust,
    //    no `protoc` binary involved).
    let fds = oxiproto_build::compile_str(PROTO_SOURCE)?;

    // 2. Build a DescriptorPool and look up the message type by its
    //    fully-qualified name ("<package>.<MessageName>").
    let pool = pool_from_fds(fds)?;
    let descriptor = pool
        .get_message_by_name("example.Order")
        .ok_or("message type 'example.Order' not found in pool")?;

    // 3. Construct a DynamicMessage and set its fields by name -- no
    //    generated struct required.
    let mut order = DynamicMessage::new(descriptor.clone());
    dynamic::set_field_by_name(&mut order, "customer", Value::String("Grace Hopper".into()))?;
    dynamic::set_field_by_name(&mut order, "item_count", Value::U32(3))?;
    dynamic::set_field_by_name(&mut order, "gift_wrap", Value::Bool(true))?;

    println!("dynamic message (text format): {order}");

    // 4. Encode to the standard protobuf wire format.
    let bytes = order.encode_to_vec();
    println!("encoded {} bytes: {bytes:02x?}", bytes.len());

    // 5. Decode back into a fresh DynamicMessage using only the descriptor
    //    (this is exactly what a schema-agnostic proxy or debugging tool
    //    would do with bytes of unknown provenance).
    let decoded = DynamicMessage::decode(descriptor, bytes.as_slice())?;
    assert_eq!(decoded.descriptor().full_name(), "example.Order");

    let customer = match dynamic::get_field_by_name(&decoded, "customer")? {
        Some(Value::String(s)) => s,
        _ => String::new(),
    };
    let item_count = match dynamic::get_field_by_name(&decoded, "item_count")? {
        Some(Value::U32(n)) => n,
        _ => 0,
    };
    println!("decoded: customer={customer:?}, item_count={item_count}");
    assert_eq!(customer, "Grace Hopper");
    assert_eq!(item_count, 3);

    // 6. Looking up a field that doesn't exist on the descriptor returns a
    //    typed ReflectError rather than panicking.
    match dynamic::get_field_by_name(&decoded, "not_a_real_field") {
        Ok(_) => unreachable!("unknown field name must not resolve"),
        Err(e) => println!("unknown field rejected as expected: {e}"),
    }

    Ok(())
}
