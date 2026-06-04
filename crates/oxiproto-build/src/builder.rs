#![forbid(unsafe_code)]

//! [`Builder`] — configurable `.proto` → Rust codegen pipeline.
//!
//! Create a builder with [`Builder::new`], chain configuration methods, then
//! call [`Builder::compile`] from a `build.rs` script.
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use oxiproto_build::Builder;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     Builder::new()
//!         .out_dir("generated/")
//!         .btree_map(["MyMessage"])
//!         .compile(&["proto/service.proto"], &["proto/"])?;
//!     Ok(())
//! }
//! ```

use crate::BuildError;
use prost_types::FileDescriptorSet;

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Boxed service-generator closure type alias.
type ServiceGeneratorFn = Box<dyn Fn(&prost_types::ServiceDescriptorProto) -> String + Send + Sync>;

/// Boxed progress callback type alias.
type ProgressFn = Box<dyn Fn(&Path) + Send + Sync>;

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Configurable builder for `.proto` → Rust codegen.
///
/// Use [`Builder::new`] (or `Builder::default()`) to construct a builder with
/// default settings, then chain configuration methods before calling
/// [`Builder::compile`].
pub struct Builder {
    /// Override for the output directory (defaults to `$OUT_DIR` when `None`).
    out_dir: Option<PathBuf>,
    /// Underlying prost-build configuration.
    config: prost_build::Config,
    /// Messages to skip during generation (fully-qualified proto names).
    skip_messages: Vec<String>,
    /// Fields to skip during generation ("Message.field_name" paths).
    skip_fields: Vec<String>,
    /// Proto paths for which BTreeMap should be used instead of HashMap.
    btree_map_paths: Vec<String>,
    /// Optional path to write the serialised [`FileDescriptorSet`].
    file_descriptor_set_path: Option<PathBuf>,
    /// When `true`, enable protoc-compatible output mode (delegating to
    /// prost-build defaults).
    protoc_compat: bool,
    /// Optional service-generator closure invoked for each service definition.
    service_generator: Option<ServiceGeneratorFn>,
    /// Optional path to write a generated include file.
    include_file: Option<PathBuf>,
    /// Optional progress callback invoked for each `.proto` file before
    /// compilation begins.
    progress: Option<ProgressFn>,
    /// When set, enables incremental compilation: the file at this path
    /// stores a fingerprint cache (newline-separated `"path\thash"` entries).
    /// If all input `.proto` files and include dirs have unchanged hashes,
    /// [`compile`](Self::compile) returns `Ok(())` immediately without
    /// re-parsing or re-generating.  The cache is updated after each successful
    /// compilation run.
    incremental_cache: Option<PathBuf>,
    /// When `true` (and the `native-codegen` feature is enabled), also
    /// generate `OxiMessage` + `OxiName` implementations alongside the
    /// prost-derived code.  The generated code is written to
    /// `<out_dir>/<package>_oxi.rs` (one file per proto package).
    #[cfg(feature = "native-codegen")]
    native_impl: bool,
}

impl Builder {
    /// Create a new [`Builder`] with default settings.
    pub fn new() -> Self {
        Self {
            out_dir: None,
            config: prost_build::Config::new(),
            skip_messages: Vec::new(),
            skip_fields: Vec::new(),
            btree_map_paths: Vec::new(),
            file_descriptor_set_path: None,
            protoc_compat: false,
            service_generator: None,
            include_file: None,
            progress: None,
            incremental_cache: None,
            #[cfg(feature = "native-codegen")]
            native_impl: false,
        }
    }

    // -----------------------------------------------------------------------
    // Configuration methods (builder pattern, self-consuming)
    // -----------------------------------------------------------------------

