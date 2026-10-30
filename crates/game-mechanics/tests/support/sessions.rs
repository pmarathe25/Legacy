use game_core::*;
use game_mechanics::*;
use game_sim::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CancelTrial(PersonId);
impl Request for CancelTrial {
    const KEY: &'static str = "mechanics-test.cancel";
    type Response = ();
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct Seen {
    people: Vec<PersonId>,
    evidence: Vec<(PersonId, u64, u64)>,
    all_settled: Vec<bool>,
    reject: bool,
}
impl Resource for Seen {
    const KEY: &'static str = "session-test.seen";
    fn validate(&self) -> Result<()> {
        require(
            !self.reject || self.evidence.is_empty(),
            "reject trial evidence",
        )
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Track(Vec<PersonId>);
impl Request for Track {
    const KEY: &'static str = "session-test.track";
    type Response = ();
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Reject;
impl Request for Reject {
    const KEY: &'static str = "session-test.reject";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_communication::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_testimony::plugin(&mut b).unwrap();
    game_choice::plugin(&mut b).unwrap();
    game_inquiry::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    game_invention::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    game_workshop::plugin(&mut b).unwrap();
    b.plugin("mechanics-test", 1).unwrap();
    b.request::<CancelTrial>(|tx, r| tx.cancel_action(r.0))
        .unwrap();
    b.resource::<Seen>().unwrap();
    b.subscribe::<Tested>("session-test.evidence", |tx, event| {
        let all_settled = tx.get::<Seen>()?.people.iter().all(|p| {
            let body = &tx.get::<game_physiology::Bodies>().unwrap().people[p];
            body.fatigue > 0 || body.fatigue_fraction > 0
        });
        let seen = tx.edit::<Seen>()?;
        seen.evidence
            .push((event.person, event.lifted, event.travel));
        seen.all_settled.push(all_settled);
        Ok(())
    })
    .unwrap();
    b.request::<Track>(|tx, r| {
        tx.edit::<Seen>()?.people = r.0;
        Ok(())
    })
    .unwrap();
    b.request::<Reject>(|tx, _| {
        tx.edit::<Seen>()?.reject = true;
        Ok(())
    })
    .unwrap();
    b
}
fn setup(count: usize) -> (Host, Vec<Experiment>) {
    setup_limit(count, 1_000_000)
}
fn setup_limit(count: usize, limit: u64) -> (Host, Vec<Experiment>) {
    let mut h = builder().build(31, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        hunger_per_tick: 0,
        fatigue_per_tick: 0,
        ..Default::default()
    }))
    .unwrap();
    h.advance(SimTime(30 * 365 * DAY.0), 1).unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Workshop".into(),
        })
        .unwrap();
    let good = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("timber")
        .unwrap();
    h.dispatch(Install(Config {
        supports: Default::default(),
        materials: [(
            good,
            Material {
                span: 12,
                limit,
                efficiency: 1000,
                supports: Default::default(),
            },
        )]
        .into(),
    }))
    .unwrap();
    let mut requests = Vec::new();
    for _ in 0..count {
        let person = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person, area }).unwrap();
        let inventory = h
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Area(area),
                capacity: 100_000,
            })
            .unwrap();
        h.dispatch(game_property::ClaimInventory {
            claimant: person,
            inventory,
            recognized_by: [person].into(),
        })
        .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory,
            good,
            quantity: 3000,
        })
        .unwrap();
        let device = h
            .dispatch(Assemble {
                person,
                inventory,
                parts: vec![Part {
                    mechanism: Mechanism::Lever,
                    good,
                    input: None,
                    drive: 2,
                    load: 1,
                }],
            })
            .unwrap();
        requests.push(Experiment {
            person,
            device,
            weights: inventory,
            load: 100,
            outlet: 0,
        });
    }
    h.dispatch(game_choice::ConfigureFrames {
        interval: Some(SimDuration(15 * MINUTE.0)),
    })
    .unwrap();
    h.dispatch(game_choice::ConfigureFrameDemand(true)).unwrap();
    h.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
        mechanics_sessions: true,
        batch_decisions: true,
        urgent_immediate: true,
        commit_batch: 128,
        ..Default::default()
    }))
    .unwrap();
    h.dispatch(Track(requests.iter().map(|r| r.person).collect()))
        .unwrap();
    (h, requests)
}
fn run(h: &mut Host, until: SimTime) {
    while h.advance(until, 128).unwrap().stop == Stop::Budget {}
}
#[test]
fn real_trial_epochs_publish_all_body_rows_before_canonical_evidence_and_replay() {
    let (initial, requests) = setup(16);
    let saved = initial.save().unwrap();
    let mut worlds = Vec::new();
    for workers in [1, 4, 12] {
        let mut h = builder().load(&saved, workers).unwrap();
        let start = h.now();
        for r in &requests {
            h.dispatch(r.clone()).unwrap();
        }
        let ids: Vec<_> = requests
            .iter()
            .map(|r| h.get::<Actions>().unwrap().0[&r.person].id)
            .collect();
        run(&mut h, start.after(HOUR).unwrap());
        assert_eq!(h.get::<Seen>().unwrap().evidence.len(), 16);
        assert!(h.get::<Seen>().unwrap().all_settled.iter().all(|x| *x));
        for (r, id) in requests.iter().zip(&ids) {
            assert_eq!(h.get::<Actions>().unwrap().0[&r.person].id, *id);
        }
        let mut h = builder().load(&h.save().unwrap(), workers).unwrap();
        run(&mut h, start.after(SimDuration(2 * HOUR.0)).unwrap());
        let seen = h.get::<Seen>().unwrap();
        assert_eq!(seen.evidence.len(), 32);
        assert!(
            seen.evidence[..16]
                .iter()
                .zip(&seen.evidence[16..])
                .all(|(a, b)| a.0 == b.0 && a.1 < b.1)
        );
        for r in &requests {
            assert!(
                h.get::<game_knowledge::Knowledge>().unwrap().0[&r.person]
                    .contains_key(format!("mechanics:device:{}:0", r.device).as_str())
            );
        }
        run(&mut h, start.after(SimDuration(4 * HOUR.0)).unwrap());
        assert!(h.get::<Actions>().unwrap().0.is_empty());
        assert_eq!(h.get::<Seen>().unwrap().evidence.len(), 64);
        worlds.push(h.save().unwrap());
    }
    assert!(worlds.windows(2).all(|p| p[0] == p[1]));
}
#[test]
fn cancellation_settles_partial_work_once_and_stale_epoch_cannot_touch_replacement() {
    let (mut h, requests) = setup(1);
    let r = requests[0].clone();
    let start = h.now();
    h.dispatch(r.clone()).unwrap();
    run(&mut h, start.after(SimDuration(HOUR.0 / 2)).unwrap());
    h.dispatch(CancelTrial(r.person)).unwrap();
    assert_eq!(h.get::<Seen>().unwrap().evidence.len(), 1);
    h.dispatch(r.clone()).unwrap();
    let id = h.get::<Actions>().unwrap().0[&r.person].id;
    run(&mut h, start.after(HOUR).unwrap());
    assert_eq!(h.get::<Actions>().unwrap().0[&r.person].id, id);
    assert_eq!(h.get::<Seen>().unwrap().evidence.len(), 1);
}
#[test]
fn trial_wave_and_all_callbacks_rollback_with_outer_failure() {
    let (mut h, requests) = setup(8);
    let until = h.now().after(HOUR).unwrap();
    for r in requests {
        h.dispatch(r).unwrap();
    }
    h.dispatch(Reject).unwrap();
    loop {
        let before = h.save().unwrap();
        match h.advance(until, 1) {
            Err(_) => {
                assert_eq!(h.save().unwrap(), before);
                break;
            }
            Ok(p) => assert_eq!(p.stop, Stop::Budget),
        }
    }
}
#[test]
fn a_tested_observer_can_interrupt_a_peer_after_its_physical_work_without_double_settlement() {
    let (initial, requests) = setup(2);
    let saved = initial.save().unwrap();
    let first = requests[0].person;
    let second = requests[1].person;
    let device = requests[1].device;
    let inventory = requests[1].weights;
    let mut b = builder();
    b.subscribe::<Tested>("session-test.cancel-peer", move |tx, e| {
        if e.person == first {
            tx.call(InterruptSession(second))?;
            tx.call(Dismantle {
                person: second,
                device,
                inventory,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let mut h = b.load(&saved, 4).unwrap();
    let until = h.now().after(HOUR).unwrap();
    for r in requests {
        h.dispatch(r).unwrap();
    }
    run(&mut h, until);
    assert_eq!(h.get::<Seen>().unwrap().evidence.len(), 2);
    assert!(!h.get::<Actions>().unwrap().0.contains_key(&second));
    assert!(!h.get::<Machines>().unwrap().devices.contains_key(&device));
    assert!(
        h.get::<game_knowledge::Knowledge>().unwrap().0[&second]
            .contains_key(format!("mechanics:device:{device}:0").as_str())
    );
}

#[test]
fn scalar_effort_hooks_keep_ordered_semantics_and_end_the_session() {
    let (initial, requests) = setup(4);
    let mut b = builder();
    b.before::<game_physiology::Exert>("session-test.scalar-effort", |_, _| Ok(()))
        .unwrap();
    let mut h = b.load(&initial.save().unwrap(), 4).unwrap();
    let until = h.now().after(HOUR).unwrap();
    for r in requests {
        h.dispatch(r).unwrap();
    }
    run(&mut h, until);
    assert_eq!(
        h.get::<Seen>().unwrap().all_settled,
        vec![false, false, false, true]
    );
    assert!(h.get::<Actions>().unwrap().0.is_empty());
}

#[test]
fn stock_observer_changes_force_rollback_then_ordered_settlement() {
    let (initial, mut requests) = setup_limit(2, 1000);
    for r in &mut requests {
        r.load = 1500;
    }
    let first_inventory = initial.get::<Machines>().unwrap().devices[&requests[0].device].inventory;
    let second = requests[1].person;
    let mut b = builder();
    b.subscribe::<game_cargo::Changed>("session-test.stock-interference", move |tx, e| {
        if e.inventory == first_inventory && !e.added {
            tx.call(game_physiology::Exert {
                person: second,
                energy: 0,
                fatigue: 500_000,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let mut h = b.load(&initial.save().unwrap(), 4).unwrap();
    let until = h.now().after(HOUR).unwrap();
    for r in &requests {
        h.dispatch(r.clone()).unwrap();
    }
    run(&mut h, until);
    assert_eq!(h.get::<Seen>().unwrap().evidence.len(), 2);
    assert!(h.get::<Actions>().unwrap().0.is_empty());
    let body = h.get::<game_physiology::Bodies>().unwrap().people[&second];
    assert!(
        (500..503).contains(&body.fatigue),
        "speculative stock hook must not apply twice"
    );
    assert_eq!(h.get::<Machines>().unwrap().waste.values().sum::<u64>(), 2);
}
