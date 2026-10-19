mod support;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Notices(u64);
impl Resource for Notices {
    const KEY: &'static str = "test.notices";
}
#[test]
fn repeated_or_rejected_testimony_does_not_claim_new_evidence() {
    let mut b = support::builder();
    b.plugin("attention-counter", 1).unwrap();
    b.resource::<Notices>().unwrap();
    b.subscribe::<game_choice::Changed>("test.count", |tx, _| {
        tx.edit::<Notices>()?.0 += 1;
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    let sender = PersonId(1);
    let recipient = PersonId(2);
    let tell = game_testimony::Report {
        sender,
        recipient,
        key: "remembered-trial".into(),
        delay: SimDuration(0),
    };
    h.dispatch(game_knowledge::Conclude {
        person: sender,
        key: tell.key.clone(),
        value: serde_json::json!(12),
        confidence: 80,
    })
    .unwrap();
    h.dispatch(tell.clone()).unwrap();
    support::run(&mut h, SimTime(0));
    assert_eq!(h.get::<Notices>().unwrap().0, 1);
    h.dispatch(tell.clone()).unwrap();
    support::run(&mut h, SimTime(0));
    assert_eq!(h.get::<Notices>().unwrap().0, 1);
    h.dispatch(game_knowledge::Conclude {
        person: recipient,
        key: tell.key.clone(),
        value: serde_json::json!(13),
        confidence: 100,
    })
    .unwrap();
    h.dispatch(tell).unwrap();
    support::run(&mut h, SimTime(0));
    assert_eq!(h.get::<Notices>().unwrap().0, 1);
}
