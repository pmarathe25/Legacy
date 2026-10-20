use game_core::*;
use game_identity::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
struct Cost(PersonId);
impl Request for Cost {
    const KEY: &'static str = "test.identity-cost";
    type Response = i64;
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b.request::<Cost>(|tx, r| Ok(resistance(tx, r.0, "politics", "change")))
        .unwrap();
    b
}
#[test]
fn acquired_commitments_and_inherited_rigidity_vary_switching_cost_without_a_veto() {
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    h.advance(SimTime(30 * 365 * DAY.0), 1).unwrap();
    for (p, rigidity) in [(PersonId(1), 0), (PersonId(2), 1000)] {
        h.dispatch(game_traits::Set {
            person: p,
            profile: [("rigidity".into(), rigidity)].into(),
        })
        .unwrap();
        h.dispatch(game_vitality::Register {
            person: p,
            born: SimTime(0),
            span: Some(SimDuration(80 * 365 * DAY.0)),
            base_health: 100,
        })
        .unwrap();
        assert_eq!(h.dispatch(Cost(p)).unwrap(), 0);
        for _ in 0..12 {
            h.dispatch(Commit {
                person: p,
                domain: "politics".into(),
                preference: "established".into(),
            })
            .unwrap();
        }
    }
    assert_eq!(h.dispatch(Cost(PersonId(1))).unwrap(), 0);
    let resistance = h.dispatch(Cost(PersonId(2))).unwrap();
    assert!(resistance > 0 && resistance < 100);
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for world in [&mut h, &mut loaded] {
        world
            .dispatch(game_vitality::Experience {
                person: PersonId(2),
                stress: 1000,
                satisfaction: 0,
            })
            .unwrap();
        assert!(world.dispatch(Cost(PersonId(2))).unwrap() < resistance);
        world
            .dispatch(Commit {
                person: PersonId(2),
                domain: "politics".into(),
                preference: "change".into(),
            })
            .unwrap();
        assert_eq!(world.dispatch(Cost(PersonId(2))).unwrap(), 0);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
