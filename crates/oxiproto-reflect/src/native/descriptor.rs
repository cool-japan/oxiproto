//! Native protobuf descriptor types.
//!
//! These types mirror the surface of `prost_reflect`'s descriptors but are
//! built natively from a [`prost_types::FileDescriptorSet`] by
//! [`DescriptorPool`](super::pool::DescriptorPool). Each descriptor is a cheap
//! handle holding an [`Arc`] to the shared [`PoolInner`] plus an index, so
//! cloning is O(1) and circular message references (a field whose type is the
//! message itself, directly or transitively) are naturally supported.

use std::collections::HashMap;
use std::sync::Arc;

use super::pool::PoolInner;

/// The resolved type of a field.
///
/// Scalar types carry no payload; `Message` and `Enum` carry the index of the
/// referenced type within the shared [`PoolInner`]. `Group` is recognised but
/// unsupported on the wire (see the codec's explicit error).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `double`
    Double,
    /// `float`
    Float,
    /// `int32`
    Int32,
    /// `int64`
    Int64,
    /// `uint32`
    Uint32,
    /// `uint64`
    Uint64,
    /// `sint32`
    Sint32,
    /// `sint64`
    Sint64,
    /// `fixed32`
    Fixed32,
    /// `fixed64`
    Fixed64,
    /// `sfixed32`
    Sfixed32,
    /// `sfixed64`
    Sfixed64,
    /// `bool`
    Bool,
    /// `string`
    String,
    /// `bytes`
    Bytes,
    /// An embedded message; the index identifies the message in the pool.
    Message(usize),
    /// An enum; the index identifies the enum in the pool.
    Enum(usize),
    /// A proto2 group (unsupported on the wire); the index identifies the
    /// synthetic group message in the pool.
    Group(usize),
}

impl Kind {
    /// Returns `true` if this kind is a length-delimited scalar (`string` or
    /// `bytes`).
    pub fn is_length_delimited_scalar(self) -> bool {
        matches!(self, Kind::String | Kind::Bytes)
    }

    /// Returns `true` if this kind is a 64-bit fixed-width scalar.
    pub fn is_fixed64(self) -> bool {
        matches!(self, Kind::Double | Kind::Fixed64 | Kind::Sfixed64)
    }

    /// Returns `true` if this kind is a 32-bit fixed-width scalar.
    pub fn is_fixed32(self) -> bool {
        matches!(self, Kind::Float | Kind::Fixed32 | Kind::Sfixed32)
    }

    /// Returns `true` if this kind is a varint-encoded scalar (including enums
    /// and booleans).
    pub fn is_varint(self) -> bool {
        matches!(
            self,
            Kind::Int32
                | Kind::Int64
                | Kind::Uint32
                | Kind::Uint64
                | Kind::Sint32
                | Kind::Sint64
                | Kind::Bool
                | Kind::Enum(_)
        )
    }

    /// Returns `true` if this kind can be packed in a repeated field (all
    /// scalar numeric/bool/enum types).
    pub fn is_packable(self) -> bool {
        self.is_varint() || self.is_fixed32() || self.is_fixed64()
    }
}

/// The cardinality (label) of a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cardinality {
    /// Optional (proto3 singular, or proto2 `optional`).
    Optional,
    /// Required (proto2 `required`).
    Required,
    /// Repeated.
    Repeated,
}

// ---------------------------------------------------------------------------
// Internal data model (owned by `PoolInner`)
// ---------------------------------------------------------------------------

