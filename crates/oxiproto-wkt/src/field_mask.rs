#![forbid(unsafe_code)]
//! Extension trait for `prost_types::FieldMask`.
//!
//! Provides path validation, canonicalisation, and set operations (union /
//! intersection) for the well-known `google.protobuf.FieldMask` type.

use prost_types::FieldMask;

/// Extension methods for [`prost_types::FieldMask`].
pub trait FieldMaskExt {
    /// Return whether a single path string is syntactically valid.
    ///
    /// A valid path consists of one or more dot-separated components, where
    /// each component:
    ///
    /// - starts with an ASCII lowercase letter (`[a-z]`) or underscore (`_`),
    /// - contains only ASCII lowercase letters, digits (`[0-9]`), or
    ///   underscores.
    ///
    /// Leading/trailing dots and empty components are rejected.
    fn is_valid_path(path: &str) -> bool;

    /// Return `false` if any path in this mask fails [`is_valid_path`].
    ///
    /// [`is_valid_path`]: FieldMaskExt::is_valid_path
    fn is_valid(&self) -> bool;

    /// Return a canonical form of this mask.
    ///
    /// The canonical form is produced by:
    ///
    /// 1. Sorting all paths lexicographically.
    /// 2. Deduplicating exact duplicates.
    /// 3. Removing any path that is a **proper sub-path** of an earlier path
    ///    in the sorted set — e.g. if `"a"` is present, `"a.b"` and `"a.b.c"`
    ///    are redundant and are dropped. The more-general path subsumes the
    ///    more-specific one.
    fn canonical(&self) -> FieldMask;

    /// Return the union of `self` and `other`, then canonicalise the result.
    fn union(&self, other: &FieldMask) -> FieldMask;

    /// Return the intersection of `self` and `other`.
    ///
    /// Only paths that appear in **both** masks (exact string match) are
    /// included. The result is also canonicalised.
    fn intersection(&self, other: &FieldMask) -> FieldMask;
}

impl FieldMaskExt for FieldMask {
    fn is_valid_path(path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        // Split on dots; each component must be non-empty and match [a-z_][a-z0-9_]*
        for component in path.split('.') {
            if !is_valid_component(component) {
                return false;
            }
        }
        true
    }

    fn is_valid(&self) -> bool {
        self.paths.iter().all(|p| Self::is_valid_path(p))
    }

    fn canonical(&self) -> FieldMask {
        let mut sorted: Vec<String> = self.paths.clone();
        sorted.sort();
        sorted.dedup();

        // Remove paths that are proper sub-paths of an already-kept path.
        // Because the list is sorted, a prefix path always appears before any
        // of its sub-paths (e.g. "a" < "a.b" lexicographically), so a single
        // forward pass suffices.
        let mut kept: Vec<String> = Vec::with_capacity(sorted.len());
        'outer: for candidate in sorted {
            for root in &kept {
                if is_prefix_of(root, &candidate) {
                    // candidate is subsumed by root — skip it
                    continue 'outer;
                }
            }
            kept.push(candidate);
        }

        FieldMask { paths: kept }
    }

    fn union(&self, other: &FieldMask) -> FieldMask {
        let mut combined = self.paths.clone();
        combined.extend_from_slice(&other.paths);
        FieldMask { paths: combined }.canonical()
    }

    fn intersection(&self, other: &FieldMask) -> FieldMask {
        // Collect paths present in both (as exact string matches).
        use std::collections::HashSet;
        let other_set: HashSet<&str> = other.paths.iter().map(|s| s.as_str()).collect();
        let paths: Vec<String> = self
            .paths
            .iter()
            .filter(|p| other_set.contains(p.as_str()))
            .cloned()
            .collect();
        FieldMask { paths }.canonical()
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Return `true` if `component` matches `[a-z_][a-z0-9_]*` (non-empty).
fn is_valid_component(component: &str) -> bool {
    if component.is_empty() {
        return false;
    }
    let mut chars = component.chars();
    // First character: [a-z_]
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !matches!(first, 'a'..='z' | '_') {
        return false;
    }
    // Remaining characters: [a-z0-9_]
    chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
}

/// Return `true` if `prefix` is a proper ancestor of `path` using the dot
/// separator convention.  That is, `path` starts with `prefix` followed by
/// a `.`.
///
/// This deliberately rejects the case where `prefix == path` (exact match),
/// since we call `dedup` first and we want to keep the root itself.
fn is_prefix_of(prefix: &str, path: &str) -> bool {
    // path must be strictly longer and the character after prefix must be '.'
    path.len() > prefix.len()
        && path.starts_with(prefix)
        && path.as_bytes().get(prefix.len()) == Some(&b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_paths() {
        assert!(FieldMask::is_valid_path("a"));
        assert!(FieldMask::is_valid_path("foo"));
        assert!(FieldMask::is_valid_path("foo_bar"));
        assert!(FieldMask::is_valid_path("_field"));
        assert!(FieldMask::is_valid_path("a.b"));
        assert!(FieldMask::is_valid_path("foo.bar.baz"));
        assert!(FieldMask::is_valid_path("a.b2.c_3"));
    }

    #[test]
    fn invalid_paths() {
        // empty string
        assert!(!FieldMask::is_valid_path(""));
        // leading dot
        assert!(!FieldMask::is_valid_path(".a"));
        // trailing dot
        assert!(!FieldMask::is_valid_path("a."));
        // double dot (empty component)
        assert!(!FieldMask::is_valid_path("a..b"));
        // camelCase
        assert!(!FieldMask::is_valid_path("fooBar"));
        // digit-starting component
        assert!(!FieldMask::is_valid_path("1foo"));
        assert!(!FieldMask::is_valid_path("a.1b"));
        // uppercase
        assert!(!FieldMask::is_valid_path("Foo"));
    }

    #[test]
    fn canonical_sorts_and_dedupes() {
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
            paths: vec!["a.b".to_string(), "a".to_string(), "a.b.c".to_string()],
        };
        let c = mask.canonical();
        assert_eq!(c.paths, vec!["a"]);
    }

    #[test]
    fn canonical_does_not_conflate_prefix_sibling() {
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
    fn union_canonicalised() {
        let a = FieldMask {
            paths: vec!["x".to_string(), "y".to_string()],
        };
        let b = FieldMask {
            paths: vec!["y".to_string(), "z".to_string(), "x.foo".to_string()],
        };
        let u = a.union(&b);
        // x.foo is subsumed by x; duplicates removed
        assert_eq!(u.paths, vec!["x", "y", "z"]);
    }

    #[test]
    fn intersection_exact_match() {
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
    fn intersection_empty_result() {
        let a = FieldMask {
            paths: vec!["a".to_string()],
        };
        let b = FieldMask {
            paths: vec!["b".to_string()],
        };
        assert!(a.intersection(&b).paths.is_empty());
    }

    #[test]
    fn empty_mask_operations() {
        let empty = FieldMask { paths: vec![] };
        let other = FieldMask {
            paths: vec!["foo".to_string()],
        };
        assert!(empty.canonical().paths.is_empty());
        assert_eq!(empty.union(&other).paths, vec!["foo"]);
        assert!(empty.intersection(&other).paths.is_empty());
    }
}
