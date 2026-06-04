//! Native [`DescriptorPool`] built from a [`prost_types::FileDescriptorSet`].
//!
//! The pool builds an in-memory, index-based descriptor model
//! ([`PoolInner`]) in two passes:
//!
//! 1. **Registration** — walk every file and recursively register every
//!    message (and its nested messages) and enum, assigning each a stable
//!    index and recording its fully-qualified name.
//! 2. **Resolution** — walk every message field and service method, resolving
//!    type-name references (e.g. `.my.pkg.Other`) against the name table to
//!    concrete indices, producing [`Kind`] values and method input/output
//!    indices.
//!
//! Because descriptors are index handles over a shared [`Arc<PoolInner>`],
//! circular references between messages are represented naturally.

use std::collections::HashMap;
use std::sync::Arc;

use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{
    DescriptorProto, EnumDescriptorProto, FileDescriptorSet, ServiceDescriptorProto,
};

use super::descriptor::{
    Cardinality, EnumData, EnumDescriptor, EnumValueData, FieldData, FileData, Kind, MessageData,
    MessageDescriptor, MethodData, OneofData, ServiceData, ServiceDescriptor,
};
use crate::ReflectError;

/// The shared, immutable backing store for all native descriptors in a pool.
///
/// All public descriptor handles hold an [`Arc`] to one of these and an index
/// into the relevant vector.
#[derive(Debug)]
pub struct PoolInner {
    pub(crate) files: Vec<FileData>,
    pub(crate) messages: Vec<MessageData>,
    pub(crate) enums: Vec<EnumData>,
    pub(crate) services: Vec<ServiceData>,
    /// Fully-qualified message name → index into `messages`.
    pub(crate) message_by_name: HashMap<String, usize>,
    /// Fully-qualified enum name → index into `enums`.
    pub(crate) enum_by_name: HashMap<String, usize>,
    /// Fully-qualified service name → index into `services`.
    pub(crate) service_by_name: HashMap<String, usize>,
}

/// A native protobuf descriptor pool.
///
/// Built from a [`prost_types::FileDescriptorSet`] via
/// [`DescriptorPool::from_file_descriptor_set`]. Cheaply cloneable (it wraps an
/// [`Arc`]).
#[derive(Clone, Debug)]
pub struct DescriptorPool {
    inner: Arc<PoolInner>,
}

/// During registration, a fully-qualified type name resolves to either a
/// message or an enum index.
#[derive(Clone, Copy)]
enum TypeRef {
    Message(usize),
    Enum(usize),
}

impl DescriptorPool {
    /// Build a pool from a decoded [`FileDescriptorSet`].
    ///
    /// # Errors
    ///
    /// Returns [`ReflectError::Pool`] if a descriptor is malformed (missing
    /// required name/number fields) or if a field, method input, or method
    /// output references a type name that is not present in the set.
    pub fn from_file_descriptor_set(fds: FileDescriptorSet) -> Result<Self, ReflectError> {
        let mut builder = Builder::default();
        builder.register(&fds)?;
        builder.resolve(&fds)?;
        Ok(Self {
            inner: Arc::new(builder.into_inner()),
        })
    }

    /// Look up a message by its fully-qualified name (no leading dot).
    pub fn get_message_by_name(&self, full_name: &str) -> Option<MessageDescriptor> {
        self.inner
            .message_by_name
            .get(full_name)
            .map(|&index| MessageDescriptor {
                pool: Arc::clone(&self.inner),
                index,
            })
    }

    /// Look up an enum by its fully-qualified name (no leading dot).
    pub fn get_enum_by_name(&self, full_name: &str) -> Option<EnumDescriptor> {
        self.inner
            .enum_by_name
            .get(full_name)
            .map(|&index| EnumDescriptor {
                pool: Arc::clone(&self.inner),
                index,
            })
    }

    /// Look up a service by its fully-qualified name (no leading dot).
    pub fn get_service_by_name(&self, full_name: &str) -> Option<ServiceDescriptor> {
        self.inner
            .service_by_name
            .get(full_name)
            .map(|&index| ServiceDescriptor {
                pool: Arc::clone(&self.inner),
                index,
            })
    }

