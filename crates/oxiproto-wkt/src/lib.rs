#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Well-Known Types interop for OxiProto.
//!
//! This crate provides extension traits for the protobuf Well-Known Types
//! defined in [`prost_types`], enabling conversion to/from standard Rust
//! time types (and optionally [`chrono`] types when the `chrono` feature is
//! enabled, or [`time`] types when the `time` feature is enabled).
//!
//! ## Re-exports
//!
//! All standard WKT types from [`prost_types`] are re-exported for
//! convenience.
//!
//! ## Features
//!
//! | Feature  | Default | Description |
//! |----------|---------|-------------|
//! | `chrono` | off     | Adds [`TimestampExt::to_chrono_utc`] / [`from_chrono_utc`] and [`DurationExt::to_chrono_duration`] / [`from_chrono_duration`] methods. |
//! | `time`   | off     | Adds [`TimestampTimeExt::to_offset_datetime`] / [`from_offset_datetime`] and [`DurationTimeExt::to_time_duration`] / [`from_time_duration`] methods. |
//!
//! [`TimestampExt::to_chrono_utc`]: crate::TimestampExt::to_chrono_utc
//! [`from_chrono_utc`]: crate::TimestampExt::from_chrono_utc
//! [`DurationExt::to_chrono_duration`]: crate::DurationExt::to_chrono_duration
//! [`from_chrono_duration`]: crate::DurationExt::from_chrono_duration
//! [`TimestampTimeExt::to_offset_datetime`]: crate::TimestampTimeExt::to_offset_datetime
//! [`from_offset_datetime`]: crate::TimestampTimeExt::from_offset_datetime
//! [`DurationTimeExt::to_time_duration`]: crate::DurationTimeExt::to_time_duration
//! [`from_time_duration`]: crate::DurationTimeExt::from_time_duration

pub mod api_ext;
pub mod empty;
pub mod field_mask;
pub mod list_value;
pub mod source_context;
pub mod type_ext;

mod any_ext;
mod duration;
mod timestamp;
mod wrappers;

#[cfg(feature = "time")]
pub mod time_feature;

pub use any_ext::AnyExt;
pub use duration::{duration_cmp, DurationExt};
pub use timestamp::{timestamp_cmp, TimestampExt};
pub use wrappers::{
    BoolValue, BytesValue, DoubleValue, FloatValue, Int32Value, Int64Value, StringValue, StructExt,
    UInt32Value, UInt64Value, ValueExt, WrapperExt,
};

pub use api_ext::ApiExt;
pub use empty::{Empty, EmptyExt, EMPTY};
pub use field_mask::FieldMaskExt;
pub use list_value::ListValueExt;
pub use source_context::SourceContextExt;
pub use type_ext::{EnumTypeExt, TypeExt};

#[cfg(feature = "time")]
pub use time_feature::{DurationTimeExt, TimestampTimeExt};

// Re-export common WKT types so callers only need this crate.
pub use prost_types::{
    Any, Duration, FieldMask, ListValue, SourceContext, Struct, Timestamp, Value,
};

/// Arithmetic overflow when converting between time representations.
///
/// Carries the name of the operation that overflowed (`operation`) and a
/// human-readable detail string (`detail`) to aid debugging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverflowError {
    operation: &'static str,
    detail: std::borrow::Cow<'static, str>,
}

impl OverflowError {
    /// Create a new `OverflowError` with the given operation name and detail.
    pub(crate) fn new(
        operation: &'static str,
        detail: impl Into<std::borrow::Cow<'static, str>>,
    ) -> Self {
        Self {
            operation,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for OverflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "overflow in {}: {}", self.operation, self.detail)
    }
}

impl std::error::Error for OverflowError {}

impl From<oxiproto_core::OxiProtoError> for OverflowError {
    fn from(e: oxiproto_core::OxiProtoError) -> Self {
        Self {
            operation: "from_oxiproto_error",
            detail: std::borrow::Cow::Owned(e.to_string()),
        }
    }
}

impl From<OverflowError> for oxiproto_core::OxiProtoError {
    fn from(e: OverflowError) -> Self {
        oxiproto_core::OxiProtoError::ParseError(e.to_string())
    }
}
