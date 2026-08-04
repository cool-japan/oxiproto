#![forbid(unsafe_code)]

//! Type registry: maps proto FQNs to Rust paths, supporting both flat and
//! package-namespaced layouts.

use prost_types::{DescriptorProto, FileDescriptorSet};
use std::collections::HashSet;

use crate::wkt_map::wkt_rust_type;

/// Registry of all known proto message/enum FQNs and their layout config.
///
/// Used to compute relative Rust paths for cross-type references under
/// `package_namespacing=true` layout.
pub(crate) struct TypeRegistry {
    /// All known FQNs with leading dot, e.g. `.foo.bar.MyMsg`.
    fqns: HashSet<String>,
    /// Every declared package, with a leading dot (`".foo.bar"`). The empty
    /// package is represented by `""`. Needed to tell where a package path ends
    /// and the (possibly nested) type path begins.
    packages: HashSet<String>,
    package_namespacing: bool,
}

impl TypeRegistry {
    /// Build from the full `FileDescriptorSet`. Collects all message/enum FQNs
    /// (with leading dot).
    pub fn build(fds: &FileDescriptorSet, package_namespacing: bool) -> Self {
        let mut fqns = HashSet::new();
        let mut packages = HashSet::new();
        for file in &fds.file {
            let pkg = file.package.as_deref().unwrap_or("");
            let prefix = if pkg.is_empty() {
                String::new()
            } else {
                format!(".{}", pkg)
            };
            for msg in &file.message_type {
                collect_message_fqns(&prefix, msg, &mut fqns);
            }
            for en in &file.enum_type {
                let name = en.name.as_deref().unwrap_or("");
                fqns.insert(format!("{}.{}", prefix, name));
            }
            packages.insert(prefix);
        }
        Self {
            fqns,
            packages,
            package_namespacing,
        }
    }

    /// The Rust identifier a known type is emitted under in *flat* layout.
    ///
    /// `emit_message` flattens nesting by joining the ancestor names with `_`
    /// (`message Outer { message Inner {} }` → `struct Outer_Inner`), so a
    /// reference to `.pkg.Outer.Inner` has to resolve to `Outer_Inner`, not to
    /// the bare last component. Groups make this path unavoidable: a `group`
    /// always synthesises a nested message.
    fn flat_name(&self, normalized: &str) -> Option<String> {
        let dotted = format!(".{normalized}");
        if !self.fqns.contains(&dotted) {
            return None;
        }
        // Longest matching package prefix wins (`.a` vs `.a.b` for `.a.b.C`).
        let pkg = self
            .packages
            .iter()
            .filter(|p| {
                p.is_empty()
                    || (dotted.starts_with(p.as_str()) && dotted[p.len()..].starts_with('.'))
            })
            .max_by_key(|p| p.len())?;
        let rest = dotted[pkg.len()..].trim_start_matches('.');
        if rest.is_empty() {
            return None;
        }
        Some(rest.replace('.', "_"))
    }

    /// Resolve a proto FQN (may have leading dot) to a Rust path, relative to
    /// `from_pkg` (the package string of the message containing the reference).
    ///
    /// Under flat layout (`package_namespacing=false`), or for WKT / unknown types,
    /// returns just the last component (or the absolute WKT path).
    pub fn resolve(&self, from_pkg: &str, target_fqn: &str) -> String {
        let normalized = target_fqn.trim_start_matches('.');

        // WKT: delegate to absolute wkt_map paths.
        if let Some(rust_path) = wkt_rust_type(&format!(".{}", normalized)) {
            return rust_path.to_string();
        }

        let target_name = last_component(target_fqn);

        // Flat layout: the type is emitted as `Ancestor_..._Name`.
        if !self.package_namespacing {
            return self.flat_name(normalized).unwrap_or(target_name);
        }
        // Unknown type under namespaced layout: fall back to the bare name.
        if !self.fqns.contains(&format!(".{}", normalized)) {
            return target_name;
        }

        // Compute relative path from from_pkg to target_fqn.
        // emit_package_modules uses raw split('.') segments (no case transform),
        // so we must do the same here for the paths to match.
        let from_segs: Vec<&str> = if from_pkg.is_empty() {
            vec![]
        } else {
            from_pkg.split('.').collect()
        };

        let target_segs: Vec<&str> = normalized.split('.').collect();
        let (target_pkg_segs, target_type_name) = match target_segs.split_last() {
            Some((last, rest)) => (rest, *last),
            None => return target_name,
        };

        // Find the length of the common prefix between from_pkg segs and target_pkg_segs.
        let common = from_segs
            .iter()
            .zip(target_pkg_segs.iter())
            .take_while(|(a, b)| a == b)
            .count();

        let ups = from_segs.len() - common;
        let downs = &target_pkg_segs[common..];

        let mut path = String::new();
        for _ in 0..ups {
            path.push_str("super::");
        }
        if ups == 0 && downs.is_empty() {
            // Same package — just the type name.
            return target_type_name.to_string();
        }
        for seg in downs {
            path.push_str(&escape_keyword(seg));
            path.push_str("::");
        }
        path.push_str(target_type_name);
        path
    }
}

fn collect_message_fqns(prefix: &str, msg: &DescriptorProto, fqns: &mut HashSet<String>) {
    let name = msg.name.as_deref().unwrap_or("");
    let fqn = format!("{}.{}", prefix, name);
    fqns.insert(fqn.clone());
    for nested in &msg.nested_type {
        collect_message_fqns(&fqn, nested, fqns);
    }
    for en in &msg.enum_type {
        let en_name = en.name.as_deref().unwrap_or("");
        fqns.insert(format!("{}.{}", fqn, en_name));
    }
}

