use crate::OverflowError;
use prost_types::Duration as ProtoDuration;
use std::cmp::Ordering;

/// Extension methods for [`prost_types::Duration`].
pub trait DurationExt: Sized {
    /// Convert this proto `Duration` to a [`std::time::Duration`].
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the duration has negative seconds (i.e.
    /// represents a negative duration) or if the combined value overflows
    /// [`std::time::Duration`].
    fn to_std_duration(&self) -> Result<std::time::Duration, OverflowError>;

    /// Build a proto `Duration` from a [`std::time::Duration`].
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the seconds value overflows `i64`.
    fn from_std_duration(d: std::time::Duration) -> Result<Self, OverflowError>;

    /// Format this `Duration` as a canonical string like `"1.5s"` or `"-3s"`.
    ///
    /// The format matches the Protobuf JSON canonical representation:
    /// decimal seconds followed by `s`.
    fn to_duration_string(&self) -> String;

    /// Parse a duration string like `"1.5s"` or `"-3600s"` into a proto
    /// `Duration`.
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the string is not a valid duration string
    /// (must end with `s`, must be a valid decimal number).
    fn from_duration_string(s: &str) -> Result<Self, OverflowError>;

    /// Returns `true` if this Duration is within the valid range
    /// (-315,576,000,000s to +315,576,000,000s).
    fn is_valid(&self) -> bool;

    /// Convert this proto `Duration` to a [`chrono::Duration`].
    ///
    /// Only available with the `chrono` feature.
    #[cfg(feature = "chrono")]
    fn to_chrono_duration(&self) -> Result<chrono::Duration, OverflowError>;

    /// Build a proto `Duration` from a [`chrono::Duration`].
    ///
    /// Only available with the `chrono` feature.
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the chrono duration overflows proto's
    /// `i64` seconds range.
    #[cfg(feature = "chrono")]
    fn from_chrono_duration(d: chrono::Duration) -> Result<Self, OverflowError>;
}

// ---------------------------------------------------------------------------
// Free comparison function (prost_types::Duration derives PartialEq/Eq/Hash
// but NOT PartialOrd/Ord as of prost-types 0.14.x).
// ---------------------------------------------------------------------------

/// Compare two [`ProtoDuration`] values.
///
/// Assumes both durations are in canonical form per the Protobuf spec:
/// `nanos` and `seconds` have the same sign (or one is zero), and
/// `|nanos| < 1_000_000_000`.  Under these conditions, lexicographic
/// comparison by `(seconds, nanos)` is correct.
pub fn duration_cmp(a: &ProtoDuration, b: &ProtoDuration) -> Ordering {
    a.seconds.cmp(&b.seconds).then(a.nanos.cmp(&b.nanos))
}

impl DurationExt for ProtoDuration {
    fn to_std_duration(&self) -> Result<std::time::Duration, OverflowError> {
        if self.seconds < 0 || self.nanos < 0 {
            return Err(OverflowError::new(
                "to_std_duration",
                "proto Duration has negative seconds/nanos — cannot represent as std::time::Duration",
            ));
        }
        let nanos_u32 = self.nanos.clamp(0, 999_999_999) as u32;
        Ok(std::time::Duration::new(self.seconds as u64, nanos_u32))
    }

    fn from_std_duration(d: std::time::Duration) -> Result<Self, OverflowError> {
        let secs = i64::try_from(d.as_secs()).map_err(|_| {
            OverflowError::new(
                "from_std_duration",
                "std::time::Duration seconds overflows i64",
            )
        })?;
        Ok(ProtoDuration {
            seconds: secs,
            nanos: d.subsec_nanos() as i32,
        })
    }

    fn to_duration_string(&self) -> String {
        if self.nanos == 0 {
            format!("{}s", self.seconds)
        } else {
            let nanos_abs = self.nanos.unsigned_abs();
            let frac = format!("{nanos_abs:09}");
            let trimmed = frac.trim_end_matches('0');
            format!("{}.{}s", self.seconds, trimmed)
        }
    }

    fn from_duration_string(s: &str) -> Result<Self, OverflowError> {
        let err = || OverflowError::new("from_duration_string", "invalid duration string format");

        let s = s.trim();
        let s = s.strip_suffix('s').ok_or_else(err)?;

        if let Some(dot_pos) = s.find('.') {
            let int_part = &s[..dot_pos];
            let frac_part = &s[dot_pos + 1..];

            let seconds: i64 = int_part.parse().map_err(|_| err())?;
            let negative = seconds < 0 || int_part.starts_with('-');

            // Pad/truncate frac to 9 digits
            let truncated = &frac_part[..frac_part.len().min(9)];
            let padded = format!("{truncated:0<9}");
            let nanos_abs: i32 = padded.parse().map_err(|_| err())?;
            let nanos = if negative { -nanos_abs } else { nanos_abs };

            Ok(ProtoDuration { seconds, nanos })
        } else {
            let seconds: i64 = s.parse().map_err(|_| err())?;
            Ok(ProtoDuration { seconds, nanos: 0 })
        }
    }

    fn is_valid(&self) -> bool {
        // Proto spec: duration range is approximately +/-10,000 years
        const MAX_SECONDS: i64 = 315_576_000_000;
        self.seconds >= -MAX_SECONDS
            && self.seconds <= MAX_SECONDS
            && self.nanos >= -999_999_999
            && self.nanos <= 999_999_999
            // Signs must agree (or one is zero)
            && (self.seconds == 0 || self.nanos == 0 || (self.seconds > 0) == (self.nanos > 0))
    }

    #[cfg(feature = "chrono")]
    fn to_chrono_duration(&self) -> Result<chrono::Duration, OverflowError> {
        chrono::Duration::try_seconds(self.seconds)
            .and_then(|d| d.checked_add(&chrono::Duration::nanoseconds(self.nanos as i64)))
            .ok_or_else(|| {
                OverflowError::new(
                    "to_chrono_duration",
                    "proto Duration overflows chrono::Duration",
                )
            })
    }

    #[cfg(feature = "chrono")]
    fn from_chrono_duration(d: chrono::Duration) -> Result<Self, OverflowError> {
        let total_nanos = d.num_nanoseconds().ok_or_else(|| {
            OverflowError::new(
                "from_chrono_duration",
                "chrono::Duration overflows nanoseconds representation",
            )
        })?;
        let seconds = total_nanos / 1_000_000_000;
        let nanos = (total_nanos % 1_000_000_000) as i32;
        Ok(ProtoDuration { seconds, nanos })
    }
}