    /// Iterate over every message in the pool (including nested messages and
    /// synthetic map-entry types), in registration order.
    pub fn all_messages(&self) -> impl ExactSizeIterator<Item = MessageDescriptor> + '_ {
        let inner = Arc::clone(&self.inner);
        (0..self.inner.messages.len()).map(move |index| MessageDescriptor {
            pool: Arc::clone(&inner),
            index,
        })
    }

    /// Iterate over every enum in the pool, in registration order.
    pub fn all_enums(&self) -> impl ExactSizeIterator<Item = EnumDescriptor> + '_ {
        let inner = Arc::clone(&self.inner);
        (0..self.inner.enums.len()).map(move |index| EnumDescriptor {
            pool: Arc::clone(&inner),
            index,
        })
    }

    /// Iterate over every service in the pool, in registration order.
    pub fn services(&self) -> impl ExactSizeIterator<Item = ServiceDescriptor> + '_ {
        let inner = Arc::clone(&self.inner);
        (0..self.inner.services.len()).map(move |index| ServiceDescriptor {
            pool: Arc::clone(&inner),
            index,
        })
    }
}

/// Mutable accumulator used while building a [`PoolInner`].
#[derive(Default)]
struct Builder {
    files: Vec<FileData>,
    messages: Vec<MessageData>,
    enums: Vec<EnumData>,
    services: Vec<ServiceData>,
    message_by_name: HashMap<String, usize>,
    enum_by_name: HashMap<String, usize>,
    service_by_name: HashMap<String, usize>,
    /// Combined type table (messages + enums) keyed by fully-qualified name,
    /// used during resolution.
    type_by_name: HashMap<String, TypeRef>,
}

impl Builder {
    fn into_inner(self) -> PoolInner {
        PoolInner {
            files: self.files,
            messages: self.messages,
            enums: self.enums,
            services: self.services,
            message_by_name: self.message_by_name,
            enum_by_name: self.enum_by_name,
            service_by_name: self.service_by_name,
        }
    }

    /// First pass: register every file, message, nested message, and enum,
    /// assigning indices and building the name tables.
    fn register(&mut self, fds: &FileDescriptorSet) -> Result<(), ReflectError> {
        for file in &fds.file {
            let package = file.package.clone().unwrap_or_default();
            let file_index = self.files.len();
            let (java_pkg, go_pkg, java_outer, deprecated, optimize_for) =
                if let Some(opts) = &file.options {
                    (
                        opts.java_package.clone(),
                        opts.go_package.clone(),
                        opts.java_outer_classname.clone(),
                        opts.deprecated.unwrap_or(false),
                        opts.optimize_for.unwrap_or(0),
                    )
                } else {
                    (None, None, None, false, 0)
                };
            self.files.push(FileData {
                name: file.name.clone().unwrap_or_default(),
                package: package.clone(),
                syntax: file.syntax.clone().unwrap_or_else(|| "proto2".to_owned()),
                dependencies: file.dependency.clone(),
                java_package: java_pkg,
                go_package: go_pkg,
                java_outer_classname: java_outer,
                deprecated,
                optimize_for,
            });

            for msg in &file.message_type {
                self.register_message(msg, &package, file_index)?;
            }
            for en in &file.enum_type {
                self.register_enum(en, &package, file_index)?;
            }
        }
        Ok(())
    }

    /// Register a message (and recursively its nested messages and enums).
    /// Returns the assigned message index.
    fn register_message(
        &mut self,
        msg: &DescriptorProto,
        scope: &str,
        file_index: usize,
    ) -> Result<usize, ReflectError> {
        let name = msg
            .name
            .clone()
            .ok_or_else(|| ReflectError::Pool("message without a name".to_owned()))?;
        let full_name = qualify(scope, &name);

        let is_map_entry = msg
            .options
            .as_ref()
            .and_then(|o| o.map_entry)
            .unwrap_or(false);

        // Reserve this message's slot before recursing so nested types get
        // larger indices and the parent index is stable.
        let index = self.messages.len();
        self.messages.push(MessageData {
            full_name: full_name.clone(),
            name,
            file_index,
            fields: Vec::new(),
            field_by_number: HashMap::new(),
            field_by_name: HashMap::new(),
            field_by_json_name: HashMap::new(),
            oneofs: Vec::new(),
            nested_messages: Vec::new(),
            nested_enums: Vec::new(),
            is_map_entry,
        });
        if self
            .message_by_name
            .insert(full_name.clone(), index)
            .is_some()
        {
            return Err(ReflectError::Pool(format!(
                "duplicate message name '{full_name}'"
            )));
        }
        self.type_by_name
            .insert(full_name.clone(), TypeRef::Message(index));

        let mut nested_messages = Vec::with_capacity(msg.nested_type.len());
        for nested in &msg.nested_type {
            let child = self.register_message(nested, &full_name, file_index)?;
            nested_messages.push(child);
        }
        let mut nested_enums = Vec::with_capacity(msg.enum_type.len());
        for nested in &msg.enum_type {
            let child = self.register_enum(nested, &full_name, file_index)?;
            nested_enums.push(child);
        }
        self.messages[index].nested_messages = nested_messages;
        self.messages[index].nested_enums = nested_enums;

        Ok(index)
    }