/// Owned data for a message type, stored in [`PoolInner::messages`].
#[derive(Debug)]
pub(crate) struct MessageData {
    pub full_name: String,
    pub name: String,
    /// Indices into `PoolInner::files` identifying the declaring file.
    pub file_index: usize,
    /// Field data in declaration order.
    pub fields: Vec<FieldData>,
    /// Map from field number to position in `fields`.
    pub field_by_number: HashMap<u32, usize>,
    /// Map from field name to position in `fields`.
    pub field_by_name: HashMap<String, usize>,
    /// Map from JSON name to position in `fields`.
    pub field_by_json_name: HashMap<String, usize>,
    /// Oneof declarations in declaration order.
    pub oneofs: Vec<OneofData>,
    /// Indices (into `PoolInner::messages`) of nested message types.
    pub nested_messages: Vec<usize>,
    /// Indices (into `PoolInner::enums`) of nested enum types.
    pub nested_enums: Vec<usize>,
    /// Whether this message is a synthetic `map<K, V>` entry type
    /// (`options.map_entry == true`).
    pub is_map_entry: bool,
}

/// Owned data for a single field.
#[derive(Debug)]
pub(crate) struct FieldData {
    pub name: String,
    pub full_name: String,
    pub json_name: String,
    pub number: u32,
    pub kind: Kind,
    pub cardinality: Cardinality,
    /// Whether the field is encoded packed (only meaningful for repeated
    /// packable scalars). Defaults follow proto3 (packed) / proto2 (unpacked).
    pub packed: bool,
    /// Index into the parent message's `oneofs`, if this field is a member of
    /// a oneof. Synthetic oneofs (proto3 optional) are included.
    pub oneof_index: Option<usize>,
    /// `true` if this field is a proto3 `optional` (synthetic-oneof) field.
    pub proto3_optional: bool,
}

/// Owned data for a oneof declaration.
#[derive(Debug)]
pub(crate) struct OneofData {
    pub name: String,
    pub full_name: String,
    /// Field positions (into the parent message's `fields`) belonging to this
    /// oneof, in declaration order.
    pub field_indices: Vec<usize>,
    /// `true` if this oneof is synthetic (generated for a proto3 `optional`
    /// field).
    pub is_synthetic: bool,
}

/// Owned data for an enum type.
#[derive(Debug)]
pub(crate) struct EnumData {
    pub full_name: String,
    pub name: String,
    pub file_index: usize,
    pub values: Vec<EnumValueData>,
    pub value_by_number: HashMap<i32, usize>,
    pub value_by_name: HashMap<String, usize>,
}

/// Owned data for a single enum value.
#[derive(Debug)]
pub(crate) struct EnumValueData {
    pub name: String,
    pub full_name: String,
    pub number: i32,
}

/// Owned data for a service.
#[derive(Debug)]
pub(crate) struct ServiceData {
    pub full_name: String,
    pub name: String,
    pub file_index: usize,
    pub methods: Vec<MethodData>,
}

/// Owned data for a single method.
#[derive(Debug)]
pub(crate) struct MethodData {
    pub name: String,
    pub full_name: String,
    /// Index into `PoolInner::messages` for the input type.
    pub input_index: usize,
    /// Index into `PoolInner::messages` for the output type.
    pub output_index: usize,
    pub client_streaming: bool,
    pub server_streaming: bool,
}

/// Owned data for a file.
#[derive(Debug)]
pub(crate) struct FileData {
    pub name: String,
    pub package: String,
    pub syntax: String,
    pub dependencies: Vec<String>,
    /// `java_package` file option, if set.
    pub java_package: Option<String>,
    /// `go_package` file option, if set.
    pub go_package: Option<String>,
    /// `java_outer_classname` file option, if set.
    pub java_outer_classname: Option<String>,
    /// Whether the `deprecated` file option is set.
    pub deprecated: bool,
    /// Whether the `optimize_for` option is set to `CODE_SIZE` (2) or
    /// `LITE_RUNTIME` (3). Stored as the raw integer from the proto enum.
    pub optimize_for: i32,
}

// ---------------------------------------------------------------------------
// Public handle types
// ---------------------------------------------------------------------------

/// A handle to a file descriptor within a [`DescriptorPool`](super::pool::DescriptorPool).
#[derive(Clone)]
pub struct FileDescriptor {
    pub(crate) pool: Arc<PoolInner>,
    pub(crate) index: usize,
}

