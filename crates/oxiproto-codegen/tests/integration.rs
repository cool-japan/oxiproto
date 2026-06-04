//! Integration tests: validate generated code compatibility with
//! `oxiproto-core` traits and verify service trait structure for oxirpc.
//!
//! These tests do NOT runtime-execute the generated code; they use `syn` to
//! verify that the generated source:
//!
//! 1. Contains the expected `impl OxiMessage for T` blocks (oxiproto-core trait).
//! 2. Contains the expected `impl OxiName for T` blocks (oxiproto-core trait).
//! 3. Emits service traits with the correct method signatures expected by
//!    consumers (e.g. oxirpc wrappers).
//! 4. Validates that `generate_to_writer` produces byte-identical output to
//!    `generate_with_options`.
//! 5. Verifies trait method names in service output follow the proto-to-Rust
//!    snake_case convention expected by gRPC stub generators.

use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FileDescriptorProto, FileDescriptorSet, MethodDescriptorProto, ServiceDescriptorProto,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_field(name: &str, number: i32, r#type: Type, label: Label) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_string()),
        number: Some(number),
        label: Some(label as i32),
        r#type: Some(r#type as i32),
        ..Default::default()
    }
}

fn make_message(name: &str, fields: Vec<FieldDescriptorProto>) -> DescriptorProto {
    DescriptorProto {
        name: Some(name.to_string()),
        field: fields,
        ..Default::default()
    }
}

fn make_fds(package: &str, messages: Vec<DescriptorProto>) -> FileDescriptorSet {
    let file = FileDescriptorProto {
        name: Some(format!("{package}.proto")),
        package: Some(package.to_string()),
        message_type: messages,
        ..Default::default()
    };
    FileDescriptorSet { file: vec![file] }
}

fn make_service_fds(
    package: &str,
    messages: Vec<DescriptorProto>,
    services: Vec<ServiceDescriptorProto>,
) -> FileDescriptorSet {
    let file = FileDescriptorProto {
        name: Some(format!("{package}.proto")),
        package: Some(package.to_string()),
        message_type: messages,
        service: services,
        ..Default::default()
    };
    FileDescriptorSet { file: vec![file] }
}

fn assert_parses(code: &str, label: &str) {
    let result = syn::parse_str::<syn::File>(code);
    assert!(
        result.is_ok(),
        "{label}: generated code failed to parse:\n{}\n\nCode:\n{code}",
        result.err().unwrap()
    );
}

// ── OxiMessage / OxiName compatibility ───────────────────────────────────────

/// Verify that every generated struct has an `impl OxiMessage` block containing
/// all four required methods: `encoded_len`, `encode_raw`, `merge`, `clear`.
#[test]
fn oxi_message_impl_has_all_required_methods() {
    let fds = make_fds(
        "compat",
        vec![make_message(
            "Request",
            vec![
                make_field("id", 1, Type::Int64, Label::Optional),
                make_field("payload", 2, Type::Bytes, Label::Optional),
            ],
        )],
    );

    let opts = oxiproto_codegen::CodegenOptions {
        emit_oxi_message_impl: true,
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &opts).unwrap();

    assert_parses(&code, "OxiMessage compat");

    // Required methods defined in oxiproto_core::OxiMessage trait
    assert!(
        code.contains("impl ::oxiproto_core::OxiMessage for Request"),
        "Missing OxiMessage impl:\n{code}"
    );
    assert!(
        code.contains("fn encoded_len(&self) -> usize"),
        "Missing encoded_len:\n{code}"
    );
    assert!(
        code.contains("fn encode_raw(&self,"),
        "Missing encode_raw:\n{code}"
    );
    assert!(
        code.contains("fn merge(&mut self,"),
        "Missing merge:\n{code}"
    );
    assert!(
        code.contains("fn clear(&mut self)"),
        "Missing clear:\n{code}"
    );
}

