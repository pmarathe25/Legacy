mod support;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Reviews(u64);
impl Resource for Reviews {
    const KEY: &'static str = "test.reviews";
}
fn builder() -> Builder {
    let mut b = support::builder();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    b.plugin("count-reviews", 1).unwrap();
    b.resource::<Reviews>().unwrap();
    game_choice::provider(&mut b, "count-reviews", |tx, _| {
        tx.edit::<Reviews>()?.0 += 1;
        Ok(())
    })
    .unwrap();
    b
}
fn setup(attention: i32) -> (Host, PersonId) {
    let mut h = builder().build(1, 1).unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    support::run(&mut h, SimTime(30 * 365 * DAY.0));
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Home".into(),
        })
        .unwrap();
    let p = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("attention".into(), attention)].into(),
    })
    .unwrap();
    h.dispatch(game_transport::Place { person: p, area })
        .unwrap();
    h.dispatch(game_characters::Register {
        person: p,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(game_characters::Start).unwrap();
    (h, p)
}
#[test]
fn attention_changes_routine_reconsideration_but_injury_still_interrupts_and_replays() {
    let (mut focused, p) = setup(16000);
    let (mut distractible, _) = setup(0);
    let end = focused.now().after(SimDuration(30 * MINUTE.0)).unwrap();
    support::run(&mut focused, end);
    support::run(&mut distractible, end);
    assert!(focused.get::<Reviews>().unwrap().0 < distractible.get::<Reviews>().unwrap().0);
    let previous = focused.get::<Reviews>().unwrap().0;
    let mut loaded = builder().load(&focused.save().unwrap(), 4).unwrap();
    for h in [&mut focused, &mut loaded] {
        h.dispatch(game_physiology::Damage {
            person: p,
            amount: 1,
        })
        .unwrap();
        support::run(h, end);
        assert_eq!(h.get::<Reviews>().unwrap().0, previous + 1);
    }
    assert_eq!(focused.save().unwrap(), loaded.save().unwrap());
}
