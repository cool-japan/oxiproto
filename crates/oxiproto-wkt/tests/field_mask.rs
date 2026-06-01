use oxiproto_wkt::{FieldMask, FieldMaskExt};

// ---------------------------------------------------------------------------
// is_valid_path
// ---------------------------------------------------------------------------

#[test]
fn valid_paths() {
    assert!(FieldMask::is_valid_path("a"));
    assert!(FieldMask::is_valid_path("foo"));
    assert!(FieldMask::is_valid_path("foo_bar"));
    assert!(FieldMask::is_valid_path("_underscore"));
    assert!(FieldMask::is_valid_path("a.b"));
    assert!(FieldMask::is_valid_path("foo.bar.baz"));
    assert!(FieldMask::is_valid_path("x.y2.z_3"));
    assert!(FieldMask::is_valid_path("a0.b1.c2"));
}

#[test]
fn invalid_path_empty() {
    assert!(!FieldMask::is_valid_path(""));
}

#[test]
fn invalid_path_leading_dot() {
    assert!(!FieldMask::is_valid_path(".a"));
    assert!(!FieldMask::is_valid_path(".foo.bar"));
}

#[test]
fn invalid_path_trailing_dot() {
    assert!(!FieldMask::is_valid_path("a."));
    assert!(!FieldMask::is_valid_path("foo.bar."));
}

#[test]
fn invalid_path_double_dot() {
    assert!(!FieldMask::is_valid_path("a..b"));
    assert!(!FieldMask::is_valid_path(".."));
}

#[test]
fn invalid_path_camel_case() {
    assert!(!FieldMask::is_valid_path("fooBar"));
    assert!(!FieldMask::is_valid_path("FooBar"));
    assert!(!FieldMask::is_valid_path("a.fooBar"));
}

#[test]
fn invalid_path_digit_start() {
    assert!(!FieldMask::is_valid_path("1foo"));
    assert!(!FieldMask::is_valid_path("a.1b"));
    assert!(!FieldMask::is_valid_path("0"));
}

// ---------------------------------------------------------------------------
// is_valid (mask level)
// ---------------------------------------------------------------------------

#[test]
fn mask_is_valid_all_valid() {
    let mask = FieldMask {
        paths: vec!["a".to_string(), "b.c".to_string()],
    };
    assert!(mask.is_valid());
}

#[test]
fn mask_is_valid_one_invalid() {
    let mask = FieldMask {
        paths: vec!["a".to_string(), "fooBar".to_string()],
    };
    assert!(!mask.is_valid());
}

#[test]
fn empty_mask_is_valid() {
    let mask = FieldMask { paths: vec![] };
    assert!(mask.is_valid());
}

// ---------------------------------------------------------------------------
// canonical
// ---------------------------------------------------------------------------

#[test]
fn canonical_sort_and_dedup() {
    let mask = FieldMask {
        paths: vec![
            "c".to_string(),
            "a".to_string(),
            "b".to_string(),
            "a".to_string(),
        ],
    };
    let c = mask.canonical();
    assert_eq!(c.paths, vec!["a", "b", "c"]);
}

#[test]
fn canonical_removes_subpaths() {
    // "a" subsumes "a.b" and "a.b.c"
    let mask = FieldMask {
        paths: vec!["a.b.c".to_string(), "a".to_string(), "a.b".to_string()],
    };
    let c = mask.canonical();
    assert_eq!(c.paths, vec!["a"]);
}

#[test]
fn canonical_keeps_sibling_not_prefix() {
    // "ab" must NOT be subsumed by "a"
    let mask = FieldMask {
        paths: vec!["ab".to_string(), "a".to_string(), "a.b".to_string()],
    };
    let c = mask.canonical();
    assert_eq!(c.paths, vec!["a", "ab"]);
}

#[test]
fn canonical_idempotent() {
    let mask = FieldMask {
        paths: vec!["x.y".to_string(), "x".to_string()],
    };
    let c1 = mask.canonical();
    let c2 = c1.canonical();
    assert_eq!(c1.paths, c2.paths);
}

#[test]
fn canonical_empty() {
    let mask = FieldMask { paths: vec![] };
    assert!(mask.canonical().paths.is_empty());
}

// ---------------------------------------------------------------------------
// union
// ---------------------------------------------------------------------------

#[test]
fn union_canonicalised() {
    let a = FieldMask {
        paths: vec!["x".to_string(), "y".to_string()],
    };
    let b = FieldMask {
        paths: vec!["y".to_string(), "z".to_string(), "x.foo".to_string()],
    };
    let u = a.union(&b);
    // "x.foo" is subsumed by "x"; duplicate "y" removed
    assert_eq!(u.paths, vec!["x", "y", "z"]);
}

#[test]
fn union_with_empty() {
    let a = FieldMask {
        paths: vec!["foo".to_string()],
    };
    let empty = FieldMask { paths: vec![] };
    assert_eq!(a.union(&empty).paths, vec!["foo"]);
    assert_eq!(empty.union(&a).paths, vec!["foo"]);
}

// ---------------------------------------------------------------------------
// intersection
// ---------------------------------------------------------------------------

#[test]
fn intersection_common_paths() {
    let a = FieldMask {
        paths: vec!["x".to_string(), "y".to_string(), "z".to_string()],
    };
    let b = FieldMask {
        paths: vec!["y".to_string(), "z".to_string(), "w".to_string()],
    };
    let i = a.intersection(&b);
    assert_eq!(i.paths, vec!["y", "z"]);
}

#[test]
fn intersection_no_common_paths() {
    let a = FieldMask {
        paths: vec!["a".to_string()],
    };
    let b = FieldMask {
        paths: vec!["b".to_string()],
    };
    assert!(a.intersection(&b).paths.is_empty());
}

#[test]
fn intersection_with_empty() {
    let a = FieldMask {
        paths: vec!["foo".to_string()],
    };
    let empty = FieldMask { paths: vec![] };
    assert!(a.intersection(&empty).paths.is_empty());
    assert!(empty.intersection(&a).paths.is_empty());
}

#[test]
fn intersection_does_not_use_prefix_logic() {
    // Intersection is exact — "a" and "a.b" are NOT considered a match
    let a = FieldMask {
        paths: vec!["a".to_string()],
    };
    let b = FieldMask {
        paths: vec!["a.b".to_string()],
    };
    assert!(a.intersection(&b).paths.is_empty());
}
