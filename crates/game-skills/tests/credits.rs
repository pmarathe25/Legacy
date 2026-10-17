use game_skills::Credits;
use std::collections::BTreeMap;

#[test]
fn dense_and_sparse_credit_pages_keep_legacy_wire_order_and_shared_snapshots() {
    let mut credits = Credits::default();
    let mut reference = BTreeMap::new();
    for id in (0..9000).step_by(3).chain([u64::MAX, u64::MAX - 4096]) {
        assert_eq!(credits.insert(id, ()), reference.insert(id, ()));
    }
    let retained = credits.clone();
    let prior_wire = serde_json::to_string(&reference).unwrap();
    assert_eq!(serde_json::to_string(&credits).unwrap(), prior_wire);
    for id in [0, 3, 4095, 4096, 8191, u64::MAX - 4095, u64::MAX] {
        assert_eq!(credits.insert(id, ()), reference.insert(id, ()));
    }
    assert_eq!(serde_json::to_string(&retained).unwrap(), prior_wire);
    assert_eq!(
        credits.keys().collect::<Vec<_>>(),
        reference.keys().copied().collect::<Vec<_>>()
    );
    let wire = serde_json::to_string(&credits).unwrap();
    assert_eq!(wire, serde_json::to_string(&reference).unwrap());
    let loaded: Credits = serde_json::from_str(&wire).unwrap();
    assert_eq!(loaded, credits);
    assert_eq!(loaded.len(), reference.len());
    for key in reference.keys() {
        assert!(loaded.contains_key(key));
    }
    assert!(!loaded.contains_key(&u64::MAX.saturating_sub(1)));
}

#[test]
fn duplicate_or_nonunit_legacy_credits_are_rejected() {
    assert!(serde_json::from_str::<Credits>(r#"{"1":null,"1":null}"#).is_err());
    assert!(serde_json::from_str::<Credits>(r#"{"1":true}"#).is_err());
    assert!(Credits::default().is_empty());
}
