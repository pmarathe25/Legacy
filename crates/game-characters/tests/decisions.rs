mod support;
use game_characters::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Default, Serialize, Deserialize)]
struct Senses(BTreeMap<PersonId, u64>);
impl Resource for Senses {
    const KEY: &'static str = "test.decision-senses";
}
#[derive(Clone, Serialize, Deserialize)]
struct Sense(PersonId);
impl Request for Sense {
    const KEY: &'static str = "test.decision-sense";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Consume(InventoryId);
impl Request for Consume {
    const KEY: &'static str = "test.consume-supply";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
struct Involve(PersonId);
impl Request for Involve {
    const KEY: &'static str = "test.involve-actor";
    type Response = ();
}
fn builder(effectful: bool) -> Builder {
    let mut b = support::builder();
    b.plugin("decision-test", 1).unwrap();
    b.resource::<Senses>().unwrap();
    b.request::<Sense>(|tx, r| {
        *tx.edit::<Senses>()?.0.entry(r.0).or_default() += 1;
        Ok(())
    })
    .unwrap();
    b.request::<Consume>(|tx, r| {
        if tx.get::<game_cargo::Cargo>()?.0[&r.0].amount(game_cargo::TIMBER) == 0 {
            return Ok(false);
        }
        tx.call(game_cargo::Consume {
            inventory: r.0,
            good: game_cargo::TIMBER,
            quantity: 1,
        })?;
        Ok(true)
    })
    .unwrap();
    b.request::<Involve>(|tx, r| tx.start_action(r.0, "joint", None, Sense(r.0)))
        .unwrap();
    game_choice::serial_provider(&mut b, "test.sensing", move |tx, e| {
        game_choice::defer(tx, Sense(e.person))?;
        if effectful {
            *tx.edit::<Senses>()?.0.entry(e.person).or_default() += 10;
        }
        Ok(())
    })
    .unwrap();
    game_choice::provider(&mut b, "test.pure-sensing", |tx, e| {
        game_choice::defer(tx, Sense(e.person))
    })
    .unwrap();
    game_choice::provider(&mut b, "test.supply", |tx, e| {
        let cargo = tx.get::<game_cargo::Cargo>()?;
        let inventory = cargo
            .0
            .iter()
            .find(|(_, v)| v.amount(game_cargo::TIMBER) > 0)
            .map(|(&inventory, _)| inventory);
        if let Some(inventory) = inventory {
            game_choice::offer(
                tx,
                e.person,
                "supply",
                game_choice::motive(1000, 0, 0, 0, 0),
                Consume(inventory),
            )?;
        }
        Ok(())
    })
    .unwrap();
    b.subscribe::<game_decision_frames::Gather>("test.cohort", |tx, _| {
        tx.call(game_decision_frames::Batch {
            people: tx.get::<Minds>()?.people.keys().copied().collect(),
        })
    })
    .unwrap();
    b
}
fn setup(effectful: bool, workers: usize) -> (Host, Vec<PersonId>, InventoryId) {
    setup_with(builder(effectful), workers)
}
fn setup_with(b: Builder, workers: usize) -> (Host, Vec<PersonId>, InventoryId) {
    let mut h = b.build(7, workers).unwrap();
    h.set_parallel_listeners(workers > 1);
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let mut people = vec![];
    for _ in 0..2 {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        h.dispatch(Register {
            person: p,
            mind: Default::default(),
        })
        .unwrap();
        people.push(p);
    }
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::TIMBER,
        quantity: 1,
    })
    .unwrap();
    h.dispatch(Start).unwrap();
    h.dispatch(game_choice::ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    h.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
        recover_idle: true,
        recheck_conflicts: true,
        batch_decisions: true,
        ..Default::default()
    }))
    .unwrap();
    support::run(&mut h, SimTime(0));
    (h, people, inventory)
}
#[test]
fn grouped_resolutions_preserve_shared_stock_arbitration_and_midframe_worker_reload() {
    for count in [1, 64] {
        replay_grouped_resolutions(count);
    }
}
fn replay_grouped_resolutions(count: usize) {
    let mut worlds = Vec::new();
    for workers in [1, 4] {
        let (mut h, people, inventory) = setup(false, workers);
        let mut tuning = h
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .tuning;
        tuning.commit_batch = count;
        h.dispatch(game_choice::ConfigureFrames {
            interval: Some(SimDuration(30_000)),
        })
        .unwrap();
        h.dispatch(game_choice::ConfigureFrameDemand(true)).unwrap();
        h.dispatch(game_choice::TuneFrames(tuning)).unwrap();
        h.advance(SimTime(0), 1).unwrap();
        assert!(
            h.scheduled()
                .any(|j| j.request.key() == "characters.resolve-frame")
        );
        h = builder(false).load(&h.save().unwrap(), 2).unwrap();
        h.advance(SimTime(0), 1).unwrap();
        h = builder(false).load(&h.save().unwrap(), 4).unwrap();
        support::run(&mut h, SimTime(0));
        assert!(h.scheduled().all(|j| j.request.key() != Resume::KEY));
        assert!(h.get::<Senses>().unwrap().0.values().all(|n| *n == 2));
        assert_eq!(h.get::<Senses>().unwrap().0.len(), people.len());
        assert_eq!(
            h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::TIMBER),
            0
        );
        assert!(
            h.get::<Minds>()
                .unwrap()
                .people
                .values()
                .all(|m| m.failures == 0)
        );
        worlds.push(h.save().unwrap());
    }
    assert_eq!(worlds[0], worlds[1]);
}
#[test]
fn a_failed_actor_rolls_back_the_entire_resolution_group() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering::Relaxed},
    };
    let failure = Arc::new(AtomicU64::new(0));
    let mut b = builder(false);
    let stop = failure.clone();
    b.before::<Resume>("test.stop-resume", move |_, r| {
        require(r.person.0 != stop.load(Relaxed), "injected failure")
    })
    .unwrap();
    let (mut h, people, _) = setup_with(b, 4);
    let mut tuning = h
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.commit_batch = 64;
    h.dispatch(game_choice::ConfigureFrames {
        interval: Some(SimDuration(30_000)),
    })
    .unwrap();
    h.dispatch(game_choice::ConfigureFrameDemand(true)).unwrap();
    h.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    h.advance(SimTime(0), 1).unwrap();
    failure.store(people[1].0, Relaxed);
    let before = h.save().unwrap();
    assert!(h.advance(SimTime(0), 1).is_err());
    assert_eq!(h.save().unwrap(), before);
    assert!(h.get::<Senses>().unwrap().0.is_empty());
}
#[test]
fn disabling_grouped_commits_or_frames_restores_existing_wake_jobs() {
    for disable_frames in [false, true] {
        let (mut h, people, _) = setup(false, 4);
        let mut tuning = h
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .tuning;
        tuning.commit_batch = 64;
        h.dispatch(game_choice::ConfigureFrames {
            interval: Some(SimDuration(30_000)),
        })
        .unwrap();
        h.dispatch(game_choice::ConfigureFrameDemand(true)).unwrap();
        h.dispatch(game_choice::TuneFrames(tuning)).unwrap();
        support::run(&mut h, SimTime(0));
        assert!(people.iter().all(|p| h.get::<Minds>().unwrap().schedule.waking.get(p) == Some(&SimTime(30_000))));
        assert!(h.scheduled().all(|j| j.request.key() != Resume::KEY));
        if disable_frames {
            h.dispatch(game_choice::ConfigureFrames { interval: None })
                .unwrap();
        } else {
            tuning.commit_batch = 0;
            h.dispatch(game_choice::TuneFrames(tuning)).unwrap();
        }
        assert_eq!(
            h.scheduled()
                .filter(|j| j.request.key() == Resume::KEY)
                .count(),
            people.len()
        );
        support::run(&mut h, SimTime(30_000));
        assert!(h.get::<Senses>().unwrap().0.values().all(|n| *n == 4));
    }
}
#[test]
fn prepared_actor_decisions_defer_sensing_and_resolve_shared_stock_deterministically_after_reload()
{
    let (mut one, people, inventory) = setup(false, 1);
    let (four, _, _) = setup(false, 4);
    assert_eq!(one.save().unwrap(), four.save().unwrap());
    assert!(one.get::<Senses>().unwrap().0.is_empty());
    assert_eq!(
        one.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::TIMBER),
        1
    );
    assert_eq!(
        one.get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .decisions
            .len(),
        2
    );
    let mut loaded = builder(false).load(&one.save().unwrap(), 4).unwrap();
    for &person in &people {
        one.dispatch(Resume { person }).unwrap();
        loaded.dispatch(Resume { person }).unwrap();
        assert_eq!(one.save().unwrap(), loaded.save().unwrap());
    }
    assert_eq!(
        one.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::TIMBER),
        0
    );
    assert!(
        one.get::<Minds>()
            .unwrap()
            .people
            .values()
            .all(|m| m.failures == 0)
    );
    assert_eq!(one.get::<game_choice::Offers>().unwrap().frame_conflicts, 1);
    assert!(one.get::<Senses>().unwrap().0.values().all(|n| *n == 2));
}
#[test]
fn a_blocked_effect_discards_partial_sensing_and_falls_back_to_live_review() {
    let (mut h, people, _) = setup(true, 4);
    let frame = h
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap();
    assert!(frame.decisions.is_empty());
    assert_eq!(frame.effectful, 2);
    assert!(h.get::<Senses>().unwrap().0.is_empty());
    h.dispatch(Resume { person: people[0] }).unwrap();
    assert_eq!(h.get::<Senses>().unwrap().0[&people[0]], 12);
}
#[test]
fn a_new_joint_activity_defers_the_entire_prepared_decision_including_sensing() {
    let (mut h, people, _) = setup(false, 4);
    h.dispatch(Involve(people[0])).unwrap();
    let action = h.get::<Actions>().unwrap().0[&people[0]].id;
    h.dispatch(Resume { person: people[0] }).unwrap();
    assert_eq!(h.get::<Actions>().unwrap().0[&people[0]].id, action);
    assert!(h.get::<Senses>().unwrap().0.is_empty());
    assert_eq!(
        h.get::<Minds>().unwrap().schedule.waking[&people[0]],
        SimTime(MINUTE.0)
    );
}
#[test]
fn small_cohorts_decide_live_without_preparation_and_preserve_worker_reload_consistency() {
    let mut worlds = Vec::new();
    for workers in [1, 4] {
        let (mut h, people, inventory) = setup(false, workers);
        let mut tuning = h
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .tuning;
        tuning.minimum_cohort = 3;
        h.dispatch(game_choice::TuneFrames(tuning)).unwrap();
        support::run(&mut h, SimTime(MINUTE.0));
        let frame = h
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap();
        assert_eq!(
            frame.prepared, 2,
            "the next small cohort must not prepare choices"
        );
        assert!(frame.decisions.is_empty());
        h.dispatch(Resume { person: people[0] }).unwrap();
        assert!(
            h.get::<Minds>()
                .unwrap()
                .schedule
                .waking
                .get(&people[0])
                .is_none(),
            "a live decision needs no snapshot recovery retry"
        );
        let saved = h.save().unwrap();
        h = builder(false).load(&saved, 2).unwrap();
        h.dispatch(Resume { person: people[1] }).unwrap();
        assert!(h.get::<Senses>().unwrap().0.values().all(|n| *n == 2));
        assert_eq!(
            h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::TIMBER),
            0
        );
        assert!(
            h.get::<Minds>()
                .unwrap()
                .people
                .values()
                .all(|m| m.failures == 0)
        );
        worlds.push(h.save().unwrap());
    }
    assert_eq!(worlds[0], worlds[1]);
}
