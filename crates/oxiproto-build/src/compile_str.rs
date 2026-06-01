#![forbid(unsafe_code)]

//! Compile an inline proto definition string to a [`prost_types::FileDescriptorSet`].

use crate::BuildError;
use prost_types::FileDescriptorSet;

#[cfg(not(feature = "native-parser"))]
use std::io::Write as _;
#[cfg(not(feature = "native-parser"))]
use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonically increasing counter that provides unique temp-file names when
/// multiple calls happen concurrently (e.g. in parallel test threads).
///
/// Only used by the protox (non-native) path.
#[cfg(not(feature = "native-parser"))]
static COUNTER: AtomicU64 = AtomicU64::new(0);

// Inner implementation for the native-parser path.
// Kept as a free function so `compile_str` can delegate cleanly without a
// cfg-gated `return` (which triggers `clippy::needless_return`).
#[cfg(feature = "native-parser")]
#[inline]
fn compile_str_impl(proto_source: &str) -> Result<FileDescriptorSet, BuildError> {
    crate::compile_str_native(proto_source)
}

// Inner implementation for the protox (non-native) path.
#[cfg(not(feature = "native-parser"))]
fn compile_str_impl(proto_source: &str) -> Result<FileDescriptorSet, BuildError> {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_dir = std::env::temp_dir();
    let proto_path = temp_dir.join(format!(
        "oxiproto-compile-str-{}-{}.proto",
        std::process::id(),
        n
    ));

    // Write proto source to a temporary file.
    {
        let mut f = std::fs::File::create(&proto_path)?;
        f.write_all(proto_source.as_bytes())?;
    }

    // Compile via protox (pure Rust). Ensure cleanup even on error.
    // Use Debug format because protox's Display omits location info whereas
    // Debug emits "file:line:col: message" which from_parse_string can parse.
    //
    // protox requires the proto argument to be a relative path (relative to one
    // of the include directories). Passing the full absolute path causes protox
    // to reject it with "file is not in any include path". We pass only the
    // filename and use `temp_dir` as the include directory.
    let proto_filename = format!("oxiproto-compile-str-{}-{}.proto", std::process::id(), n);
    let result = protox::compile(
        std::iter::once(proto_filename.as_str()),
        std::iter::once(temp_dir.as_path()),
    )
    .map_err(|e| BuildError::from_parse_string(&format!("{e:?}")));

    // Always remove the temp file; ignore cleanup errors.
    let _ = std::fs::remove_file(&proto_path);

    result
}

/// Compile an inline proto definition string to a [`FileDescriptorSet`].
///
/// When the `native-parser` feature is enabled, parsing is performed entirely
/// in-process via the native pure-Rust parser with no temporary files.
///
/// When the feature is disabled (default), the source is written to a
/// uniquely-named temporary file under [`std::env::temp_dir()`], compiled via
/// `protox` (pure Rust, no `protoc`), then the temp file is removed before
/// returning.
///
/// The generated temp filename (non-native path) is
/// `oxiproto-compile-str-{PID}-{N}.proto` where `PID` is the current process
/// ID and `N` is an atomically-incremented counter, ensuring uniqueness across
/// concurrent callers within and across processes (e.g. nextest spawns one
/// process per test).
///
/// # Errors
///
/// - [`BuildError::Parse`] — the proto source contains syntax or semantic
///   errors.
/// - [`BuildError::Io`] — the temporary file could not be created or written
///   (non-native path only).
pub fn compile_str(proto_source: &str) -> Result<FileDescriptorSet, BuildError> {
    compile_str_impl(proto_source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_valid_proto() {
        let src = r#"syntax = "proto3";
package inline;
message Ping { string payload = 1; }
"#;
        let fds = compile_str(src).expect("valid proto should compile");
        assert_eq!(
            fds.file.len(),
            1,
            "expected exactly one FileDescriptorProto"
        );
    }

    #[test]
    fn compile_invalid_proto_returns_parse_error() {
        // Deliberately broken proto (missing semicolon after field declaration)
        let src = r#"syntax = "proto3";
message Bad { string x = 1 }
"#;
        let err = compile_str(src).expect_err("broken proto should fail");
        match err {
            BuildError::Parse { .. } => {}
            other => panic!("expected BuildError::Parse, got {other:?}"),
        }
    }

    // This test is only meaningful on the protox path (native path has no temp file).
    #[cfg(not(feature = "native-parser"))]
    #[test]
    fn temp_file_cleaned_up_after_compile() {
        // Atomically reserve a slot: fetch_add returns the OLD value, which
        // is the exact N that the *next* call to compile_str will use (if no
        // other thread races us in between). By immediately calling compile_str
        // from the same thread with no yield points, the counter advances
        // exactly once from our reserved value.
        //
        // Note: this test is inherently racy with concurrent parallel callers
        // from other unit-test threads. We tolerate that by running the test
        // under `cargo test -- --test-threads=1` in CI, or by accepting that
        // the assertion may be skipped when the expected path no longer matches.
        // For robustness in parallel runs we simply record the counter *before*
        // and *after* and verify that none of those slots survive.
        let n_before = COUNTER.fetch_add(0, Ordering::SeqCst); // peek without incrementing
        let src = r#"syntax = "proto3"; message CleanupCheck { int32 id = 1; }"#;
        let _ = compile_str(src);
        let n_after = COUNTER.load(Ordering::SeqCst);

        let temp_dir = std::env::temp_dir();
        // Check all slots that could have been used by this call.
        for n in n_before..n_after {
            let path = temp_dir.join(format!(
                "oxiproto-compile-str-{}-{}.proto",
                std::process::id(),
                n
            ));
            assert!(
                !path.exists(),
                "temp file {path:?} was not cleaned up after compile_str"
            );
        }
    }
}
