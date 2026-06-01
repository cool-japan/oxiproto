#![cfg(feature = "chrono")]

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use oxiproto_wkt::{Duration, DurationExt, Timestamp, TimestampExt};

// ── Timestamp chrono round-trip tests ────────────────────────────────────────

#[test]
fn timestamp_chrono_round_trip_epoch() {
    let dt = Utc.timestamp_opt(0, 0).single().expect("valid epoch");
    let ts = Timestamp::from_chrono_utc(dt);
    let dt2 = ts.to_chrono_utc();
    assert_eq!(dt, dt2);
}

#[test]
fn timestamp_chrono_round_trip_known_value() {
    // 2023-11-14T22:13:20.123456789Z
    let secs = 1_700_000_000i64;
    let nanos = 123_456_789u32;
    let dt = Utc.timestamp_opt(secs, nanos).single().expect("valid");
    let ts = Timestamp::from_chrono_utc(dt);
    assert_eq!(ts.seconds, secs);
    assert_eq!(ts.nanos, nanos as i32);
    let dt2 = ts.to_chrono_utc();
    assert_eq!(dt, dt2);
}

#[test]
fn timestamp_chrono_round_trip_far_future() {
    // ~2100-01-01T00:00:00Z
    let secs = 4_102_444_800i64;
    let dt = Utc.timestamp_opt(secs, 0).single().expect("valid");
    let ts = Timestamp::from_chrono_utc(dt);
    let dt2 = ts.to_chrono_utc();
    assert_eq!(dt, dt2);
}

#[test]
fn timestamp_chrono_round_trip_pre_epoch() {
    // 1969-12-31T23:59:59.500000000Z  → seconds=-1, nanos=500_000_000
    let secs = -1i64;
    let nanos = 500_000_000u32;
    let dt = Utc.timestamp_opt(secs, nanos).single().expect("valid");
    let ts = Timestamp::from_chrono_utc(dt);
    let dt2 = ts.to_chrono_utc();
    assert_eq!(dt, dt2);
}

// ── Duration chrono round-trip tests ─────────────────────────────────────────

#[test]
fn duration_chrono_positive_round_trip() {
    let cd = ChronoDuration::milliseconds(500);
    let pd = Duration::from_chrono_duration(cd).expect("should convert");
    let cd2 = pd.to_chrono_duration().expect("should convert");
    assert_eq!(cd, cd2);
}

#[test]
fn duration_chrono_negative_round_trip() {
    let cd = ChronoDuration::milliseconds(-500);
    let pd = Duration::from_chrono_duration(cd).expect("should convert");
    let cd2 = pd.to_chrono_duration().expect("should convert");
    assert_eq!(cd, cd2);
}

#[test]
fn duration_chrono_zero_round_trip() {
    let cd = ChronoDuration::zero();
    let pd = Duration::from_chrono_duration(cd).expect("should convert");
    let cd2 = pd.to_chrono_duration().expect("should convert");
    assert_eq!(cd, cd2);
}

#[test]
fn duration_chrono_large_positive_round_trip() {
    // 1 day = 86400 seconds
    let cd = ChronoDuration::seconds(86_400);
    let pd = Duration::from_chrono_duration(cd).expect("should convert");
    assert_eq!(pd.seconds, 86_400);
    assert_eq!(pd.nanos, 0);
    let cd2 = pd.to_chrono_duration().expect("should convert");
    assert_eq!(cd, cd2);
}

#[test]
fn duration_chrono_negative_large_round_trip() {
    let cd = ChronoDuration::seconds(-86_400);
    let pd = Duration::from_chrono_duration(cd).expect("should convert");
    let cd2 = pd.to_chrono_duration().expect("should convert");
    assert_eq!(cd, cd2);
}

// ── OverflowError context test ────────────────────────────────────────────────

#[test]
fn overflow_error_carries_operation_in_message() {
    // i64::MAX seconds + 1 more second should overflow add_duration.
    let ts = Timestamp {
        seconds: i64::MAX,
        nanos: 999_999_999,
    };
    let big_dur = Duration {
        seconds: 1,
        nanos: 0,
    };
    let result = ts.add_duration(&big_dur);
    let err = result.expect_err("should overflow with i64::MAX seconds");
    let msg = err.to_string();
    assert!(
        msg.contains("add_duration"),
        "OverflowError message should contain the operation name 'add_duration', got: {msg}"
    );
    assert!(
        msg.contains("overflow"),
        "OverflowError message should mention overflow, got: {msg}"
    );
}
