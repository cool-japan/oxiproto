#![forbid(unsafe_code)]

//! Mapping from proto well-known type FQNs to Rust type paths.

/// Returns the Rust type path for a well-known type proto FQN, if known.
///
/// Both leading-dot and non-leading-dot forms are accepted.
///
/// # Examples
///
/// ```
/// use oxiproto_codegen::wkt_map::wkt_rust_type;
///
/// assert_eq!(
///     wkt_rust_type("google.protobuf.Timestamp"),
///     Some("::oxiproto_wkt::Timestamp")
/// );
/// assert_eq!(
///     wkt_rust_type(".google.protobuf.Timestamp"),
///     Some("::oxiproto_wkt::Timestamp")
/// );
/// assert_eq!(wkt_rust_type("google.protobuf.Unknown"), None);
/// ```
pub fn wkt_rust_type(proto_fqn: &str) -> Option<&'static str> {
    match proto_fqn {
        ".google.protobuf.Timestamp" | "google.protobuf.Timestamp" => {
            Some("::oxiproto_wkt::Timestamp")
        }
        ".google.protobuf.Duration" | "google.protobuf.Duration" => {
            Some("::oxiproto_wkt::Duration")
        }
        ".google.protobuf.Any" | "google.protobuf.Any" => Some("::oxiproto_wkt::Any"),
        ".google.protobuf.Empty" | "google.protobuf.Empty" => Some("::oxiproto_wkt::Empty"),
        ".google.protobuf.FieldMask" | "google.protobuf.FieldMask" => {
            Some("::oxiproto_wkt::FieldMask")
        }
        ".google.protobuf.Struct" | "google.protobuf.Struct" => Some("::prost_types::Struct"),
        ".google.protobuf.Value" | "google.protobuf.Value" => Some("::prost_types::Value"),
        ".google.protobuf.ListValue" | "google.protobuf.ListValue" => {
            Some("::prost_types::ListValue")
        }
        // Wrapper types — map to Option<inner>
        ".google.protobuf.StringValue" | "google.protobuf.StringValue" => {
            Some("::core::option::Option<::std::string::String>")
        }
        ".google.protobuf.BytesValue" | "google.protobuf.BytesValue" => {
            Some("::core::option::Option<::std::vec::Vec<u8>>")
        }
        ".google.protobuf.BoolValue" | "google.protobuf.BoolValue" => {
            Some("::core::option::Option<bool>")
        }
        ".google.protobuf.Int32Value" | "google.protobuf.Int32Value" => {
            Some("::core::option::Option<i32>")
        }
        ".google.protobuf.Int64Value" | "google.protobuf.Int64Value" => {
            Some("::core::option::Option<i64>")
        }
        ".google.protobuf.UInt32Value" | "google.protobuf.UInt32Value" => {
            Some("::core::option::Option<u32>")
        }
        ".google.protobuf.UInt64Value" | "google.protobuf.UInt64Value" => {
            Some("::core::option::Option<u64>")
        }
        ".google.protobuf.FloatValue" | "google.protobuf.FloatValue" => {
            Some("::core::option::Option<f32>")
        }
        ".google.protobuf.DoubleValue" | "google.protobuf.DoubleValue" => {
            Some("::core::option::Option<f64>")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_both_forms() {
        assert_eq!(
            wkt_rust_type("google.protobuf.Timestamp"),
            Some("::oxiproto_wkt::Timestamp")
        );
        assert_eq!(
            wkt_rust_type(".google.protobuf.Timestamp"),
            Some("::oxiproto_wkt::Timestamp")
        );
    }

    #[test]
    fn unknown_returns_none() {
        assert_eq!(wkt_rust_type("google.protobuf.Unknown"), None);
        assert_eq!(wkt_rust_type("my.custom.Type"), None);
    }

    #[test]
    fn wrapper_types() {
        assert_eq!(
            wkt_rust_type("google.protobuf.StringValue"),
            Some("::core::option::Option<::std::string::String>")
        );
        assert_eq!(
            wkt_rust_type("google.protobuf.BoolValue"),
            Some("::core::option::Option<bool>")
        );
    }
}