/// A handle to a message descriptor.
#[derive(Clone)]
pub struct MessageDescriptor {
    pub(crate) pool: Arc<PoolInner>,
    pub(crate) index: usize,
}

/// A handle to a single field of a message.
#[derive(Clone)]
pub struct FieldDescriptor {
    pub(crate) pool: Arc<PoolInner>,
    pub(crate) message_index: usize,
    pub(crate) field_index: usize,
}

/// A handle to a oneof declaration of a message.
#[derive(Clone)]
pub struct OneofDescriptor {
    pub(crate) pool: Arc<PoolInner>,
    pub(crate) message_index: usize,
    pub(crate) oneof_index: usize,
}

/// A handle to an enum descriptor.
#[derive(Clone)]
pub struct EnumDescriptor {
    pub(crate) pool: Arc<PoolInner>,
    pub(crate) index: usize,
}

/// A handle to a single value of an enum.
#[derive(Clone)]
pub struct EnumValueDescriptor {
    pub(crate) pool: Arc<PoolInner>,
    pub(crate) enum_index: usize,
    pub(crate) value_index: usize,
}

/// A handle to a service descriptor.
#[derive(Clone)]
pub struct ServiceDescriptor {
    pub(crate) pool: Arc<PoolInner>,
    pub(crate) index: usize,
}

/// A handle to a single method of a service.
#[derive(Clone)]
pub struct MethodDescriptor {
    pub(crate) pool: Arc<PoolInner>,
    pub(crate) service_index: usize,
    pub(crate) method_index: usize,
}

// ---------------------------------------------------------------------------
// FileDescriptor impl
// ---------------------------------------------------------------------------

impl FileDescriptor {
    /// The file name (path), e.g. `"my/package/file.proto"`.
    pub fn name(&self) -> &str {
        &self.pool.files[self.index].name
    }

    /// The protobuf package, e.g. `"my.package"` (empty if none).
    pub fn package_name(&self) -> &str {
        &self.pool.files[self.index].package
    }

    /// The declared syntax (`"proto2"` or `"proto3"`).
    pub fn syntax(&self) -> &str {
        &self.pool.files[self.index].syntax
    }

    /// The names of imported (dependency) files.
    pub fn dependencies(&self) -> impl Iterator<Item = &str> + '_ {
        self.pool.files[self.index]
            .dependencies
            .iter()
            .map(String::as_str)
    }

    /// The `java_package` file option, if set.
    pub fn java_package(&self) -> Option<&str> {
        self.pool.files[self.index].java_package.as_deref()
    }

    /// The `go_package` file option, if set.
    pub fn go_package(&self) -> Option<&str> {
        self.pool.files[self.index].go_package.as_deref()
    }

    /// The `java_outer_classname` file option, if set.
    pub fn java_outer_classname(&self) -> Option<&str> {
        self.pool.files[self.index].java_outer_classname.as_deref()
    }

    /// Returns `true` if the `deprecated` file option is set to `true`.
    pub fn is_deprecated(&self) -> bool {
        self.pool.files[self.index].deprecated
    }

    /// The raw `optimize_for` option value (0 = SPEED, 2 = CODE_SIZE, 3 =
    /// LITE_RUNTIME; 0 is the default when unset).
    pub fn optimize_for(&self) -> i32 {
        self.pool.files[self.index].optimize_for
    }
}

impl std::fmt::Debug for FileDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileDescriptor")
            .field("name", &self.name())
            .field("package", &self.package_name())
            .finish()
    }
}

impl PartialEq for FileDescriptor {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pool, &other.pool) && self.index == other.index
    }
}

// ---------------------------------------------------------------------------
// MessageDescriptor impl
// ---------------------------------------------------------------------------

impl MessageDescriptor {
    pub(crate) fn data(&self) -> &MessageData {
        &self.pool.messages[self.index]
    }

    /// The simple (unqualified) name of the message.
    pub fn name(&self) -> &str {
        &self.data().name
    }