    /// Register an enum. Returns the assigned enum index.
    fn register_enum(
        &mut self,
        en: &EnumDescriptorProto,
        scope: &str,
        file_index: usize,
    ) -> Result<usize, ReflectError> {
        let name = en
            .name
            .clone()
            .ok_or_else(|| ReflectError::Pool("enum without a name".to_owned()))?;
        let full_name = qualify(scope, &name);

        let mut values = Vec::with_capacity(en.value.len());
        let mut value_by_number = HashMap::new();
        let mut value_by_name = HashMap::new();
        for value in &en.value {
            let value_name = value
                .name
                .clone()
                .ok_or_else(|| ReflectError::Pool("enum value without a name".to_owned()))?;
            let number = value
                .number
                .ok_or_else(|| ReflectError::Pool("enum value without a number".to_owned()))?;
            let value_index = values.len();
            // Enum value names are scoped to the *enclosing* scope of the enum,
            // not the enum itself (C++ scoping rules), but for lookup we record
            // the qualified-under-enum name which is what most tooling expects.
            let value_full_name = qualify(&full_name, &value_name);
            values.push(EnumValueData {
                name: value_name.clone(),
                full_name: value_full_name,
                number,
            });
            // First occurrence of a number wins for the by-number map (protobuf
            // allows aliases when `allow_alias` is set).
            value_by_number.entry(number).or_insert(value_index);
            value_by_name.insert(value_name, value_index);
        }

        let index = self.enums.len();
        self.enums.push(EnumData {
            full_name: full_name.clone(),
            name,
            file_index,
            values,
            value_by_number,
            value_by_name,
        });
        if self.enum_by_name.insert(full_name.clone(), index).is_some() {
            return Err(ReflectError::Pool(format!(
                "duplicate enum name '{full_name}'"
            )));
        }
        self.type_by_name.insert(full_name, TypeRef::Enum(index));

        Ok(index)
    }

    /// Second pass: resolve all field type references and service methods.
    fn resolve(&mut self, fds: &FileDescriptorSet) -> Result<(), ReflectError> {
        // Resolve message fields. We re-walk the FDS in the same order as
        // registration so message indices line up.
        let mut message_cursor = 0usize;
        for file in &fds.file {
            let syntax = file.syntax.as_deref().unwrap_or("proto2");
            for msg in &file.message_type {
                self.resolve_message(msg, &mut message_cursor, syntax)?;
            }
        }

        // Resolve services.
        for file in &fds.file {
            let package = file.package.clone().unwrap_or_default();
            for svc in &file.service {
                self.resolve_service(svc, &package)?;
            }
        }

        Ok(())
    }

