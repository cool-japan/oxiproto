#![forbid(unsafe_code)]

//! Name resolution for the native parser.
//!
//! Takes a parsed [`ProtoFile`] AST and resolves all type references
//! (`FieldType::Named(s)`, RPC input/output types) to leading-dot
//! fully-qualified names such as `.my.pkg.Foo`.
//!
//! Also performs a duplicate field-number check across each message body.

use std::collections::{HashMap, HashSet};

use crate::parser::{
    ast::{Field, FieldType, Message, Method, ProtoFile, Service},
    error::ParseError,
    span::Span,
};
// ExtendBlock is used in the resolve path for symbol collection
use crate::parser::ast::ExtendBlock;

#[cfg(feature = "native-parser")]
use crate::parser::loader::FileSymbols;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Resolve all type references in `proto_file` to fully-qualified leading-dot
/// paths and verify that no two fields in the same message share a field
/// number.
///
/// Returns a cloned [`ProtoFile`] with all `FieldType::Named(s)` strings
/// replaced by their fully-qualified form, or a [`ParseError`] on the first
/// resolution or validation failure.
pub fn resolve(proto_file: &ProtoFile) -> Result<ProtoFile, ParseError> {
    let mut resolved = proto_file.clone();

    // Build the symbol table (FQNs for all defined types + enum FQN set).
    let pkg = proto_file.package.as_deref().unwrap_or("");
    let mut symbols: HashSet<String> = HashSet::new();
    let mut enum_fqns: HashSet<String> = HashSet::new();

    for msg in &proto_file.messages {
        collect_message_symbols(msg, pkg, &mut symbols, &mut enum_fqns);
    }
    for en in &proto_file.enums {
        let fqn = make_fqn(pkg, &[], &en.name);
        symbols.insert(fqn.clone());
        enum_fqns.insert(fqn);
    }

    let has_imports = !proto_file.imports.is_empty();

    // Resolve top-level messages.
    for msg in &mut resolved.messages {
        resolve_message(msg, pkg, &[], &symbols, has_imports)?;
    }

    // Resolve top-level services.
    for svc in &mut resolved.services {
        resolve_service(svc, pkg, &symbols, has_imports)?;
    }

    // Resolve field types inside extend blocks.
    for eb in &mut resolved.extends {
        resolve_extend_block(eb, pkg, &symbols, has_imports)?;
    }

    Ok(resolved)
}

