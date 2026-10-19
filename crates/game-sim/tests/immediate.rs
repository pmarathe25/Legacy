use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Serialize, Deserialize)]
struct Invalid(BTreeMap<Vec<u8>, u64>);
impl Request for Invalid {
    const KEY: &'static str = "test.invalid";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Forward(Pending);
impl Request for Forward {
    const KEY: &'static str = "test.forward";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Retain;
impl Request for Retain {
    const KEY: &'static str = "test.retain";
    type Response = ();
}
#[test]
fn retaining_immediate_work_still_validates_nested_payloads_and_rolls_back() {
    let mut b = Builder::new();
    b.plugin("immediate-test", 1).unwrap();
    b.request::<Invalid>(|_, _| Ok(())).unwrap();
    b.request::<Forward>(|tx, r| {
        tx.invoke(r.0)?;
        Ok(())
    })
    .unwrap();
    b.request::<Retain>(|tx, _| {
        tx.schedule(
            SimTime(1),
            0,
            Forward(Pending::immediate(Invalid([(vec![1, 2], 1)].into()))),
        )?;
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    let before = h.save().unwrap();
    assert!(h.dispatch(Retain).is_err());
    assert_eq!(h.save().unwrap(), before);
    assert!(
        h.dispatch(Forward(Pending::immediate(Invalid([(vec![1], 1)].into()))))
            .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
}
