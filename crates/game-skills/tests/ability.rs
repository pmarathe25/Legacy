use game_core::*;
use game_sim::*;
#[test]
fn learning_characteristics_change_practice_gain_without_inheriting_experience() {
    let mut b = Builder::new();
    game_traits::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    let mut h = b.build(1, 1).unwrap();
    let mut traits = game_traits::Traits::default();
    traits.definitions.insert(
        "learning".into(),
        game_traits::Trait {
            name: "Learning".into(),
            minimum: 0,
            maximum: 16000,
            default: 1000,
            founder_spread: 65535,
            weights: Default::default(),
        },
    );
    h.dispatch(game_traits::Install(traits)).unwrap();
    for (person, learning, expected) in [
        (PersonId(1), 0, 0),
        (PersonId(2), 1000, 100),
        (PersonId(3), 12000, 1200),
    ] {
        h.dispatch(game_traits::Set {
            person,
            profile: [("learning".into(), learning)].into(),
        })
        .unwrap();
        assert_eq!(
            h.dispatch(game_skills::Practice {
                person,
                skill: "making".into(),
                effort: 100,
                attribution: person.0
            })
            .unwrap(),
            expected
        );
    }
}