    /// The fully-qualified name, e.g. `"my.package.MyMessage"`.
    pub fn full_name(&self) -> &str {
        &self.data().full_name
    }

    /// The [`FileDescriptor`] that declares this message.
    pub fn parent_file(&self) -> FileDescriptor {
        FileDescriptor {
            pool: Arc::clone(&self.pool),
            index: self.data().file_index,
        }
    }

    /// `true` if this message is a synthetic `map<K, V>` entry type.
    pub fn is_map_entry(&self) -> bool {
        self.data().is_map_entry
    }

    /// Iterate over the message's fields in declaration order.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = FieldDescriptor> + '_ {
        let pool = Arc::clone(&self.pool);
        let message_index = self.index;
        (0..self.data().fields.len()).map(move |field_index| FieldDescriptor {
            pool: Arc::clone(&pool),
            message_index,
            field_index,
        })
    }

    /// Look up a field by its number.
    pub fn get_field(&self, number: u32) -> Option<FieldDescriptor> {
        self.data()
            .field_by_number
            .get(&number)
            .map(|&field_index| FieldDescriptor {
                pool: Arc::clone(&self.pool),
                message_index: self.index,
                field_index,
            })
    }

    /// Look up a field by its name.
    pub fn get_field_by_name(&self, name: &str) -> Option<FieldDescriptor> {
        self.data()
            .field_by_name
            .get(name)
            .map(|&field_index| FieldDescriptor {
                pool: Arc::clone(&self.pool),
                message_index: self.index,
                field_index,
            })
    }

    /// Look up a field by its JSON name.
    pub fn get_field_by_json_name(&self, json_name: &str) -> Option<FieldDescriptor> {
        self.data()
            .field_by_json_name
            .get(json_name)
            .map(|&field_index| FieldDescriptor {
                pool: Arc::clone(&self.pool),
                message_index: self.index,
                field_index,
            })
    }

    /// Iterate over the message's oneof declarations (including synthetic
    /// proto3-optional oneofs) in declaration order.
    pub fn oneofs(&self) -> impl ExactSizeIterator<Item = OneofDescriptor> + '_ {
        let pool = Arc::clone(&self.pool);
        let message_index = self.index;
        (0..self.data().oneofs.len()).map(move |oneof_index| OneofDescriptor {
            pool: Arc::clone(&pool),
            message_index,
            oneof_index,
        })
    }

    /// Iterate over nested message types declared inside this message.
    pub fn nested_messages(&self) -> impl ExactSizeIterator<Item = MessageDescriptor> + '_ {
        let pool = Arc::clone(&self.pool);
        self.data()
            .nested_messages
            .iter()
            .map(move |&index| MessageDescriptor {
                pool: Arc::clone(&pool),
                index,
            })
    }

    /// Iterate over nested enum types declared inside this message.
    pub fn nested_enums(&self) -> impl ExactSizeIterator<Item = EnumDescriptor> + '_ {
        let pool = Arc::clone(&self.pool);
        self.data()
            .nested_enums
            .iter()
            .map(move |&index| EnumDescriptor {
                pool: Arc::clone(&pool),
                index,
            })
    }
}

impl std::fmt::Debug for MessageDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageDescriptor")
            .field("full_name", &self.full_name())
            .finish()
    }
}

impl PartialEq for MessageDescriptor {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pool, &other.pool) && self.index == other.index
    }
}

// ---------------------------------------------------------------------------
// FieldDescriptor impl
// ---------------------------------------------------------------------------

impl FieldDescriptor {
    pub(crate) fn data(&self) -> &FieldData {
        &self.pool.messages[self.message_index].fields[self.field_index]
    }

    /// The simple field name.
    pub fn name(&self) -> &str {
        &self.data().name
    }

    /// The fully-qualified field name, e.g. `"my.package.MyMessage.my_field"`.
    pub fn full_name(&self) -> &str {
        &self.data().full_name
    }

    /// The JSON name of the field (camelCase by default).
    pub fn json_name(&self) -> &str {
        &self.data().json_name
    }

