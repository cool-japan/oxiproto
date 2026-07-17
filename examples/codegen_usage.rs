//! Generate plain Rust source (structs, enums, `OxiMessage` impls) from a
//! `.proto` schema, entirely in-process -- the same path `oxiproto-cli gen`
//! takes, minus the file I/O.
//!
//! This is the piece that lets a `build.rs` regenerate protobuf bindings on
//! a bare `rust:slim` container: no `protoc` binary, no bundled C++ parser.
//!
//! Run with:
//! ```text
//! cargo run --example codegen_usage -p oxiproto-examples
//! ```

use oxiproto_codegen::CodegenOptions;

const PROTO_SOURCE: &str = r#"
syntax = "proto3";
package example;

// A single line item within an invoice.
message LineItem {
  string sku      = 1;
  uint32 quantity = 2;
  double price    = 3;
}

message Invoice {
  string id                 = 1;
  repeated LineItem items   = 2;
  bool    paid              = 3;
}
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Parse .proto text to a FileDescriptorSet via the native pure-Rust
    //    parser (no `protoc`, no protox fallback needed here).
    let fds = oxiproto_build::compile_str(PROTO_SOURCE)?;

    // 2. Configure codegen: emit doc comments and `impl OxiMessage` blocks
    //    so the generated types are immediately usable with the native wire
    //    codec shown in the `encode_decode` example.
    let mut options = CodegenOptions::new();
    options.generate_docs = true;
    options.emit_oxi_message_impl = true;

    // 3. Generate the Rust source as a string. In a real build.rs this would
    //    be written to `$OUT_DIR/example.rs` and `include!`-ed from `lib.rs`.
    let generated = oxiproto_codegen::generate_with_options(&fds, &options)?;

    println!("--- generated Rust source ({} bytes) ---", generated.len());
    println!("{generated}");

    // Sanity-check that codegen actually produced the two message types we
    // defined above, so this example fails loudly if codegen output ever
    // changes shape.
    assert!(generated.contains("struct LineItem"));
    assert!(generated.contains("struct Invoice"));
    assert!(generated.contains("impl") && generated.contains("OxiMessage"));

    Ok(())
}
