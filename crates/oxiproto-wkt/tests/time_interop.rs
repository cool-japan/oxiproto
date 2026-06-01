#![cfg(feature = "time")]

use oxiproto_wkt::{Duration, DurationTimeExt, Timestamp, TimestampTimeExt};

// ---------------------------------------------------------------------------
// Timestamp ↔ OffsetDateTime
// ---------------------------------------------------------------------------

#[test]
fn timestamp_epoch_to_offset_datetime() {
    let ts = Timestamp {
        seconds: 0,
        nanos: 0,
    };
    let dt = ts.to_offset_datetime().expect("epoch should convert");
    assert_eq!(dt.unix_timestamp(), 0);
    assert_eq!(dt.nanosecond(), 0);
}

#[test]
fn timestamp_epoch_round_trip() {
    let ts = Timestamp {
        seconds: 0,
        nanos: 0,
    };
    let dt = ts.to_offset_datetime().expect("epoch should convert");
    let back = Timestamp::from_offset_datetime(dt);
    assert_eq!(back.seconds, ts.seconds);
    assert_eq!(back.nanos, ts.nanos);
}

#[test]
fn timestamp_far_future() {
    // 2100-01-01T00:00:00Z ≈ unix seconds 4102444800
    let ts = Timestamp {
        seconds: 4_102_444_800,
        nanos: 999_999_999,
    };
    let dt = ts.to_offset_datetime().expect("far future should convert");
    let back = Timestamp::from_offset_datetime(dt);
    assert_eq!(back.seconds, ts.seconds);
    assert_eq!(back.nanos, ts.nanos);
}

#[test]
fn timestamp_pre_epoch_negative_seconds() {
    // Canonical form: -1.5s = seconds=-2, nanos=500_000_000
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
fn timestamp_sub_second_precision() {
    let ts = Timestamp {
        seconds: 1_700_000_000,
        nanos: 123_456_789,
    };
    let dt = ts.to_offset_datetime().expect("should convert");
    assert_eq!(dt.nanosecond(), 123_456_789);
    let back = Timestamp::from_offset_datetime(dt);
    assert_eq!(back.seconds, ts.seconds);
    assert_eq!(back.nanos, ts.nanos);
}

// ---------------------------------------------------------------------------
// Duration ↔ time::Duration
// ---------------------------------------------------------------------------

#[test]
fn duration_one_and_half_seconds() {
    let pd = Duration {
        seconds: 1,
        nanos: 500_000_000,
    };
    let td = pd.to_time_duration().expect("should convert");
    assert_eq!(td.whole_seconds(), 1);
    assert_eq!(td.subsec_nanoseconds(), 500_000_000);

    let back = Duration::from_time_duration(td).expect("round trip");
    assert_eq!(back.seconds, pd.seconds);
    assert_eq!(back.nanos, pd.nanos);
}

#[test]
fn duration_negative() {
    let pd = Duration {
        seconds: -3,
        nanos: 0,
    };
    let td = pd.to_time_duration().expect("should convert");
    assert_eq!(td.whole_seconds(), -3);

    let back = Duration::from_time_duration(td).expect("round trip");
    assert_eq!(back.seconds, pd.seconds);
    assert_eq!(back.nanos, pd.nanos);
}

#[test]
fn duration_zero_round_trip() {
    let pd = Duration {
        seconds: 0,
        nanos: 0,
    };
    let td = pd.to_time_duration().expect("should convert");
    assert_eq!(td.whole_seconds(), 0);

    let back = Duration::from_time_duration(td).expect("round trip");
    assert_eq!(back.seconds, pd.seconds);
    assert_eq!(back.nanos, pd.nanos);
}

#[test]
fn duration_fractional_nanoseconds() {
    let pd = Duration {
        seconds: 0,
        nanos: 1,
    };
    let td = pd.to_time_duration().expect("should convert");
    let back = Duration::from_time_duration(td).expect("round trip");
    assert_eq!(back.nanos, 1);
}