    /// The field number.
    pub fn number(&self) -> u32 {
        self.data().number
    }

    /// The resolved [`Kind`] of the field.
    pub fn kind(&self) -> Kind {
        self.data().kind
    }

    /// The [`Cardinality`] (label) of the field.
    pub fn cardinality(&self) -> Cardinality {
        self.data().cardinality
    }

    /// `true` if this is a `repeated` field.
    pub fn is_list(&self) -> bool {
        // A map field is repeated at the descriptor level, but is treated as a
        // map; callers should consult `is_map`.
        matches!(self.data().cardinality, Cardinality::Repeated) && !self.is_map()
    }

    /// `true` if this field is a `map<K, V>` (its element type is a synthetic
    /// map-entry message and the label is repeated).
    pub fn is_map(&self) -> bool {
        if !matches!(self.data().cardinality, Cardinality::Repeated) {
            return false;
        }
        match self.data().kind {
            Kind::Message(idx) => self.pool.messages[idx].is_map_entry,
            _ => false,
        }
    }

    /// `true` if this repeated scalar field is encoded packed.
    pub fn is_packed(&self) -> bool {
        self.data().packed
    }

    /// `true` if this field is a proto3 `optional`.
    pub fn is_proto3_optional(&self) -> bool {
        self.data().proto3_optional
    }

    /// The [`OneofDescriptor`] this field belongs to, if any.
    pub fn containing_oneof(&self) -> Option<OneofDescriptor> {
        self.data().oneof_index.map(|oneof_index| OneofDescriptor {
            pool: Arc::clone(&self.pool),
            message_index: self.message_index,
            oneof_index,
        })
    }

    /// The message that declares this field.
    pub fn parent_message(&self) -> MessageDescriptor {
        MessageDescriptor {
            pool: Arc::clone(&self.pool),
            index: self.message_index,
        }
    }

    /// If this field is of message kind, the referenced [`MessageDescriptor`].
    pub fn message_type(&self) -> Option<MessageDescriptor> {
        match self.data().kind {
            Kind::Message(index) | Kind::Group(index) => Some(MessageDescriptor {
                pool: Arc::clone(&self.pool),
                index,
            }),
            _ => None,
        }
    }

    /// If this field is of enum kind, the referenced [`EnumDescriptor`].
    pub fn enum_type(&self) -> Option<EnumDescriptor> {
        match self.data().kind {
            Kind::Enum(index) => Some(EnumDescriptor {
                pool: Arc::clone(&self.pool),
                index,
            }),
            _ => None,
        }
    }

    /// For a map field, the key field descriptor of the synthetic entry.
    pub(crate) fn map_entry_key_field(&self) -> Option<FieldDescriptor> {
        match self.data().kind {
            Kind::Message(idx) if self.pool.messages[idx].is_map_entry => Some(FieldDescriptor {
                pool: Arc::clone(&self.pool),
                message_index: idx,
                field_index: self.pool.messages[idx].field_by_number.get(&1).copied()?,
            }),
            _ => None,
        }
    }

    /// For a map field, the value field descriptor of the synthetic entry.
    pub(crate) fn map_entry_value_field(&self) -> Option<FieldDescriptor> {
        match self.data().kind {
            Kind::Message(idx) if self.pool.messages[idx].is_map_entry => Some(FieldDescriptor {
                pool: Arc::clone(&self.pool),
                message_index: idx,
                field_index: self.pool.messages[idx].field_by_number.get(&2).copied()?,
            }),
            _ => None,
        }
    }
}

impl std::fmt::Debug for FieldDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldDescriptor")
            .field("name", &self.name())
            .field("number", &self.number())
            .field("kind", &self.kind())
            .field("cardinality", &self.cardinality())
            .finish()
    }
}

impl PartialEq for FieldDescriptor {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pool, &other.pool)
            && self.message_index == other.message_index
            && self.field_index == other.field_index
    }
}

