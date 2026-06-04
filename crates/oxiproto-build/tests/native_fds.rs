#![forbid(unsafe_code)]

//! Cross-validation tests: native parser FDS vs protox FDS.
//!
//! Enabled only when the `native-parser` feature is active.

#[cfg(feature = "native-parser")]
mod native_fds_tests {
    use oxiproto_build::{compile_str, compile_str_native};
    use prost_types::{DescriptorProto, FieldDescriptorProto, FileDescriptorSet};

    // -----------------------------------------------------------------------
    // Normalization helpers
    // -----------------------------------------------------------------------

    fn normalize_field(f: &mut FieldDescriptorProto) {
        f.options = None;
        // json_name is intentionally NOT cleared — it is part of what we validate.
    }

    fn normalize_message(msg: &mut DescriptorProto) {
        // Do NOT clear options: map_entry is part of what we validate.
        for field in &mut msg.field {
            normalize_field(field);
        }
        for nested in &mut msg.nested_type {
            normalize_message(nested);
        }
    }

    fn normalize_fds(fds: &mut FileDescriptorSet) {
        for file in &mut fds.file {
            file.source_code_info = None;
            file.options = None;
            for msg in &mut file.message_type {
                normalize_message(msg);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Structural assertion helpers
    // -----------------------------------------------------------------------

    fn assert_field_eq(native: &FieldDescriptorProto, protox: &FieldDescriptorProto) {
        assert_eq!(native.name, protox.name, "field name mismatch");
        assert_eq!(
            native.number, protox.number,
            "field number mismatch for {:?}",
            native.name
        );
        assert_eq!(
            native.r#type, protox.r#type,
            "field type mismatch for {:?}",
            native.name
        );
        assert_eq!(
            native.type_name, protox.type_name,
            "field type_name mismatch for {:?}",
            native.name
        );
        assert_eq!(
            native.label, protox.label,
            "field label mismatch for {:?}",
            native.name
        );
        assert_eq!(
            native.proto3_optional, protox.proto3_optional,
            "proto3_optional mismatch for {:?}",
            native.name
        );
        assert_eq!(
            native.json_name, protox.json_name,
            "json_name mismatch for {:?}",
            native.name
        );
        assert_eq!(
            native.oneof_index, protox.oneof_index,
            "oneof_index mismatch for {:?}",
            native.name
        );
    }

    fn assert_message_eq(native: &DescriptorProto, protox: &DescriptorProto) {
        assert_eq!(native.name, protox.name, "message name mismatch");
        assert_eq!(
            native.field.len(),
            protox.field.len(),
            "field count mismatch in {:?}: native={:?}, protox={:?}",
            native.name,
            native.field.iter().map(|f| &f.name).collect::<Vec<_>>(),
            protox.field.iter().map(|f| &f.name).collect::<Vec<_>>()
        );
        for (nf, pf) in native.field.iter().zip(protox.field.iter()) {
            assert_field_eq(nf, pf);
        }
        assert_eq!(
            native.oneof_decl.len(),
            protox.oneof_decl.len(),
            "oneof_decl count mismatch in {:?}: native={:?}, protox={:?}",
            native.name,
            native
                .oneof_decl
                .iter()
                .map(|o| &o.name)
                .collect::<Vec<_>>(),
            protox
                .oneof_decl
                .iter()
                .map(|o| &o.name)
                .collect::<Vec<_>>()
        );
        for (no, po) in native.oneof_decl.iter().zip(protox.oneof_decl.iter()) {
            assert_eq!(
                no.name, po.name,
                "oneof name mismatch in message {:?}",
                native.name
            );
        }
        assert_eq!(
            native.nested_type.len(),
            protox.nested_type.len(),
            "nested message count mismatch in {:?}: native={:?}, protox={:?}",
            native.name,
            native
                .nested_type
                .iter()
                .map(|m| &m.name)
                .collect::<Vec<_>>(),
            protox
                .nested_type
                .iter()
                .map(|m| &m.name)
                .collect::<Vec<_>>()
        );
        for (nm, pm) in native.nested_type.iter().zip(protox.nested_type.iter()) {
            assert_message_eq(nm, pm);
        }
        // map_entry option.
        let native_map_entry = native.options.as_ref().and_then(|o| o.map_entry);
        let protox_map_entry = protox.options.as_ref().and_then(|o| o.map_entry);
        assert_eq!(
            native_map_entry, protox_map_entry,
            "map_entry mismatch in {:?}",
            native.name
        );
    }

    fn run_cross_validation(proto_source: &str, test_name: &str) {
        let native_fds = compile_str_native(proto_source)
            .unwrap_or_else(|e| panic!("native parse failed for {test_name}: {e}"));
        let mut protox_fds = compile_str(proto_source)
            .unwrap_or_else(|e| panic!("protox compile failed for {test_name}: {e}"));

        normalize_fds(&mut protox_fds);

        assert_eq!(native_fds.file.len(), 1, "expected single-file native FDS");
        assert_eq!(protox_fds.file.len(), 1, "expected single-file protox FDS");

        let nf = &native_fds.file[0];
        let pf = &protox_fds.file[0];

        assert_eq!(nf.syntax, pf.syntax, "syntax mismatch in {test_name}");
        assert_eq!(nf.package, pf.package, "package mismatch in {test_name}");

        assert_eq!(
            nf.message_type.len(),
            pf.message_type.len(),
            "top-level message count mismatch in {test_name}: native={:?}, protox={:?}",
            nf.message_type.iter().map(|m| &m.name).collect::<Vec<_>>(),
            pf.message_type.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
        for (nm, pm) in nf.message_type.iter().zip(pf.message_type.iter()) {
            assert_message_eq(nm, pm);
        }

        assert_eq!(
            nf.enum_type.len(),
            pf.enum_type.len(),
            "top-level enum count mismatch in {test_name}"
        );
        for (ne, pe) in nf.enum_type.iter().zip(pf.enum_type.iter()) {
            assert_eq!(ne.name, pe.name, "enum name mismatch in {test_name}");
            assert_eq!(
                ne.value.len(),
                pe.value.len(),
                "enum value count mismatch in {:?} in {test_name}",
                ne.name
            );
            for (nev, pev) in ne.value.iter().zip(pe.value.iter()) {
                assert_eq!(nev.name, pev.name, "enum value name mismatch");
                assert_eq!(
                    nev.number, pev.number,
                    "enum value number mismatch for {:?}",
                    nev.name
                );
            }
        }

        assert_eq!(
            nf.service.len(),
            pf.service.len(),
            "service count mismatch in {test_name}"
        );
        for (ns, ps) in nf.service.iter().zip(pf.service.iter()) {
            assert_eq!(ns.name, ps.name, "service name mismatch");
            assert_eq!(
                ns.method.len(),
                ps.method.len(),
                "method count mismatch in {:?}",
                ns.name
            );
            for (nm, pm) in ns.method.iter().zip(ps.method.iter()) {
                assert_eq!(nm.name, pm.name, "method name mismatch");
                assert_eq!(
                    nm.input_type, pm.input_type,
                    "input_type mismatch for {:?}",
                    nm.name
                );
                assert_eq!(
                    nm.output_type, pm.output_type,
                    "output_type mismatch for {:?}",
                    nm.name
                );
                assert_eq!(
                    nm.client_streaming, pm.client_streaming,
                    "client_streaming mismatch for {:?}",
                    nm.name
                );
                assert_eq!(
                    nm.server_streaming, pm.server_streaming,
                    "server_streaming mismatch for {:?}",
                    nm.name
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Cross-validation tests
    // -----------------------------------------------------------------------

    #[test]
    fn cross_validate_scalars() {
        let src = r#"syntax = "proto3"; package test;
            message Scalars {
                double f_double = 1;
                float f_float = 2;
                int32 f_int32 = 3;
                int64 f_int64 = 4;
                uint32 f_uint32 = 5;
                uint64 f_uint64 = 6;
                sint32 f_sint32 = 7;
                sint64 f_sint64 = 8;
                fixed32 f_fixed32 = 9;
                fixed64 f_fixed64 = 10;
                sfixed32 f_sfixed32 = 11;
                sfixed64 f_sfixed64 = 12;
                bool f_bool = 13;
                string f_string = 14;
                bytes f_bytes = 15;
                repeated string tags = 16;
            }
        "#;
        run_cross_validation(src, "scalars");
    }

    #[test]
    fn cross_validate_optional_fields() {
        let src = r#"syntax = "proto3"; package test;
            message WithOptionals {
                optional int32 opt_id = 1;
                optional string opt_name = 2;
            }
        "#;
        run_cross_validation(src, "optionals");
    }

    #[test]
    fn cross_validate_nested_messages() {
        let src = r#"syntax = "proto3"; package test;
            message Outer {
                message Inner {
                    message Leaf {
                        int32 value = 1;
                    }
                    Leaf leaf = 1;
                    string name = 2;
                }
                Inner inner = 1;
                int32 id = 2;
            }
        "#;
        run_cross_validation(src, "nested");
    }

    #[test]
    fn cross_validate_oneof_and_map() {
        let src = r#"syntax = "proto3"; package test;
            message OneofMsg {
                int32 id = 1;
                oneof payload {
                    string text = 2;
                    bytes data = 3;
                    int64 number = 4;
                }
                map<string, int32> counts = 5;
            }
        "#;
        run_cross_validation(src, "oneof_map");
    }

    #[test]
    fn cross_validate_enum() {
        let src = r#"syntax = "proto3"; package test;
            enum Status {
                UNKNOWN = 0;
                ACTIVE = 1;
                INACTIVE = 2;
            }
            message WithEnum {
                Status status = 1;
                int32 id = 2;
            }
        "#;
        run_cross_validation(src, "enum");
    }

    #[test]
    fn cross_validate_services() {
        let src = r#"syntax = "proto3"; package test;
            message Request { string query = 1; }
            message Response { string result = 1; }
            service TestService {
                rpc Unary(Request) returns (Response);
                rpc ClientStream(stream Request) returns (Response);
                rpc ServerStream(Request) returns (stream Response);
                rpc Bidi(stream Request) returns (stream Response);
            }
        "#;
        run_cross_validation(src, "services");
    }

    /// Verify that field order matches source declaration order even when
    /// field numbers are non-monotonic.  This exercises span-based ordering
    /// rather than field-number ordering.
    #[test]
    fn cross_validate_out_of_order_field_numbers() {
        let src = r#"syntax = "proto3"; package test;
            message OutOfOrder {
                string b = 2;
                int32 a = 1;
                oneof payload {
                    bytes blob = 5;
                    int64 num = 3;
                }
                bool flag = 4;
            }
        "#;
        run_cross_validation(src, "out_of_order");
    }

    // -----------------------------------------------------------------------
    // Error case tests
    // -----------------------------------------------------------------------

    #[test]
    fn native_rejects_duplicate_field_number() {
        let src = r#"syntax = "proto3";
            message Dup { int32 a = 1; int32 b = 1; }
        "#;
        let result = compile_str_native(src);
        assert!(
            result.is_err(),
            "expected error for duplicate field numbers, got Ok"
        );
    }

    // -----------------------------------------------------------------------
    // OPT slice: option / reserved fidelity tests
    // -----------------------------------------------------------------------

    /// Test that the `deprecated` field option is faithfully emitted in the
    /// native descriptor.  Both native and protox should agree on the value.
    #[test]
    fn native_field_deprecated_option() {
        let src = r#"syntax = "proto3";
package opt_test;
message Msg {
    string name = 1 [deprecated = true];
    int32 count = 2;
}
"#;
        let native = compile_str_native(src).expect("native parse should succeed");
        let mut protox = compile_str(src).expect("protox compile should succeed");
        // Strip protox source_code_info; do NOT strip field options.
        for file in &mut protox.file {
            file.source_code_info = None;
        }

        let native_msg = &native.file[0].message_type[0];
        let protox_msg = &protox.file[0].message_type[0];

        // field index 0 is "name" (source order)
        let native_field = &native_msg.field[0];
        let protox_field = &protox_msg.field[0];
        assert_eq!(native_field.name(), "name");
        assert_eq!(
            native_field.options.as_ref().and_then(|o| o.deprecated),
            protox_field.options.as_ref().and_then(|o| o.deprecated),
            "deprecated option mismatch on field 'name'"
        );
        // field index 1 "count" should have no deprecated option
        let native_count = &native_msg.field[1];
        assert_eq!(
            native_count.options.as_ref().and_then(|o| o.deprecated),
            None,
            "field 'count' should not have deprecated=true"
        );
    }

    /// Test that reserved field ranges and names are faithfully emitted.
    #[test]
    fn native_reserved_ranges_and_names() {
        let src = r#"syntax = "proto3";
package reserved_test;
message Msg {
    string name = 1;
    reserved 2, 15, 9 to 11;
    reserved "foo", "bar";
}
"#;
        let native = compile_str_native(src).expect("native parse should succeed");
        let mut protox = compile_str(src).expect("protox compile should succeed");
        for file in &mut protox.file {
            file.source_code_info = None;
        }

        let native_msg = &native.file[0].message_type[0];
        let protox_msg = &protox.file[0].message_type[0];

        // Compare reserved_name lists (sort both to ignore order differences).
        let mut native_names = native_msg.reserved_name.clone();
        let mut protox_names = protox_msg.reserved_name.clone();
        native_names.sort();
        protox_names.sort();
        assert_eq!(native_names, protox_names, "reserved names mismatch");

        // Compare reserved_range counts.
        assert_eq!(
            native_msg.reserved_range.len(),
            protox_msg.reserved_range.len(),
            "reserved range count mismatch: native={:?} protox={:?}",
            native_msg.reserved_range,
            protox_msg.reserved_range,
        );
    }

    /// Test that file-level options (java_package, go_package) are emitted by
    /// the native parser and match what protox produces.
    #[test]
    fn native_file_options() {
        let src = r#"syntax = "proto3";
package file_opts_test;
option java_package = "com.example.proto";
option go_package = "example.com/proto";
message Empty {}
"#;
        let native = compile_str_native(src).expect("native parse should succeed");
        let protox = compile_str(src).expect("protox compile should succeed");

        // Compare ONLY the options OPT implements (java_package, go_package).
        let native_opts = native.file[0].options.as_ref();
        let protox_opts = protox.file[0].options.as_ref();
        assert_eq!(
            native_opts.and_then(|o| o.java_package.as_deref()),
            protox_opts.and_then(|o| o.java_package.as_deref()),
            "java_package mismatch"
        );
        assert_eq!(
            native_opts.and_then(|o| o.go_package.as_deref()),
            protox_opts.and_then(|o| o.go_package.as_deref()),
            "go_package mismatch"
        );
    }

    // -----------------------------------------------------------------------
    // proto2 tests
    // -----------------------------------------------------------------------

    // prost_types label values
    const LABEL_OPTIONAL: i32 = 1;
    const LABEL_REQUIRED: i32 = 2;

    /// proto2 required field: label must be REQUIRED (2), no synthetic oneof.
    #[test]
    fn proto2_required_field() {
        let src = r#"syntax = "proto2";
package proto2test;
message Msg {
    required int32 id = 1;
}
"#;
        let native =
            compile_str_native(src).expect("native parse should succeed for proto2 required field");

        assert_eq!(native.file.len(), 1);
        let file = &native.file[0];
        // Native emits "proto2"; protox omits the field (None)
        assert_eq!(file.syntax, Some("proto2".to_owned()), "syntax field");
        assert_eq!(file.message_type.len(), 1, "one message");

        let msg = &file.message_type[0];
        assert_eq!(msg.field.len(), 1, "one field");
        let f = &msg.field[0];
        assert_eq!(f.name, Some("id".to_owned()), "field name");
        assert_eq!(f.label, Some(LABEL_REQUIRED), "label must be REQUIRED");
        assert_eq!(
            f.oneof_index, None,
            "required field must not be in a synthetic oneof"
        );
        assert_eq!(
            f.proto3_optional, None,
            "required field must not have proto3_optional"
        );

        // No synthetic oneofs
        assert_eq!(
            msg.oneof_decl.len(),
            0,
            "required field must not produce synthetic oneof"
        );

        // Cross-validate with protox: only check label and oneof aspects
        // (syntax field is None in protox for proto2)
        let protox = compile_str(src).expect("protox compile should succeed");
        let pmsg = &protox.file[0].message_type[0];
        let pf = &pmsg.field[0];
        assert_eq!(pf.label, Some(LABEL_REQUIRED), "protox also emits REQUIRED");
        assert_eq!(pf.oneof_index, None, "protox: no oneof for required");
        assert_eq!(pmsg.oneof_decl.len(), 0, "protox: no oneofs");
    }

    /// proto2 optional field: LABEL_OPTIONAL, proto3_optional=None, no synthetic oneof.
    #[test]
    fn proto2_optional_no_synthetic_oneof() {
        let src = r#"syntax = "proto2";
package proto2test;
message Msg {
    optional int32 value = 1;
}
"#;
        let native =
            compile_str_native(src).expect("native parse should succeed for proto2 optional field");

        let msg = &native.file[0].message_type[0];
        assert_eq!(msg.field.len(), 1);
        let f = &msg.field[0];
        assert_eq!(f.label, Some(LABEL_OPTIONAL), "label must be OPTIONAL");
        assert_eq!(
            f.oneof_index, None,
            "proto2 optional must NOT be in a synthetic oneof"
        );
        assert_eq!(
            f.proto3_optional, None,
            "proto3_optional must be None for proto2"
        );

        // No synthetic oneofs generated
        assert_eq!(
            msg.oneof_decl.len(),
            0,
            "proto2 optional must not produce synthetic oneof"
        );

        // Cross-validate with protox
        let protox = compile_str(src).expect("protox compile should succeed");
        let pmsg = &protox.file[0].message_type[0];
        let pf = &pmsg.field[0];
        assert_eq!(pf.label, Some(LABEL_OPTIONAL), "protox label OPTIONAL");
        assert_eq!(pf.oneof_index, None, "protox: no oneof");
        assert_eq!(pf.proto3_optional, None, "protox: proto3_optional=None");
        assert_eq!(pmsg.oneof_decl.len(), 0, "protox: no synthetic oneofs");
    }

    /// extensions 100 to 199; → extension_range[0] = {start:100, end:200}
    #[test]
    fn proto2_extensions_range() {
        let src = r#"syntax = "proto2";
package proto2test;
message Extendable {
    optional string name = 1;
    extensions 100 to 199;
}
"#;
        let native =
            compile_str_native(src).expect("native parse should succeed for extensions range");

        let msg = &native.file[0].message_type[0];
        assert_eq!(msg.extension_range.len(), 1, "one extension_range");
        let er = &msg.extension_range[0];
        assert_eq!(er.start, Some(100), "extension_range start");
        assert_eq!(
            er.end,
            Some(200),
            "extension_range end (exclusive, 199+1=200)"
        );

        // Cross-validate with protox
        let protox = compile_str(src).expect("protox compile should succeed");
        let pmsg = &protox.file[0].message_type[0];
        assert_eq!(pmsg.extension_range.len(), 1, "protox: one extension_range");
        let per = &pmsg.extension_range[0];
        assert_eq!(per.start, Some(100), "protox extension_range start");
        assert_eq!(per.end, Some(200), "protox extension_range end");
    }

    /// extend Foo { optional int32 bar = 100; } → file.extension[0].extendee = ".mypkg.Foo"
    #[test]
    fn proto2_extend_block() {
        let src = r#"syntax = "proto2";
package mypkg;
message Foo {
    extensions 100 to 199;
}
extend Foo {
    optional int32 bar = 100;
}
"#;
        let native = compile_str_native(src).expect("native parse should succeed for extend block");

        let file = &native.file[0];
        assert_eq!(file.extension.len(), 1, "one file-level extension field");
        let ext = &file.extension[0];
        assert_eq!(ext.name, Some("bar".to_owned()), "extension field name");
        assert_eq!(ext.number, Some(100), "extension field number");
        assert_eq!(ext.label, Some(LABEL_OPTIONAL), "extension field label");
        assert_eq!(
            ext.extendee,
            Some(".mypkg.Foo".to_owned()),
            "extendee must be fully-qualified"
        );

        // Cross-validate with protox
        let protox = compile_str(src).expect("protox compile should succeed");
        let pfile = &protox.file[0];
        assert_eq!(pfile.extension.len(), 1, "protox: one file-level extension");
        let pext = &pfile.extension[0];
        assert_eq!(pext.name, ext.name, "protox extension name matches");
        assert_eq!(pext.number, ext.number, "protox extension number matches");
        assert_eq!(pext.label, ext.label, "protox extension label matches");
        assert_eq!(pext.extendee, ext.extendee, "protox extendee matches");
    }

    /// optional int32 x = 1 [default = 42]; → default_value = Some("42")
    #[test]
    fn proto2_default_value_scalar() {
        let src = r#"syntax = "proto2";
package proto2test;
message Msg {
    optional int32 x = 1 [default = 42];
}
"#;
        let native =
            compile_str_native(src).expect("native parse should succeed for scalar default");

        let msg = &native.file[0].message_type[0];
        let f = &msg.field[0];
        assert_eq!(f.name, Some("x".to_owned()), "field name");
        assert_eq!(
            f.default_value,
            Some("42".to_owned()),
            "default_value must be '42'"
        );

        // Cross-validate with protox
        let protox = compile_str(src).expect("protox compile should succeed");
        let pf = &protox.file[0].message_type[0].field[0];
        assert_eq!(
            pf.default_value,
            Some("42".to_owned()),
            "protox default_value"
        );
    }

    /// optional string s = 1 [default = "hello"]; → default_value = Some("hello")
    #[test]
    fn proto2_default_value_string() {
        let src = r#"syntax = "proto2";
package proto2test;
message Msg {
    optional string s = 1 [default = "hello"];
}
"#;
        let native =
            compile_str_native(src).expect("native parse should succeed for string default");

        let msg = &native.file[0].message_type[0];
        let f = &msg.field[0];
        assert_eq!(f.name, Some("s".to_owned()), "field name");
        assert_eq!(
            f.default_value,
            Some("hello".to_owned()),
            "default_value must be 'hello' (without quotes)"
        );

        // Cross-validate with protox
        let protox = compile_str(src).expect("protox compile should succeed");
        let pf = &protox.file[0].message_type[0].field[0];
        assert_eq!(
            pf.default_value,
            Some("hello".to_owned()),
            "protox default_value"
        );
    }

    // -----------------------------------------------------------------------
    // source_code_info tests (SCI slice)
    // -----------------------------------------------------------------------

    /// Helper: find a Location in source_code_info by path.
    fn find_location<'a>(
        fds: &'a prost_types::FileDescriptorSet,
        path: &[i32],
    ) -> Option<&'a prost_types::source_code_info::Location> {
        let sci = fds.file.first()?.source_code_info.as_ref()?;
        sci.location.iter().find(|loc| loc.path == path)
    }

    /// Verify basic span correctness for a message with 0-based line/col.
    /// `syntax = "proto3"; package t;` on line 0 (one line).
    /// `message Msg {` begins at the start of line 1 (0-indexed).
    #[test]
    fn source_code_info_spans() {
        // Deliberately simple: message starts at the beginning of line 2 (0-based).
        let src = "syntax = \"proto3\";\n\
package t;\n\
message Msg {\n\
  int32 x = 1;\n\
}\n";
        let fds = compile_str_native(src).expect("native parse ok");

        // path [4, 0] = first top-level message
        let loc = find_location(&fds, &[4, 0]).expect("message location present");

        // Message starts at line 2 (0-based), col 0.
        assert_eq!(loc.span[0], 2, "message start line (0-based)");
        assert_eq!(loc.span[1], 0, "message start col (0-based)");
    }

    /// Verify that a leading `//` comment is captured for a message.
    #[test]
    fn source_code_info_line_comments() {
        let src = "syntax = \"proto3\"; package t;\n\
// This is the message doc.\n\
message Doc {\n\
  int32 x = 1;\n\
}\n";
        let fds = compile_str_native(src).expect("native parse ok");
        let loc = find_location(&fds, &[4, 0]).expect("message location present");

        let leading = loc.leading_comments.as_deref().unwrap_or("");
        assert!(
            leading.contains("This is the message doc."),
            "expected doc comment in leading_comments, got: {leading:?}"
        );
    }

    /// Verify that a leading `/* */` block comment is captured.
    #[test]
    fn source_code_info_block_comments() {
        let src = "syntax = \"proto3\"; package t;\n\
/* A block comment. */\n\
message BlockDoc {\n\
  int32 x = 1;\n\
}\n";
        let fds = compile_str_native(src).expect("native parse ok");
        let loc = find_location(&fds, &[4, 0]).expect("message location present");

        let leading = loc.leading_comments.as_deref().unwrap_or("");
        assert!(
            leading.contains("A block comment."),
            "expected block comment in leading_comments, got: {leading:?}"
        );
    }

    /// Verify that a blank-line-separated comment becomes a detached comment.
    #[test]
    fn source_code_info_detached_comment() {
        // A comment separated from the declaration by a blank line should be
        // detached, not leading.
        let src = "syntax = \"proto3\"; package t;\n\
// Detached comment.\n\
\n\
// Leading comment.\n\
message Msg {\n\
  int32 x = 1;\n\
}\n";
        let fds = compile_str_native(src).expect("native parse ok");
        let loc = find_location(&fds, &[4, 0]).expect("message location present");

        let leading = loc.leading_comments.as_deref().unwrap_or("");
        let detached = &loc.leading_detached_comments;

        assert!(
            leading.contains("Leading comment."),
            "expected leading comment, got: {leading:?}"
        );
        assert!(
            detached.iter().any(|d| d.contains("Detached comment.")),
            "expected detached comment in leading_detached_comments, got: {detached:?}"
        );
    }

    /// Verify field-level leading AND trailing comments.
    #[test]
    fn source_code_info_field_leading_comment() {
        let src = "syntax = \"proto3\"; package t;\n\
message Msg {\n\
  // The identifier field.\n\
  int32 id = 1; // inline trailing\n\
  string name = 2;\n\
}\n";
        let fds = compile_str_native(src).expect("native parse ok");
        // field index 0 = id (first in source order)
        let loc = find_location(&fds, &[4, 0, 2, 0]).expect("field[0] location present");

        let leading = loc.leading_comments.as_deref().unwrap_or("");
        assert!(
            leading.contains("The identifier field."),
            "expected field leading comment, got: {leading:?}"
        );

        let trailing = loc.trailing_comments.as_deref().unwrap_or("");
        assert!(
            trailing.contains("inline trailing"),
            "expected trailing comment, got: {trailing:?}"
        );
    }

    /// Verify that field indices align with source-declaration order even when
    /// field numbers are non-monotonic (mirrors cross_validate_out_of_order).
    #[test]
    fn source_code_info_field_order_with_oneof() {
        let src = "syntax = \"proto3\"; package t;\n\
message M {\n\
  // Comment for b.\n\
  string b = 2;\n\
  // Comment for a.\n\
  int32 a = 1;\n\
  oneof payload {\n\
    bytes blob = 5;\n\
    int64 num = 3;\n\
  }\n\
  bool flag = 4;\n\
}\n";
        let fds = compile_str_native(src).expect("native parse ok");

        // Source order (all fields including oneof members, sorted by span.start):
        //   idx 0 → b (field number 2)
        //   idx 1 → a (field number 1)
        //   idx 2 → blob (oneof member)
        //   idx 3 → num  (oneof member) — but wait, we need to verify actual ordering

        // Key assertion: location path [4,0,2,0] (first field in source order = b)
        // should have "Comment for b." in leading_comments.
        let loc0 = find_location(&fds, &[4, 0, 2, 0]).expect("field[0] location");
        let leading0 = loc0.leading_comments.as_deref().unwrap_or("");
        assert!(
            leading0.contains("Comment for b."),
            "field[0] should be 'b', leading={leading0:?}"
        );

        // path [4,0,2,1] = second field in source order = a
        let loc1 = find_location(&fds, &[4, 0, 2, 1]).expect("field[1] location");
        let leading1 = loc1.leading_comments.as_deref().unwrap_or("");
        assert!(
            leading1.contains("Comment for a."),
            "field[1] should be 'a', leading={leading1:?}"
        );
    }

    /// Verify enum and enum-value locations.
    #[test]
    fn source_code_info_enum_and_values() {
        let src = "syntax = \"proto3\"; package t;\n\
// Status enumeration.\n\
enum Status {\n\
  // Default/unknown.\n\
  UNKNOWN = 0;\n\
  ACTIVE = 1;\n\
}\n";
        let fds = compile_str_native(src).expect("native parse ok");

        // path [5, 0] = first top-level enum
        let en_loc = find_location(&fds, &[5, 0]).expect("enum location");
        let en_leading = en_loc.leading_comments.as_deref().unwrap_or("");
        assert!(
            en_leading.contains("Status enumeration."),
            "enum leading comment: {en_leading:?}"
        );

        // path [5, 0, 2, 0] = first enum value
        let val_loc = find_location(&fds, &[5, 0, 2, 0]).expect("enum value[0] location");
        let val_leading = val_loc.leading_comments.as_deref().unwrap_or("");
        assert!(
            val_leading.contains("Default/unknown."),
            "enum value[0] leading comment: {val_leading:?}"
        );
    }

    /// Verify service and method locations.
    #[test]
    fn source_code_info_service_and_methods() {
        let src = "syntax = \"proto3\"; package t;\n\
message Req { string q = 1; }\n\
message Resp { string r = 1; }\n\
// The service.\n\
service Svc {\n\
  // The method.\n\
  rpc Call(Req) returns (Resp);\n\
}\n";
        let fds = compile_str_native(src).expect("native parse ok");

        // path [6, 0] = first service
        let svc_loc = find_location(&fds, &[6, 0]).expect("service location");
        let svc_leading = svc_loc.leading_comments.as_deref().unwrap_or("");
        assert!(
            svc_leading.contains("The service."),
            "service leading: {svc_leading:?}"
        );

        // path [6, 0, 2, 0] = first method
        let method_loc = find_location(&fds, &[6, 0, 2, 0]).expect("method location");
        let method_leading = method_loc.leading_comments.as_deref().unwrap_or("");
        assert!(
            method_leading.contains("The method."),
            "method leading: {method_leading:?}"
        );
    }

    /// End-to-end codegen oracle: native FDS with leading comments → generate
    /// Rust with `generate_docs: true` → doc comments appear in output.
    #[test]
    fn source_code_info_codegen_oracle() {
        use oxiproto_codegen::{generate_with_options, CodegenOptions};

        let src = "syntax = \"proto3\"; package docs;\n\
// A request message.\n\
message Request {\n\
  // The query string.\n\
  string query = 1;\n\
}\n";
        let fds = compile_str_native(src).expect("native parse ok");

        let opts = CodegenOptions {
            generate_docs: true,
            ..CodegenOptions::default()
        };
        let code = generate_with_options(&fds, &opts).expect("codegen ok");

        // Generated code should contain the doc-comment text.
        assert!(
            code.contains("A request message."),
            "expected doc comment for message in generated code, got:\n{code}"
        );
        assert!(
            code.contains("The query string."),
            "expected doc comment for field in generated code, got:\n{code}"
        );
    }

    /// Cross-validate source_code_info comments between native and protox.
    /// For every native Location with `leading_comments.is_some()`, find the
    /// protox Location with the same path and assert the comments match.
    /// We never assert equal *counts* (protox emits many more sub-locations).
    #[test]
    fn source_code_info_cross_validate() {
        let src = "syntax = \"proto3\"; package t;\n\
// The message.\n\
message Msg {\n\
  // The field.\n\
  int32 x = 1;\n\
  string name = 2;\n\
}\n\
// An enum.\n\
enum Color {\n\
  // Default.\n\
  RED = 0;\n\
  GREEN = 1;\n\
}\n";

        let native_fds = compile_str_native(src).expect("native parse ok");
        let protox_fds = compile_str(src).expect("protox parse ok");

        assert_source_code_info_comments_match(&native_fds, &protox_fds);
    }

    /// Helper: for every native Location with leading_comments, assert protox
    /// has the same-path Location with equal leading_comments.
    fn assert_source_code_info_comments_match(
        native_fds: &prost_types::FileDescriptorSet,
        protox_fds: &prost_types::FileDescriptorSet,
    ) {
        use std::collections::HashMap;

        let native_file = native_fds.file.first().expect("native has file");
        let protox_file = protox_fds.file.first().expect("protox has file");

        let native_sci = match native_file.source_code_info.as_ref() {
            Some(s) => s,
            None => return, // nothing to compare
        };
        let protox_sci = match protox_file.source_code_info.as_ref() {
            Some(s) => s,
            None => panic!("protox should always emit source_code_info"),
        };

        // Build path → Location map for protox.
        let protox_map: HashMap<&[i32], &prost_types::source_code_info::Location> = protox_sci
            .location
            .iter()
            .map(|loc| (loc.path.as_slice(), loc))
            .collect();

        for native_loc in &native_sci.location {
            let Some(ref native_leading) = native_loc.leading_comments else {
                continue; // only check entries that have comments
            };

            let path = native_loc.path.as_slice();
            let protox_loc = match protox_map.get(path) {
                Some(l) => l,
                None => {
                    // If protox doesn't emit a location for this path, skip
                    // (protox may merge or omit some sub-paths).
                    continue;
                }
            };

            if let Some(ref protox_leading) = protox_loc.leading_comments {
                assert_eq!(
                    native_leading, protox_leading,
                    "leading_comments mismatch at path {path:?}: \
                     native={native_leading:?}, protox={protox_leading:?}"
                );
            }
            // If protox has no leading_comments for this path, skip — we only
            // assert equality when protox also has comments.
        }
    }

    // -----------------------------------------------------------------------
    // Message-literal option value tests (run 6)
    // -----------------------------------------------------------------------

    /// A proto with a message-literal option value is parsed and the
    /// `uninterpreted_option` entry for that option should have a non-empty
    /// `aggregate_value` containing the expected key-value text.
    #[test]
    fn custom_option_message_literal_in_uninterpreted() {
        let src = r#"syntax = "proto3";
message Foo {
  option some_option_name = { key: "value", num: 7 };
  int32 id = 1;
}
"#;
        let fds = compile_str_native(src)
            .expect("native parse should succeed for message-literal option");

        let msg = fds
            .file
            .first()
            .expect("file present")
            .message_type
            .first()
            .expect("message present");

        let opts = msg
            .options
            .as_ref()
            .expect("MessageOptions should be present (has uninterpreted option)");

        let entry = opts
            .uninterpreted_option
            .iter()
            .find(|u| {
                u.name
                    .first()
                    .map(|np| np.name_part == "some_option_name")
                    .unwrap_or(false)
            })
            .expect("uninterpreted_option for some_option_name should be present");

        let agg = entry
            .aggregate_value
            .as_deref()
            .expect("aggregate_value must be set for a message-literal option");

        assert!(
            agg.contains("key"),
            "aggregate_value should contain 'key', got: {agg:?}"
        );
        assert!(
            agg.contains("value"),
            "aggregate_value should contain 'value', got: {agg:?}"
        );
        assert!(
            agg.contains("num"),
            "aggregate_value should contain 'num', got: {agg:?}"
        );
        assert!(
            agg.contains("7"),
            "aggregate_value should contain '7', got: {agg:?}"
        );
    }

    /// Verify that scalar options (like `deprecated = true`) still work
    /// correctly after the message-literal feature was added.
    #[test]
    fn custom_option_scalar_still_works() {
        let src = r#"syntax = "proto3";
message DeprecatedMsg {
  option deprecated = true;
  int32 id = 1;
}
"#;
        let fds =
            compile_str_native(src).expect("native parse should succeed for deprecated message");

        let msg = fds
            .file
            .first()
            .expect("file present")
            .message_type
            .first()
            .expect("message present");

        let opts = msg
            .options
            .as_ref()
            .expect("MessageOptions should be present");

        assert_eq!(
            opts.deprecated,
            Some(true),
            "deprecated option must be true"
        );

        // The uninterpreted_option list should NOT contain 'deprecated'
        // (it's handled as an interpreted option).
        let has_deprecated_uninterp = opts.uninterpreted_option.iter().any(|u| {
            u.name
                .first()
                .map(|np| np.name_part == "deprecated")
                .unwrap_or(false)
        });
        assert!(
            !has_deprecated_uninterp,
            "'deprecated' must be interpreted, not in uninterpreted_option"
        );
    }

    /// A proto with a nested message-literal option value should produce a
    /// non-empty `aggregate_value` containing both outer and inner fields.
    #[test]
    fn option_value_message_literal_nested() {
        let src = r#"syntax = "proto3";
message Foo {
  option nested_opt = { inner: { x: 1 } };
  int32 id = 1;
}
"#;
        let fds = compile_str_native(src)
            .expect("native parse should succeed for nested message-literal option");

        let msg = fds
            .file
            .first()
            .expect("file present")
            .message_type
            .first()
            .expect("message present");

        let opts = msg
            .options
            .as_ref()
            .expect("MessageOptions should be present");

        let entry = opts
            .uninterpreted_option
            .iter()
            .find(|u| {
                u.name
                    .first()
                    .map(|np| np.name_part == "nested_opt")
                    .unwrap_or(false)
            })
            .expect("uninterpreted_option for nested_opt should be present");

        let agg = entry
            .aggregate_value
            .as_deref()
            .expect("aggregate_value must be set for a nested message-literal option");

        assert!(
            !agg.is_empty(),
            "aggregate_value must be non-empty for nested literal"
        );
        assert!(
            agg.contains("inner"),
            "aggregate_value should contain 'inner', got: {agg:?}"
        );
        assert!(
            agg.contains("x"),
            "aggregate_value should contain 'x', got: {agg:?}"
        );
        assert!(
            agg.contains("1"),
            "aggregate_value should contain '1', got: {agg:?}"
        );
    }

    // -----------------------------------------------------------------------
    // COPT tests: scalar custom/extension option handling (run 7)
    // -----------------------------------------------------------------------

    /// Canary test documenting protox behavior for scalar extension options.
    ///
    /// protox errors on undefined extension option names because its
    /// interpretation phase requires all extension names to be resolved.
    /// This behavior may change across protox versions — this test acts as a
    /// canary.
    ///
    /// Under `native-parser`, `compile_str` routes through the native parser
    /// which has no extension-resolution phase, so the same input succeeds.
    /// This test is therefore gated to the non-native path only.
    #[cfg(not(feature = "native-parser"))]
    #[test]
    fn protox_scalar_custom_option_behavior() {
        let src = r#"syntax = "proto3";
option (my.fake_option) = true;
message M {
  option (my.fake_msg_option) = 42;
  string f = 1 [(my.fake_field_option) = "hello"];
}
"#;
        // NOTE: protox errors on undefined extensions because it runs a full
        // interpretation pass that requires extension names to be resolved.
        // The native parser preserves unknown options as uninterpreted_option
        // entries (consistent with how it handles message-literal custom options).
        let result = compile_str(src);
        assert!(
            result.is_err(),
            "protox should error on undefined extension options; if this fails, \
             protox changed behavior and the COPT tests below may need updating"
        );
    }

    /// Test that native parser preserves scalar custom options as
    /// `uninterpreted_option` entries.
    ///
    /// Since the native parser has no interpretation phase, it stores
    /// all unknown option values (scalar or message-literal) in
    /// `uninterpreted_option`, which is internally consistent.
    #[test]
    fn native_scalar_custom_option_preserved_as_uninterpreted() {
        let src = r#"syntax = "proto3";
message M {
  option (my.fake_msg_option) = 42;
  string f = 1 [(my.fake_field_option) = "hello"];
}
"#;
        let fds = compile_str_native(src)
            .expect("native parser should succeed (no extension resolution phase)");

        let file = fds.file.first().expect("file present");
        let msg = file.message_type.first().expect("message M present");

        // Message-level custom option: (my.fake_msg_option) = 42
        // Should be in uninterpreted_option with positive_int_value = 42.
        let msg_opts = msg
            .options
            .as_ref()
            .expect("MessageOptions should be present");
        let msg_uninterp = msg_opts
            .uninterpreted_option
            .iter()
            .find(|u| {
                u.name
                    .first()
                    .map(|np| np.name_part == "my.fake_msg_option" && np.is_extension)
                    .unwrap_or(false)
            })
            .expect("uninterpreted_option for (my.fake_msg_option) should be present");
        assert_eq!(
            msg_uninterp.positive_int_value,
            Some(42),
            "(my.fake_msg_option) = 42 should set positive_int_value = 42"
        );

        // Field-level custom option: (my.fake_field_option) = "hello"
        // Should be in uninterpreted_option with string_value = b"hello".
        let field = msg.field.first().expect("field 'f' present");
        let field_opts = field
            .options
            .as_ref()
            .expect("FieldOptions should be present");
        let field_uninterp = field_opts
            .uninterpreted_option
            .iter()
            .find(|u| {
                u.name
                    .first()
                    .map(|np| np.name_part == "my.fake_field_option" && np.is_extension)
                    .unwrap_or(false)
            })
            .expect("uninterpreted_option for (my.fake_field_option) should be present");
        assert_eq!(
            field_uninterp.string_value.as_deref(),
            Some(b"hello".as_slice()),
            "(my.fake_field_option) = \"hello\" should set string_value = b\"hello\""
        );
    }

    /// Test that bool scalar options go to identifier_value "true"/"false".
    #[test]
    fn native_bool_custom_option_as_identifier_value() {
        let src = r#"syntax = "proto3";
message M {
  option (my.bool_opt) = true;
  int32 x = 1;
}
"#;
        let fds = compile_str_native(src).expect("native parser should succeed");
        let msg = fds.file[0].message_type.first().expect("message present");
        let opts = msg.options.as_ref().expect("options present");
        let u = opts
            .uninterpreted_option
            .iter()
            .find(|u| {
                u.name
                    .first()
                    .map(|np| np.name_part == "my.bool_opt")
                    .unwrap_or(false)
            })
            .expect("bool option in uninterpreted_option");
        assert_eq!(
            u.identifier_value.as_deref(),
            Some("true"),
            "bool true should become identifier_value = 'true'"
        );
    }

    /// Test that negative integer custom options go to negative_int_value.
    #[test]
    fn native_negative_int_custom_option() {
        let src = r#"syntax = "proto3";
message M {
  option (my.neg_opt) = -7;
  int32 x = 1;
}
"#;
        let fds = compile_str_native(src).expect("native parser should succeed");
        let msg = fds.file[0].message_type.first().expect("message present");
        let opts = msg.options.as_ref().expect("options present");
        let u = opts
            .uninterpreted_option
            .iter()
            .find(|u| {
                u.name
                    .first()
                    .map(|np| np.name_part == "my.neg_opt")
                    .unwrap_or(false)
            })
            .expect("neg option in uninterpreted_option");
        assert_eq!(
            u.negative_int_value,
            Some(7),
            "int -7 should set negative_int_value = 7 (absolute magnitude)"
        );
        assert_eq!(
            u.positive_int_value, None,
            "positive_int_value must be None for negative"
        );
    }

    /// Test that the NamePart split is correct for `(my.fake_option)`:
    /// one part with is_extension=true and name_part = "my.fake_option".
    #[test]
    fn native_custom_option_name_parts_extension_format() {
        let src = r#"syntax = "proto3";
message M {
  option (my.pkg.option_name) = 1;
  int32 x = 1;
}
"#;
        let fds = compile_str_native(src).expect("native parser should succeed");
        let msg = fds.file[0].message_type.first().expect("message present");
        let opts = msg.options.as_ref().expect("options present");
        let u = opts
            .uninterpreted_option
            .first()
            .expect("one uninterpreted_option entry");
        assert_eq!(
            u.name.len(),
            1,
            "extension option name should have exactly one NamePart"
        );
        assert_eq!(
            u.name[0].name_part, "my.pkg.option_name",
            "name_part should be the dotted extension name"
        );
        assert!(
            u.name[0].is_extension,
            "is_extension must be true for (...)"
        );
    }

    // -----------------------------------------------------------------------
    // GRP tests: proto2 group field support (run 7)
    // -----------------------------------------------------------------------

    // prost_types type values
    const TYPE_GROUP: i32 = 10;

    /// Cross-validate a proto2 file with a `repeated group` field.
    ///
    /// Verifies: field type = TYPE_GROUP (10), field name = lowercased group name,
    /// field type_name = FQN of the nested message, nested message exists.
    #[test]
    fn proto2_group_repeated_field() {
        let src = r#"syntax = "proto2";
package grptest;
message SearchResponse {
  repeated group Result = 1 {
    required string url = 2;
    optional string title = 3;
  }
}
"#;
        // Native parse: verify group field structure
        let native =
            compile_str_native(src).expect("native parser should handle proto2 group fields");

        let file = &native.file[0];
        let msg = &file.message_type[0];
        assert_eq!(msg.name, Some("SearchResponse".to_owned()));

        // The group field
        assert_eq!(msg.field.len(), 1, "one field: the group field");
        let gf = &msg.field[0];
        assert_eq!(
            gf.name,
            Some("result".to_owned()),
            "field name = lowercased group name"
        );
        assert_eq!(gf.r#type, Some(TYPE_GROUP), "field type = TYPE_GROUP (10)");
        assert_eq!(
            gf.type_name,
            Some(".grptest.SearchResponse.Result".to_owned()),
            "type_name = FQN of the synthesized nested message"
        );
        assert_eq!(
            gf.json_name,
            Some("Result".to_owned()),
            "json_name = capitalized group name"
        );

        // The synthesized nested message
        assert_eq!(
            msg.nested_type.len(),
            1,
            "one nested message: the group message"
        );
        let nested = &msg.nested_type[0];
        assert_eq!(
            nested.name,
            Some("Result".to_owned()),
            "nested message name = group name"
        );
        assert_eq!(
            nested.field.len(),
            2,
            "two fields in group body: url and title"
        );

        // Cross-validate with protox
        let protox = compile_str(src).expect("protox should handle proto2 group fields");
        let pmsg = &protox.file[0].message_type[0];
        let pgf = pmsg
            .field
            .iter()
            .find(|f| f.name.as_deref() == Some("result"))
            .expect("protox group field 'result'");
        assert_eq!(pgf.r#type, Some(TYPE_GROUP), "protox: TYPE_GROUP");
        assert_eq!(pgf.type_name, gf.type_name, "protox: type_name matches");
    }

    /// Verify a `optional group` field (single-instance group).
    #[test]
    fn proto2_group_field() {
        let src = r#"syntax = "proto2";
package grptest2;
message Outer {
  optional group Inner = 1 {
    optional int32 value = 1;
  }
}
"#;
        let native =
            compile_str_native(src).expect("native parser should handle optional proto2 group");

        let msg = &native.file[0].message_type[0];
        let gf = &msg.field[0];
        assert_eq!(gf.name, Some("inner".to_owned()), "field name = lowercased");
        assert_eq!(gf.r#type, Some(TYPE_GROUP), "field type = TYPE_GROUP");
        assert_eq!(
            gf.type_name,
            Some(".grptest2.Outer.Inner".to_owned()),
            "type_name = nested message FQN"
        );

        // Cross-validate with protox
        let protox = compile_str(src).expect("protox handles optional proto2 group");
        let pmsg = &protox.file[0].message_type[0];
        let pgf = pmsg
            .field
            .iter()
            .find(|f| f.name.as_deref() == Some("inner"))
            .expect("protox: group field 'inner'");
        assert_eq!(pgf.r#type, Some(TYPE_GROUP), "protox: TYPE_GROUP");
        assert_eq!(pgf.type_name, gf.type_name, "protox: type_name matches");
    }

    /// Lowercase group name must be rejected with a parse error.
    #[test]
    fn proto2_group_malformed_lowercase_name() {
        let src = r#"syntax = "proto2";
message M {
  optional group result = 1 {
    required string url = 2;
  }
}
"#;
        let result = compile_str_native(src);
        assert!(
            result.is_err(),
            "lowercase group name 'result' should produce a parse error"
        );
    }

    // -----------------------------------------------------------------------
    // Edition 2023 descriptor tests
    // -----------------------------------------------------------------------

    /// Edition 2023 file should produce `syntax = "editions"` in the FDP.
    #[test]
    fn edition_2023_syntax_sentinel_in_fdp() {
        let src = r#"edition = "2023";
package ed2023;
message Hello {
  string name = 1;
  int32  id   = 2;
}
"#;
        let fds = compile_str_native(src).expect("edition 2023 must compile");
        assert_eq!(fds.file.len(), 1);
        let fdp = &fds.file[0];
        assert_eq!(
            fdp.syntax.as_deref(),
            Some("editions"),
            "Edition 2023 must produce syntax = 'editions' sentinel"
        );
        assert_eq!(fdp.package.as_deref(), Some("ed2023"));
        assert_eq!(fdp.message_type.len(), 1);
        assert_eq!(fdp.message_type[0].name.as_deref(), Some("Hello"));
    }

    /// Edition 2023 `optional` field gets a synthetic oneof (same as proto3 optional).
    #[test]
    fn edition_2023_optional_gets_synthetic_oneof() {
        let src = r#"edition = "2023";
message Msg {
  optional string name = 1;
  int32 id = 2;
}
"#;
        let fds = compile_str_native(src).expect("must compile");
        let msg = &fds.file[0].message_type[0];
        // `optional` in edition 2023 → synthetic oneof + proto3_optional=true
        let opt_field = msg
            .field
            .iter()
            .find(|f| f.name.as_deref() == Some("name"))
            .expect("name field");
        assert_eq!(
            opt_field.proto3_optional,
            Some(true),
            "Edition 2023 optional field must have proto3_optional = true"
        );
        assert!(
            opt_field.oneof_index.is_some(),
            "Edition 2023 optional field must be in a synthetic oneof"
        );
        // The synthetic oneof must exist
        assert!(
            !msg.oneof_decl.is_empty(),
            "synthetic oneof must be added for optional field"
        );
    }

    /// Edition 2023 singular (unlabeled) field has no synthetic oneof.
    #[test]
    fn edition_2023_singular_no_synthetic_oneof() {
        let src = r#"edition = "2023";
message Msg {
  int32 count = 1;
  string name = 2;
}
"#;
        let fds = compile_str_native(src).expect("must compile");
        let msg = &fds.file[0].message_type[0];
        for f in &msg.field {
            assert!(
                f.proto3_optional.is_none() || f.proto3_optional == Some(false),
                "singular field {:?} must not have proto3_optional=true",
                f.name
            );
            assert!(
                f.oneof_index.is_none(),
                "singular field {:?} must not be in a oneof",
                f.name
            );
        }
        assert!(
            msg.oneof_decl.is_empty(),
            "no synthetic oneofs for singular fields"
        );
    }

    /// Edition 2023 map fields desugar to `XxxEntry` nested messages (same as proto3).
    #[test]
    fn edition_2023_map_field_desugaring() {
        let src = r#"edition = "2023";
package edmap;
message Config {
  map<string, int32> settings = 1;
}
"#;
        let fds = compile_str_native(src).expect("must compile");
        let msg = &fds.file[0].message_type[0];
        // The map field itself should be repeated
        let map_field = msg
            .field
            .iter()
            .find(|f| f.name.as_deref() == Some("settings"))
            .expect("settings field");
        assert_eq!(
            map_field.label,
            Some(prost_types::field_descriptor_proto::Label::Repeated as i32),
            "map field must be LABEL_REPEATED"
        );
        assert_eq!(
            map_field.type_name.as_deref(),
            Some(".edmap.Config.SettingsEntry"),
            "map field type_name = XxxEntry FQN"
        );
        // A nested XxxEntry message must exist with map_entry=true
        let entry_msg = msg
            .nested_type
            .iter()
            .find(|m| m.name.as_deref() == Some("SettingsEntry"))
            .expect("SettingsEntry nested message");
        let map_entry = entry_msg
            .options
            .as_ref()
            .and_then(|o| o.map_entry)
            .unwrap_or(false);
        assert!(map_entry, "SettingsEntry must have map_entry = true");
    }

    /// Edition 2023 repeated fields are correctly labelled.
    #[test]
    fn edition_2023_repeated_field_label() {
        let src = r#"edition = "2023";
message List {
  repeated string tags = 1;
  repeated int64 ids = 2;
}
"#;
        let fds = compile_str_native(src).expect("must compile");
        let msg = &fds.file[0].message_type[0];
        for f in &msg.field {
            assert_eq!(
                f.label,
                Some(prost_types::field_descriptor_proto::Label::Repeated as i32),
                "field {:?} must be LABEL_REPEATED",
                f.name
            );
        }
    }

    /// Edition 2023 services are emitted correctly.
    #[test]
    fn edition_2023_service_descriptor() {
        let src = r#"edition = "2023";
message Req { int32 id = 1; }
message Resp { string result = 1; }
service Greeter {
  rpc SayHello (Req) returns (Resp);
  rpc Subscribe (Req) returns (stream Resp);
}
"#;
        let fds = compile_str_native(src).expect("must compile");
        let fdp = &fds.file[0];
        assert_eq!(fdp.service.len(), 1);
        let svc = &fdp.service[0];
        assert_eq!(svc.name.as_deref(), Some("Greeter"));
        assert_eq!(svc.method.len(), 2);
        assert!(!svc.method[0].server_streaming.unwrap_or(false));
        assert!(svc.method[1].server_streaming.unwrap_or(false));
    }

    /// Edition 2023 nested messages are correctly represented.
    #[test]
    fn edition_2023_nested_message_descriptor() {
        let src = r#"edition = "2023";
package ednest;
message Outer {
  message Inner {
    int32 value = 1;
  }
  Inner inner = 1;
  repeated Inner items = 2;
}
"#;
        let fds = compile_str_native(src).expect("must compile");
        let outer = &fds.file[0].message_type[0];
        assert_eq!(outer.name.as_deref(), Some("Outer"));
        assert_eq!(outer.nested_type.len(), 1);
        assert_eq!(outer.nested_type[0].name.as_deref(), Some("Inner"));
        assert_eq!(outer.field.len(), 2);
    }
}