/// Escape Rust keywords with `r#` prefix so they work as module path segments.
fn escape_keyword(s: &str) -> String {
    match s {
        "as" | "async" | "await" | "break" | "const" | "continue" | "crate" | "dyn" | "else"
        | "enum" | "extern" | "false" | "fn" | "for" | "if" | "impl" | "in" | "let" | "loop"
        | "match" | "mod" | "move" | "mut" | "pub" | "ref" | "return" | "self" | "Self"
        | "static" | "struct" | "super" | "trait" | "true" | "type" | "union" | "unsafe"
        | "use" | "where" | "while" => format!("r#{}", s),
        _ => s.to_string(),
    }
}

/// Extract the last component of a dotted type name, e.g. `.foo.bar.Baz` → `Baz`.
fn last_component(fqn: &str) -> String {
    fqn.split('.')
        .rfind(|s| !s.is_empty())
        .unwrap_or(fqn)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_types::{
        DescriptorProto, EnumDescriptorProto, FileDescriptorProto, FileDescriptorSet,
    };

    fn make_msg(name: &str) -> DescriptorProto {
        DescriptorProto {
            name: Some(name.to_string()),
            ..Default::default()
        }
    }

    fn make_file(pkg: &str, msgs: Vec<DescriptorProto>) -> FileDescriptorProto {
        FileDescriptorProto {
            package: Some(pkg.to_string()),
            message_type: msgs,
            ..Default::default()
        }
    }

    fn make_fds(files: Vec<FileDescriptorProto>) -> FileDescriptorSet {
        FileDescriptorSet { file: files }
    }

    /// Build a registry with two packages: "foo" has "MyMsg" and "bar" has "Other".
    fn registry_two_pkgs(pkg_ns: bool) -> TypeRegistry {
        let fds = make_fds(vec![
            make_file("foo", vec![make_msg("MyMsg")]),
            make_file("bar", vec![make_msg("Other")]),
        ]);
        TypeRegistry::build(&fds, pkg_ns)
    }

    #[test]
    fn escape_keyword_works() {
        assert_eq!(escape_keyword("type"), "r#type");
        assert_eq!(escape_keyword("mod"), "r#mod");
        assert_eq!(escape_keyword("bar"), "bar");
    }

    #[test]
    fn build_collects_fqns() {
        let reg = registry_two_pkgs(true);
        assert!(reg.fqns.contains(".foo.MyMsg"));
        assert!(reg.fqns.contains(".bar.Other"));
    }

    #[test]
    fn resolve_flat_returns_last_component() {
        let reg = registry_two_pkgs(false);
        assert_eq!(reg.resolve("foo", ".bar.Other"), "Other");
        assert_eq!(reg.resolve("foo", ".foo.MyMsg"), "MyMsg");
    }

    #[test]
    fn resolve_same_package_returns_type_name() {
        let reg = registry_two_pkgs(true);
        assert_eq!(reg.resolve("foo", ".foo.MyMsg"), "MyMsg");
    }

    #[test]
    fn resolve_sibling_package() {
        let reg = registry_two_pkgs(true);
        // from "foo" to ".bar.Other" → "super::bar::Other"
        assert_eq!(reg.resolve("foo", ".bar.Other"), "super::bar::Other");
    }

    #[test]
    fn resolve_nested_package() {
        // from "a.b.c" to ".a.b.d.T" → "super::d::T"
        let fds = make_fds(vec![
            make_file("a.b.c", vec![make_msg("S")]),
            make_file("a.b.d", vec![make_msg("T")]),
        ]);
        let reg = TypeRegistry::build(&fds, true);
        assert_eq!(reg.resolve("a.b.c", ".a.b.d.T"), "super::d::T");
    }

    #[test]
    fn resolve_root_pkg_flat() {
        // from "" (root) to ".MyMsg" (root)
        let fds = make_fds(vec![make_file("", vec![make_msg("MyMsg")])]);
        let reg = TypeRegistry::build(&fds, false);
        assert_eq!(reg.resolve("", ".MyMsg"), "MyMsg");
    }

    #[test]
    fn resolve_root_pkg_namespaced() {
        // from "" to ".MyMsg" under namespacing: same module, just type name
        let fds = make_fds(vec![make_file("", vec![make_msg("MyMsg")])]);
        let reg = TypeRegistry::build(&fds, true);
        assert_eq!(reg.resolve("", ".MyMsg"), "MyMsg");
    }

    #[test]
    fn resolve_wkt_timestamp() {
        let reg = registry_two_pkgs(true);
        assert_eq!(
            reg.resolve("foo", ".google.protobuf.Timestamp"),
            "::oxiproto_wkt::Timestamp"
        );
    }

    #[test]
    fn resolve_unknown_type_returns_last_component() {
        // Unknown type (not in registry) under namespacing → fallback to last component
        let reg = registry_two_pkgs(true);
        assert_eq!(reg.resolve("foo", ".external.Msg"), "Msg");
    }

    #[test]
    fn collect_enum_fqns() {
        let fds = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                package: Some("mypack".to_string()),
                enum_type: vec![EnumDescriptorProto {
                    name: Some("Color".to_string()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };
        let reg = TypeRegistry::build(&fds, true);
        assert!(reg.fqns.contains(".mypack.Color"));
    }
}
