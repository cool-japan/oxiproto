use crate::OverflowError;
use prost_types::{Duration as ProtoDuration, Timestamp};
use std::cmp::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Extension methods for [`prost_types::Timestamp`].
pub trait TimestampExt: Sized {
    /// Create a `Timestamp` representing the current wall-clock time.
    fn now() -> Self;

    /// Convert this `Timestamp` to a [`SystemTime`].
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the timestamp's seconds value does not
    /// fit in a `u64` (i.e. is negative and would underflow `UNIX_EPOCH`).
    fn to_system_time(&self) -> Result<SystemTime, OverflowError>;

    /// Build a `Timestamp` from a [`SystemTime`].
    ///
    /// Nanoseconds are clamped to `[0, 999_999_999]`.
    fn from_system_time(t: SystemTime) -> Self;

    /// Format this `Timestamp` as an RFC 3339 string.
    ///
    /// Produces `"YYYY-MM-DDTHH:MM:SSZ"` for whole seconds, or
    /// `"YYYY-MM-DDTHH:MM:SS.nnnZ"` with trailing zeros trimmed for
    /// sub-second precision.
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the timestamp seconds are out of the
    /// representable calendar range.
    fn to_rfc3339(&self) -> Result<String, OverflowError>;

    /// Parse an RFC 3339 string into a `Timestamp`.
    ///
    /// Accepts standard formats like `"2023-11-14T22:13:20Z"` and
    /// `"2023-11-14T22:13:20.5Z"`.
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the string is not a valid RFC 3339
    /// timestamp.
    fn from_rfc3339(s: &str) -> Result<Self, OverflowError>;

    /// Returns `true` if this Timestamp is within the valid range
    /// (0001-01-01T00:00:00Z to 9999-12-31T23:59:59.999999999Z).
    fn is_valid(&self) -> bool;

    /// Convert this `Timestamp` to a [`chrono::DateTime<chrono::Utc>`].
    ///
    /// Only available with the `chrono` feature.
    #[cfg(feature = "chrono")]
    fn to_chrono_utc(&self) -> chrono::DateTime<chrono::Utc>;

    /// Build a `Timestamp` from a [`chrono::DateTime<chrono::Utc>`].
    ///
    /// Only available with the `chrono` feature.
    #[cfg(feature = "chrono")]
    fn from_chrono_utc(dt: chrono::DateTime<chrono::Utc>) -> Self;

    /// Add a proto [`Duration`][prost_types::Duration] to this `Timestamp`.
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the result would overflow `i64` seconds.
    fn add_duration(&self, duration: &ProtoDuration) -> Result<Timestamp, OverflowError>;

    /// Subtract a proto [`Duration`][prost_types::Duration] from this `Timestamp`.
    ///
    /// # Errors
    ///
    /// Returns [`OverflowError`] if the result would overflow `i64` seconds.
    fn sub_duration(&self, duration: &ProtoDuration) -> Result<Timestamp, OverflowError>;

    /// Compute the `Duration` elapsed from `earlier` to `self`.
    ///
    /// If `self` is before `earlier`, the returned duration is negative
    /// (seconds ≤ 0, nanos in canonical sign-agreement form).
    fn duration_since(&self, earlier: &Timestamp) -> ProtoDuration;
}

// ---------------------------------------------------------------------------
// Free comparison functions (prost_types::Timestamp derives PartialEq/Eq/Hash
// but NOT PartialOrd/Ord as of prost-types 0.14.x, so we provide free
// functions to avoid the orphan rule).
// ---------------------------------------------------------------------------

/// Compare two [`Timestamp`] values.
///
/// Timestamps are compared by `(seconds, nanos)` in lexicographic order.
/// This is correct because the Protobuf spec requires `nanos ∈ [0, 999_999_999]`
/// for canonical timestamps.
pub fn timestamp_cmp(a: &Timestamp, b: &Timestamp) -> Ordering {
    a.seconds.cmp(&b.seconds).then(a.nanos.cmp(&b.nanos))
}

