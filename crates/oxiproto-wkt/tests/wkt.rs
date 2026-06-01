use oxiproto_wkt::{
    duration_cmp, timestamp_cmp, Duration, DurationExt, Empty, EmptyExt, SourceContext,
    SourceContextExt, Timestamp, TimestampExt, EMPTY,
};
use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

// ── Timestamp tests ──────────────────────────────────────────────────────────

#[test]
fn timestamp_now_is_after_epoch() {
    let ts = Timestamp::now();
    // `now()` should be well past 2020 (seconds > 1_577_836_800).
    assert!(ts.seconds > 1_577_836_800, "unexpected timestamp: {ts:?}");
    assert!(
        (0..=999_999_999).contains(&ts.nanos),
        "nanos out of range: {}",
        ts.nanos
    );
}

#[test]
fn timestamp_roundtrip_system_time() {
    let original = SystemTime::now();
    let ts = Timestamp::from_system_time(original);
    let recovered = ts.to_system_time().expect("to_system_time should succeed");

    // Allow ±1 µs tolerance for the subsecond rounding.
    let diff = if recovered >= original {
        recovered.duration_since(original).unwrap()
    } else {
        original.duration_since(recovered).unwrap()
    };
    assert!(diff.as_micros() < 2, "roundtrip error too large: {diff:?}");
}

#[test]
fn timestamp_unix_epoch_roundtrip() {
    let ts = Timestamp::from_system_time(UNIX_EPOCH);
    assert_eq!(ts.seconds, 0);
    assert_eq!(ts.nanos, 0);
    let back = ts.to_system_time().expect("epoch should convert");
    assert_eq!(back, UNIX_EPOCH);
}

#[test]
fn timestamp_known_value() {
    // 2024-01-01T00:00:00Z = 1704067200 seconds since epoch
    let ts = Timestamp {
        seconds: 1_704_067_200,
        nanos: 0,
    };
    let st = ts.to_system_time().expect("known timestamp should convert");
    let dur = st
        .duration_since(UNIX_EPOCH)
        .expect("should be after epoch");
    assert_eq!(dur.as_secs(), 1_704_067_200);
}

// ── Duration tests ────────────────────────────────────────────────────────────

#[test]
fn duration_from_std_roundtrip() {
    let original = std::time::Duration::new(12345, 678_901_234);
    let proto_dur =
        Duration::from_std_duration(original).expect("from_std_duration should succeed");
    assert_eq!(proto_dur.seconds, 12345);
    assert_eq!(proto_dur.nanos, 678_901_234);

    let back = proto_dur
        .to_std_duration()
        .expect("to_std_duration should succeed");
    assert_eq!(back, original);
}

#[test]
fn duration_zero_roundtrip() {
    let proto_dur = Duration::from_std_duration(std::time::Duration::ZERO)
        .expect("zero duration should succeed");
    assert_eq!(proto_dur.seconds, 0);
    assert_eq!(proto_dur.nanos, 0);

    let back = proto_dur
        .to_std_duration()
        .expect("to_std_duration should succeed");
    assert_eq!(back, std::time::Duration::ZERO);
}

#[test]
fn duration_negative_returns_overflow_error() {
    let negative_dur = Duration {
        seconds: -1,
        nanos: 0,
    };
    let result = negative_dur.to_std_duration();
    assert!(
        result.is_err(),
        "negative proto Duration should not convert to std Duration"
    );
}

#[test]
fn duration_overflow_boundary() {
    // i64::MAX seconds — too big for i64 from u64.
    // Build a std Duration that would overflow i64 when converting back.
    let huge = std::time::Duration::from_secs(u64::MAX);
    let result = Duration::from_std_duration(huge);
    assert!(
        result.is_err(),
        "u64::MAX seconds should overflow i64 in proto Duration"
    );
}

// ── Timestamp arithmetic ──────────────────────────────────────────────────────

#[test]
fn timestamp_add_duration_round_trip() {
    let ts = Timestamp {
        seconds: 1_000,
        nanos: 500_000_000,
    };
    let dur = Duration {
        seconds: 3,
        nanos: 600_000_000,
    };
    // 1000.5 + 3.6 = 1004.1
    let result = ts.add_duration(&dur).expect("add should succeed");
    assert_eq!(result.seconds, 1004);
    assert_eq!(result.nanos, 100_000_000);
}