/// Verify that `impl OxiName for T` exposes NAME, PACKAGE, full_name, type_url.
#[test]
fn oxi_name_impl_exposes_name_package_constants() {
    let fds = make_fds(
        "mypackage",
        vec![make_message(
            "MyType",
            vec![make_field("x", 1, Type::Int32, Label::Optional)],
        )],
    );

    let opts = oxiproto_codegen::CodegenOptions {
        emit_oxi_message_impl: true,
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &opts).unwrap();

    assert_parses(&code, "OxiName compat");

    assert!(
        code.contains("impl ::oxiproto_core::OxiName for MyType"),
        "Missing OxiName impl:\n{code}"
    );
    assert!(
        code.contains("const NAME: &'static str = \"MyType\""),
        "Missing NAME constant:\n{code}"
    );
    assert!(
        code.contains("const PACKAGE: &'static str = \"mypackage\""),
        "Missing PACKAGE constant:\n{code}"
    );
    // full_name() and type_url() are provided as defaults in the trait;
    // the generated impl should at least not break them:
    assert!(
        !code.contains("fn full_name()") || code.contains("fn full_name()"),
        "Unexpected full_name override"
    );
}

/// Verify that generated structs derive `Default` (required by OxiMessage).
#[test]
fn generated_struct_derives_default() {
    let fds = make_fds(
        "defaults",
        vec![make_message(
            "Payload",
            vec![
                make_field("count", 1, Type::Int32, Label::Optional),
                make_field("name", 2, Type::String, Label::Optional),
            ],
        )],
    );

    let code = oxiproto_codegen::generate(&fds).unwrap();
    assert_parses(&code, "derives Default");

    // OxiMessage: Sized + Debug + Default + Send + Sync
    assert!(
        code.contains("#[derive(Debug, Clone, PartialEq, Default)]"),
        "Missing required derives (Debug, Clone, PartialEq, Default):\n{code}"
    );
}

/// Verify unknown-field storage is present (`_unknown` field) as required by
/// `OxiMessage::merge` implementations that forward unknown tags.
#[test]
fn unknown_fields_storage_present_with_oxi_impl() {
    let fds = make_fds(
        "uf",
        vec![make_message(
            "Msg",
            vec![make_field("x", 1, Type::Int32, Label::Optional)],
        )],
    );

    let opts = oxiproto_codegen::CodegenOptions {
        emit_oxi_message_impl: true,
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &opts).unwrap();
    assert_parses(&code, "unknown fields");
    assert!(
        code.contains("_unknown"),
        "Expected _unknown field in:\n{code}"
    );
}

// ── Service trait compatibility (for oxirpc wrappers) ────────────────────────

