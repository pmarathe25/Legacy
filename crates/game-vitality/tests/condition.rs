use game_core::*;
use game_sim::*;
use game_vitality::*;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_traits::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b
}
#[test]
fn resilience_varies_stress_and_experience_recovers_without_inheriting_current_mood() {
    let mut h = builder().build(2, 1).unwrap();
    let mut traits = game_traits::Traits::default();
    traits.definitions.insert(
        "resilience".into(),
        game_traits::Trait {
            name: "Resilience".into(),
            minimum: 0,
            maximum: 16000,
            default: 1000,
            founder_spread: 65535,
            weights: Default::default(),
        },
    );
    h.dispatch(game_traits::Install(traits)).unwrap();
    for (p, resilience) in [(PersonId(1), 0), (PersonId(2), 5000)] {
        h.dispatch(game_traits::Set {
            person: p,
            profile: [("resilience".into(), resilience)].into(),
        })
        .unwrap();
        h.dispatch(Register {
            person: p,
            born: SimTime(0),
            span: Some(SimDuration(80 * 365 * DAY.0)),
            base_health: 100,
        })
        .unwrap();
        h.dispatch(Experience {
            person: p,
            stress: 100,
            satisfaction: -10,
        })
        .unwrap();
    }
    let v = h.get::<Vitality>().unwrap();
    assert!(v.people[&PersonId(1)].stress > v.people[&PersonId(2)].stress);
    assert_eq!(v.people[&PersonId(1)].stress, 200);
    h.dispatch(Experience {
        person: PersonId(1),
        stress: -20,
        satisfaction: 10,
    })
    .unwrap();
    assert_eq!(
        h.get::<Vitality>().unwrap().people[&PersonId(1)].stress,
        180
    );
    h.advance(SimTime(10 * DAY.0), 1).unwrap();
    assert_eq!(
        h.get::<Vitality>().unwrap().people[&PersonId(1)]
            .condition(h.now())
            .0,
        150
    );
    let saved = h.save().unwrap();
    assert_eq!(builder().load(&saved, 2).unwrap().save().unwrap(), saved);
}