#[test]
fn timestamp_sub_duration_round_trip() {
    let ts = Timestamp {
        seconds: 1_000,
        nanos: 0,
    };
    let dur = Duration {
        seconds: 3,
        nanos: 0,
    };
    let result = ts.sub_duration(&dur).expect("sub should succeed");
    assert_eq!(result.seconds, 997);
    assert_eq!(result.nanos, 0);
}

#[test]
fn timestamp_add_duration_overflow_returns_error() {
    let ts = Timestamp {
        seconds: i64::MAX,
        nanos: 0,
    };
    let dur = Duration {
        seconds: 1,
        nanos: 0,
    };
    assert!(ts.add_duration(&dur).is_err());
}

#[test]
fn timestamp_duration_since_positive() {
    let later = Timestamp {
        seconds: 10,
        nanos: 0,
    };
    let earlier = Timestamp {
        seconds: 3,
        nanos: 0,
    };
    let d = later.duration_since(&earlier);
    assert_eq!(d.seconds, 7);
    assert_eq!(d.nanos, 0);
}

#[test]
fn timestamp_duration_since_negative() {
    // earlier > self → negative duration
    let past = Timestamp {
        seconds: 3,
        nanos: 0,
    };
    let future = Timestamp {
        seconds: 10,
        nanos: 0,
    };
    let d = past.duration_since(&future);
    assert_eq!(d.seconds, -7);
    assert_eq!(d.nanos, 0);
}

#[test]
fn timestamp_duration_since_sub_second() {
    let later = Timestamp {
        seconds: 5,
        nanos: 200_000_000,
    };
    let earlier = Timestamp {
        seconds: 5,
        nanos: 100_000_000,
    };
    let d = later.duration_since(&earlier);
    assert_eq!(d.seconds, 0);
    assert_eq!(d.nanos, 100_000_000);
}

// ── Timestamp comparison ──────────────────────────────────────────────────────

#[test]
fn timestamp_cmp_ordering() {
    let past = Timestamp {
        seconds: 100,
        nanos: 0,
    };
    let future = Timestamp {
        seconds: 200,
        nanos: 0,
    };
    assert_eq!(timestamp_cmp(&past, &future), Ordering::Less);
    assert_eq!(timestamp_cmp(&future, &past), Ordering::Greater);
    assert_eq!(timestamp_cmp(&past, &past), Ordering::Equal);
}

#[test]
fn timestamp_cmp_nanos_tiebreak() {
    let a = Timestamp {
        seconds: 1,
        nanos: 0,
    };
    let b = Timestamp {
        seconds: 1,
        nanos: 1,
    };
    assert_eq!(timestamp_cmp(&a, &b), Ordering::Less);
}

// ── Duration comparison ──────────────────────────────────────────────────────

#[test]
fn duration_cmp_ordering() {
    let short = Duration {
        seconds: 1,
        nanos: 0,
    };
    let long = Duration {
        seconds: 2,
        nanos: 0,
    };
    assert_eq!(duration_cmp(&short, &long), Ordering::Less);
    assert_eq!(duration_cmp(&long, &short), Ordering::Greater);
    assert_eq!(duration_cmp(&short, &short), Ordering::Equal);
}

#[test]
fn duration_cmp_negative() {
    let neg = Duration {
        seconds: -3,
        nanos: 0,
    };
    let zero = Duration {
        seconds: 0,
        nanos: 0,
    };
    assert_eq!(duration_cmp(&neg, &zero), Ordering::Less);
}

// ── EmptyExt ──────────────────────────────────────────────────────────────────

#[test]
fn empty_new_equals_const() {
    assert_eq!(Empty::new(), EMPTY);
}

#[test]
fn empty_const_accessible() {
    let _e = EMPTY;
}

// ── SourceContextExt ─────────────────────────────────────────────────────────

#[test]
fn source_context_new_and_file_name() {
    let sc = SourceContext::new("google/protobuf/timestamp.proto");
    assert_eq!(sc.file_name(), "google/protobuf/timestamp.proto");
}

#[test]
fn source_context_empty_file_name() {
    let sc = SourceContext::new("");
    assert_eq!(sc.file_name(), "");
}