impl TimestampExt for Timestamp {
    fn now() -> Self {
        let t = SystemTime::now();
        Self::from_system_time(t)
    }

    fn to_system_time(&self) -> Result<SystemTime, OverflowError> {
        if self.seconds < 0 {
            // Canonical negative Timestamp: seconds is negative, nanos is in
            // [0, 999_999_999].  The total offset before epoch is:
            //   abs(seconds) - nanos/1e9
            // We implement this as: subtract `abs_secs` whole seconds, then
            // add back the nanos fraction.
            let abs_secs = self.seconds.unsigned_abs();
            let nanos_u32 = self.nanos.clamp(0, 999_999_999) as u32;
            let t = UNIX_EPOCH
                .checked_sub(Duration::from_secs(abs_secs))
                .ok_or_else(|| {
                    OverflowError::new("to_system_time", "seconds underflows SystemTime")
                })?;
            t.checked_add(Duration::from_nanos(nanos_u32 as u64))
                .ok_or_else(|| OverflowError::new("to_system_time", "nanos overflows SystemTime"))
        } else {
            let nanos_u32 = self.nanos.clamp(0, 999_999_999) as u32;
            let after = Duration::new(self.seconds as u64, nanos_u32);
            UNIX_EPOCH
                .checked_add(after)
                .ok_or_else(|| OverflowError::new("to_system_time", "seconds overflows SystemTime"))
        }
    }

    fn from_system_time(t: SystemTime) -> Self {
        match t.duration_since(UNIX_EPOCH) {
            Ok(dur) => Timestamp {
                seconds: dur.as_secs() as i64,
                nanos: dur.subsec_nanos() as i32,
            },
            Err(e) => {
                // Time is before UNIX_EPOCH.  The Google Timestamp spec
                // requires nanos ∈ [0, 999_999_999] even for negative
                // timestamps.  Canonical form: seconds is one more negative
                // than the whole seconds if there are subsecond nanos.
                //
                // Example: -0.3 s → seconds=-1, nanos=700_000_000
                //          -1.0 s → seconds=-1, nanos=0
                let dur = e.duration();
                let sub_nanos = dur.subsec_nanos();
                if sub_nanos == 0 {
                    Timestamp {
                        seconds: -(dur.as_secs() as i64),
                        nanos: 0,
                    }
                } else {
                    // Borrow one second from the negative side.
                    let whole_secs = dur.as_secs().saturating_add(1);
                    Timestamp {
                        seconds: -(whole_secs as i64),
                        nanos: (1_000_000_000 - sub_nanos) as i32,
                    }
                }
            }
        }
    }

    fn to_rfc3339(&self) -> Result<String, OverflowError> {
        // Convert to SystemTime, then format using manual calendar computation.
        // We do pure-Rust date formatting without depending on chrono for the
        // default (non-chrono) code path.
        let total_seconds = self.seconds;
        let nanos = self.nanos.clamp(0, 999_999_999);

        // Protobuf timestamp valid range: 0001-01-01T00:00:00Z to
        // 9999-12-31T23:59:59.999999999Z
        // In Unix seconds: -62135596800 to 253402300799
        if !(-62_135_596_800..=253_402_300_799).contains(&total_seconds) {
            return Err(OverflowError::new(
                "to_rfc3339",
                "seconds out of RFC 3339 range",
            ));
        }

        let (year, month, day, hour, minute, second) = unix_to_calendar(total_seconds);

        if nanos == 0 {
            Ok(format!(
                "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
            ))
        } else {
            // Format with nanoseconds, then trim trailing zeros
            let frac = format!("{nanos:09}");
            let trimmed = frac.trim_end_matches('0');
            Ok(format!(
                "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{trimmed}Z"
            ))
        }
    }

    fn from_rfc3339(s: &str) -> Result<Self, OverflowError> {
        parse_rfc3339(s)
    }

    fn is_valid(&self) -> bool {
        // Valid range per protobuf spec
        self.seconds >= -62_135_596_800
            && self.seconds <= 253_402_300_799
            && self.nanos >= 0
            && self.nanos <= 999_999_999
    }