/// Resolve field types inside a top-level extend block.
fn resolve_extend_block(
    eb: &mut ExtendBlock,
    pkg: &str,
    symbols: &HashSet<String>,
    has_imports: bool,
) -> Result<(), ParseError> {
    for field in &mut eb.fields {
        resolve_field(field, pkg, &[], symbols, has_imports)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Symbol collection helpers
// ---------------------------------------------------------------------------

fn make_fqn(pkg: &str, scope: &[&str], name: &str) -> String {
    let mut fqn = String::with_capacity(
        1 + pkg.len() + scope.iter().map(|s| s.len() + 1).sum::<usize>() + name.len() + 1,
    );
    fqn.push('.');
    if !pkg.is_empty() {
        fqn.push_str(pkg);
        fqn.push('.');
    }
    for part in scope {
        fqn.push_str(part);
        fqn.push('.');
    }
    fqn.push_str(name);
    fqn
}

fn collect_message_symbols(
    msg: &Message,
    pkg: &str,
    symbols: &mut HashSet<String>,
    enum_fqns: &mut HashSet<String>,
) {
    collect_message_symbols_scoped(msg, pkg, &[], symbols, enum_fqns);
}

fn collect_message_symbols_scoped(
    msg: &Message,
    pkg: &str,
    scope: &[&str],
    symbols: &mut HashSet<String>,
    enum_fqns: &mut HashSet<String>,
) {
    let fqn = make_fqn(pkg, scope, &msg.name);
    symbols.insert(fqn.clone());

    // Build the new scope by appending the current message name.
    let mut new_scope: Vec<&str> = scope.to_vec();
    new_scope.push(&msg.name);
    let new_scope_ref: Vec<&str> = new_scope.clone();

    for nested in &msg.nested_messages {
        collect_message_symbols_scoped(nested, pkg, &new_scope_ref, symbols, enum_fqns);
    }
    for en in &msg.nested_enums {
        let en_fqn = make_fqn(pkg, &new_scope_ref, &en.name);
        symbols.insert(en_fqn.clone());
        enum_fqns.insert(en_fqn);
    }
}

// ---------------------------------------------------------------------------
// Resolution helpers
// ---------------------------------------------------------------------------

/// Attempt to resolve a bare (non-dot-prefixed) type name `s` from the given
/// innermost scope outward, using the symbol table.  Returns the first
/// fully-qualified name found, or `None` if no match exists.
fn lookup(name: &str, pkg: &str, scope: &[&str], symbols: &HashSet<String>) -> Option<String> {
    // Walk from innermost scope to outermost, then to package-root, then root.
    let scope_depth = scope.len();
    for depth in (0..=scope_depth).rev() {
        let candidate = make_fqn(pkg, &scope[..depth], name);
        if symbols.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn resolve_field_type(
    ty: &mut FieldType,
    pkg: &str,
    scope: &[&str],
    symbols: &HashSet<String>,
    has_imports: bool,
    span: &Span,
) -> Result<(), ParseError> {
    match ty {
        FieldType::Scalar(_) => {}
        FieldType::Map { key: _, value } => {
            resolve_field_type(value.as_mut(), pkg, scope, symbols, has_imports, span)?;
        }
        FieldType::Named(s) | FieldType::Group(s) => {
            if s.starts_with('.') {
                // Already fully qualified — leave as-is.
            } else {
                match lookup(s.as_str(), pkg, scope, symbols) {
                    Some(fqn) => *s = fqn,
                    None if has_imports => {
                        // Best-guess: apply leading dot + package prefix.
                        // The type might come from an imported file.
                        let guessed = if pkg.is_empty() {
                            format!(".{s}")
                        } else {
                            format!(".{pkg}.{s}")
                        };
                        *s = guessed;
                    }
                    None => {
                        return Err(ParseError::UnexpectedToken {
                            expected: "known type".to_owned(),
                            found: s.clone(),
                            span: *span,
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

fn resolve_field(
    field: &mut Field,
    pkg: &str,
    scope: &[&str],
    symbols: &HashSet<String>,
    has_imports: bool,
) -> Result<(), ParseError> {
    resolve_field_type(&mut field.ty, pkg, scope, symbols, has_imports, &field.span)
}

fn check_duplicate_field_numbers(fields_all: &[Field], span: &Span) -> Result<(), ParseError> {
    let mut seen: HashMap<i32, &str> = HashMap::new();
    for f in fields_all {
        if let Some(prev_name) = seen.insert(f.number, &f.name) {
            return Err(ParseError::UnexpectedToken {
                expected: format!(
                    "unique field number (field '{}' already uses number {})",
                    prev_name, f.number
                ),
                found: format!("field '{}' also uses number {}", f.name, f.number),
                span: *span,
            });
        }
    }
    Ok(())
}

fn resolve_message(
    msg: &mut Message,
    pkg: &str,
    scope: &[&str],
    symbols: &HashSet<String>,
    has_imports: bool,
) -> Result<(), ParseError> {
    // We need the name to live long enough; borrow from msg.name.
    let msg_name_ref: &str = &msg.name;

    // Collect all direct fields (regular + oneof members) for dup-number check.
    let mut all_fields: Vec<Field> = msg.fields.clone();
    for oneof in &msg.oneofs {
        all_fields.extend_from_slice(&oneof.fields);
    }
    check_duplicate_field_numbers(&all_fields, &msg.span)?;

    // Build scope with this message name appended.
    let mut inner_scope = scope.to_vec();
    inner_scope.push(msg_name_ref);
    let inner_scope_ref: Vec<&str> = inner_scope.clone();

    // Resolve regular fields.
    for field in &mut msg.fields {
        resolve_field(field, pkg, &inner_scope_ref, symbols, has_imports)?;
    }

    // Resolve oneof member fields.
    for oneof in &mut msg.oneofs {
        for field in &mut oneof.fields {
            resolve_field(field, pkg, &inner_scope_ref, symbols, has_imports)?;
        }
    }

    // Recurse into nested messages.
    for nested in &mut msg.nested_messages {
        resolve_message(nested, pkg, &inner_scope_ref, symbols, has_imports)?;
    }

    Ok(())
}

fn resolve_rpc_type(
    type_name: &mut String,
    pkg: &str,
    symbols: &HashSet<String>,
    has_imports: bool,
    span: &Span,
) -> Result<(), ParseError> {
    if type_name.starts_with('.') {
        return Ok(());
    }
    match lookup(type_name.as_str(), pkg, &[], symbols) {
        Some(fqn) => *type_name = fqn,
        None if has_imports => {
            let guessed = if pkg.is_empty() {
                format!(".{type_name}")
            } else {
                format!(".{pkg}.{type_name}")
            };
            *type_name = guessed;
        }
        None => {
            return Err(ParseError::UnexpectedToken {
                expected: "known message type".to_owned(),
                found: type_name.clone(),
                span: *span,
            });
        }
    }
    Ok(())
}

fn resolve_service(
    svc: &mut Service,
    pkg: &str,
    symbols: &HashSet<String>,
    has_imports: bool,
) -> Result<(), ParseError> {
    for method in &mut svc.methods {
        resolve_method(method, pkg, symbols, has_imports)?;
    }
    Ok(())
}

fn resolve_method(
    method: &mut Method,
    pkg: &str,
    symbols: &HashSet<String>,
    has_imports: bool,
) -> Result<(), ParseError> {
    resolve_rpc_type(
        &mut method.input_type,
        pkg,
        symbols,
        has_imports,
        &method.span,
    )?;
    resolve_rpc_type(
        &mut method.output_type,
        pkg,
        symbols,
        has_imports,
        &method.span,
    )
}

// ---------------------------------------------------------------------------
// Cross-file resolution (native-parser feature)
// ---------------------------------------------------------------------------

/// Lookup a name in the cross-file visible symbol set.
///
/// Searches from innermost scope outward, then tries a root-anchored fallback
/// (e.g. `google.protobuf.Timestamp` → `.google.protobuf.Timestamp`).
#[cfg(feature = "native-parser")]
fn lookup_in_visible(
    name: &str,
    pkg: &str,
    scope: &[&str],
    visible: &FileSymbols,
) -> Option<String> {
    // 1. Innermost scope outward (standard proto lookup priority)
    for depth in (0..=scope.len()).rev() {
        let cand = make_fqn(pkg, &scope[..depth], name);
        if visible.contains(&cand) {
            return Some(cand);
        }
    }
    // 2. Root-anchored fallback for cross-file/cross-package refs
    //    e.g. "google.protobuf.Timestamp" → ".google.protobuf.Timestamp"
    let root = format!(".{name}");
    if visible.contains(&root) {
        return Some(root);
    }
    None
}

#[cfg(feature = "native-parser")]
fn resolve_field_type_with_context(
    ty: &mut FieldType,
    pkg: &str,
    scope: &[&str],
    visible: &FileSymbols,
    span: &Span,
) -> Result<(), ParseError> {
    match ty {
        FieldType::Scalar(_) => {}
        FieldType::Map { key: _, value } => {
            resolve_field_type_with_context(value.as_mut(), pkg, scope, visible, span)?;
        }
        FieldType::Named(s) | FieldType::Group(s) => {
            if s.starts_with('.') {
                // Already fully qualified — leave as-is.
            } else {
                match lookup_in_visible(s.as_str(), pkg, scope, visible) {
                    Some(fqn) => *s = fqn,
                    None => {
                        return Err(ParseError::UnexpectedToken {
                            expected: "known type".to_owned(),
                            found: s.clone(),
                            span: *span,
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(feature = "native-parser")]
fn resolve_field_with_context(
    field: &mut Field,
    pkg: &str,
    scope: &[&str],
    visible: &FileSymbols,
) -> Result<(), ParseError> {
    resolve_field_type_with_context(&mut field.ty, pkg, scope, visible, &field.span)
}

#[cfg(feature = "native-parser")]
fn resolve_message_with_context(
    msg: &mut Message,
    pkg: &str,
    scope: &[&str],
    visible: &FileSymbols,
) -> Result<(), ParseError> {
    let msg_name_ref: &str = &msg.name;

    // Collect all direct fields (regular + oneof members) for dup-number check.
    let mut all_fields: Vec<Field> = msg.fields.clone();
    for oneof in &msg.oneofs {
        all_fields.extend_from_slice(&oneof.fields);
    }
    check_duplicate_field_numbers(&all_fields, &msg.span)?;

    // Build scope with this message name appended.
    let mut inner_scope = scope.to_vec();
    inner_scope.push(msg_name_ref);
    let inner_scope_ref: Vec<&str> = inner_scope.clone();

    // Resolve regular fields.
    for field in &mut msg.fields {
        resolve_field_with_context(field, pkg, &inner_scope_ref, visible)?;
    }

    // Resolve oneof member fields.
    for oneof in &mut msg.oneofs {
        for field in &mut oneof.fields {
            resolve_field_with_context(field, pkg, &inner_scope_ref, visible)?;
        }
    }

    // Recurse into nested messages.
    for nested in &mut msg.nested_messages {
        resolve_message_with_context(nested, pkg, &inner_scope_ref, visible)?;
    }

    Ok(())
}

#[cfg(feature = "native-parser")]
fn resolve_rpc_type_with_context(
    type_name: &mut String,
    pkg: &str,
    visible: &FileSymbols,
    span: &Span,
) -> Result<(), ParseError> {
    if type_name.starts_with('.') {
        return Ok(());
    }
    match lookup_in_visible(type_name.as_str(), pkg, &[], visible) {
        Some(fqn) => *type_name = fqn,
        None => {
            return Err(ParseError::UnexpectedToken {
                expected: "known message type".to_owned(),
                found: type_name.clone(),
                span: *span,
            });
        }
    }
    Ok(())
}

#[cfg(feature = "native-parser")]
fn resolve_service_with_context(
    svc: &mut Service,
    pkg: &str,
    visible: &FileSymbols,
) -> Result<(), ParseError> {
    for method in &mut svc.methods {
        resolve_rpc_type_with_context(&mut method.input_type, pkg, visible, &method.span)?;
        resolve_rpc_type_with_context(&mut method.output_type, pkg, visible, &method.span)?;
    }
    Ok(())
}

/// Resolve all type references in `proto_file` using a pre-built cross-file
/// visible symbol set and verify that no two fields in the same message share
/// a field number.
///
/// Unlike [`resolve`], this function:
/// - Uses the supplied `visible` set for lookup (includes imported symbols).
/// - Errors when a name cannot be resolved (no guess heuristic).
/// - Performs root-anchored fallback (`google.protobuf.Timestamp` →
///   `.google.protobuf.Timestamp`) for cross-package references.
#[cfg(feature = "native-parser")]
pub(crate) fn resolve_with_context(
    proto_file: &ProtoFile,
    visible: &FileSymbols,
    _all_enums: &HashSet<String>,
) -> Result<ProtoFile, ParseError> {
    let mut resolved = proto_file.clone();
    let pkg = proto_file.package.as_deref().unwrap_or("");

    // Resolve top-level messages.
    for msg in &mut resolved.messages {
        resolve_message_with_context(msg, pkg, &[], visible)?;
    }

    // Resolve top-level services.
    for svc in &mut resolved.services {
        resolve_service_with_context(svc, pkg, visible)?;
    }

    // Resolve field types inside extend blocks.
    for eb in &mut resolved.extends {
        for field in &mut eb.fields {
            resolve_field_with_context(field, pkg, &[], visible)?;
        }
    }

    Ok(resolved)
}
