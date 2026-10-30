use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Counts {
    tick: u64,
    other: u64,
}
impl Resource for Counts {
    const KEY: &'static str = "test.counts";
}
#[derive(Clone, Serialize, Deserialize)]
struct Tick;
impl Request for Tick {
    const KEY: &'static str = "test.tick";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Other;
impl Request for Other {
    const KEY: &'static str = "test.other";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Seed;
impl Request for Seed {
    const KEY: &'static str = "test.seed";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Reset {
    fail: bool,
}
impl Request for Reset {
    const KEY: &'static str = "test.reset";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Alien;
impl Request for Alien {
    const KEY: &'static str = "test.alien";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.plugin("clock", 1).unwrap();
    b.resource::<Counts>().unwrap();
    b.request::<Tick>(|tx, _| {
        tx.edit::<Counts>()?.tick += 1;
        Ok(())
    })
    .unwrap();
    b.request::<Other>(|tx, _| {
        tx.edit::<Counts>()?.other += 1;
        Ok(())
    })
    .unwrap();
    b.request::<Seed>(|tx, _| {
        tx.schedule(SimTime(HOUR.0), 10, Tick)?;
        tx.schedule(SimTime(HOUR.0), 10, Other)?;
        Ok(())
    })
    .unwrap();
    b.request::<Reset>(|tx, r| {
        tx.schedule(SimTime(HOUR.0), 10, Tick)?;
        tx.cancel_scheduled::<Tick>()?;
        require(!r.fail, "rollback reset")
    })
    .unwrap();
    b.plugin("unrelated", 1).unwrap();
    b.request::<Alien>(|tx, _| tx.cancel_scheduled::<Tick>())
        .unwrap();
    b
}
#[test]
fn cancelling_owned_callbacks_includes_staged_work_and_preserves_unrelated_jobs() {
    let mut h = builder().build(1, 1).unwrap();
    h.dispatch(Seed).unwrap();
    h.dispatch(Reset { fail: false }).unwrap();
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    for host in [&mut h, &mut loaded] {
        while host.advance(SimTime(HOUR.0), 128).unwrap().stop == Stop::Budget {}
        assert_eq!(host.get::<Counts>().unwrap().tick, 0);
        assert_eq!(host.get::<Counts>().unwrap().other, 1);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn failed_or_foreign_cancellation_preserves_the_pending_calendar() {
    let mut h = builder().build(1, 1).unwrap();
    h.dispatch(Seed).unwrap();
    let saved = h.save().unwrap();
    assert!(h.dispatch(Alien).is_err());
    assert_eq!(h.save().unwrap(), saved);
    assert!(h.dispatch(Reset { fail: true }).is_err());
    assert_eq!(h.save().unwrap(), saved);
    while h.advance(SimTime(HOUR.0), 128).unwrap().stop == Stop::Budget {}
    assert_eq!(h.get::<Counts>().unwrap().tick, 1);
    assert_eq!(h.get::<Counts>().unwrap().other, 1);
}
