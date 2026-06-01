//! Tests for RFC 3339 Timestamp formatting/parsing and Duration strings.

use oxiproto_wkt::{Duration, DurationExt, Timestamp, TimestampExt};

// ── Timestamp RFC 3339 ──────────────────────────────────────────────────────

#[test]
fn timestamp_to_rfc3339_epoch() {
    let ts = Timestamp {
        seconds: 0,
        nanos: 0,
    };
    assert_eq!(ts.to_rfc3339().expect("format"), "1970-01-01T00:00:00Z");
}

#[test]
fn timestamp_to_rfc3339_known() {
    // 2023-11-14T22:13:20Z = 1700000000
    let ts = Timestamp {
        seconds: 1_700_000_000,
        nanos: 0,
    };
    assert_eq!(ts.to_rfc3339().expect("format"), "2023-11-14T22:13:20Z");
}

#[test]
fn timestamp_to_rfc3339_with_nanos() {
    let ts = Timestamp {
        seconds: 1_700_000_000,
        nanos: 500_000_000,
    };
    assert_eq!(ts.to_rfc3339().expect("format"), "2023-11-14T22:13:20.5Z");
}

#[test]
fn timestamp_to_rfc3339_nano_precision() {
    let ts = Timestamp {
        seconds: 0,
        nanos: 123_456_789,
    };
    assert_eq!(
        ts.to_rfc3339().expect("format"),
        "1970-01-01T00:00:00.123456789Z"
    );
}

#[test]
fn timestamp_from_rfc3339_epoch() {
    let ts = Timestamp::from_rfc3339("1970-01-01T00:00:00Z").expect("parse");
    assert_eq!(ts.seconds, 0);
    assert_eq!(ts.nanos, 0);
}

#[test]
fn timestamp_from_rfc3339_known() {
    let ts = Timestamp::from_rfc3339("2023-11-14T22:13:20Z").expect("parse");
    assert_eq!(ts.seconds, 1_700_000_000);
    assert_eq!(ts.nanos, 0);
}

#[test]
fn timestamp_from_rfc3339_with_fraction() {
    let ts = Timestamp::from_rfc3339("2023-11-14T22:13:20.5Z").expect("parse");
    assert_eq!(ts.seconds, 1_700_000_000);
    assert_eq!(ts.nanos, 500_000_000);
}

#[test]
fn timestamp_from_rfc3339_with_timezone() {
    // 22:13:20+02:00 means UTC is 20:13:20
    let ts = Timestamp::from_rfc3339("2023-11-15T00:13:20+02:00").expect("parse");
    // 2023-11-14T22:13:20Z
    assert_eq!(ts.seconds, 1_700_000_000);
}

#[test]
fn timestamp_from_rfc3339_negative_timezone() {
    // 20:13:20-02:00 means UTC is 22:13:20
    let ts = Timestamp::from_rfc3339("2023-11-14T20:13:20-02:00").expect("parse");
    assert_eq!(ts.seconds, 1_700_000_000);
}

#[test]
fn timestamp_rfc3339_round_trip() {
    let originals = [
        Timestamp {
            seconds: 0,
            nanos: 0,
        },
        Timestamp {
            seconds: 1_700_000_000,
            nanos: 0,
        },
        Timestamp {
            seconds: 1_700_000_000,
            nanos: 123_456_789,
        },
        Timestamp {
            seconds: 1_000_000_000,
            nanos: 999_000_000,
        },
    ];
    for ts in &originals {
        let s = ts.to_rfc3339().expect("format");
        let parsed = Timestamp::from_rfc3339(&s).expect("parse");
        assert_eq!(&parsed, ts, "round-trip failed for {s}");
    }
}

#[test]
fn timestamp_from_rfc3339_invalid() {
    assert!(Timestamp::from_rfc3339("not a date").is_err());
    assert!(Timestamp::from_rfc3339("2023-13-01T00:00:00Z").is_err()); // month 13
    assert!(Timestamp::from_rfc3339("2023-01-32T00:00:00Z").is_err()); // day 32
    assert!(Timestamp::from_rfc3339("2023-01-01T25:00:00Z").is_err()); // hour 25
}

#[test]
fn timestamp_before_epoch() {
    // 1960-01-01 is before the Unix epoch
    let ts = Timestamp::from_rfc3339("1960-01-01T00:00:00Z").expect("parse");
    assert!(ts.seconds < 0);
    // Round-trip
    let s = ts.to_rfc3339().expect("format");
    assert_eq!(s, "1960-01-01T00:00:00Z");
}

#[test]
fn timestamp_is_valid() {
    assert!(Timestamp {
        seconds: 0,
        nanos: 0
    }
    .is_valid());
    assert!(!Timestamp {
        seconds: 0,
        nanos: -1
    }
    .is_valid());
    assert!(!Timestamp {
        seconds: 999_999_999_999_999,
        nanos: 0
    }
    .is_valid());
}

// ── Duration strings ─────────────────────────────────────────────────────────

#[test]
fn duration_to_string_whole() {
    let d = Duration {
        seconds: 3,
        nanos: 0,
    };
    assert_eq!(d.to_duration_string(), "3s");
}

#[test]
fn duration_to_string_fractional() {
    let d = Duration {
        seconds: 1,
        nanos: 500_000_000,
    };
    assert_eq!(d.to_duration_string(), "1.5s");
}

#[test]
fn duration_to_string_negative() {
    let d = Duration {
        seconds: -1,
        nanos: 0,
    };
    assert_eq!(d.to_duration_string(), "-1s");
}

#[test]
fn duration_from_string_whole() {
    let d = Duration::from_duration_string("3s").expect("parse");
    assert_eq!(d.seconds, 3);
    assert_eq!(d.nanos, 0);
}

#[test]
fn duration_from_string_fractional() {
    let d = Duration::from_duration_string("1.5s").expect("parse");
    assert_eq!(d.seconds, 1);
    assert_eq!(d.nanos, 500_000_000);
}

#[test]
fn duration_from_string_negative() {
    let d = Duration::from_duration_string("-3600s").expect("parse");
    assert_eq!(d.seconds, -3600);
    assert_eq!(d.nanos, 0);
}

#[test]
fn duration_string_round_trip() {
    let originals = [
        Duration {
            seconds: 0,
            nanos: 0,
        },
        Duration {
            seconds: 3,
            nanos: 0,
        },
        Duration {
            seconds: 1,
            nanos: 500_000_000,
        },
        Duration {
            seconds: -1,
            nanos: 0,
        },
        Duration {
            seconds: 100,
            nanos: 250_000_000,
        },
    ];
    for d in &originals {
        let s = d.to_duration_string();
        let parsed = Duration::from_duration_string(&s).expect("parse");
        assert_eq!(&parsed, d, "round-trip failed for {s}");
    }
}

#[test]
fn duration_from_string_invalid() {
    assert!(Duration::from_duration_string("3").is_err()); // no 's'
    assert!(Duration::from_duration_string("abcs").is_err()); // not a number
}

#[test]
fn duration_is_valid() {
    assert!(Duration {
        seconds: 0,
        nanos: 0
    }
    .is_valid());
    assert!(Duration {
        seconds: 100,
        nanos: 500
    }
    .is_valid());
    // Mismatched signs are invalid
    assert!(!Duration {
        seconds: 1,
        nanos: -1
    }
    .is_valid());
}
