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

#[test]
fn key_versions_ignore_payloads_and_scene_refreshes_but_detect_membership_changes() {
    let mut facts = Facts::default();
    let first = scene(&["patch:1", "roads:1"], None);
    facts.observe(first.clone(), stamp(1));
    let original = facts.clone();
    facts.insert("roads:1".into(), fact("closed", 2));
    assert!(facts.same_keys(&original));
    facts.observe(first.clone(), stamp(3));
    assert!(facts.same_keys(&original));
    // Leaving a scene retains its evidence in cold storage.
    facts.observe(scene(&["patch:1"], Some(&first)), stamp(4));
    assert!(facts.same_keys(&original));
    facts.observe(scene(&["patch:2"], None), stamp(5));
    assert!(!facts.same_keys(&original));
    let expanded = facts.clone();
    facts.remove("missing");
    assert!(facts.same_keys(&expanded));
    // Removing a visible key materializes the other scene facts correctly.
    let removed = facts.remove("patch:2").unwrap();
    assert!(!facts.same_keys(&expanded));
    assert_eq!(facts.len(), 2);
    facts.insert("patch:2".into(), removed);
    assert!(!facts.same_keys(&expanded));
    assert_eq!(facts, expanded);
    let mut fork = expanded.clone();
    fork.remove("roads:1");
    fork.insert("roads:2".into(), fact("open", 6));
    assert_eq!(fork.len(), expanded.len());
    assert!(!fork.same_keys(&expanded));
    assert_eq!(original.len(), 2);
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
fn changed_scenes_invalidate_only_changed_payload_namespaces_and_keep_cold_evidence() {
    let first = scene(&["roads:1", "patch:1"], None);
    let mut facts = Facts::default();
    facts.observe(first.clone(), stamp(1));
    let original = facts.clone();
    let roads = facts.namespace_version("roads");
    let patch = facts.namespace_version("patch");
    let next = Arc::new(
        Scene::new(
            vec![
                ("roads:1".into(), Arc::new("roads:1".into())),
                ("patch:1".into(), Arc::new("depleted".into())),
            ],
            Some(&first),
        )
        .unwrap(),
    );
    facts.observe(next.clone(), stamp(2));
    assert!(roads.ptr_eq(&facts.namespace_version("roads")));
    assert!(!patch.ptr_eq(&facts.namespace_version("patch")));
    let changed = facts.clone();
    // A new scene allocation with identical shared payloads changes only metadata.
    facts.observe(scene(&["roads:1"], Some(&next)), stamp(3));
    assert!(facts.same_namespace(&changed, "roads"));
    assert!(facts.same_namespace(&changed, "patch"));
    assert_eq!(facts.get("patch:1").unwrap().value.as_str(), "depleted");
    assert_eq!(facts.get("patch:1").unwrap().observed, SimTime(20));
    facts.insert("roads:1".into(), fact("closed", 4));
    let reported = facts.clone();
    facts.observe(first, stamp(5));
    assert!(!facts.same_namespace(&reported, "roads"));
    assert_eq!(original.get("roads:1").unwrap().observed, SimTime(10));
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

#[test]
fn payload_deltas_match_a_full_scan_across_metadata_refreshes_and_branching() {
    let mut facts = Facts::default();
    let mut snapshots = vec![];
    let scenes = [
        scene(&["patch:1", "roads:1"], None),
        scene(&["patch:2", "roads:2"], None),
    ];
    for i in 0..200 {
        let previous = facts.clone();
        match i % 5 {
            0 | 1 => facts.observe(scenes[(i / 5) % 2].clone(), stamp(i as u64)),
            2 => {
                facts.insert("store:1".into(), fact("stock", i as u64));
            }
            3 => {
                let mut refreshed = facts
                    .get("store:1")
                    .map(|f| f.to_owned())
                    .unwrap_or_else(|| fact("stock", i as u64));
                refreshed.revision += 1;
                refreshed.confidence = 80;
                facts.insert("store:1".into(), refreshed);
            }
            _ => {
                facts.insert("roads:1".into(), fact("report", i as u64));
            }
        }
        snapshots.push(previous);
        if i % 13 == 0 {
            facts = snapshots[i / 2].clone();
        }
        for old in &snapshots {
            let mut actual = BTreeMap::new();
            facts.changed_values(old, |k, v| {
                actual.insert(k.clone(), v.cloned());
            });
            let keys: std::collections::BTreeSet<_> =
                facts.keys().chain(old.keys()).cloned().collect();
            let expected: BTreeMap<_, _> = keys
                .into_iter()
                .filter_map(|k| {
                    let new = facts.get(&k).map(|f| f.value);
                    let prior = old.get(&k).map(|f| f.value);
                    let same = match (new, prior) {
                        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                        (None, None) => true,
                        _ => false,
                    };
                    (!same).then(|| (k, new.cloned()))
                })
                .collect();
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn namespace_versions_track_forks_scenes_and_payloads_without_tracking_metadata() {
    let mut original = Facts::default();
    original.insert("store:1".into(), fact("ten apples", 1));
    let mut left = original.clone();
    let mut right = original.clone();
    left.insert("store:1".into(), fact("twenty apples", 2));
    right.insert("store:1".into(), fact("thirty apples", 2));
    assert!(!left.same_namespace(&right, "store"));
    assert!(!left.same_namespace(&original, "store"));
    let mut metadata = original.clone();
    let value = original.get("store:1").unwrap().value.clone();
    metadata.insert(
        "store:1".into(),
        Fact {
            value,
            ..fact("unused", 10)
        },
    );
    assert!(metadata.same_namespace(&original, "store"));
    let mut unrelated = original.clone();
    unrelated.insert("theory:falling".into(), fact("falls down", 3));
    assert!(unrelated.same_namespace(&original, "store"));
    let visible = scene(&["store:2"], None);
    let mut observed = original.clone();
    observed.observe(visible.clone(), stamp(4));
    assert!(!observed.same_namespace(&original, "store"));
    let before = observed.clone();
    observed.observe(visible, stamp(5));
    assert!(observed.same_namespace(&before, "store"));
}