// ---------------------------------------------------------------------------
// OneofDescriptor impl
// ---------------------------------------------------------------------------

impl OneofDescriptor {
    pub(crate) fn data(&self) -> &OneofData {
        &self.pool.messages[self.message_index].oneofs[self.oneof_index]
    }

    /// The simple oneof name.
    pub fn name(&self) -> &str {
        &self.data().name
    }

    /// The fully-qualified oneof name.
    pub fn full_name(&self) -> &str {
        &self.data().full_name
    }

    /// `true` if this oneof is synthetic (generated for a proto3 `optional`).
    pub fn is_synthetic(&self) -> bool {
        self.data().is_synthetic
    }

    /// Iterate over the fields belonging to this oneof.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = FieldDescriptor> + '_ {
        let pool = Arc::clone(&self.pool);
        let message_index = self.message_index;
        self.data()
            .field_indices
            .iter()
            .map(move |&field_index| FieldDescriptor {
                pool: Arc::clone(&pool),
                message_index,
                field_index,
            })
    }
}

impl std::fmt::Debug for OneofDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OneofDescriptor")
            .field("full_name", &self.full_name())
            .finish()
    }
}

impl PartialEq for OneofDescriptor {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pool, &other.pool)
            && self.message_index == other.message_index
            && self.oneof_index == other.oneof_index
    }
}

// ---------------------------------------------------------------------------
// EnumDescriptor impl
// ---------------------------------------------------------------------------

impl EnumDescriptor {
    pub(crate) fn data(&self) -> &EnumData {
        &self.pool.enums[self.index]
    }

    /// The simple enum name.
    pub fn name(&self) -> &str {
        &self.data().name
    }

    /// The fully-qualified enum name.
    pub fn full_name(&self) -> &str {
        &self.data().full_name
    }

    /// The file that declares this enum.
    pub fn parent_file(&self) -> FileDescriptor {
        FileDescriptor {
            pool: Arc::clone(&self.pool),
            index: self.data().file_index,
        }
    }

    /// Iterate over the enum values in declaration order.
    pub fn values(&self) -> impl ExactSizeIterator<Item = EnumValueDescriptor> + '_ {
        let pool = Arc::clone(&self.pool);
        let enum_index = self.index;
        (0..self.data().values.len()).map(move |value_index| EnumValueDescriptor {
            pool: Arc::clone(&pool),
            enum_index,
            value_index,
        })
    }

    /// Look up an enum value by its number.
    pub fn get_value(&self, number: i32) -> Option<EnumValueDescriptor> {
        self.data()
            .value_by_number
            .get(&number)
            .map(|&value_index| EnumValueDescriptor {
                pool: Arc::clone(&self.pool),
                enum_index: self.index,
                value_index,
            })
    }

    /// Look up an enum value by its name.
    pub fn get_value_by_name(&self, name: &str) -> Option<EnumValueDescriptor> {
        self.data()
            .value_by_name
            .get(name)
            .map(|&value_index| EnumValueDescriptor {
                pool: Arc::clone(&self.pool),
                enum_index: self.index,
                value_index,
            })
    }

    /// The default value of this enum (the value with number 0, or the first
    /// declared value if 0 is absent — proto2 semantics).
    pub fn default_value(&self) -> Option<EnumValueDescriptor> {
        self.get_value(0).or_else(|| {
            if self.data().values.is_empty() {
                None
            } else {
                Some(EnumValueDescriptor {
                    pool: Arc::clone(&self.pool),
                    enum_index: self.index,
                    value_index: 0,
                })
            }
        })
    }
}

impl std::fmt::Debug for EnumDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnumDescriptor")
            .field("full_name", &self.full_name())
            .finish()
    }
}

impl PartialEq for EnumDescriptor {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pool, &other.pool) && self.index == other.index
    }
}

// ---------------------------------------------------------------------------
// EnumValueDescriptor impl
// ---------------------------------------------------------------------------

impl EnumValueDescriptor {
    pub(crate) fn data(&self) -> &EnumValueData {
        &self.pool.enums[self.enum_index].values[self.value_index]
    }

