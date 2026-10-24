use game_choice::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct State {
    value: u64,
    observations: u64,
}
impl Resource for State {
    const KEY: &'static str = "test.frame-state";
}
#[derive(Clone, Serialize, Deserialize)]
struct Change(u64);
impl Request for Change {
    const KEY: &'static str = "test.frame-change";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Observe;
impl Request for Observe {
    const KEY: &'static str = "test.frame-observe";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Consume;
impl Request for Consume {
    const KEY: &'static str = "test.frame-consume";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
struct CheckStale {
    person: PersonId,
    proposal: Proposal,
}
impl Request for CheckStale {
    const KEY: &'static str = "test.frame-stale";
    type Response = bool;
}
fn builder(effectful: bool) -> Builder {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    b.plugin("test.frames", 1).unwrap();
    b.resource::<State>().unwrap();
    b.request::<CheckStale>(|tx, r| frame_stale(tx, r.person, &r.proposal))
        .unwrap();
    b.request::<Change>(|tx, r| {
        tx.edit::<State>()?.value = r.0;
        Ok(())
    })
    .unwrap();
    b.request::<Observe>(|tx, _| {
        tx.edit::<State>()?.observations += 1;
        Ok(())
    })
    .unwrap();
    b.request::<Consume>(|tx, _| {
        if tx.get::<State>()?.value == 0 {
            return Ok(false);
        }
        tx.edit::<State>()?.value -= 1;
        Ok(true)
    })
    .unwrap();
    provider(&mut b, "a.pure", |tx, e| {
        let value = tx.get::<State>()?.value;
        offer(
            tx,
            e.person,
            format!("value:{value}"),
            motive(value as i64, 0, 0, 0, 0),
            Wait,
        )?;
        Ok(())
    })
    .unwrap();
    if effectful {
        provider(&mut b, "b.observe", |tx, _| tx.call(Observe)).unwrap();
    }
    b.subscribe::<game_decision_frames::Gather>("test.cohort", |tx, _| {
        tx.call(game_decision_frames::Batch {
            people: vec![PersonId(2), PersonId(1), PersonId(1)],
        })
    })
    .unwrap();
    b
}

#[test]
fn a_taken_opportunity_is_rechecked_without_executing_it_or_learning_a_failure() {
    for workers in [1, 4] {
        let mut h = builder(false).build(7, workers).unwrap();
        h.dispatch(Change(1)).unwrap();
        h.dispatch(ConfigureFrames {
            interval: Some(MINUTE),
        })
        .unwrap();
        h.dispatch(TuneFrames(FrameTuning {
            recheck_conflicts: true,
            ..Default::default()
        }))
        .unwrap();
        h.advance(SimTime(0), 1).unwrap();
        let proposal = h
            .dispatch(Collect {
                person: PersonId(2),
            })
            .unwrap()
            .remove(0);
        assert!(
            !h.dispatch(CheckStale {
                person: PersonId(2),
                proposal: proposal.clone()
            })
            .unwrap()
        );
        h.dispatch(Change(42)).unwrap();
        assert!(
            h.dispatch(CheckStale {
                person: PersonId(2),
                proposal
            })
            .unwrap()
        );
        assert_eq!(
            h.get::<State>().unwrap().value,
            42,
            "eligibility check must be pure"
        );
        assert_eq!(h.get::<State>().unwrap().observations, 0);
        assert_eq!(h.get::<Offers>().unwrap().frame_conflicts, 1);
    }
}

#[test]
fn actors_can_plan_the_same_supply_but_only_one_commits_its_consumption() {
    let mut b = builder(false);
    provider(&mut b, "c.consume", |tx, e| {
        if tx.get::<State>()?.value > 0 {
            offer(tx, e.person, "eat", motive(100, 0, 0, 0, 0), Consume)?;
        }
        Ok(())
    })
    .unwrap();
    let mut h = b.build(7, 4).unwrap();
    h.dispatch(Change(1)).unwrap();
    h.dispatch(ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    h.advance(SimTime(0), 1).unwrap();
    for (person, succeeds) in [(PersonId(1), true), (PersonId(2), false)] {
        let proposal = h
            .dispatch(Collect { person })
            .unwrap()
            .into_iter()
            .find(|p| p.key == "eat")
            .unwrap();
        assert_eq!(h.dispatch(Run(proposal.request)).is_ok(), succeeds);
    }
    assert_eq!(h.get::<State>().unwrap().value, 0);
}

#[test]
fn a_caught_effect_discards_only_its_provider_and_keeps_other_chunk_results() {
    let mut saves = Vec::new();
    for workers in [1, 4] {
        let mut b = builder(false);
        provider(&mut b, "b.caught", |tx, e| {
            let _ = tx.call(Observe);
            offer(tx, e.person, "effect", motive(1, 0, 0, 0, 0), Wait)?;
            Ok(())
        })
        .unwrap();
        provider(&mut b, "c.after", |tx, e| {
            offer(tx, e.person, "after", motive(1, 0, 0, 0, 0), Wait)?;
            Ok(())
        })
        .unwrap();
        let mut h = b.build(7, workers).unwrap();
        h.dispatch(ConfigureFrames {
            interval: Some(MINUTE),
        })
        .unwrap();
        h.advance(SimTime(0), 1).unwrap();
        let frame = h.get::<Offers>().unwrap().frame.as_ref().unwrap();
        assert!(frame.disabled.contains("b.caught"));
        assert_eq!(frame.prepared, 4); // Both actors retain the two pure providers.
        assert_eq!(h.get::<State>().unwrap().observations, 0);
        for person in [PersonId(1), PersonId(2)] {
            assert_eq!(h.dispatch(Collect { person }).unwrap().len(), 3);
        }
        assert_eq!(h.get::<State>().unwrap().observations, 2);
        saves.push(h.save().unwrap());
    }
    assert_eq!(saves[0], saves[1]);
}
fn start(workers: usize, effectful: bool) -> Host {
    let mut h = builder(effectful).build(7, workers).unwrap();
    h.dispatch(ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    h.advance(SimTime(0), 1).unwrap();
    h
}
#[test]
fn actors_share_one_frame_then_receive_new_inputs_next_step() {
    let mut h = start(4, false);
    assert_eq!(
        h.dispatch(Collect {
            person: PersonId(1)
        })
        .unwrap()[0]
            .key,
        "value:0"
    );
    h.dispatch(Change(42)).unwrap();
    assert_eq!(
        h.dispatch(Collect {
            person: PersonId(2)
        })
        .unwrap()[0]
            .key,
        "value:0"
    );
    h.advance(SimTime(MINUTE.0), 10).unwrap();
    assert_eq!(
        h.dispatch(Collect {
            person: PersonId(2)
        })
        .unwrap()[0]
            .key,
        "value:42"
    );
    h.dispatch(ConfigureFrames { interval: None }).unwrap();
    h.dispatch(Change(99)).unwrap();
    assert_eq!(
        h.dispatch(Collect {
            person: PersonId(1)
        })
        .unwrap()[0]
            .key,
        "value:99"
    );
    assert_eq!(h.scheduled().count(), 0);
}
#[test]
fn frames_preserve_complete_state_across_workers_and_midframe_reload() {
    let mut serial = start(1, true);
    let mut parallel = start(4, true);
    serial.dispatch(Change(42)).unwrap();
    parallel.dispatch(Change(42)).unwrap();
    let saved = parallel.save().unwrap();
    parallel = builder(true).load(&saved, 2).unwrap();
    parallel.set_parallel_listeners(true);
    for p in [PersonId(1), PersonId(2)] {
        assert_eq!(
            serial.dispatch(Collect { person: p }).unwrap(),
            parallel.dispatch(Collect { person: p }).unwrap()
        );
    }
    assert_eq!(serial.get::<State>().unwrap().observations, 2);
    assert!(
        serial
            .get::<Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .disabled
            .contains("b.observe")
    );
    assert_eq!(serial.save().unwrap(), parallel.save().unwrap());
    serial.advance(SimTime(MINUTE.0), 10).unwrap();
    parallel.advance(SimTime(MINUTE.0), 10).unwrap();
    assert_eq!(serial.save().unwrap(), parallel.save().unwrap());
}
#[test]
fn invalid_configuration_rolls_back_and_disabled_saves_keep_the_original_shape() {
    let mut h = builder(false).build(7, 4).unwrap();
    let saved = h.save().unwrap();
    assert!(
        h.dispatch(ConfigureFrames {
            interval: Some(SimDuration(0))
        })
        .is_err()
    );
    assert_eq!(saved, h.save().unwrap());
    let value: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(value["resources"]["choice.offers"], serde_json::json!({}));
}

#[test]
fn exact_drafts_cross_barriers_but_changed_reads_recompute_at_the_original_position() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::Relaxed},
    };
    let mut worlds = Vec::new();
    for workers in [1, 4] {
        let mut b = builder(false);
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        serial_provider(&mut b, "b.barrier", move |tx, _| {
            if workers > 1 {
                assert_eq!(seen.load(Relaxed), 1);
            }
            tx.call(Change(42))
        })
        .unwrap();
        let count = calls.clone();
        provider(&mut b, "z.late", move |tx, e| {
            count.fetch_add(1, Relaxed);
            let value = tx.get::<State>()?.value;
            offer(
                tx,
                e.person,
                format!("late:{value}"),
                motive(1, 0, 0, 0, 0),
                Wait,
            )?;
            Ok(())
        })
        .unwrap();
        let mut h = b.build(7, workers).unwrap();
        h.set_parallel_listeners(true);
        let proposals = h
            .dispatch(Collect {
                person: PersonId(1),
            })
            .unwrap();
        assert_eq!(proposals[0].key, "late:42");
        assert_eq!(proposals[1].key, "value:0");
        assert_eq!(calls.load(Relaxed), if workers == 1 { 1 } else { 2 });
        worlds.push(h.save().unwrap());
    }
    assert_eq!(worlds[0], worlds[1]);
}
