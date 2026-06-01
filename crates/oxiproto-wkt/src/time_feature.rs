#![forbid(unsafe_code)]
//! Conversions between protobuf WKT and the [`time`] crate.
//!
//! Available only when the `time` feature is enabled.
//!
//! ## Timestamp
//!
//! [`TimestampTimeExt`] converts between [`prost_types::Timestamp`] and
//! [`time::OffsetDateTime`].  The `OffsetDateTime` is always in UTC.
//!
//! ## Duration
//!
//! [`DurationTimeExt`] converts between [`prost_types::Duration`] and
//! [`time::Duration`].  The `time::Duration` can represent negative spans.

use crate::OverflowError;
use prost_types::{Duration as ProtoDuration, Timestamp};
use time::{Duration as TimeDuration, OffsetDateTime};

// ---------------------------------------------------------------------------
// TimestampTimeExt
// ---------------------------------------------------------------------------

/// Extension methods for [`prost_types::Timestamp`] providing conversions to
/// and from [`time::OffsetDateTime`].
///
/// Only available with the `time` feature.
pub trait TimestampTimeExt {
    /// Convert this `Timestamp` to a [`time::OffsetDateTime`] in UTC.
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the `seconds` value is out of the range
    /// accepted by [`time::OffsetDateTime::from_unix_timestamp`].
    fn to_offset_datetime(&self) -> Result<OffsetDateTime, OverflowError>;

    /// Build a `Timestamp` from a [`time::OffsetDateTime`].
    ///
    /// Sub-second precision is preserved at nanosecond resolution.
    fn from_offset_datetime(dt: OffsetDateTime) -> Timestamp;
}

impl TimestampTimeExt for Timestamp {
    fn to_offset_datetime(&self) -> Result<OffsetDateTime, OverflowError> {
        // Build the whole-second part first.
        let dt = OffsetDateTime::from_unix_timestamp(self.seconds).map_err(|_| {
            OverflowError::new(
                "to_offset_datetime",
                "Timestamp seconds out of time::OffsetDateTime range",
            )
        })?;

        // Add the nanoseconds offset.
        let nanos_clamped = self.nanos.clamp(0, 999_999_999);
        let dt = dt + TimeDuration::nanoseconds(nanos_clamped as i64);
        Ok(dt)
    }

    fn from_offset_datetime(dt: OffsetDateTime) -> Timestamp {
        // OffsetDateTime::unix_timestamp() returns whole seconds; the
        // sub-second part is available via .nanosecond().
        let seconds = dt.unix_timestamp();
        // .nanosecond() returns the sub-second component in [0, 999_999_999].
        let nanos = dt.nanosecond() as i32;
        Timestamp { seconds, nanos }
    }
}

// ---------------------------------------------------------------------------
// DurationTimeExt
// ---------------------------------------------------------------------------

/// Extension methods for [`prost_types::Duration`] providing conversions to
/// and from [`time::Duration`].
///
/// Only available with the `time` feature.
pub trait DurationTimeExt {
    /// Convert this proto `Duration` to a [`time::Duration`].
    ///
    /// `time::Duration` can represent negative spans, so no range error is
    /// expected under normal inputs.  The conversion is exact for any value
    /// whose total nanosecond count fits in `i128` (all valid proto durations
    /// satisfy this).
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the arithmetic overflows (only possible
    /// for extreme out-of-spec proto durations).
    fn to_time_duration(&self) -> Result<TimeDuration, OverflowError>;

    /// Build a proto `Duration` from a [`time::Duration`].
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the `time::Duration` seconds component
    /// overflows `i64`.
    fn from_time_duration(d: TimeDuration) -> Result<ProtoDuration, OverflowError>;
}

impl DurationTimeExt for ProtoDuration {
    fn to_time_duration(&self) -> Result<TimeDuration, OverflowError> {
        // Combine seconds and nanos into a single i128 nanosecond count to
        // avoid overflow in the intermediate sum.
        let total_nanos: i128 = (self.seconds as i128)
            .checked_mul(1_000_000_000)
            .and_then(|s| s.checked_add(self.nanos as i128))
            .ok_or_else(|| {
                OverflowError::new(
                    "to_time_duration",
                    "proto Duration nanoseconds overflow i128",
                )
            })?;
        Ok(TimeDuration::nanoseconds_i128(total_nanos))
    }

    fn from_time_duration(d: TimeDuration) -> Result<ProtoDuration, OverflowError> {
        // time::Duration stores sub-second nanos as a non-negative u32 in
        // [0, 999_999_999] and whole seconds as i64.
        let seconds = d.whole_seconds();
        // subsec_nanoseconds() returns nanos in the sub-second component
        // (always 0..=999_999_999 in magnitude, matching the seconds sign).
        let nanos = d.subsec_nanoseconds();
        Ok(ProtoDuration { seconds, nanos })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_epoch_round_trip() {
        let ts = Timestamp {
            seconds: 0,
            nanos: 0,
        };
        let dt = ts.to_offset_datetime().expect("epoch should convert");
        assert_eq!(dt.unix_timestamp(), 0);

        let back = Timestamp::from_offset_datetime(dt);
        assert_eq!(back.seconds, 0);
        assert_eq!(back.nanos, 0);
    }

    #[test]
    fn timestamp_positive_nanos_round_trip() {
        let ts = Timestamp {
            seconds: 1_700_000_000,
            nanos: 123_456_789,
        };
        let dt = ts.to_offset_datetime().expect("far future should convert");
        let back = Timestamp::from_offset_datetime(dt);
        assert_eq!(back.seconds, ts.seconds);
        assert_eq!(back.nanos, ts.nanos);
    }

    #[test]
    fn timestamp_pre_epoch_round_trip() {
        // -1.5s → seconds=-2, nanos=500_000_000 in canonical Timestamp form.
        let ts = Timestamp {
            seconds: -2,
            nanos: 500_000_000,
        };
        let dt = ts.to_offset_datetime().expect("pre-epoch should convert");
        let back = Timestamp::from_offset_datetime(dt);
        assert_eq!(back.seconds, ts.seconds);
        assert_eq!(back.nanos, ts.nanos);
    }

    #[test]
    fn duration_one_and_half_seconds_round_trip() {
        let pd = ProtoDuration {
            seconds: 1,
            nanos: 500_000_000,
        };
        let td = pd.to_time_duration().expect("should convert");
        assert_eq!(td.whole_seconds(), 1);
        assert_eq!(td.subsec_nanoseconds(), 500_000_000);

        let back = ProtoDuration::from_time_duration(td).expect("round trip");
        assert_eq!(back.seconds, pd.seconds);
        assert_eq!(back.nanos, pd.nanos);
    }

    #[test]
    fn duration_negative_round_trip() {
        let pd = ProtoDuration {
            seconds: -3,
            nanos: 0,
        };
        let td = pd.to_time_duration().expect("should convert");
        assert_eq!(td.whole_seconds(), -3);

        let back = ProtoDuration::from_time_duration(td).expect("round trip");
        assert_eq!(back.seconds, pd.seconds);
        assert_eq!(back.nanos, pd.nanos);
    }
}
