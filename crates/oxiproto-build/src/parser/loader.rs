#![forbid(unsafe_code)]
//! Multi-file loader for the native .proto parser.
//!
//! Handles include-path resolution, WKT loading, recursive DFS loading,
//! symbol table construction, and multi-file FDS assembly.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use prost_types::{FileDescriptorProto, FileDescriptorSet};

use crate::error::BuildError;
use crate::parser::ast::{ImportModifier, ProtoFile};
use crate::parser::error::ParseError;
use crate::parser::parse::parse_file;
use crate::parser::span::offset_to_line_col;

// ---------------------------------------------------------------------------
// Well-known type names
// ---------------------------------------------------------------------------

const WELL_KNOWN_PROTO_NAMES: &[&str] = &[
    "google/protobuf/any.proto",
    "google/protobuf/api.proto",
    "google/protobuf/compiler/plugin.proto",
    "google/protobuf/descriptor.proto",
    "google/protobuf/duration.proto",
    "google/protobuf/empty.proto",
    "google/protobuf/field_mask.proto",
    "google/protobuf/source_context.proto",
    "google/protobuf/struct.proto",
    "google/protobuf/timestamp.proto",
    "google/protobuf/type.proto",
    "google/protobuf/wrappers.proto",
];

// ---------------------------------------------------------------------------
// LoadedFile
// ---------------------------------------------------------------------------

/// Represents either a user-authored proto (to be resolved+built) or a
/// prebuilt WKT (pushed verbatim into the FDS).
pub(crate) enum LoadedFile {
    Parsed {
        name: String,
        ast: Box<ProtoFile>,
        source: String,
    },
    Prebuilt {
        fdp: Box<FileDescriptorProto>,
    },
}

// ---------------------------------------------------------------------------
// Include-path resolution
// ---------------------------------------------------------------------------

