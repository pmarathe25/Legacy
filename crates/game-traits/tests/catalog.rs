use game_core::*;
use game_sim::*;
use game_traits::*;
use std::collections::BTreeMap;
#[test]
fn arbitrary_trait_names_change_existing_utility_terms_and_validate_profiles() {
    let mut data = Traits::default();
    data.definitions.insert(
        "curiosity".into(),
        Trait {
            name: "Curiosity".into(),
            minimum: 0,
            maximum: 100,
            default: 0,
            founder_spread: 65535,
            weights: BTreeMap::from([("goal".into(), 100)]),
        },
    );
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    let mut h = b.build(1, 1).unwrap();
    h.dispatch(Install(data)).unwrap();
    h.dispatch(Set {
        person: PersonId(1),
        profile: BTreeMap::from([("curiosity".into(), 50)]),
    })
    .unwrap();
    assert_eq!(h.get::<Traits>().unwrap().weights(PersonId(1))["goal"], 150);
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Set {
            person: PersonId(1),
            profile: BTreeMap::from([("curiosity".into(), 101)])
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
}

#[test]
fn sparse_profiles_preserve_lexical_clamping_and_default_traits_have_no_adjustment() {
    use std::collections::BTreeMap;
    let mut traits = Traits::default();
    for id in 0..1000 {
        traits.definitions.insert(
            format!("middle-{id:04}"),
            Trait {
                name: format!("Trait {id}"),
                minimum: -100,
                maximum: 100,
                default: 0,
                founder_spread: 65535,
                weights: BTreeMap::from([("need".into(), 10_000)]),
            },
        );
    }
    for key in ["aa", "zz"] {
        traits.definitions.insert(
            key.into(),
            Trait {
                name: key.into(),
                minimum: -100,
                maximum: 100,
                default: 0,
                founder_spread: 65535,
                weights: BTreeMap::from([("need".into(), 10_000)]),
            },
        );
    }
    assert_eq!(traits.weights(PersonId(1))["need"], 100);
    traits.people.insert(
        PersonId(1),
        BTreeMap::from([("aa".into(), 100), ("zz".into(), -100)]),
    );
    traits.validate().unwrap();
    assert_eq!(traits.weights(PersonId(1))["need"], 0);
    assert_eq!(traits.weights(PersonId(1))["risk"], 100);
}

#[test]
fn changing_profiles_recompiles_weights_and_loading_reconstructs_them() {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    let mut host = b.build(1, 1).unwrap();
    let mut data = Traits::default();
    data.definitions.insert(
        "resolve".into(),
        Trait {
            name: "Resolve".into(),
            minimum: -100,
            maximum: 100,
            default: 0,
            founder_spread: 65535,
            weights: std::collections::BTreeMap::from([("need".into(), 100)]),
        },
    );
    host.dispatch(Install(data)).unwrap();
    for value in [-100, 0, 100] {
        host.dispatch(Set {
            person: PersonId(1),
            profile: std::collections::BTreeMap::from([("resolve".into(), value)]),
        })
        .unwrap();
        assert_eq!(
            host.get::<Traits>().unwrap().weights(PersonId(1))["need"],
            i64::from(100 + value)
        );
        let saved = host.save().unwrap();
        let mut b = Builder::new();
        plugin(&mut b).unwrap();
        let loaded = b.load(&saved, 4).unwrap();
        assert_eq!(loaded.save().unwrap(), saved);
        assert_eq!(
            loaded.get::<Traits>().unwrap().weights(PersonId(1)),
            host.get::<Traits>().unwrap().weights(PersonId(1))
        );
    }
}

#[test]
fn numeric_characteristics_follow_profiles_defaults_loading_and_direct_revisions() {
    let mut data = Traits::default();
    for (key, minimum, maximum, default) in [
        ("intelligence", 0, 16000, 1000),
        ("longevity", 250, 3000, 1000),
        ("temperament", -100, 100, 0),
    ] {
        data.definitions.insert(
            key.into(),
            Trait {
                name: key.into(),
                minimum,
                maximum,
                default,
                founder_spread: 65535,
                weights: BTreeMap::new(),
            },
        );
    }
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    let mut h = b.build(1, 1).unwrap();
    h.dispatch(Install(data)).unwrap();
    for value in [0, 1000, 8000, 16000] {
        h.dispatch(Set {
            person: PersonId(1),
            profile: [("intelligence".into(), value)].into(),
        })
        .unwrap();
        for p in [PersonId(1), PersonId(2)] {
            let traits = h.get::<Traits>().unwrap();
            for (key, definition) in &traits.definitions {
                let raw = traits
                    .people
                    .get(&p)
                    .and_then(|r| r.get(key))
                    .copied()
                    .unwrap_or(definition.default);
                assert_eq!(traits.ability(p, key), raw.max(0) as u64);
                assert_eq!(
                    traits.disposition(p, key),
                    (i64::from(raw) - i64::from(definition.minimum)) * 1000
                        / (i64::from(definition.maximum) - i64::from(definition.minimum))
                );
            }
            assert_eq!(traits.ability(p, "unknown"), 1000);
            assert_eq!(traits.disposition(p, "unknown"), 500);
        }
    }
    let save = h.save().unwrap();
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    let loaded = b.load(&save, 4).unwrap();
    assert_eq!(loaded.save().unwrap(), save);
    let mut direct = loaded.get::<Traits>().unwrap().clone();
    direct
        .people
        .get_mut(&PersonId(1))
        .unwrap()
        .insert("intelligence".into(), 4000);
    assert_eq!(direct.ability(PersonId(1), "intelligence"), 4000);
    direct.definitions.get_mut("intelligence").unwrap().maximum = 8000;
    assert_eq!(direct.disposition(PersonId(1), "intelligence"), 500);
    direct.people.remove(&PersonId(1));
    assert_eq!(direct.ability(PersonId(1), "intelligence"), 1000);
}

#[test]
fn recent_numeric_views_follow_shared_snapshots_world_switches_and_sparse_edits() {
    let make = |value| {
        let mut b = Builder::new();
        plugin(&mut b).unwrap();
        let mut h = b.build(1, 1).unwrap();
        let mut data = Traits::default();
        data.definitions.insert(
            "intelligence".into(),
            Trait {
                name: "Intelligence".into(),
                minimum: 0,
                maximum: 16000,
                default: value,
                founder_spread: 65535,
                weights: BTreeMap::new(),
            },
        );
        h.dispatch(Install(data)).unwrap();
        for p in 0..24 {
            h.dispatch(Set {
                person: PersonId(p),
                profile: [("intelligence".into(), value + p as i32)].into(),
            })
            .unwrap();
        }
        h
    };
    let (mut a, b) = (make(1000), make(4000));
    let retained = a.get::<Traits>().unwrap().clone();
    for value in [8000, 2000, 16000] {
        a.dispatch(Set {
            person: PersonId(1),
            profile: [("intelligence".into(), value)].into(),
        })
        .unwrap();
        for _ in 0..3 {
            for p in 0..24 {
                for (t, expected) in [
                    (
                        a.get::<Traits>().unwrap(),
                        if p == 1 { value as u64 } else { 1000 + p },
                    ),
                    (b.get::<Traits>().unwrap(), 4000 + p),
                    (&retained, 1000 + p),
                ] {
                    assert_eq!(t.ability(PersonId(p), "intelligence"), expected);
                    assert_eq!(
                        t.disposition(PersonId(p), "intelligence"),
                        expected as i64 * 1000 / 16000
                    );
                }
            }
        }
        let mut edited = a.get::<Traits>().unwrap().clone();
        edited
            .people
            .get_mut(&PersonId(1))
            .unwrap()
            .insert("intelligence".into(), 1234);
        assert_eq!(edited.ability(PersonId(1), "intelligence"), 1234);
        edited.people.remove(&PersonId(1));
        assert_eq!(edited.ability(PersonId(1), "intelligence"), 1000);
        assert_eq!(
            a.get::<Traits>()
                .unwrap()
                .ability(PersonId(1), "intelligence"),
            value as u64
        );
    }
}
