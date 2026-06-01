/// Configuration for Protobuf-JSON serialization and deserialization.
///
/// This builder follows the canonical Protobuf-JSON spec with sensible
/// defaults. Create one via [`JsonCodec::default()`] and customise with the
/// builder methods.
#[derive(Debug, Clone, Default)]
pub struct JsonCodec {
    /// If `true`, use the original proto field names instead of camelCase.
    preserve_proto_field_names: bool,
    /// If `true`, always include fields even when they hold their default value.
    always_print_fields: bool,
    /// If `true`, emit enum values as their numeric representation.
    emit_enum_as_number: bool,
}

impl JsonCodec {
    /// Create a new codec with default settings.
    ///
    /// Defaults: camelCase field names, default values omitted, enum names as
    /// strings.
    pub fn new() -> Self {
        Self::default()
    }

    /// When `true`, the original proto field names (snake_case) are used for
    /// JSON output instead of the camelCase JSON names.
    pub fn preserve_proto_field_names(mut self, yes: bool) -> Self {
        self.preserve_proto_field_names = yes;
        self
    }

    /// When `true`, all fields are included in JSON output even when they hold
    /// their proto3 default values (0, "", false, empty list/map).
    pub fn always_print_fields(mut self, yes: bool) -> Self {
        self.always_print_fields = yes;
        self
    }

    /// When `true`, enum values are serialized as their integer number rather
    /// than their string name.
    pub fn emit_enum_as_number(mut self, yes: bool) -> Self {
        self.emit_enum_as_number = yes;
        self
    }

    /// Returns `true` if proto field names should be preserved.
    pub(crate) fn uses_proto_names(&self) -> bool {
        self.preserve_proto_field_names
    }

    /// Returns `true` if all fields should be included regardless of default.
    pub(crate) fn always_print(&self) -> bool {
        self.always_print_fields
    }

    /// Returns `true` if enums should be emitted as numbers.
    pub(crate) fn enum_as_number(&self) -> bool {
        self.emit_enum_as_number
    }
}