fn resolve_include(import_path: &str, includes: &[PathBuf]) -> Option<(String, PathBuf)> {
    for dir in includes {
        let candidate = dir.join(import_path);
        if candidate.is_file() {
            return Some((import_path.to_owned(), candidate));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// WKT global pool lookup
// ---------------------------------------------------------------------------

fn open_wkt(name: &str) -> Option<FileDescriptorProto> {
    if !WELL_KNOWN_PROTO_NAMES.contains(&name) {
        return None;
    }
    prost_reflect::DescriptorPool::global()
        .get_file_by_name(name)
        .map(|f| f.file_descriptor_proto().clone())
}

// ---------------------------------------------------------------------------
// Symbol table
// ---------------------------------------------------------------------------

/// Visible and exported symbol sets for cross-file resolution.
pub(crate) struct SymbolTable {
    /// All enum FQNs across all files
    pub(crate) all_enums: HashSet<String>,
    /// Per-file EXPORTED symbols (defs + transitive public re-exports)
    exported: HashMap<String, FileSymbols>,
}

#[derive(Default, Clone)]
pub(crate) struct FileSymbols {
    pub(crate) messages: HashSet<String>,
    pub(crate) enums: HashSet<String>,
}

impl FileSymbols {
    pub(crate) fn contains(&self, fqn: &str) -> bool {
        self.messages.contains(fqn) || self.enums.contains(fqn)
    }

    fn union_into(&self, other: &mut FileSymbols) {
        other.messages.extend(self.messages.iter().cloned());
        other.enums.extend(self.enums.iter().cloned());
    }
}

// ---------------------------------------------------------------------------
// Loader struct
// ---------------------------------------------------------------------------

struct Loader<'a> {
    includes: &'a [PathBuf],
    files: HashMap<String, LoadedFile>,
    order: Vec<String>,        // topological (post-order): deps before dependents
    visited: HashSet<String>,  // fully processed (post-order done)
    on_stack: HashSet<String>, // ancestors in current DFS path (cycle detection)
}

impl<'a> Loader<'a> {
    fn open_file(
        &self,
        name: &str,
        importer_info: Option<(&str, u32, u32)>,
    ) -> Result<LoadedFile, BuildError> {
        // Try include dirs first
        if let Some((canonical_name, path)) = resolve_include(name, self.includes) {
            let source = std::fs::read_to_string(&path).map_err(BuildError::Io)?;
            let ast = parse_file(&source).map_err(|pe| {
                let (line, col) = match &pe {
                    ParseError::UnexpectedToken { span, .. } => {
                        offset_to_line_col(&source, span.start)
                    }
                    ParseError::UnbalancedBraces { span } => {
                        offset_to_line_col(&source, span.start)
                    }
                    _ => (0, 0),
                };
                BuildError::Parse {
                    file: canonical_name.clone(),
                    line,
                    col,
                    message: pe.to_string(),
                }
            })?;
            return Ok(LoadedFile::Parsed {
                name: canonical_name,
                ast: Box::new(ast),
                source,
            });
        }

        // Try WKT global pool
        if let Some(fdp) = open_wkt(name) {
            return Ok(LoadedFile::Prebuilt { fdp: Box::new(fdp) });
        }

        // Not found
        let (file, line, col) = importer_info
            .map(|(f, l, c)| (f.to_owned(), l, c))
            .unwrap_or_else(|| (name.to_owned(), 0, 0));
        Err(BuildError::Parse {
            file,
            line,
            col,
            message: format!("import not found: '{name}'"),
        })
    }

    fn load(
        &mut self,
        name: &str,
        importer_info: Option<(&str, u32, u32)>,
    ) -> Result<(), BuildError> {
        if self.visited.contains(name) {
            return Ok(()); // already processed (diamond)
        }
        if !self.on_stack.insert(name.to_owned()) {
            return Err(BuildError::Parse {
                file: importer_info
                    .map(|(f, _, _)| f.to_owned())
                    .unwrap_or_else(|| name.to_owned()),
                line: importer_info.map(|(_, l, _)| l).unwrap_or(0),
                col: importer_info.map(|(_, _, c)| c).unwrap_or(0),
                message: format!("import cycle detected involving '{name}'"),
            });
        }

        let loaded = self.open_file(name, importer_info)?;

        // Extract deps + spans before moving `loaded` into the map
        let import_spans: Vec<(String, u32, u32)> = match &loaded {
            LoadedFile::Parsed { ast, source, .. } => ast
                .imports
                .iter()
                .map(|imp| {
                    let (line, col) = offset_to_line_col(source, imp.span.start);
                    (imp.path.clone(), line, col)
                })
                .collect(),
            LoadedFile::Prebuilt { fdp, .. } => {
                fdp.dependency.iter().map(|d| (d.clone(), 0, 0)).collect()
            }
        };

        // Duplicate import check
        {
            let mut seen = HashSet::new();
            for (dep, line, col) in &import_spans {
                if !seen.insert(dep.clone()) {
                    return Err(BuildError::Parse {
                        file: name.to_owned(),
                        line: *line,
                        col: *col,
                        message: format!("duplicate import '{dep}'"),
                    });
                }
            }
        }

        // Store file before recursing
        self.files.insert(name.to_owned(), loaded);

        // Recurse into deps (post-order: deps pushed before self)
        for (dep, line, col) in import_spans {
            self.load(&dep, Some((name, line, col)))?;
        }

        self.on_stack.remove(name);
        self.visited.insert(name.to_owned());
        self.order.push(name.to_owned());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Symbol collection helpers
// ---------------------------------------------------------------------------

fn collect_ast_symbols(ast: &ProtoFile, package: &str, out: &mut FileSymbols) {
    for msg in &ast.messages {
        let fqn = if package.is_empty() {
            format!(".{}", msg.name)
        } else {
            format!(".{}.{}", package, msg.name)
        };
        collect_message_symbols_scoped(&fqn, msg, out);
    }
    for en in &ast.enums {
        let fqn = if package.is_empty() {
            format!(".{}", en.name)
        } else {
            format!(".{}.{}", package, en.name)
        };
        out.enums.insert(fqn);
    }
}

fn collect_message_symbols_scoped(
    fqn: &str,
    msg: &crate::parser::ast::Message,
    out: &mut FileSymbols,
) {
    out.messages.insert(fqn.to_owned());
    for nested in &msg.nested_messages {
        let nested_fqn = format!("{}.{}", fqn, nested.name);
        collect_message_symbols_scoped(&nested_fqn, nested, out);
    }
    for en in &msg.nested_enums {
        let en_fqn = format!("{}.{}", fqn, en.name);
        out.enums.insert(en_fqn);
    }
}

fn collect_fdp_symbols(fdp: &FileDescriptorProto, out: &mut FileSymbols) {
    let pkg = fdp.package();
    for msg in &fdp.message_type {
        let fqn = if pkg.is_empty() {
            format!(".{}", msg.name())
        } else {
            format!(".{}.{}", pkg, msg.name())
        };
        collect_fdp_message_symbols(&fqn, msg, out);
    }
    for en in &fdp.enum_type {
        let fqn = if pkg.is_empty() {
            format!(".{}", en.name())
        } else {
            format!(".{}.{}", pkg, en.name())
        };
        out.enums.insert(fqn);
    }
}

fn collect_fdp_message_symbols(
    fqn: &str,
    msg: &prost_types::DescriptorProto,
    out: &mut FileSymbols,
) {
    out.messages.insert(fqn.to_owned());
    for nested in &msg.nested_type {
        let nested_fqn = format!("{}.{}", fqn, nested.name());
        collect_fdp_message_symbols(&nested_fqn, nested, out);
    }
    for en in &msg.enum_type {
        let en_fqn = format!("{}.{}", fqn, en.name());
        out.enums.insert(en_fqn);
    }
}

// ---------------------------------------------------------------------------
// Symbol table builder
// ---------------------------------------------------------------------------

fn compute_exported(
    name: &str,
    defs: &HashMap<String, FileSymbols>,
    public_imports: &HashMap<String, Vec<String>>,
    exported: &mut HashMap<String, FileSymbols>,
    on_stack: &mut HashSet<String>,
) {
    if exported.contains_key(name) {
        return;
    }
    if !on_stack.insert(name.to_owned()) {
        return; // cycle guard
    }
    let mut result = defs.get(name).cloned().unwrap_or_default();
    if let Some(pubs) = public_imports.get(name) {
        for pub_imp in pubs.clone() {
            compute_exported(&pub_imp, defs, public_imports, exported, on_stack);
            if let Some(exp) = exported.get(&pub_imp) {
                exp.clone().union_into(&mut result);
            }
        }
    }
    on_stack.remove(name);
    exported.insert(name.to_owned(), result);
}

fn build_symbol_table(files: &HashMap<String, LoadedFile>) -> SymbolTable {
    let mut all_enums = HashSet::new();
    let mut defs: HashMap<String, FileSymbols> = HashMap::new();

    for (name, loaded) in files {
        let mut syms = FileSymbols::default();
        match loaded {
            LoadedFile::Parsed { ast, .. } => {
                collect_ast_symbols(ast, ast.package.as_deref().unwrap_or(""), &mut syms);
            }
            LoadedFile::Prebuilt { fdp, .. } => {
                collect_fdp_symbols(fdp, &mut syms);
            }
        }
        all_enums.extend(syms.enums.iter().cloned());
        defs.insert(name.clone(), syms);
    }

    // Build adjacency list of public imports
    let mut public_imports: HashMap<String, Vec<String>> = HashMap::new();
    for (name, loaded) in files {
        let pubs = match loaded {
            LoadedFile::Parsed { ast, .. } => ast
                .imports
                .iter()
                .filter(|i| i.modifier == ImportModifier::Public)
                .map(|i| i.path.clone())
                .collect(),
            LoadedFile::Prebuilt { fdp, .. } => fdp
                .public_dependency
                .iter()
                .filter_map(|&idx| fdp.dependency.get(idx as usize).cloned())
                .collect(),
        };
        public_imports.insert(name.clone(), pubs);
    }

    // Memoized DFS for exported sets
    let mut exported: HashMap<String, FileSymbols> = HashMap::new();
    let all_names: Vec<String> = files.keys().cloned().collect();
    for name in &all_names {
        compute_exported(
            name,
            &defs,
            &public_imports,
            &mut exported,
            &mut HashSet::new(),
        );
    }

    SymbolTable {
        all_enums,
        exported,
    }
}

/// Compute V(F) = defs(F) ∪ ⋃{I∈direct-imports(F)} E(I)
pub(crate) fn compute_visible_symbols(
    name: &str,
    files: &HashMap<String, LoadedFile>,
    symbol_table: &SymbolTable,
) -> FileSymbols {
    let mut visible = FileSymbols::default();

    // Add defs(F)
    if let Some(loaded) = files.get(name) {
        match loaded {
            LoadedFile::Parsed { ast, .. } => {
                collect_ast_symbols(ast, ast.package.as_deref().unwrap_or(""), &mut visible);
            }
            LoadedFile::Prebuilt { fdp, .. } => {
                collect_fdp_symbols(fdp, &mut visible);
            }
        }
    }

    // Add E(I) for each direct import I
    let direct_imports: Vec<String> = match files.get(name) {
        Some(LoadedFile::Parsed { ast, .. }) => {
            ast.imports.iter().map(|i| i.path.clone()).collect()
        }
        Some(LoadedFile::Prebuilt { fdp, .. }) => fdp.dependency.clone(),
        None => vec![],
    };
    for imp in &direct_imports {
        if let Some(exp) = symbol_table.exported.get(imp) {
            exp.clone().union_into(&mut visible);
        }
    }
    visible
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Compile a set of `.proto` files to a [`FileDescriptorSet`] using the
/// native parser.
///
/// Include directories are searched in order to resolve imports.
/// Well-known types (google/protobuf/*.proto) are loaded from the bundled pool.
pub(crate) fn compile_files(
    root_protos: &[impl AsRef<Path>],
    includes: &[PathBuf],
) -> Result<FileDescriptorSet, BuildError> {
    // Compute canonical name for each root proto
    let roots: Vec<(String, PathBuf)> = root_protos
        .iter()
        .map(|p| {
            let path = p.as_ref();
            let name = includes
                .iter()
                .find_map(|dir| {
                    path.strip_prefix(dir).ok().map(|rel| {
                        rel.components()
                            .map(|c| c.as_os_str().to_string_lossy().into_owned())
                            .collect::<Vec<_>>()
                            .join("/")
                    })
                })
                .unwrap_or_else(|| {
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.to_string_lossy().into_owned())
                });
            (name, path.to_owned())
        })
        .collect();

    let mut loader = Loader {
        includes,
        files: HashMap::new(),
        order: Vec::new(),
        visited: HashSet::new(),
        on_stack: HashSet::new(),
    };

    // Pre-load root protos (read from disk directly, not include-path search for roots)
    for (name, path) in &roots {
        if loader.visited.contains(name.as_str()) {
            continue;
        }
        loader.on_stack.insert(name.clone());
        let source = std::fs::read_to_string(path).map_err(BuildError::Io)?;
        let ast = parse_file(&source).map_err(|pe| {
            let (line, col) = match &pe {
                ParseError::UnexpectedToken { span, .. } => offset_to_line_col(&source, span.start),
                ParseError::UnbalancedBraces { span } => offset_to_line_col(&source, span.start),
                _ => (0, 0),
            };
            BuildError::Parse {
                file: name.clone(),
                line,
                col,
                message: pe.to_string(),
            }
        })?;

        let import_spans: Vec<(String, u32, u32)> = ast
            .imports
            .iter()
            .map(|imp| {
                let (line, col) = offset_to_line_col(&source, imp.span.start);
                (imp.path.clone(), line, col)
            })
            .collect();

        // Duplicate import check
        {
            let mut seen = HashSet::new();
            for (dep, line, col) in &import_spans {
                if !seen.insert(dep.clone()) {
                    return Err(BuildError::Parse {
                        file: name.clone(),
                        line: *line,
                        col: *col,
                        message: format!("duplicate import '{dep}'"),
                    });
                }
            }
        }

        loader.files.insert(
            name.clone(),
            LoadedFile::Parsed {
                name: name.clone(),
                ast: Box::new(ast),
                source,
            },
        );

        for (dep, line, col) in import_spans {
            loader.load(&dep, Some((name, line, col)))?;
        }

        loader.on_stack.remove(name.as_str());
        loader.visited.insert(name.clone());
        loader.order.push(name.clone());
    }

    // Build cross-file symbol table
    let symbol_table = build_symbol_table(&loader.files);

    // Resolve each Parsed file
    let mut resolved: HashMap<String, ProtoFile> = HashMap::new();
    for name in &loader.order {
        if let Some(LoadedFile::Parsed { name: n, ast, .. }) = loader.files.get(name) {
            let visible = compute_visible_symbols(n, &loader.files, &symbol_table);
            let resolved_ast = crate::parser::resolve::resolve_with_context(
                ast,
                &visible,
                &symbol_table.all_enums,
            )
            .map_err(|pe| {
                let source = if let Some(LoadedFile::Parsed { source, .. }) = loader.files.get(name)
                {
                    source.as_str()
                } else {
                    ""
                };
                let (line, col) = match &pe {
                    ParseError::UnexpectedToken { span, .. } => {
                        offset_to_line_col(source, span.start)
                    }
                    _ => (0, 0),
                };
                BuildError::Parse {
                    file: name.clone(),
                    line,
                    col,
                    message: pe.to_string(),
                }
            })?;
            resolved.insert(n.clone(), resolved_ast);
        }
    }

    // Build FDS in topological order
    let fdps = crate::parser::descriptor::build_fds_multi(
        &loader.order,
        &loader.files,
        &resolved,
        &symbol_table.all_enums,
    );
    Ok(FileDescriptorSet { file: fdps })
}