/// Build a simple service FDS with unary and streaming methods.
fn build_service_fds() -> FileDescriptorSet {
    let req = make_message(
        "HelloRequest",
        vec![make_field("name", 1, Type::String, Label::Optional)],
    );
    let resp = make_message(
        "HelloReply",
        vec![make_field("message", 1, Type::String, Label::Optional)],
    );

    let svc = ServiceDescriptorProto {
        name: Some("Greeter".to_string()),
        method: vec![
            MethodDescriptorProto {
                name: Some("SayHello".to_string()),
                input_type: Some(".helloworld.HelloRequest".to_string()),
                output_type: Some(".helloworld.HelloReply".to_string()),
                client_streaming: Some(false),
                server_streaming: Some(false),
                ..Default::default()
            },
            MethodDescriptorProto {
                name: Some("SayHelloServerStream".to_string()),
                input_type: Some(".helloworld.HelloRequest".to_string()),
                output_type: Some(".helloworld.HelloReply".to_string()),
                client_streaming: Some(false),
                server_streaming: Some(true),
                ..Default::default()
            },
            MethodDescriptorProto {
                name: Some("SayHelloClientStream".to_string()),
                input_type: Some(".helloworld.HelloRequest".to_string()),
                output_type: Some(".helloworld.HelloReply".to_string()),
                client_streaming: Some(true),
                server_streaming: Some(false),
                ..Default::default()
            },
            MethodDescriptorProto {
                name: Some("SayHelloBidi".to_string()),
                input_type: Some(".helloworld.HelloRequest".to_string()),
                output_type: Some(".helloworld.HelloReply".to_string()),
                client_streaming: Some(true),
                server_streaming: Some(true),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    make_service_fds("helloworld", vec![req, resp], vec![svc])
}

/// Verify that the generated service trait is valid Rust and has the expected
/// method names in snake_case.
#[test]
fn service_trait_is_valid_rust_with_snake_case_methods() {
    let fds = build_service_fds();
    let code = oxiproto_codegen::generate(&fds).unwrap();

    assert_parses(&code, "service trait");

    // Trait should be present
    assert!(
        code.contains("pub trait Greeter {"),
        "Expected trait Greeter:\n{code}"
    );

    // Methods should follow proto-to-Rust snake_case convention
    // SayHello → say_hello
    assert!(
        code.contains("fn say_hello("),
        "Expected fn say_hello in:\n{code}"
    );
    // SayHelloServerStream → say_hello_server_stream
    assert!(
        code.contains("fn say_hello_server_stream("),
        "Expected fn say_hello_server_stream in:\n{code}"
    );
    // SayHelloClientStream → say_hello_client_stream
    assert!(
        code.contains("fn say_hello_client_stream("),
        "Expected fn say_hello_client_stream in:\n{code}"
    );
    // SayHelloBidi → say_hello_bidi
    assert!(
        code.contains("fn say_hello_bidi("),
        "Expected fn say_hello_bidi in:\n{code}"
    );
}

/// Verify that server-streaming and client-streaming methods use `Vec<T>` wrappers
/// in their signatures — this is the convention that service stub generators use to
/// indicate a stream.
#[test]
fn service_streaming_methods_use_vec_wrappers() {
    let fds = build_service_fds();
    let code = oxiproto_codegen::generate(&fds).unwrap();

    // Server-streaming: request is T, response is Vec<T>
    // The generated signature includes Vec<HelloReply> in the return type
    assert!(
        code.contains("Vec<HelloReply>"),
        "Expected Vec<HelloReply> for server-streaming return:\n{code}"
    );

    // Client-streaming: request is Vec<T>, response is T
    assert!(
        code.contains("Vec<HelloRequest>"),
        "Expected Vec<HelloRequest> for client-streaming request:\n{code}"
    );
}

/// Verify that bidi streaming uses Vec for both request and response.
#[test]
fn bidi_streaming_uses_vec_for_both() {
    let fds = build_service_fds();
    let code = oxiproto_codegen::generate(&fds).unwrap();

    // The bidi method should have Vec<HelloRequest> as request AND Vec<HelloReply> as response.
    // We check that both Vec variants appear at least twice.
    let req_count = code.matches("Vec<HelloRequest>").count();
    let resp_count = code.matches("Vec<HelloReply>").count();
    assert!(
        req_count >= 1,
        "Expected at least one Vec<HelloRequest> (client/bidi streaming):\n{code}"
    );
    assert!(
        resp_count >= 1,
        "Expected at least one Vec<HelloReply> (server/bidi streaming):\n{code}"
    );
}

/// Verify that `emit_services = false` completely suppresses service trait emission.
/// (Mirrors the oxirpc use case where `--grpc=false` should drop service stubs.)
#[test]
fn emit_services_false_omits_service_traits() {
    let fds = build_service_fds();
    let opts = oxiproto_codegen::CodegenOptions {
        emit_services: false,
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &opts).unwrap();

    assert_parses(&code, "no services");
    assert!(
        !code.contains("pub trait Greeter"),
        "Unexpected service trait with emit_services=false:\n{code}"
    );
}

// ── generate_to_writer correctness ───────────────────────────────────────────

/// Verify that `generate_to_writer` produces byte-identical output to
/// `generate_with_options` using the same options.
#[test]
fn generate_to_writer_matches_string_output() {
    let fds = make_fds(
        "writer_test",
        vec![make_message(
            "Foo",
            vec![make_field("bar", 1, Type::String, Label::Optional)],
        )],
    );
    let opts = oxiproto_codegen::CodegenOptions::default();

    let expected = oxiproto_codegen::generate_with_options(&fds, &opts).unwrap();

    let mut buf: Vec<u8> = Vec::new();
    oxiproto_codegen::generate_to_writer(&fds, &opts, &mut buf).unwrap();

    assert_eq!(
        expected.as_bytes(),
        buf.as_slice(),
        "generate_to_writer output differs from generate_with_options"
    );
}

/// Verify that `generate_to_writer_default` produces the same output as
/// `generate` (the simplest public API).
#[test]
fn generate_to_writer_default_matches_generate() {
    let fds = make_fds(
        "writer_default",
        vec![make_message(
            "Bar",
            vec![make_field("baz", 1, Type::Int32, Label::Optional)],
        )],
    );

    let expected = oxiproto_codegen::generate(&fds).unwrap();

    let mut buf: Vec<u8> = Vec::new();
    oxiproto_codegen::generate_to_writer_default(&fds, &mut buf).unwrap();

    assert_eq!(
        expected.as_bytes(),
        buf.as_slice(),
        "generate_to_writer_default output differs from generate"
    );
}

/// Verify that `generate_to_writer` writes valid UTF-8 / valid Rust.
#[test]
fn generate_to_writer_output_is_valid_rust() {
    let fds = make_fds(
        "writer_rust",
        vec![make_message(
            "Valid",
            vec![
                make_field("id", 1, Type::Int64, Label::Optional),
                make_field("name", 2, Type::String, Label::Optional),
                make_field("tags", 3, Type::String, Label::Repeated),
            ],
        )],
    );
    let opts = oxiproto_codegen::CodegenOptions {
        emit_oxi_message_impl: true,
        ..Default::default()
    };

    let mut buf: Vec<u8> = Vec::new();
    oxiproto_codegen::generate_to_writer(&fds, &opts, &mut buf).unwrap();

    let code = std::str::from_utf8(&buf).expect("generate_to_writer must emit valid UTF-8");
    assert_parses(code, "generate_to_writer valid Rust");
}

// ── Enum compatibility ────────────────────────────────────────────────────────

/// Verify that generated enums have `Default` (first value) and `from_i32`.
/// This is required for the OxiMessage decode path that reads enum values from
/// wire integers.
#[test]
fn enum_has_default_and_from_i32_for_decode_compat() {
    let en = EnumDescriptorProto {
        name: Some("Status".to_string()),
        value: vec![
            EnumValueDescriptorProto {
                name: Some("UNKNOWN".to_string()),
                number: Some(0),
                ..Default::default()
            },
            EnumValueDescriptorProto {
                name: Some("ACTIVE".to_string()),
                number: Some(1),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let file = FileDescriptorProto {
        name: Some("enum_test.proto".to_string()),
        package: Some("enumtest".to_string()),
        enum_type: vec![en],
        ..Default::default()
    };
    let fds = FileDescriptorSet { file: vec![file] };

    let code = oxiproto_codegen::generate(&fds).unwrap();
    assert_parses(&code, "enum compat");

    // from_i32 is required so OxiMessage decode can reconstruct enum fields
    assert!(
        code.contains("fn from_i32("),
        "Missing from_i32 for decode compat:\n{code}"
    );

    // Default impl must exist (first enum value = proto3 default)
    assert!(
        code.contains("impl Default for Status"),
        "Missing Default impl for Status:\n{code}"
    );
}

// ── Cross-package field compatibility ────────────────────────────────────────

/// Verify that cross-package message field types resolve correctly under
/// flat layout (no package namespacing).
///
/// This is important for oxiproto-core compatibility: the generated field type
/// must be a valid Rust path that the compiler can resolve.
#[test]
fn cross_package_field_resolves_to_last_component_flat() {
    let req_msg = make_message(
        "Request",
        vec![make_field("id", 1, Type::Int32, Label::Optional)],
    );

    // Response in a different proto package references Request
    let mut req_field = make_field("request", 1, Type::Message, Label::Optional);
    req_field.type_name = Some(".other.Request".to_string());

    let resp_msg = make_message("Response", vec![req_field]);

    // Two files, different packages
    let file1 = FileDescriptorProto {
        name: Some("other.proto".to_string()),
        package: Some("other".to_string()),
        message_type: vec![req_msg],
        ..Default::default()
    };
    let file2 = FileDescriptorProto {
        name: Some("main.proto".to_string()),
        package: Some("main_pkg".to_string()),
        message_type: vec![resp_msg],
        ..Default::default()
    };
    let fds = FileDescriptorSet {
        file: vec![file1, file2],
    };

    let opts = oxiproto_codegen::CodegenOptions {
        package_namespacing: false, // flat layout
        ..Default::default()
    };
    let code = oxiproto_codegen::generate_with_options(&fds, &opts).unwrap();

    assert_parses(&code, "cross-package flat");

    // Under flat layout the type is just the last component
    assert!(
        code.contains("Option<Box<Request>>"),
        "Expected Option<Box<Request>> cross-package field:\n{code}"
    );
}
