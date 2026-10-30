use super::support;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use support::*;
#[derive(Clone, Serialize, Deserialize)]
struct Attraction(PersonId, PersonId);
impl Request for Attraction {
    const KEY: &'static str = "test.attraction";
    type Response = i64;
}
fn test_builder() -> Builder {
    let mut b = builder();
    b.request::<Attraction>(|tx, r| Ok(game_family_life::attraction(tx, r.0, r.1)))
        .unwrap();
    b
}
#[test]
fn genetic_tastes_learned_beauty_and_heard_interests_make_attraction_subjective() {
    let (mut original, area) = world();
    let (a, _) = person(&mut original, area);
    let (b, _) = person(&mut original, area);
    let (q, _) = person(&mut original, area);
    let mut h = test_builder().load(&original.save().unwrap(), 1).unwrap();
    for (person, size) in [(a, 0), (b, 1000)] {
        h.dispatch(game_traits::Set {
            person,
            profile: [
                ("taste-size".into(), size),
                ("taste-build".into(), 500),
                ("aesthetic-sensitivity".into(), 1000),
                ("reproductive-drive".into(), 0),
            ]
            .into(),
        })
        .unwrap();
    }
    h.dispatch(game_traits::Set {
        person: q,
        profile: [("size".into(), 2000), ("strength".into(), 1000)].into(),
    })
    .unwrap();
    assert!(h.dispatch(Attraction(b, q)).unwrap() > h.dispatch(Attraction(a, q)).unwrap());
    let before = h.dispatch(Attraction(a, q)).unwrap();
    set_opinion(&mut h, a, "beauty:size", 100);
    assert!(h.dispatch(Attraction(a, q)).unwrap() > before);
    set_opinion(&mut h, a, "music", 80);
    set_opinion(&mut h, q, "music", -80);
    let private = h.dispatch(Attraction(a, q)).unwrap();
    set_opinion(&mut h, q, "music", 80);
    assert_eq!(
        h.dispatch(Attraction(a, q)).unwrap(),
        private,
        "private values are not an attraction oracle"
    );
    h.dispatch(game_relationships::HearOpinion {
        listener: a,
        speaker: q,
        via: q,
        concept: "music".into(),
        stance: 80,
        coerced: false,
    })
    .unwrap();
    assert!(h.dispatch(Attraction(a, q)).unwrap() > private);
    let saved = h.save().unwrap();
    assert_eq!(
        test_builder().load(&saved, 4).unwrap().save().unwrap(),
        saved
    );
}
