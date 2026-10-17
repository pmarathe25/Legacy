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
