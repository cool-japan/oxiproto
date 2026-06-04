//! Pure-Rust proptest no-panic fuzz harness for the .proto parser.
//!
//! This test feeds adversarial and arbitrary byte sequences to the proto parser
//! and asserts that it never panics — only returns `Ok` or `Err`. This verifies
//! the parser's robustness against malformed, truncated, or adversarial input
//! without requiring cargo-fuzz/libFuzzer (which is C++, violating the
//! Pure-Rust Policy).
//!
//! Strategy categories:
//! 1. Fully arbitrary `Vec<u8>` converted to lossy UTF-8 — tests garbage input.
//! 2. Structurally valid prefix + truncated/random suffix — tests early-EOF handling.
//! 3. Valid-looking keyword patterns with corrupted bodies — tests parser recovery.
//! 4. Deeply nested braces — tests recursion depth limits and stack overflow safety.
//! 5. Very long strings and identifiers — tests allocator safety.
//! 6. Unicode strings (valid and invalid UTF-8) — tests string handling.

use oxiproto_build::parser::parse_file;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Strategy 1: Fully arbitrary bytes → lossy UTF-8
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fuzz_arbitrary_bytes_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        let s = String::from_utf8_lossy(&bytes);
        // Must never panic — Ok or Err both accepted.
        let _ = parse_file(&s);
    }
}

proptest! {
    #[test]
    fn fuzz_arbitrary_short_bytes_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..64)) {
        let s = String::from_utf8_lossy(&bytes);
        let _ = parse_file(&s);
    }
}

