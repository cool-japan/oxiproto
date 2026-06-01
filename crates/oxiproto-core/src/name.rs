#![forbid(unsafe_code)]

//! Native OxiProto name trait.
//!
//! Every generated protobuf message type implements [`OxiName`] to provide
//! its fully-qualified proto name and type URL.

use prost::alloc::{format, string::String};

/// The native OxiProto name trait.
///
/// Every generated protobuf message type implements this trait to provide
/// its fully-qualified proto name and type URL.
pub trait OxiName {
    /// The simple proto message name (e.g., `"MyMessage"`).
    const NAME: &'static str;
    /// The proto package (e.g., `"my.package"` or `""` for top-level).
    const PACKAGE: &'static str;

    /// The fully-qualified proto name: `"my.package.MyMessage"`.
    fn full_name() -> String {
        if Self::PACKAGE.is_empty() {
            String::from(Self::NAME)
        } else {
            format!("{}.{}", Self::PACKAGE, Self::NAME)
        }
    }

    /// The type URL: `"type.googleapis.com/my.package.MyMessage"`.
    fn type_url() -> String {
        format!("type.googleapis.com/{}", Self::full_name())
    }
}
