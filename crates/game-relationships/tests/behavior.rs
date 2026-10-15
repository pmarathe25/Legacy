use game_core::*;
use game_relationships::*;
use game_sim::*;
#[test]
fn beliefs_produce_different_directed_trust_updates() {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    let mut h = b.build(1, 1).unwrap();
    for (person, sharing) in [(PersonId(1), 10), (PersonId(2), -10)] {
        h.dispatch(DefineBeliefs {
            person,
            beliefs: Beliefs {
                sharing,
                ..Default::default()
            },
        })
        .unwrap();
        h.dispatch(Interpret {
            observer: person,
            actor: PersonId(3),
            shared: true,
            harmed: false,
            credible: true,
        })
        .unwrap();
    }
    let s = h.get::<Social>().unwrap();
    assert_eq!(s.relationships[&PersonId(1)][&PersonId(3)].trust, 10);
    assert_eq!(s.relationships[&PersonId(2)][&PersonId(3)].trust, -10);
    assert!(!s.relationships.contains_key(&PersonId(3)));
}