// ---------------------------------------------------------------------------
// Strategy 2: Valid prefix + random suffix (truncation + corruption)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fuzz_valid_prefix_then_garbage(
        prefix in prop_oneof![
            Just(r#"syntax = "proto3";"#.to_owned()),
            Just(r#"syntax = "proto2"; package foo.bar;"#.to_owned()),
            Just(r#"syntax = "proto3"; message M {"#.to_owned()),
            Just(r#"syntax = "proto3"; enum E { A = 0;"#.to_owned()),
            Just(r#"syntax = "proto3"; import "other.proto";"#.to_owned()),
        ],
        suffix in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        let s = format!("{}{}", prefix, String::from_utf8_lossy(&suffix));
        let _ = parse_file(&s);
    }
}

// ---------------------------------------------------------------------------
// Strategy 3: Valid-looking keyword patterns with corrupted bodies
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fuzz_keyword_with_random_body(
        keyword in prop_oneof![
            Just("message"),
            Just("enum"),
            Just("service"),
            Just("oneof"),
            Just("rpc"),
            Just("import"),
            Just("package"),
            Just("option"),
            Just("syntax"),
            Just("extend"),
            Just("extensions"),
            Just("reserved"),
        ],
        body in prop::string::string_regex("[a-zA-Z0-9 _.,;:={}()<>\"'/\\\\*@#%^&!\\[\\]+-]{0,200}").unwrap(),
    ) {
        let src = format!("{keyword} {body}");
        let _ = parse_file(&src);
    }
}

// ---------------------------------------------------------------------------
// Strategy 4: Deeply nested braces
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fuzz_deeply_nested_braces_no_panic(depth in 1usize..100) {
        let open = "{".repeat(depth);
        let close = "}".repeat(depth);
        let src = format!(r#"syntax = "proto3"; message M {open}{close}"#);
        let _ = parse_file(&src);
    }
}

proptest! {
    #[test]
    fn fuzz_unbalanced_braces_no_panic(
        opens in 0usize..50,
        closes in 0usize..50,
    ) {
        let open = "{".repeat(opens);
        let close = "}".repeat(closes);
        let src = format!(r#"syntax = "proto3"; message M {open} int32 x = 1; {close}"#);
        let _ = parse_file(&src);
    }
}

// ---------------------------------------------------------------------------
// Strategy 5: Very long strings and identifiers
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fuzz_very_long_identifier_no_panic(
        len in 1usize..4096,
        char_seed in any::<u8>(),
    ) {
        let id_char = if char_seed % 2 == 0 { 'a' } else { 'Z' };
        let long_id = id_char.to_string().repeat(len);
        let src = format!(r#"syntax = "proto3"; message {} {{ }}"#, long_id);
        let _ = parse_file(&src);
    }
}

proptest! {
    #[test]
    fn fuzz_very_long_string_literal_no_panic(
        content in prop::string::string_regex("[a-z]{0,2000}").unwrap(),
    ) {
        let src = format!(r#"syntax = "proto3"; option java_package = "{content}";"#);
        let _ = parse_file(&src);
    }
}

// ---------------------------------------------------------------------------
// Strategy 6: Unicode and non-ASCII content
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fuzz_unicode_string_no_panic(s in "\\PC*") {
        // \\PC* generates arbitrary Unicode strings including surrogates and
        // control characters.
        let _ = parse_file(&s);
    }
}

proptest! {
    #[test]
    fn fuzz_unicode_in_string_literal_no_panic(content in "\\PC{0,200}") {
        let src = format!(r#"syntax = "proto3"; option foo = "{content}";"#);
        let _ = parse_file(&src);
    }
}

// ---------------------------------------------------------------------------
// Strategy 7: Repeated valid field declarations with random numbers
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fuzz_message_with_random_field_numbers_no_panic(
        nums in prop::collection::vec(any::<u64>(), 1..20),
    ) {
        let fields: String = nums
            .iter()
            .enumerate()
            .map(|(i, n)| format!("  int32 field_{i} = {n};\n"))
            .collect();
        let src = format!("syntax = \"proto3\";\nmessage M {{\n{fields}}}\n");
        let _ = parse_file(&src);
    }
}

// ---------------------------------------------------------------------------
// Strategy 8: Adversarial option values (potential integer/float overflow)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fuzz_extreme_integer_literals_no_panic(
        n in prop_oneof![
            Just(u128::MAX.to_string()),
            Just(i128::MIN.to_string()),
            Just("99999999999999999999999999999999999999".to_owned()),
            Just("-99999999999999999999999999999999999999".to_owned()),
            Just("0".to_owned()),
            Just("1".to_owned()),
            (any::<u64>()).prop_map(|v| v.to_string()),
            (any::<i64>()).prop_map(|v| v.to_string()),
        ],
    ) {
        let src = format!("syntax = \"proto3\"; message M {{ int32 f = {n}; }}\n");
        let _ = parse_file(&src);
    }
}

proptest! {
    #[test]
    fn fuzz_extreme_float_literals_no_panic(
        f in prop_oneof![
            Just("nan".to_owned()),
            Just("inf".to_owned()),
            Just("-inf".to_owned()),
            Just("1e308".to_owned()),
            Just("-1e308".to_owned()),
            Just("1.7976931348623157e+308".to_owned()),
            Just("0.0".to_owned()),
            (any::<f64>()).prop_map(|v| format!("{v}")),
        ],
    ) {
        let src = format!("syntax = \"proto3\"; option x = {f};\n");
        let _ = parse_file(&src);
    }
}

// ---------------------------------------------------------------------------
// Strategy 9: Comment injection (shouldn't break parsing)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fuzz_comment_injection_no_panic(
        comment in "([^\n*/]|[^/][*]|[^*][/]){0,200}",
    ) {
        let src = format!(
            "// {comment}\nsyntax = \"proto3\"; /* block {comment} */ message M {{ }}\n"
        );
        let _ = parse_file(&src);
    }
}

// ---------------------------------------------------------------------------
// Strategy 10: Import path injection
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fuzz_import_paths_no_panic(
        path in prop::string::string_regex("[a-zA-Z0-9/_.-]{0,100}").unwrap(),
    ) {
        let src = format!(
            "syntax = \"proto3\";\nimport \"{path}\";\nmessage M {{ }}\n"
        );
        let _ = parse_file(&src);
    }
}
