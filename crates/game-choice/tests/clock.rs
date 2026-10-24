use game_choice::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Counts(u64);
impl Resource for Counts {
    const KEY: &'static str = "test.clock-counts";
}
#[derive(Clone, Serialize, Deserialize)]
struct Cue(SimTime);
impl Request for Cue {
    const KEY: &'static str = "test.clock-cue";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct FailCue(SimTime);
impl Request for FailCue {
    const KEY: &'static str = "test.clock-fail-cue";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    b.plugin("test.clock", 1).unwrap();
    b.resource::<Counts>().unwrap();
    b.subscribe::<game_decision_frames::Gather>("test.count-frame", |tx, _| {
        tx.edit::<Counts>()?.0 += 1;
        Ok(())
    })
    .unwrap();
    b.request::<Cue>(|tx, r| wake_frame(tx, r.0)).unwrap();
    b.request::<FailCue>(|tx, r| {
        wake_frame(tx, r.0)?;
        Err(Error::Rejected("test rollback".into()))
    })
    .unwrap();
    b
}
fn sleeping(workers: usize) -> Host {
    let mut h = builder().build(7, workers).unwrap();
    h.dispatch(ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    h.dispatch(ConfigureFrameDemand(true)).unwrap();
    h.advance(SimTime(DAY.0), 100).unwrap();
    assert_eq!(h.get::<Counts>().unwrap().0, 1);
    assert_eq!(h.scheduled().count(), 0, "empty frames must stop");
    h
}
#[test]
fn sleeping_clock_wakes_earlier_without_duplicate_jobs_or_phase_drift() {
    let mut h = sleeping(1);
    let now = h.now();
    h.dispatch(Cue(now.after(SimDuration(30_000)).unwrap()))
        .unwrap();
    assert_eq!(h.scheduled().next().unwrap().at, now.after(MINUTE).unwrap());
    let save = h.save().unwrap();
    h.dispatch(Cue(now.after(HOUR).unwrap())).unwrap();
    assert_eq!(
        h.scheduled().count(),
        1,
        "a later cue must share the earlier frame"
    );
    let before: serde_json::Value = serde_json::from_str(&save).unwrap();
    let mut after: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    after["revision"] = before["revision"].clone();
    assert_eq!(before, after, "a coalesced cue consumes no identity");
    h.dispatch(Cue(now)).unwrap();
    assert_eq!(h.scheduled().count(), 1);
    assert_eq!(h.scheduled().next().unwrap().at, now);
    h.advance(now, 100).unwrap();
    assert_eq!(h.get::<Counts>().unwrap().0, 2);
    h.dispatch(Cue(now)).unwrap();
    assert_eq!(h.scheduled().next().unwrap().at, now.after(MINUTE).unwrap());
}
#[test]
fn pending_demand_survives_reload_and_failed_wake_rolls_back() {
    let mut h = sleeping(1);
    let at = h.now().after(HOUR).unwrap();
    h.dispatch(Cue(at)).unwrap();
    let save = h.save().unwrap();
    assert!(h.dispatch(FailCue(h.now())).is_err());
    assert_eq!(save, h.save().unwrap());
    let mut restored = builder().load(&save, 4).unwrap();
    h.advance(at.after(DAY).unwrap(), 100).unwrap();
    restored.advance(at.after(DAY).unwrap(), 100).unwrap();
    assert_eq!(h.save().unwrap(), restored.save().unwrap());
    assert_eq!(h.get::<Counts>().unwrap().0, 2);
    assert_eq!(h.scheduled().count(), 0);
}
#[test]
fn changing_interval_keeps_demand_and_disabling_restores_periodic_frames() {
    let mut h = sleeping(1);
    h.dispatch(ConfigureFrames {
        interval: Some(HOUR),
    })
    .unwrap();
    assert!(
        h.get::<Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .clock
            .is_some()
    );
    h.advance(h.now().after(DAY).unwrap(), 100).unwrap();
    assert_eq!(h.get::<Counts>().unwrap().0, 2);
    h.dispatch(ConfigureFrameDemand(false)).unwrap();
    h.advance(h.now().after(DAY).unwrap(), 100).unwrap();
    assert_eq!(h.get::<Counts>().unwrap().0, 27);
    assert_eq!(h.scheduled().count(), 1);
    h.dispatch(ConfigureFrames { interval: None }).unwrap();
    assert_eq!(h.scheduled().count(), 0);
}
#[test]
fn waking_preserves_a_nonzero_initial_frame_phase() {
    let mut h = builder().build(7, 1).unwrap();
    let origin = SimTime(12_345);
    h.advance(origin, 100).unwrap();
    h.dispatch(ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    h.dispatch(ConfigureFrameDemand(true)).unwrap();
    h.advance(origin, 100).unwrap();
    h.dispatch(Cue(origin.after(SimDuration(1)).unwrap()))
        .unwrap();
    assert_eq!(
        h.scheduled().next().unwrap().at,
        origin.after(MINUTE).unwrap()
    );
}
