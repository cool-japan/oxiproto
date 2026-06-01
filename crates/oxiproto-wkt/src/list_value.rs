#![forbid(unsafe_code)]
//! Extension trait for `prost_types::ListValue`.
//!
//! Provides ergonomic construction and access methods for the well-known
//! `google.protobuf.ListValue` type.

use prost_types::{ListValue, Value};

/// Extension methods for [`prost_types::ListValue`].
pub trait ListValueExt {
    /// Create a `ListValue` from a `Vec<Value>`.
    fn from_vec(values: Vec<Value>) -> ListValue;

    /// Iterate over the values in this list.
    fn iter(&self) -> impl Iterator<Item = &Value> + '_;

    /// Return the number of values in this list.
    fn len(&self) -> usize;

    /// Return `true` if this list contains no values.
    fn is_empty(&self) -> bool;

    /// Return a reference to the value at `index`, or `None` if out of bounds.
    fn get(&self, index: usize) -> Option<&Value>;
}

impl ListValueExt for ListValue {
    fn from_vec(values: Vec<Value>) -> ListValue {
        ListValue { values }
    }

    fn iter(&self) -> impl Iterator<Item = &Value> + '_ {
        self.values.iter()
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    fn get(&self, index: usize) -> Option<&Value> {
        self.values.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wrappers::ValueExt;

    #[test]
    fn from_vec_and_len() {
        let vals = vec![Value::from_bool(true), Value::from_f64(42.0)];
        let lv = ListValue::from_vec(vals);
        assert_eq!(lv.len(), 2);
        assert!(!lv.is_empty());
    }

    #[test]
    fn empty_list() {
        let lv = ListValue::from_vec(vec![]);
        assert_eq!(lv.len(), 0);
        assert!(lv.is_empty());
    }

    #[test]
    fn iter_and_get() {
        let vals = vec![
            Value::from_f64(1.0),
            Value::from_f64(2.0),
            Value::from_f64(3.0),
        ];
        let lv = ListValue::from_vec(vals);
        let numbers: Vec<f64> = lv.iter().filter_map(|v| v.as_number()).collect();
        assert_eq!(numbers, vec![1.0, 2.0, 3.0]);

        assert_eq!(lv.get(0).and_then(|v| v.as_number()), Some(1.0));
        assert_eq!(lv.get(2).and_then(|v| v.as_number()), Some(3.0));
        assert!(lv.get(10).is_none());
    }
}
