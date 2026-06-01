use oxiproto_wkt::{ListValue, ListValueExt, Value, ValueExt};

#[test]
fn from_vec_round_trip() {
    let vals = vec![
        Value::from_bool(true),
        Value::from_f64(42.0),
        Value::from_string("hello"),
        Value::null(),
    ];
    let lv = ListValue::from_vec(vals.clone());
    assert_eq!(lv.len(), 4);
    assert!(!lv.is_empty());

    // Verify round-trip by iterating
    let collected: Vec<&Value> = lv.iter().collect();
    assert_eq!(collected.len(), 4);
    assert_eq!(collected[0].as_bool(), Some(true));
    assert_eq!(collected[1].as_number(), Some(42.0));
    assert_eq!(collected[2].as_string(), Some("hello"));
    assert!(collected[3].is_null());
}

#[test]
fn empty_list_value() {
    let lv = ListValue::from_vec(vec![]);
    assert_eq!(lv.len(), 0);
    assert!(lv.is_empty());
    assert_eq!(lv.iter().count(), 0);
    assert!(lv.get(0).is_none());
}

#[test]
fn get_by_index() {
    let lv = ListValue::from_vec(vec![
        Value::from_f64(10.0),
        Value::from_f64(20.0),
        Value::from_f64(30.0),
    ]);
    assert_eq!(lv.get(0).and_then(|v| v.as_number()), Some(10.0));
    assert_eq!(lv.get(1).and_then(|v| v.as_number()), Some(20.0));
    assert_eq!(lv.get(2).and_then(|v| v.as_number()), Some(30.0));
    assert!(lv.get(3).is_none());
    assert!(lv.get(usize::MAX).is_none());
}

#[test]
fn typed_accessors_via_value_ext() {
    let lv = ListValue::from_vec(vec![
        Value::from_bool(false),
        Value::from_f64(-1.5),
        Value::from_string("world"),
    ]);
    let values: Vec<Option<f64>> = lv.iter().map(|v| v.as_number()).collect();
    assert_eq!(values, vec![None, Some(-1.5), None]);

    let strings: Vec<Option<&str>> = lv.iter().map(|v| v.as_string()).collect();
    assert_eq!(strings, vec![None, None, Some("world")]);
}

#[test]
fn iter_is_lazy() {
    let lv = ListValue::from_vec(vec![
        Value::from_f64(1.0),
        Value::from_f64(2.0),
        Value::from_f64(3.0),
    ]);
    // Verify iterator adapters work correctly
    let sum: f64 = lv.iter().filter_map(|v| v.as_number()).sum();
    assert!((sum - 6.0).abs() < f64::EPSILON);
}