    #[cfg(feature = "chrono")]
    fn to_chrono_utc(&self) -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone;
        chrono::Utc
            .timestamp_opt(self.seconds, self.nanos.clamp(0, 999_999_999) as u32)
            .single()
            .unwrap_or(chrono::DateTime::<chrono::Utc>::from(UNIX_EPOCH))
    }

    #[cfg(feature = "chrono")]
    fn from_chrono_utc(dt: chrono::DateTime<chrono::Utc>) -> Self {
        Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        }
    }

    fn add_duration(&self, duration: &ProtoDuration) -> Result<Timestamp, OverflowError> {
        // Total nanos = self.nanos + duration.nanos (both i32, sum fits in i64)
        let total_nanos = (self.nanos as i64) + (duration.nanos as i64);
        // Carry: floor-divide to keep nanos in [0, 999_999_999]
        let carry_secs = total_nanos.div_euclid(1_000_000_000i64);
        let result_nanos = total_nanos.rem_euclid(1_000_000_000i64) as i32;

        let result_secs = self
            .seconds
            .checked_add(duration.seconds)
            .and_then(|s| s.checked_add(carry_secs))
            .ok_or_else(|| OverflowError::new("add_duration", "seconds overflow i64"))?;

        Ok(Timestamp {
            seconds: result_secs,
            nanos: result_nanos,
        })
    }

    fn sub_duration(&self, duration: &ProtoDuration) -> Result<Timestamp, OverflowError> {
        // Negate the duration and add.
        let neg_dur = ProtoDuration {
            seconds: duration.seconds.checked_neg().ok_or_else(|| {
                OverflowError::new("sub_duration", "duration seconds overflow i64")
            })?,
            nanos: -duration.nanos,
        };
        self.add_duration(&neg_dur)
    }

    fn duration_since(&self, earlier: &Timestamp) -> ProtoDuration {
        // diff_nanos = (self - earlier) expressed in nanoseconds at the
        // sub-second level.
        let diff_nanos = (self.nanos as i64) - (earlier.nanos as i64);
        // Carry: use Euclidean division so nanos stays in [0, 999_999_999],
        // but for a Duration we want canonical sign-agreement form where
        // nanos has the same sign as seconds (or one is zero).
        let carry = diff_nanos.div_euclid(1_000_000_000i64);
        let nanos_part = diff_nanos.rem_euclid(1_000_000_000i64);

        // Total whole-second difference
        let secs_diff = self
            .seconds
            .wrapping_sub(earlier.seconds)
            .wrapping_add(carry);

        // Re-canonicalise so that signs agree: if secs_diff < 0 and nanos_part > 0
        // we borrow one second.
        if secs_diff < 0 && nanos_part > 0 {
            ProtoDuration {
                seconds: secs_diff + 1,
                nanos: (nanos_part - 1_000_000_000) as i32,
            }
        } else {
            ProtoDuration {
                seconds: secs_diff,
                nanos: nanos_part as i32,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pure-Rust calendar helpers (no chrono dependency required)
// ---------------------------------------------------------------------------

/// Convert Unix timestamp (seconds since 1970-01-01) to calendar components.
///
/// Returns `(year, month, day, hour, minute, second)`.
fn unix_to_calendar(unix_secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    // Algorithm based on Howard Hinnant's civil_from_days
    // https://howardhinnant.github.io/date_algorithms.html

    let secs_per_day: i64 = 86400;
    let mut days = unix_secs.div_euclid(secs_per_day);
    let time_of_day = unix_secs.rem_euclid(secs_per_day) as u32;

    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    // Shift epoch from 1970-01-01 to 0000-03-01
    days += 719468;

    let era = if days >= 0 {
        days / 146097
    } else {
        (days - 146096) / 146097
    };
    let doe = (days - era * 146097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month prime [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if m <= 2 { y + 1 } else { y };

    (y, m, d, hour, minute, second)
}

/// Convert calendar date/time to Unix timestamp (seconds since 1970-01-01).
fn calendar_to_unix(year: i64, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> i64 {
    // Howard Hinnant's days_from_civil
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = (y - era * 400) as u32;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + (doe as i64) - 719468;
    days * 86400 + (hour as i64) * 3600 + (min as i64) * 60 + (sec as i64)
}

/// Parse an RFC 3339 timestamp string into a `Timestamp`.
fn parse_rfc3339(s: &str) -> Result<Timestamp, OverflowError> {
    // Expected format: YYYY-MM-DDTHH:MM:SS[.fractional]Z
    // or YYYY-MM-DDTHH:MM:SS[.fractional]+HH:MM / -HH:MM
    let err = || OverflowError::new("from_rfc3339", "invalid RFC 3339 timestamp format");

    let s = s.trim();
    if s.len() < 20 {
        return Err(err());
    }

    // Parse date part
    let year: i64 = s[0..4].parse().map_err(|_| err())?;
    if s.as_bytes()[4] != b'-' {
        return Err(err());
    }
    let month: u32 = s[5..7].parse().map_err(|_| err())?;
    if s.as_bytes()[7] != b'-' {
        return Err(err());
    }
    let day: u32 = s[8..10].parse().map_err(|_| err())?;

    // 'T' or 't' separator
    let sep = s.as_bytes()[10];
    if sep != b'T' && sep != b't' {
        return Err(err());
    }

    // Parse time part
    let hour: u32 = s[11..13].parse().map_err(|_| err())?;
    if s.as_bytes()[13] != b':' {
        return Err(err());
    }
    let minute: u32 = s[14..16].parse().map_err(|_| err())?;
    if s.as_bytes()[16] != b':' {
        return Err(err());
    }
    let second: u32 = s[17..19].parse().map_err(|_| err())?;

    // Validate ranges
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(err());
    }

    let rest = &s[19..];

    // Parse optional fractional seconds and timezone
    let (nanos, tz_offset_secs) = parse_frac_and_tz(rest)?;

    let mut unix_secs = calendar_to_unix(year, month, day, hour, minute, second);
    unix_secs -= tz_offset_secs; // Convert to UTC

    Ok(Timestamp {
        seconds: unix_secs,
        nanos,
    })
}

/// Parse the fractional seconds and timezone offset from the remainder of an
/// RFC 3339 string (everything after "HH:MM:SS").
fn parse_frac_and_tz(rest: &str) -> Result<(i32, i64), OverflowError> {
    let err = || OverflowError::new("from_rfc3339", "invalid RFC 3339 timestamp format");

    let (nanos, after_frac) = if rest.starts_with('.') {
        // Parse fractional seconds
        let frac_start = 1;
        let mut frac_end = frac_start;
        for c in rest[frac_start..].chars() {
            if c.is_ascii_digit() {
                frac_end += 1;
            } else {
                break;
            }
        }
        if frac_end == frac_start {
            return Err(err());
        }
        let frac_str = &rest[frac_start..frac_end];
        // Pad to 9 digits
        let padded = format!("{:0<9}", &frac_str[..frac_str.len().min(9)]);
        let nanos: i32 = padded.parse().map_err(|_| err())?;
        (nanos, &rest[frac_end..])
    } else {
        (0, rest)
    };

    // Parse timezone
    let tz_offset = if after_frac == "Z" || after_frac == "z" {
        0i64
    } else if after_frac.starts_with('+') || after_frac.starts_with('-') {
        let sign = if after_frac.starts_with('-') {
            -1i64
        } else {
            1i64
        };
        let tz = &after_frac[1..];
        if tz.len() < 5 || tz.as_bytes()[2] != b':' {
            return Err(err());
        }
        let tz_hour: i64 = tz[0..2].parse().map_err(|_| err())?;
        let tz_min: i64 = tz[3..5].parse().map_err(|_| err())?;
        sign * (tz_hour * 3600 + tz_min * 60)
    } else {
        return Err(err());
    };

    Ok((nanos, tz_offset))
}