    /// Resolve a single message's fields, advancing `cursor` over this message
    /// and all of its nested messages (matching registration order).
    fn resolve_message(
        &mut self,
        msg: &DescriptorProto,
        cursor: &mut usize,
        syntax: &str,
    ) -> Result<(), ReflectError> {
        let index = *cursor;
        *cursor += 1;

        let message_full_name = self.messages[index].full_name.clone();

        // Build field data.
        let mut fields: Vec<FieldData> = Vec::with_capacity(msg.field.len());
        let mut field_by_number = HashMap::new();
        let mut field_by_name = HashMap::new();
        let mut field_by_json_name = HashMap::new();

        for field in &msg.field {
            let fname = field
                .name
                .clone()
                .ok_or_else(|| ReflectError::Pool("field without a name".to_owned()))?;
            let number = field
                .number
                .ok_or_else(|| ReflectError::Pool(format!("field '{fname}' without a number")))?;
            let number = u32::try_from(number).map_err(|_| {
                ReflectError::Pool(format!("field '{fname}' has invalid number {number}"))
            })?;

            let kind = self.resolve_kind(field, &fname)?;

            let label = field
                .label
                .and_then(|l| Label::try_from(l).ok())
                .unwrap_or(Label::Optional);
            let cardinality = match label {
                Label::Optional => Cardinality::Optional,
                Label::Required => Cardinality::Required,
                Label::Repeated => Cardinality::Repeated,
            };

            let proto3_optional = field.proto3_optional.unwrap_or(false);

            let packed = compute_packed(field, kind, cardinality, syntax);

            let oneof_index = field
                .oneof_index
                .map(|i| usize::try_from(i).unwrap_or(usize::MAX));

            let json_name = field
                .json_name
                .clone()
                .unwrap_or_else(|| to_json_name(&fname));

            let field_full_name = qualify(&message_full_name, &fname);
            let pos = fields.len();
            field_by_number.insert(number, pos);
            field_by_name.insert(fname.clone(), pos);
            field_by_json_name.insert(json_name.clone(), pos);

            fields.push(FieldData {
                name: fname,
                full_name: field_full_name,
                json_name,
                number,
                kind,
                cardinality,
                packed,
                oneof_index,
                proto3_optional,
            });
        }

        // Build oneof data, then attach field indices.
        let mut oneofs: Vec<OneofData> = Vec::with_capacity(msg.oneof_decl.len());
        for decl in &msg.oneof_decl {
            let oname = decl
                .name
                .clone()
                .ok_or_else(|| ReflectError::Pool("oneof without a name".to_owned()))?;
            let oneof_full_name = qualify(&message_full_name, &oname);
            oneofs.push(OneofData {
                name: oname,
                full_name: oneof_full_name,
                field_indices: Vec::new(),
                // Provisionally non-synthetic; refined below.
                is_synthetic: false,
            });
        }
        for (pos, field) in fields.iter().enumerate() {
            if let Some(oi) = field.oneof_index {
                if let Some(oneof) = oneofs.get_mut(oi) {
                    oneof.field_indices.push(pos);
                    // A proto3 `optional` field is implemented as a synthetic
                    // single-field oneof.
                    if field.proto3_optional {
                        oneof.is_synthetic = true;
                    }
                }
            }
        }

        self.messages[index].fields = fields;
        self.messages[index].field_by_number = field_by_number;
        self.messages[index].field_by_name = field_by_name;
        self.messages[index].field_by_json_name = field_by_json_name;
        self.messages[index].oneofs = oneofs;

        // Recurse into nested messages, keeping the cursor in registration
        // order.
        for nested in &msg.nested_type {
            self.resolve_message(nested, cursor, syntax)?;
        }

        Ok(())
    }

    /// Resolve a field's [`Kind`] from its protobuf type and (for
    /// message/enum) its `type_name`.
    fn resolve_kind(
        &self,
        field: &prost_types::FieldDescriptorProto,
        fname: &str,
    ) -> Result<Kind, ReflectError> {
        let ty = field
            .r#type
            .and_then(|t| Type::try_from(t).ok())
            .ok_or_else(|| ReflectError::Pool(format!("field '{fname}' without a type")))?;

        let kind = match ty {
            Type::Double => Kind::Double,
            Type::Float => Kind::Float,
            Type::Int64 => Kind::Int64,
            Type::Uint64 => Kind::Uint64,
            Type::Int32 => Kind::Int32,
            Type::Fixed64 => Kind::Fixed64,
            Type::Fixed32 => Kind::Fixed32,
            Type::Bool => Kind::Bool,
            Type::String => Kind::String,
            Type::Bytes => Kind::Bytes,
            Type::Uint32 => Kind::Uint32,
            Type::Sfixed32 => Kind::Sfixed32,
            Type::Sfixed64 => Kind::Sfixed64,
            Type::Sint32 => Kind::Sint32,
            Type::Sint64 => Kind::Sint64,
            Type::Group => {
                let idx = self.resolve_type_name(field, fname, true)?;
                Kind::Group(idx)
            }
            Type::Message => {
                let idx = self.resolve_type_name(field, fname, true)?;
                Kind::Message(idx)
            }
            Type::Enum => {
                let idx = self.resolve_type_name(field, fname, false)?;
                Kind::Enum(idx)
            }
        };
        Ok(kind)
    }