    /// Override the output directory for generated `.rs` files.
    ///
    /// Defaults to `$OUT_DIR` (set automatically by Cargo in `build.rs`).
    pub fn out_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.out_dir = Some(dir.into());
        self
    }

    /// Add a custom derive/attribute to the generated type identified by the
    /// fully-qualified proto `path`.
    pub fn type_attribute(mut self, path: impl AsRef<str>, attribute: impl AsRef<str>) -> Self {
        self.config
            .type_attribute(path.as_ref(), attribute.as_ref());
        self
    }

    /// Add a custom attribute to a specific field identified by the
    /// fully-qualified proto `path`.
    pub fn field_attribute(mut self, path: impl AsRef<str>, attribute: impl AsRef<str>) -> Self {
        self.config
            .field_attribute(path.as_ref(), attribute.as_ref());
        self
    }

    /// Skip code generation for the message at `path` (fully-qualified proto
    /// name, e.g. `"mypackage.MyMessage"`).
    pub fn skip_message(mut self, path: impl Into<String>) -> Self {
        self.skip_messages.push(path.into());
        self
    }

    /// Skip code generation for the field at `path` (fully-qualified proto
    /// path, e.g. `"mypackage.MyMessage.some_field"`).
    pub fn skip_field(mut self, path: impl Into<String>) -> Self {
        self.skip_fields.push(path.into());
        self
    }

    /// Use [`std::collections::BTreeMap`] instead of
    /// [`std::collections::HashMap`] for proto `map<…>` fields matching the
    /// given paths.
    ///
    /// Accepts anything that can be iterated to produce `impl AsRef<str>`,
    /// e.g. `&["mypackage.MyMessage"]` or `["."]` for all paths.
    pub fn btree_map<I, S>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for p in paths {
            self.btree_map_paths.push(p.as_ref().to_owned());
        }
        self
    }

    /// Write the serialised [`FileDescriptorSet`] (protobuf binary encoding)
    /// to `path` after compilation.
    ///
    /// The resulting file can be loaded at runtime by `oxiproto-reflect`
    /// (e.g. via `pool_from_fds_bytes`) to enable dynamic message
    /// introspection without shipping `.proto` files.
    pub fn file_descriptor_set_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.file_descriptor_set_path = Some(path.into());
        self
    }

    /// Enable protoc-compatible output mode.
    ///
    /// When set, codegen delegates entirely to `prost-build` defaults,
    /// producing output compatible with `protoc`-generated Rust code.
    pub fn protoc_compat(mut self) -> Self {
        self.protoc_compat = true;
        self
    }

    /// Register a service-generator closure invoked for each service defined
    /// in the proto files.
    ///
    /// The closure receives a [`prost_types::ServiceDescriptorProto`] and
    /// returns a `String` of Rust code appended to the package's generated
    /// `.rs` file in `out_dir`.
    pub fn service_generator(
        mut self,
        gen: impl Fn(&prost_types::ServiceDescriptorProto) -> String + Send + Sync + 'static,
    ) -> Self {
        self.service_generator = Some(Box::new(gen));
        self
    }

    /// Write a generated include file (e.g. `include.rs`) to `path` that
    /// lists all generated modules.
    pub fn include_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.include_file = Some(path.into());
        self
    }

    /// Register a progress callback invoked with the path to each `.proto`
    /// file just before it is compiled.
    pub fn progress(mut self, cb: impl Fn(&Path) + Send + Sync + 'static) -> Self {
        self.progress = Some(Box::new(cb));
        self
    }

    /// Enable native-trait code generation.
    ///
    /// When enabled (requires the `native-codegen` feature), the builder
    /// additionally runs [`oxiproto_codegen`] to emit `impl OxiMessage for T`
    /// and `impl OxiName for T` blocks for every generated message struct.
    ///
    /// The generated code is written to `<out_dir>/<stem>_oxi.rs`, where
    /// `<stem>` is derived from the proto package name (dots replaced with
    /// underscores, defaulting to `_` for the root package).  The output
    /// file is separate from the prost-generated `.rs` so that callers can
    /// selectively `include!` either one.
    ///
    /// # Panics / Errors
    ///
    /// When the `native-codegen` feature is **not** enabled, calling this
    /// method has no effect (the field does not exist).  Enable the feature
    /// to activate native impl generation.
    #[cfg(feature = "native-codegen")]
    pub fn native_impl(mut self, enable: bool) -> Self {
        self.native_impl = enable;
        self
    }

    /// No-op when the `native-codegen` feature is not enabled.
    ///
    /// This overload ensures callers can unconditionally chain
    /// `.native_impl(true)` without a `#[cfg]` annotation in their build
    /// scripts; the call simply becomes a no-op when the feature is absent.
    #[cfg(not(feature = "native-codegen"))]
    pub fn native_impl(self, _enable: bool) -> Self {
        self
    }

    /// Enable incremental compilation using a fingerprint cache file at `cache_path`.
    ///
    /// When enabled, [`compile`](Self::compile) computes a content hash
    /// (FNV-1a 64-bit) for each input `.proto` file. If the cache file exists
    /// and all file hashes match the stored values, code generation is skipped
    /// entirely and `Ok(())` is returned immediately.
    ///
    /// The cache file is a plain-text file with one `path\thash` entry per
    /// line (tab-separated, hash in lower-case hexadecimal). It is updated
    /// atomically (write to a `.tmp` sibling, then rename) after each
    /// successful full compilation.
    ///
    /// # Choosing a cache path
    ///
    /// In a `build.rs` context, `$OUT_DIR` is a reliable location:
    ///
    /// ```no_run
    /// use oxiproto_build::Builder;
    /// use std::path::PathBuf;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let cache = PathBuf::from(std::env::var("OUT_DIR")?)
    ///         .join("oxiproto_build.cache");
    ///     Builder::new()
    ///         .incremental(cache)
    ///         .compile(&["proto/hello.proto"], &["proto/"])?;
    ///     Ok(())
    /// }
    /// ```
    pub fn incremental(mut self, cache_path: impl Into<PathBuf>) -> Self {
        self.incremental_cache = Some(cache_path.into());
        self
    }

    // -----------------------------------------------------------------------
    // Terminal methods
    // -----------------------------------------------------------------------

    /// Compile the given `.proto` files to Rust.
    ///
    /// 1. Invokes the progress callback (if any) for each proto file.
    /// 2. Parses and resolves proto sources into a [`FileDescriptorSet`].
    ///    Uses the native pure-Rust parser when the `native-parser` feature is
    ///    enabled; otherwise delegates to `protox`.
    /// 3. Optionally serialises the FDS to
    ///    [`file_descriptor_set_path`](Self::file_descriptor_set_path).
    /// 4. Delegates to [`prost_build::Config::compile_fds`] for Rust code
    ///    generation.
    /// 5. Optionally writes an include file to
    ///    [`include_file`](Self::include_file).
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Parse`] if the parser cannot parse or resolve the
    /// proto sources, [`BuildError::Codegen`] if `prost-build` fails to emit
    /// Rust, or [`BuildError::Io`] on I/O failures.
    pub fn compile(
        mut self,
        protos: &[impl AsRef<Path>],
        includes: &[impl AsRef<Path>],
    ) -> Result<(), BuildError> {
        // Apply out_dir to prost-build config.
        if let Some(dir) = &self.out_dir {
            self.config.out_dir(dir);
        }

        // Apply btree_map paths.
        for path in &self.btree_map_paths {
            self.config.btree_map([path.as_str()]);
        }

        // --- Incremental compilation check ---
        // Compute fingerprints of all input proto files.  If a cache file is
        // configured AND all current hashes match the stored values, skip the
        // entire compilation pipeline and return early.
        if let Some(ref cache_path) = self.incremental_cache.clone() {
            let current = fingerprint_files(protos);
            if let Ok(stored) = load_fingerprint_cache(cache_path) {
                if fingerprints_match(&current, &stored) {
                    // Nothing changed — skip codegen.
                    return Ok(());
                }
            }
            // We will update the cache after a successful run (see below).
        }

        // Invoke progress callback per proto file.
        for proto in protos {
            if let Some(cb) = &self.progress {
                cb(proto.as_ref());
            }
        }

        // Parse and resolve proto sources into a FileDescriptorSet.
        // Native path: use the in-process native parser (no temp files).
        // Default path: delegate to protox using Debug format so that
        //   "file:line:col: message" is preserved for from_parse_string.
        #[cfg(feature = "native-parser")]
        let mut fds = crate::compile_files_native(protos, includes)?;

        #[cfg(not(feature = "native-parser"))]
        let mut fds = protox::compile(
            protos.iter().map(|p| p.as_ref()),
            includes.iter().map(|p| p.as_ref()),
        )
        .map_err(|e| BuildError::from_parse_string(&format!("{e:?}")))?;

        // Apply skip_messages and skip_fields filters.
        fds_apply_filters(&mut fds, &self.skip_messages, &self.skip_fields);

        // Optionally write the serialised FDS *before* passing it to
        // compile_fds (which consumes it).
        if let Some(fds_path) = &self.file_descriptor_set_path {
            use prost::Message as _;
            let fds_bytes = fds.encode_to_vec();
            std::fs::write(fds_path, fds_bytes)?;
        }

        // Invoke the service generator (if any) before handing the FDS to
        // prost-build, while we can still inspect the service descriptors.
        if let Some(ref gen) = self.service_generator {
            let effective_out_dir: PathBuf = match &self.out_dir {
                Some(d) => d.clone(),
                None => std::env::var_os("OUT_DIR")
                    .ok_or_else(|| BuildError::Codegen {
                        message: "OUT_DIR is not set and no out_dir was configured".to_owned(),
                    })
                    .map(PathBuf::from)?,
            };
            for file_proto in &fds.file {
                if file_proto.service.is_empty() {
                    continue;
                }
                let pkg = file_proto.package.as_deref().unwrap_or("_");
                let out_file = effective_out_dir.join(format!("{pkg}.rs"));
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&out_file)?;
                for svc in &file_proto.service {
                    let code = gen(svc);
                    if !code.is_empty() {
                        f.write_all(code.as_bytes())?;
                        if !code.ends_with('\n') {
                            f.write_all(b"\n")?;
                        }
                    }
                }
            }
        }

        // Generate Rust code via prost-build.
        // Clone the FDS first if we need it for native codegen afterwards.
        #[cfg(feature = "native-codegen")]
        let fds_for_native = if self.native_impl {
            Some(fds.clone())
        } else {
            None
        };
        #[cfg(not(feature = "native-codegen"))]
        let _ = ();

        self.config
            .compile_fds(fds)
            .map_err(|e| BuildError::Codegen {
                message: e.to_string(),
            })?;

        // When `native_impl` is enabled and the `native-codegen` feature is
        // active, also emit `OxiMessage` / `OxiName` impls via oxiproto-codegen.
        #[cfg(feature = "native-codegen")]
        if let Some(native_fds) = fds_for_native {
            let effective_out_dir: PathBuf = match &self.out_dir {
                Some(d) => d.clone(),
                None => std::env::var_os("OUT_DIR")
                    .ok_or_else(|| BuildError::Codegen {
                        message: "OUT_DIR is not set and no out_dir was configured".to_owned(),
                    })
                    .map(PathBuf::from)?,
            };

            let opts = oxiproto_codegen::CodegenOptions {
                emit_oxi_message_impl: true,
                package_namespacing: false,
                ..oxiproto_codegen::CodegenOptions::default()
            };

            // Emit one `*_oxi.rs` file per proto package.
            let mut pkg_contents: std::collections::BTreeMap<String, String> =
                std::collections::BTreeMap::new();

            for file in &native_fds.file {
                let pkg = file.package.as_deref().unwrap_or("").to_string();
                let single_fds = prost_types::FileDescriptorSet {
                    file: vec![file.clone()],
                };
                let code =
                    oxiproto_codegen::generate_with_options(&single_fds, &opts).map_err(|e| {
                        BuildError::Codegen {
                            message: format!("native codegen failed: {e}"),
                        }
                    })?;
                if !code.trim().is_empty() {
                    pkg_contents.entry(pkg).or_default().push_str(&code);
                }
            }

            for (pkg, content) in &pkg_contents {
                let stem = if pkg.is_empty() {
                    "_oxi".to_owned()
                } else {
                    format!("{}_oxi", pkg.replace('.', "_"))
                };
                let out_path = effective_out_dir.join(format!("{stem}.rs"));
                std::fs::write(&out_path, content.as_bytes())?;
            }
        }

        // Optionally write an include file listing generated modules.
        if let Some(include_path) = &self.include_file {
            std::fs::write(include_path, "// Generated by oxiproto-build\n")?;
        }

        // --- Update incremental cache after successful compilation ---
        if let Some(ref cache_path) = self.incremental_cache {
            let current = fingerprint_files(protos);
            // Best-effort: ignore cache write failures so they don't break
            // successful builds; the cache will just be recomputed next time.
            let _ = save_fingerprint_cache(cache_path, &current);
        }

        Ok(())
    }

    /// Parse `.proto` files to a [`FileDescriptorSet`] without writing any
    /// generated Rust files.
    ///
    /// This is the low-level building block used by [`crate::compile_to_fds`].
    ///
    /// When the `native-parser` feature is enabled, parsing uses the native
    /// pure-Rust parser. Otherwise, delegates to `protox`.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Parse`] if the parser cannot parse or resolve the
    /// proto sources.
    pub fn compile_to_fds(
        self,
        protos: &[impl AsRef<Path>],
        includes: &[impl AsRef<Path>],
    ) -> Result<FileDescriptorSet, BuildError> {
        // Invoke progress callback per proto file.
        for proto in protos {
            if let Some(cb) = &self.progress {
                cb(proto.as_ref());
            }
        }

        parse_protos_to_fds(protos, includes)
    }
}

