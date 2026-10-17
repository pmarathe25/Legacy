use game_core::*;
use game_facts::*;
use std::{collections::BTreeMap, ops::Bound, sync::Arc};
fn stamp(n: u64) -> Stamp {
    Stamp {
        source: PersonId(n),
        observed: SimTime(n * 10),
        revision: n,
        confidence: 100,
    }
}
fn fact(value: &str, n: u64) -> Fact<String> {
    Fact {
        value: Arc::new(value.into()),
        source: PersonId(n),
        observed: SimTime(n * 10),
        revision: n,
        confidence: 100,
    }
}
fn scene(keys: &[&str], old: Option<&Scene<String>>) -> Arc<Scene<String>> {
    Arc::new(
        Scene::new(
            keys.iter()
                .map(|k| (Arc::<str>::from(*k), Arc::new(k.to_string())))
                .collect(),
            old,
        )
        .unwrap(),
    )
}
#[test]
fn shared_scenes_keep_individual_evidence_without_per_fact_payload_copies() {
    let s = scene(&["patch:1", "roads:1"], None);
    let mut people: Vec<Facts<String>> = (0..1000).map(|_| Facts::default()).collect();
    for (i, f) in people.iter_mut().enumerate() {
        f.observe(s.clone(), stamp(i as u64));
    }
    let old = people[3].clone();
    let value = people[3].get("roads:1").unwrap().value.clone();
    let count = Arc::strong_count(&value);
    for (i, f) in people.iter_mut().enumerate() {
        f.observe(s.clone(), stamp(i as u64 + 1000));
    }
    assert_eq!(Arc::strong_count(&value), count);
    assert_eq!(old.get("roads:1").unwrap().source, PersonId(3));
    assert_eq!(people[3].get("roads:1").unwrap().source, PersonId(1003));
    assert!(Arc::ptr_eq(
        old.get("roads:1").unwrap().value,
        people[3].get("roads:1").unwrap().value
    ));
}
#[test]
fn payload_deltas_ignore_metadata_and_include_overrides_and_disappeared_memories() {
    fn delta(new: &Facts<String>, old: &Facts<String>) -> BTreeMap<String, Option<String>> {
        let mut changes = BTreeMap::new();
        new.changed_values(old, |key, value| {
            changes.insert(key.to_string(), value.map(|v| v.as_ref().clone()));
        });
        changes
    }
    let s = scene(&["patch:1", "roads:1"], None);
    let mut facts = Facts::default();
    facts.observe(s.clone(), stamp(1));
    let old = facts.clone();
    facts.observe(s.clone(), stamp(2));
    assert!(delta(&facts, &old).is_empty());
    facts.insert("roads:1".into(), fact("reported map", 3));
    assert_eq!(
        delta(&facts, &old),
        BTreeMap::from([("roads:1".into(), Some("reported map".into()))])
    );
    let reported = facts.clone();
    facts.observe(s, stamp(4));
    assert_eq!(
        delta(&facts, &reported),
        BTreeMap::from([("roads:1".into(), Some("roads:1".into()))])
    );
    assert_eq!(
        delta(&Facts::default(), &facts),
        BTreeMap::from([("patch:1".into(), None), ("roads:1".into(), None)])
    );
}
#[test]
fn moves_membership_changes_overrides_and_wire_form_preserve_old_evidence() {
    let a = scene(&["patch:1", "roads:1"], None);
    let mut facts = Facts::default();
    facts.observe(a.clone(), stamp(1));
    let old = facts.clone();
    facts.insert("roads:1".into(), fact("reported map", 2));
    assert_eq!(facts.len(), 2);
    assert_eq!(facts.get("roads:1").unwrap().source, PersonId(2));
    let b = scene(&["roads:1", "patch:1"], Some(&a));
    facts.observe(b, stamp(3));
    assert_eq!(facts.get("roads:1").unwrap().source, PersonId(3));
    assert_eq!(facts.get("roads:1").unwrap().value.as_str(), "roads:1");
    facts.observe(scene(&["patch:2", "roads:2"], None), stamp(4));
    assert_eq!(facts.len(), 4);
    assert_eq!(facts.get("patch:1").unwrap().observed, SimTime(30));
    assert_eq!(old.get("patch:1").unwrap().observed, SimTime(10));
    facts.insert("patch:1".into(), fact("explicit", 5));
    facts.observe(scene(&["roads:2"], None), stamp(6));
    assert_eq!(facts.get("patch:2").unwrap().observed, SimTime(40));
    let map: BTreeMap<_, _> = facts
        .iter()
        .map(|(k, v)| (k.clone(), v.to_owned()))
        .collect();
    assert_eq!(
        serde_json::to_string(&facts).unwrap(),
        serde_json::to_string(&map).unwrap()
    );
    for lower in [
        Bound::Unbounded,
        Bound::Included("patch:1"),
        Bound::Excluded("patch:1"),
        Bound::Included("zzz"),
    ] {
        for upper in [
            Bound::Unbounded,
            Bound::Included("roads:2"),
            Bound::Excluded("roads:2"),
        ] {
            let actual: Vec<_> = facts
                .range((lower, upper))
                .map(|(k, _)| k.as_ref())
                .collect();
            let expected: Vec<_> = map
                .keys()
                .filter(|k| match lower {
                    Bound::Unbounded => true,
                    Bound::Included(v) => k.as_ref() >= v,
                    Bound::Excluded(v) => k.as_ref() > v,
                })
                .filter(|k| match upper {
                    Bound::Unbounded => true,
                    Bound::Included(v) => k.as_ref() <= v,
                    Bound::Excluded(v) => k.as_ref() < v,
                })
                .map(|k| k.as_ref())
                .collect();
            assert_eq!(actual, expected);
        }
    }
    assert!(
        Scene::<String>::new(
            vec![
                ("x".into(), Arc::new("a".into())),
                ("x".into(), Arc::new("b".into()))
            ],
            None
        )
        .is_err()
    );
}
