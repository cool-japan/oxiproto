//! Recursion-depth DoS regression for the *generated* (codegen) decode path.
//!
//! `build.rs` runs `oxiproto-codegen` on a self-referential message
//! (`message RecNested { RecNested child = 1; int32 v = 2; }`) with
//! `emit_oxi_message_impl = true`, writing the result to
//! `$OUT_DIR/dos_fixture.rs`. We `include!()` that generated code here and feed
//! it a deeply nested payload: the generated `merge` must return a decode error
//! (recursion limit) rather than overflowing the stack.

// Generated OxiMessage/OxiName impl for `RecNested`. The generated code is
// machine-emitted and not written to satisfy clippy, so silence lints for it.
#[allow(clippy::all, dead_code)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/dos_fixture.rs"));
}

use generated::RecNested;
use oxiproto::{OxiMessage, OxiProtoError};
use oxiproto_core::wire::WireError;

/// Append `value` to `out` as a base-128 varint.
fn push_varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Build `depth` levels of nested `RecNested`, inside-out. Field 1 (message)
/// has LEN tag `(1 << 3) | 2 == 0x0A`.
fn deeply_nested(depth: usize) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    for _ in 0..depth {
        let mut next = Vec::new();
        next.push(0x0A);
        push_varint(buf.len() as u64, &mut next);
        next.extend_from_slice(&buf);
        buf = next;
    }
    buf
}

#[test]
fn generated_merge_rejects_deeply_nested_message() {
    // Thousands of nested submessages must abort with the recursion-limit error
    // rather than overflowing the stack.
    let bytes = deeply_nested(5000);
    match RecNested::decode(&bytes) {
        Err(OxiProtoError::WireFormatError(WireError::RecursionLimitExceeded)) => {}
        other => panic!("expected RecursionLimitExceeded, got {other:?}"),
    }
}

#[test]
fn generated_merge_accepts_shallow_nesting() {
    // A legitimately nested payload (well within the budget) still decodes.
    let bytes = deeply_nested(10);
    RecNested::decode(&bytes).expect("shallow nesting decodes");
}
