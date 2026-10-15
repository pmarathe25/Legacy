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