    /// Resolve a `type_name` reference (e.g. `.pkg.Msg` or `pkg.Msg`) to a
    /// message or enum index.
    fn resolve_type_name(
        &self,
        field: &prost_types::FieldDescriptorProto,
        fname: &str,
        expect_message: bool,
    ) -> Result<usize, ReflectError> {
        let raw = field.type_name.as_deref().ok_or_else(|| {
            ReflectError::Pool(format!(
                "field '{fname}' is a message/enum but has no type_name"
            ))
        })?;
        let key = raw.strip_prefix('.').unwrap_or(raw);
        match self.type_by_name.get(key) {
            Some(TypeRef::Message(i)) if expect_message => Ok(*i),
            Some(TypeRef::Enum(i)) if !expect_message => Ok(*i),
            Some(_) => Err(ReflectError::Pool(format!(
                "field '{fname}' type '{key}' resolved to the wrong kind"
            ))),
            None => Err(ReflectError::Pool(format!(
                "field '{fname}' references unknown type '{key}'"
            ))),
        }
    }

    /// Resolve a service and its methods.
    fn resolve_service(
        &mut self,
        svc: &ServiceDescriptorProto,
        package: &str,
    ) -> Result<(), ReflectError> {
        let name = svc
            .name
            .clone()
            .ok_or_else(|| ReflectError::Pool("service without a name".to_owned()))?;
        let full_name = qualify(package, &name);

        let mut methods = Vec::with_capacity(svc.method.len());
        for method in &svc.method {
            let mname = method
                .name
                .clone()
                .ok_or_else(|| ReflectError::Pool("method without a name".to_owned()))?;
            let input_index =
                self.resolve_message_ref(method.input_type.as_deref(), &mname, "input")?;
            let output_index =
                self.resolve_message_ref(method.output_type.as_deref(), &mname, "output")?;
            let method_full_name = qualify(&full_name, &mname);
            methods.push(MethodData {
                name: mname,
                full_name: method_full_name,
                input_index,
                output_index,
                client_streaming: method.client_streaming.unwrap_or(false),
                server_streaming: method.server_streaming.unwrap_or(false),
            });
        }

        let index = self.services.len();
        self.services.push(ServiceData {
            full_name: full_name.clone(),
            name,
            file_index: self.file_index_for_package(package),
            methods,
        });
        if self
            .service_by_name
            .insert(full_name.clone(), index)
            .is_some()
        {
            return Err(ReflectError::Pool(format!(
                "duplicate service name '{full_name}'"
            )));
        }
        Ok(())
    }

    /// Resolve a method input/output message type name to a message index.
    fn resolve_message_ref(
        &self,
        type_name: Option<&str>,
        method_name: &str,
        role: &str,
    ) -> Result<usize, ReflectError> {
        let raw = type_name.ok_or_else(|| {
            ReflectError::Pool(format!("method '{method_name}' has no {role} type"))
        })?;
        let key = raw.strip_prefix('.').unwrap_or(raw);
        match self.type_by_name.get(key) {
            Some(TypeRef::Message(i)) => Ok(*i),
            _ => Err(ReflectError::Pool(format!(
                "method '{method_name}' {role} type '{key}' is not a known message"
            ))),
        }
    }

    /// Best-effort lookup of a file index for a package, used to set a
    /// service's parent file. Falls back to the first file (index 0) if no
    /// match is found and at least one file exists.
    fn file_index_for_package(&self, package: &str) -> usize {
        self.files
            .iter()
            .position(|f| f.package == package)
            .unwrap_or(0)
    }
}

/// Join a scope and a name with a `.` separator, omitting the separator when
/// the scope is empty.
fn qualify(scope: &str, name: &str) -> String {
    if scope.is_empty() {
        name.to_owned()
    } else {
        format!("{scope}.{name}")
    }
}

/// Compute the effective `packed` flag for a field.
///
/// Only repeated packable scalars can be packed. proto3 packs by default;
/// proto2 does not. An explicit `options.packed` overrides the default.
fn compute_packed(
    field: &prost_types::FieldDescriptorProto,
    kind: Kind,
    cardinality: Cardinality,
    syntax: &str,
) -> bool {
    if !matches!(cardinality, Cardinality::Repeated) || !kind.is_packable() {
        return false;
    }
    if let Some(opts) = field.options.as_ref() {
        if let Some(packed) = opts.packed {
            return packed;
        }
    }
    syntax == "proto3"
}

/// Derive the default JSON name (lowerCamelCase) from a snake_case field name,
/// matching protobuf's algorithm.
fn to_json_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper_next = false;
    for ch in name.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}