    /// The simple value name.
    pub fn name(&self) -> &str {
        &self.data().name
    }

    /// The fully-qualified value name.
    pub fn full_name(&self) -> &str {
        &self.data().full_name
    }

    /// The integer number of this value.
    pub fn number(&self) -> i32 {
        self.data().number
    }

    /// The enum that declares this value.
    pub fn parent_enum(&self) -> EnumDescriptor {
        EnumDescriptor {
            pool: Arc::clone(&self.pool),
            index: self.enum_index,
        }
    }
}

impl std::fmt::Debug for EnumValueDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnumValueDescriptor")
            .field("full_name", &self.full_name())
            .field("number", &self.number())
            .finish()
    }
}

impl PartialEq for EnumValueDescriptor {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pool, &other.pool)
            && self.enum_index == other.enum_index
            && self.value_index == other.value_index
    }
}

// ---------------------------------------------------------------------------
// ServiceDescriptor impl
// ---------------------------------------------------------------------------

impl ServiceDescriptor {
    pub(crate) fn data(&self) -> &ServiceData {
        &self.pool.services[self.index]
    }

    /// The simple service name.
    pub fn name(&self) -> &str {
        &self.data().name
    }

    /// The fully-qualified service name.
    pub fn full_name(&self) -> &str {
        &self.data().full_name
    }

    /// The file that declares this service.
    pub fn parent_file(&self) -> FileDescriptor {
        FileDescriptor {
            pool: Arc::clone(&self.pool),
            index: self.data().file_index,
        }
    }

    /// Iterate over the methods of this service in declaration order.
    pub fn methods(&self) -> impl ExactSizeIterator<Item = MethodDescriptor> + '_ {
        let pool = Arc::clone(&self.pool);
        let service_index = self.index;
        (0..self.data().methods.len()).map(move |method_index| MethodDescriptor {
            pool: Arc::clone(&pool),
            service_index,
            method_index,
        })
    }
}

impl std::fmt::Debug for ServiceDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceDescriptor")
            .field("full_name", &self.full_name())
            .finish()
    }
}

impl PartialEq for ServiceDescriptor {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pool, &other.pool) && self.index == other.index
    }
}

// ---------------------------------------------------------------------------
// MethodDescriptor impl
// ---------------------------------------------------------------------------

impl MethodDescriptor {
    pub(crate) fn data(&self) -> &MethodData {
        &self.pool.services[self.service_index].methods[self.method_index]
    }

    /// The simple method name.
    pub fn name(&self) -> &str {
        &self.data().name
    }

    /// The fully-qualified method name.
    pub fn full_name(&self) -> &str {
        &self.data().full_name
    }

    /// The input (request) message type.
    pub fn input(&self) -> MessageDescriptor {
        MessageDescriptor {
            pool: Arc::clone(&self.pool),
            index: self.data().input_index,
        }
    }

    /// The output (response) message type.
    pub fn output(&self) -> MessageDescriptor {
        MessageDescriptor {
            pool: Arc::clone(&self.pool),
            index: self.data().output_index,
        }
    }

    /// `true` if the client streams multiple request messages.
    pub fn is_client_streaming(&self) -> bool {
        self.data().client_streaming
    }

    /// `true` if the server streams multiple response messages.
    pub fn is_server_streaming(&self) -> bool {
        self.data().server_streaming
    }

    /// The service that declares this method.
    pub fn parent_service(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            pool: Arc::clone(&self.pool),
            index: self.service_index,
        }
    }
}

impl std::fmt::Debug for MethodDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MethodDescriptor")
            .field("full_name", &self.full_name())
            .field("client_streaming", &self.is_client_streaming())
            .field("server_streaming", &self.is_server_streaming())
            .finish()
    }
}

impl PartialEq for MethodDescriptor {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pool, &other.pool)
            && self.service_index == other.service_index
            && self.method_index == other.method_index
    }
}
