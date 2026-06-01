#![forbid(unsafe_code)]
//! Extension trait for `prost_types::Api`.
//!
//! Provides ergonomic construction and access methods for the well-known
//! `google.protobuf.Api` type (API interface descriptor).

use prost_types::{Api, Method};

/// Extension methods for [`prost_types::Api`].
pub trait ApiExt {
    /// Create an `Api` with the given fully-qualified name, no methods, and
    /// an empty version string.
    #[allow(clippy::new_ret_no_self)]
    fn new(name: impl Into<String>) -> Api;

    /// Return the fully-qualified name of this API interface.
    fn name(&self) -> &str;

    /// Return the methods defined in this API.
    fn methods(&self) -> &[Method];

    /// Return the version string of this API (may be empty).
    fn version(&self) -> &str;

    /// Consume `self` and return a new `Api` with `method` appended to its
    /// method list.  Useful for fluent builder-style construction.
    fn with_method(self, method: Method) -> Api;
}

impl ApiExt for Api {
    fn new(name: impl Into<String>) -> Api {
        Api {
            name: name.into(),
            methods: Vec::new(),
            options: Vec::new(),
            version: String::new(),
            source_context: None,
            mixins: Vec::new(),
            syntax: 0,
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn methods(&self) -> &[Method] {
        &self.methods
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn with_method(mut self, method: Method) -> Api {
        self.methods.push(method);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_accessors() {
        let api = Api::new("google.pubsub.v1.Subscriber");
        assert_eq!(api.name(), "google.pubsub.v1.Subscriber");
        assert!(api.methods().is_empty());
        assert_eq!(api.version(), "");
    }

    #[test]
    fn with_method_appends() {
        let method = Method {
            name: "Pull".to_string(),
            request_type_url: String::new(),
            request_streaming: false,
            response_type_url: String::new(),
            response_streaming: false,
            options: Vec::new(),
            syntax: 0,
        };
        let api = Api::new("MyService").with_method(method);
        assert_eq!(api.methods().len(), 1);
        assert_eq!(api.methods()[0].name, "Pull");
    }
}