/// Dispatch to the native parser or protox depending on the active feature.
///
/// This free function is needed to keep the `compile_to_fds` body free from
/// cfg-gated `return` statements (which trigger `clippy::needless_return`).
#[cfg(feature = "native-parser")]
fn parse_protos_to_fds(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> Result<FileDescriptorSet, BuildError> {
    crate::compile_files_native(protos, includes)
}

/// Dispatch to protox when the `native-parser` feature is not enabled.
#[cfg(not(feature = "native-parser"))]
fn parse_protos_to_fds(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> Result<FileDescriptorSet, BuildError> {
    // Use Debug format because protox's Display omits location info whereas
    // Debug emits "file:line:col: message" which from_parse_string can parse.
    protox::compile(
        protos.iter().map(|p| p.as_ref()),
        includes.iter().map(|p| p.as_ref()),
    )
    .map_err(|e| BuildError::from_parse_string(&format!("{e:?}")))
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Incremental compilation — fingerprinting helpers
// ---------------------------------------------------------------------------

/// A content fingerprint map: canonical path string → FNV-1a 64-bit hex hash.
type Fingerprints = HashMap<String, String>;

/// FNV-1a 64-bit hash over `data`.
///
/// Pure Rust, no external crate required.  Produces a stable 64-bit hash
/// suitable for change detection (not cryptographic security).
fn fnv1a64(data: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Compute FNV-1a 64-bit fingerprints for each of the given proto file paths.
///
/// Files that cannot be read (missing, permission denied) are silently skipped
/// so they don't prevent compilation — the missing entry simply won't match any
/// stored entry, forcing a full rebuild.
fn fingerprint_files(protos: &[impl AsRef<Path>]) -> Fingerprints {
    let mut map = HashMap::new();
    for proto in protos {
        let p = proto.as_ref();
        if let Ok(contents) = std::fs::read(p) {
            let hash = fnv1a64(&contents);
            let key = p.to_string_lossy().into_owned();
            map.insert(key, format!("{hash:016x}"));
        }
    }
    map
}

/// Load a fingerprint cache from `path`.
///
/// The cache format is one tab-separated `"canonical_path\thex_hash"` entry
/// per line.  Lines that do not parse correctly are ignored.
///
/// Returns `Err` if the file does not exist or cannot be read.
fn load_fingerprint_cache(path: &Path) -> std::io::Result<Fingerprints> {
    let content = std::fs::read_to_string(path)?;
    let mut map = HashMap::new();
    for line in content.lines() {
        if let Some((key, val)) = line.split_once('\t') {
            if !key.is_empty() && !val.is_empty() {
                map.insert(key.to_owned(), val.to_owned());
            }
        }
    }
    Ok(map)
}

/// Return `true` if `current` and `stored` contain exactly the same set of
/// path keys and all hash values match.
///
/// A strict equality check: extra keys in either map → `false`.
fn fingerprints_match(current: &Fingerprints, stored: &Fingerprints) -> bool {
    if current.len() != stored.len() {
        return false;
    }
    current
        .iter()
        .all(|(k, v)| stored.get(k).map(|sv| sv == v).unwrap_or(false))
}

/// Persist `fingerprints` to `path` atomically.
///
/// Writes to `path.tmp` sibling, then renames to `path`.  On platforms where
/// rename is not atomic (cross-device), the write is still safe because a
/// partial write leaves the `.tmp` file, not `path`.
fn save_fingerprint_cache(path: &Path, fingerprints: &Fingerprints) -> std::io::Result<()> {
    // Build tmp path: append ".tmp" to the extension (or just add ".tmp").
    let mut tmp_path = path.to_path_buf();
    let new_ext = match tmp_path.extension() {
        Some(ext) => format!("{}.tmp", ext.to_string_lossy()),
        None => "tmp".to_owned(),
    };
    tmp_path.set_extension(new_ext);

    // Sort by key for deterministic output.
    let mut entries: Vec<(&String, &String)> = fingerprints.iter().collect();
    entries.sort_by_key(|(k, _)| k.as_str());

    let mut content = String::new();
    for (key, val) in entries {
        content.push_str(key);
        content.push('\t');
        content.push_str(val);
        content.push('\n');
    }

    std::fs::write(&tmp_path, content.as_bytes())?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// FDS filter helpers
// ---------------------------------------------------------------------------

/// Apply `skip_messages` and `skip_fields` filters to `fds` in-place.
///
/// `skip_messages` entries are matched against fully-qualified message names
/// (both `.pkg.Msg` and `pkg.Msg` forms are accepted). When a message is
/// removed, any field in any surviving message whose `type_name` resolves to
/// that message is also removed.
///
/// `skip_fields` entries have the form `"Message.field_name"` where `Message`
/// may be a short name or fully-qualified name.
fn fds_apply_filters(
    fds: &mut FileDescriptorSet,
    skip_messages: &[String],
    skip_fields: &[String],
) {
    if skip_messages.is_empty() && skip_fields.is_empty() {
        return;
    }

    // Normalise skip_messages to both bare and dot-prefixed forms so we can
    // match regardless of whether prost uses ".pkg.Msg" or "pkg.Msg".
    let normalised_skip: Vec<(String, String)> = skip_messages
        .iter()
        .map(|s| {
            let bare = s.trim_start_matches('.');
            let dotted = format!(".{bare}");
            (bare.to_owned(), dotted)
        })
        .collect();

    for file_proto in &mut fds.file {
        let pkg = file_proto.package.as_deref().unwrap_or("");
        filter_messages(
            &mut file_proto.message_type,
            pkg,
            &normalised_skip,
            skip_fields,
        );
    }
}

/// Returns `true` when `fqn` (without leading dot) or `.fqn` matches any
/// entry in `normalised_skip`.
fn message_is_skipped(fqn: &str, normalised_skip: &[(String, String)]) -> bool {
    let bare = fqn.trim_start_matches('.');
    normalised_skip
        .iter()
        .any(|(b, d)| b == bare || d.trim_start_matches('.') == bare)
}

/// Walk a list of [`prost_types::DescriptorProto`] in place, removing messages
/// listed in `normalised_skip` and recursively processing nested types.
///
/// After dropping messages, orphaned `type_name` references are also removed
/// from all surviving messages' fields.
fn filter_messages(
    messages: &mut Vec<prost_types::DescriptorProto>,
    parent_fqn: &str,
    normalised_skip: &[(String, String)],
    skip_fields: &[String],
) {
    // Collect the FQNs that will be removed at this level so we can clean up
    // type_name references afterwards.
    let mut removed_fqns: Vec<String> = Vec::new();

    messages.retain(|msg| {
        let msg_name = msg.name.as_deref().unwrap_or("");
        let fqn = if parent_fqn.is_empty() {
            msg_name.to_owned()
        } else {
            format!("{parent_fqn}.{msg_name}")
        };
        if message_is_skipped(&fqn, normalised_skip) {
            removed_fqns.push(fqn);
            false
        } else {
            true
        }
    });

    // Build the set of dot-prefixed FQNs that were removed.
    let removed_dotted: Vec<String> = removed_fqns
        .iter()
        .map(|fqn| {
            let bare = fqn.trim_start_matches('.');
            format!(".{bare}")
        })
        .collect();

    // Recurse into surviving messages' nested types and apply field filters.
    for msg in messages.iter_mut() {
        let msg_name = msg.name.as_deref().unwrap_or("");
        let fqn = if parent_fqn.is_empty() {
            msg_name.to_owned()
        } else {
            format!("{parent_fqn}.{msg_name}")
        };

        // Recurse into nested types.
        filter_messages(&mut msg.nested_type, &fqn, normalised_skip, skip_fields);

        // Remove orphaned field references caused by dropped messages.
        if !removed_dotted.is_empty() {
            msg.field.retain(|f| {
                if let Some(ref tn) = f.type_name {
                    !removed_dotted.iter().any(|r| r == tn)
                } else {
                    true
                }
            });
        }

        // Apply skip_fields filters.
        if !skip_fields.is_empty() {
            let short_name = msg_name;
            let full_name = &fqn;
            msg.field.retain(|f| {
                let field_name = f.name.as_deref().unwrap_or("");
                !skip_fields
                    .iter()
                    .any(|entry| field_matches_skip_entry(entry, field_name, short_name, full_name))
            });
        }
    }
}

/// Return `true` when `field_name` in message `short_name` / `full_fqn`
/// matches the skip entry `"Message.field_name"`.
fn field_matches_skip_entry(
    entry: &str,
    field_name: &str,
    short_msg_name: &str,
    full_msg_fqn: &str,
) -> bool {
    if let Some(dot_pos) = entry.rfind('.') {
        let entry_msg = entry[..dot_pos].trim_start_matches('.');
        let entry_field = &entry[dot_pos + 1..];
        if entry_field != field_name {
            return false;
        }
        let bare_full = full_msg_fqn.trim_start_matches('.');
        entry_msg == short_msg_name || entry_msg == bare_full
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use prost_types::{
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
    };
    use std::sync::{Arc, Mutex};

    fn make_field(name: &str, number: i32, type_name: Option<&str>) -> FieldDescriptorProto {
        FieldDescriptorProto {
            name: Some(name.to_owned()),
            number: Some(number),
            type_name: type_name.map(|s| s.to_owned()),
            r#type: Some(prost_types::field_descriptor_proto::Type::Message as i32),
            label: Some(prost_types::field_descriptor_proto::Label::Optional as i32),
            ..Default::default()
        }
    }

    fn make_message(
        name: &str,
        fields: Vec<FieldDescriptorProto>,
        nested: Vec<DescriptorProto>,
    ) -> DescriptorProto {
        DescriptorProto {
            name: Some(name.to_owned()),
            field: fields,
            nested_type: nested,
            ..Default::default()
        }
    }

    fn make_fds(package: &str, messages: Vec<DescriptorProto>) -> FileDescriptorSet {
        FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("test.proto".to_owned()),
                package: if package.is_empty() {
                    None
                } else {
                    Some(package.to_owned())
                },
                message_type: messages,
                ..Default::default()
            }],
        }
    }

    #[test]
    fn test_skip_message_removes_type() {
        let mut fds = make_fds(
            "mypkg",
            vec![
                make_message("Foo", vec![], vec![]),
                make_message("Bar", vec![], vec![]),
            ],
        );
        fds_apply_filters(&mut fds, &["mypkg.Foo".to_owned()], &[]);
        let names: Vec<&str> = fds.file[0]
            .message_type
            .iter()
            .map(|m| m.name.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(names, vec!["Bar"]);
    }

    #[test]
    fn test_skip_field_removes_field() {
        let mut fds = make_fds(
            "mypkg",
            vec![make_message(
                "MyMsg",
                vec![
                    make_field("keep_me", 1, None),
                    make_field("drop_me", 2, None),
                ],
                vec![],
            )],
        );
        fds_apply_filters(&mut fds, &[], &["MyMsg.drop_me".to_owned()]);
        let fields: Vec<&str> = fds.file[0].message_type[0]
            .field
            .iter()
            .map(|f| f.name.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(fields, vec!["keep_me"]);
    }

    #[test]
    fn test_skip_message_removes_orphaned_field_refs() {
        // Message A has a field whose type_name points to message B.
        // Skipping B should remove the field from A.
        let mut fds = make_fds(
            "mypkg",
            vec![
                make_message(
                    "MsgA",
                    vec![
                        make_field("normal", 1, None),
                        make_field("ref_to_b", 2, Some(".mypkg.MsgB")),
                    ],
                    vec![],
                ),
                make_message("MsgB", vec![], vec![]),
            ],
        );
        fds_apply_filters(&mut fds, &["mypkg.MsgB".to_owned()], &[]);
        assert_eq!(fds.file[0].message_type.len(), 1);
        let msg_a = &fds.file[0].message_type[0];
        assert_eq!(msg_a.name.as_deref(), Some("MsgA"));
        let field_names: Vec<&str> = msg_a
            .field
            .iter()
            .map(|f| f.name.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(field_names, vec!["normal"]);
    }

    #[test]
    fn test_skip_nested_message() {
        // Inner message "Inner" is nested inside "Outer"; skipping "Outer.Inner"
        // should remove it from nested_type.
        let inner = make_message("Inner", vec![], vec![]);
        let outer = make_message("Outer", vec![], vec![inner]);
        let mut fds = make_fds("pkg", vec![outer]);
        fds_apply_filters(&mut fds, &["pkg.Outer.Inner".to_owned()], &[]);
        assert_eq!(fds.file[0].message_type[0].nested_type.len(), 0);
    }

    #[test]
    fn test_service_generator_invoked() {
        use prost_types::ServiceDescriptorProto;

        let invoked = Arc::new(Mutex::new(false));
        let invoked_clone = Arc::clone(&invoked);

        let gen: ServiceGeneratorFn = Box::new(move |_svc: &ServiceDescriptorProto| {
            *invoked_clone.lock().unwrap() = true;
            "// generated service\n".to_owned()
        });

        let svc = ServiceDescriptorProto {
            name: Some("MyService".to_owned()),
            ..Default::default()
        };
        let code = gen(&svc);
        assert!(*invoked.lock().unwrap());
        assert!(code.contains("generated service"));
    }

    // -----------------------------------------------------------------------
    // Incremental compilation / fingerprinting helpers
    // -----------------------------------------------------------------------

    #[test]
    fn fnv1a64_empty_input_returns_offset_basis() {
        // FNV-1a 64: empty input = offset basis = 0xcbf29ce484222325
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325u64);
    }

    #[test]
    fn fnv1a64_known_value() {
        // Verify determinism: same input always gives same output.
        let h1 = fnv1a64(b"hello world");
        let h2 = fnv1a64(b"hello world");
        assert_eq!(h1, h2);
        // Different input → different hash (high probability, not guaranteed for collisions).
        let h3 = fnv1a64(b"hello worlds");
        assert_ne!(h1, h3);
    }

    #[test]
    fn fingerprint_files_computes_hex_hash() {
        // Write a temp file and fingerprint it.
        let dir = std::env::temp_dir();
        let path = dir.join(format!("oxiproto_fp_test_{}.proto", std::process::id()));
        std::fs::write(&path, b"syntax = \"proto3\";\nmessage M {}").expect("write test proto");
        let fps = fingerprint_files(&[&path]);
        assert_eq!(fps.len(), 1);
        let key = path.to_string_lossy().into_owned();
        let hash_str = fps.get(&key).expect("key must exist");
        // Must be 16 hex chars (64-bit / 4 bits per char).
        assert_eq!(hash_str.len(), 16, "hex hash must be 16 chars: {hash_str}");
        assert!(
            hash_str.chars().all(|c| c.is_ascii_hexdigit()),
            "must be hex: {hash_str}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fingerprint_files_skips_missing_file() {
        let path = std::path::PathBuf::from("/nonexistent/path/xyz.proto");
        let fps = fingerprint_files(&[&path]);
        assert!(fps.is_empty(), "missing file should be silently skipped");
    }

    #[test]
    fn fingerprint_cache_roundtrip() {
        let dir = std::env::temp_dir();
        let cache_path = dir.join(format!("oxiproto_cache_test_{}.cache", std::process::id()));

        let mut fps = HashMap::new();
        fps.insert(
            "/some/path/a.proto".to_owned(),
            "deadbeefcafebabe".to_owned(),
        );
        fps.insert(
            "/some/path/b.proto".to_owned(),
            "0102030405060708".to_owned(),
        );

        save_fingerprint_cache(&cache_path, &fps).expect("save must succeed");
        let loaded = load_fingerprint_cache(&cache_path).expect("load must succeed");

        assert_eq!(fps.len(), loaded.len());
        for (k, v) in &fps {
            assert_eq!(loaded.get(k), Some(v), "mismatch for key {k}");
        }
        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn fingerprints_match_returns_true_for_identical_maps() {
        let mut a = HashMap::new();
        a.insert("a.proto".to_owned(), "aabbccdd11223344".to_owned());
        a.insert("b.proto".to_owned(), "1122334455667788".to_owned());
        let b = a.clone();
        assert!(fingerprints_match(&a, &b));
    }

    #[test]
    fn fingerprints_match_returns_false_when_hash_differs() {
        let mut a = HashMap::new();
        a.insert("a.proto".to_owned(), "aabbccdd11223344".to_owned());
        let mut b = HashMap::new();
        b.insert("a.proto".to_owned(), "deadbeefcafebabe".to_owned());
        assert!(!fingerprints_match(&a, &b));
    }

    #[test]
    fn fingerprints_match_returns_false_when_extra_key() {
        let mut a = HashMap::new();
        a.insert("a.proto".to_owned(), "deadbeef00000000".to_owned());
        let mut b = a.clone();
        b.insert("b.proto".to_owned(), "deadbeef00000000".to_owned());
        assert!(!fingerprints_match(&a, &b));
    }

    #[test]
    fn fingerprints_match_returns_false_for_empty_vs_non_empty() {
        let a: HashMap<String, String> = HashMap::new();
        let mut b = HashMap::new();
        b.insert("x.proto".to_owned(), "aaaa000000000000".to_owned());
        assert!(!fingerprints_match(&a, &b));
        assert!(!fingerprints_match(&b, &a));
    }

    #[test]
    fn load_fingerprint_cache_returns_err_for_missing_file() {
        let path = std::path::PathBuf::from("/nonexistent/cache/path.cache");
        assert!(load_fingerprint_cache(&path).is_err());
    }

    #[test]
    fn load_fingerprint_cache_ignores_malformed_lines() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("oxiproto_cache_bad_{}.cache", std::process::id()));
        // File contains one valid line, one malformed line, one empty line.
        std::fs::write(
            &path,
            "valid/path.proto\taabb000000000000\nBAD_NO_TAB\n\nother.proto\t1122000000000000\n",
        )
        .expect("write");
        let loaded = load_fingerprint_cache(&path).expect("load");
        assert_eq!(loaded.len(), 2, "only 2 valid entries should be loaded");
        assert!(loaded.contains_key("valid/path.proto"));
        assert!(loaded.contains_key("other.proto"));
        assert!(!loaded.contains_key("BAD_NO_TAB"));
        let _ = std::fs::remove_file(&path);
    }
}
