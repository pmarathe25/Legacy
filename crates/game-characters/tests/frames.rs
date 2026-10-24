mod support;
use game_characters::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Counts(u64);
impl Resource for Counts {
    const KEY: &'static str = "test.frame-reviews";
}
#[derive(Clone, Serialize, Deserialize)]
struct Evidence(PersonId);
impl Request for Evidence {
    const KEY: &'static str = "test.frame-evidence";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Involve(PersonId);
impl Request for Involve {
    const KEY: &'static str = "test.frame-involve";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
struct MealsQuery {
    person: PersonId,
    snapshot: bool,
}
impl Request for MealsQuery {
    const KEY: &'static str = "test.frame-meals";
    type Response = Vec<game_provisioning::Opportunity>;
}
fn builder() -> Builder {
    let mut b = support::builder();
    b.plugin("frame-test", 1).unwrap();
    b.resource::<Counts>().unwrap();
    b.request::<MealsQuery>(|tx, r| {
        if r.snapshot {
            game_decision_food::options(tx, r.person, 1)
        } else {
            game_provisioning::options(tx, r.person, 1)
        }
    })
    .unwrap();
    b.request::<Evidence>(|tx, e| tx.notify(game_choice::Changed { person: e.0 }))
        .unwrap();
    b.request::<Involve>(|tx, r| {
        tx.start_action(r.0, "joint-activity", None, Evidence(r.0))?;
        Ok(tx.get::<Actions>()?.0[&r.0].id)
    })
    .unwrap();
    game_choice::serial_provider(&mut b, "test.count", |tx, _| {
        tx.edit::<Counts>()?.0 += 1;
        Ok(())
    })
    .unwrap();
    b.subscribe::<game_decision_frames::Gather>("test.cohort", |tx, _| {
        let people = tx.get::<Minds>()?.people.keys().copied().collect();
        tx.call(game_decision_frames::Batch { people })
    })
    .unwrap();
    b
}

fn actor(h: &mut Host) -> PersonId {
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person, area }).unwrap();
    h.dispatch(Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    person
}

#[test]
fn idle_recovery_runs_once_next_step_and_survives_current_review_cleanup_and_reload() {
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(Configure(Rules {
        review: SimDuration(12 * HOUR.0),
        ..Default::default()
    }))
    .unwrap();
    let person = actor(&mut h);
    h.dispatch(Start).unwrap();
    h.dispatch(game_choice::ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    h.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
        recover_idle: true,
        ..Default::default()
    }))
    .unwrap();
    support::run(&mut h, SimTime(0));
    h.dispatch(Resume { person }).unwrap();
    assert_eq!(h.get::<Counts>().unwrap().0, 1);
    assert_eq!(
        h.get::<Minds>().unwrap().schedule.waking[&person],
        SimTime(MINUTE.0)
    );
    support::run(&mut h, SimTime(MINUTE.0 / 2));
    let mut four = builder().load(&h.save().unwrap(), 4).unwrap();
    for target in [SimTime(MINUTE.0), SimTime(2 * MINUTE.0)] {
        support::run(&mut h, target);
        support::run(&mut four, target);
        assert_eq!(
            h.get::<Counts>().unwrap().0,
            2,
            "idle retries must be bounded"
        );
        assert_eq!(h.save().unwrap(), four.save().unwrap());
    }
    assert!(
        h.get::<Minds>()
            .unwrap()
            .schedule
            .waking
            .get(&person)
            .is_none()
    );
    h.dispatch(Evidence(person)).unwrap();
    support::run(&mut h, SimTime(4 * MINUTE.0));
    assert_eq!(
        h.get::<Counts>().unwrap().0,
        4,
        "a later cue gets its own recovery"
    );
}

#[test]
fn cohort_food_inputs_survive_reload_and_never_become_executable_choices() {
    let mut h = builder().build(7, 1).unwrap();
    let person = actor(&mut h);
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(person),
            capacity: 10000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore { person, inventory })
        .unwrap();
    h.dispatch(game_choice::ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    h.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
        batch_food: true,
        ..Default::default()
    }))
    .unwrap();
    support::run(&mut h, SimTime(0));
    let before = h
        .dispatch(MealsQuery {
            person,
            snapshot: true,
        })
        .unwrap();
    assert!(!before.is_empty());
    assert!(
        h.dispatch(game_choice::Collect { person })
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        support::food(&h, inventory),
        10,
        "preparation must not consume stock"
    );
    h.dispatch(game_cargo::Consume {
        inventory,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore { person, inventory })
        .unwrap();
    assert!(
        h.dispatch(MealsQuery {
            person,
            snapshot: false
        })
        .unwrap()
        .is_empty()
    );
    let mut four = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut four] {
        assert_eq!(
            host.dispatch(MealsQuery {
                person,
                snapshot: true
            })
            .unwrap(),
            before
        );
        support::run(host, SimTime(MINUTE.0));
        assert!(
            host.dispatch(MealsQuery {
                person,
                snapshot: true
            })
            .unwrap()
            .is_empty()
        );
    }
    assert_eq!(h.save().unwrap(), four.save().unwrap());
}

#[test]
fn vanished_snapshot_opportunities_skip_failure_memory_but_real_refusals_still_teach() {
    #[derive(Clone, Default, Serialize, Deserialize)]
    struct Availability(bool);
    impl Resource for Availability {
        const KEY: &'static str = "test.availability";
    }
    #[derive(Clone, Serialize, Deserialize)]
    struct Open(bool);
    impl Request for Open {
        const KEY: &'static str = "test.open";
        type Response = ();
    }
    #[derive(Clone, Serialize, Deserialize)]
    struct Refuse;
    impl Request for Refuse {
        const KEY: &'static str = "test.refuse";
        type Response = bool;
    }
    for (recheck, available, failures, conflicts) in [
        (false, false, 1, 0),
        (true, false, 0, 1),
        (true, true, 1, 0),
    ] {
        let mut b = builder();
        b.resource::<Availability>().unwrap();
        b.request::<Open>(|tx, r| {
            tx.edit::<Availability>()?.0 = r.0;
            Ok(())
        })
        .unwrap();
        b.request::<Refuse>(|_, _| Ok(false)).unwrap();
        game_choice::provider(&mut b, "test.fragile", |tx, e| {
            if tx.get::<Availability>()?.0 {
                game_choice::offer(
                    tx,
                    e.person,
                    "fragile",
                    game_choice::motive(10000, 0, 0, 0, 0),
                    Refuse,
                )?;
            }
            Ok(())
        })
        .unwrap();
        let mut h = b.build(7, 4).unwrap();
        let person = actor(&mut h);
        h.dispatch(Start).unwrap();
        h.dispatch(Open(true)).unwrap();
        h.dispatch(game_choice::ConfigureFrames {
            interval: Some(MINUTE),
        })
        .unwrap();
        h.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
            recover_idle: true,
            recheck_conflicts: recheck,
            ..Default::default()
        }))
        .unwrap();
        support::run(&mut h, SimTime(0));
        h.dispatch(Open(available)).unwrap();
        h.dispatch(Resume { person }).unwrap();
        let mind = &h.get::<Minds>().unwrap().people[&person];
        assert_eq!(mind.failures, failures);
        assert_eq!(
            mind.retry_after
                .contains_key(&game_choice::identity("fragile")),
            failures > 0
        );
        assert_eq!(
            h.get::<game_choice::Offers>().unwrap().frame_conflicts,
            conflicts
        );
        assert_eq!(
            h.get::<Minds>().unwrap().schedule.waking[&person],
            SimTime(MINUTE.0)
        );
    }
}
#[test]
fn step_notifications_coalesce_and_new_information_waits_for_the_next_step() {
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person, area }).unwrap();
    h.dispatch(Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(Start).unwrap();
    h.dispatch(game_choice::ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    support::run(&mut h, SimTime(MINUTE.0 / 2));
    h.dispatch(Resume { person }).unwrap(); // A pre-existing midstep callback defers.
    assert_eq!(h.get::<Counts>().unwrap().0, 0);
    for _ in 0..5 {
        h.dispatch(Evidence(person)).unwrap();
    }
    assert_eq!(
        h.scheduled()
            .filter(|j| j.request.request::<Resume>().is_some())
            .count(),
        1
    );
    support::run(&mut h, SimTime(MINUTE.0 - 1));
    assert_eq!(h.get::<Counts>().unwrap().0, 0);
    support::run(&mut h, SimTime(MINUTE.0));
    assert_eq!(h.get::<Counts>().unwrap().0, 1);
    h.dispatch(Resume { person }).unwrap(); // A duplicate callback cannot review twice.
    assert_eq!(h.get::<Counts>().unwrap().0, 1);
    for _ in 0..5 {
        h.dispatch(Evidence(person)).unwrap();
    }
    assert_eq!(h.get::<Counts>().unwrap().0, 1);
    support::run(&mut h, SimTime(MINUTE.0 * 3 / 2));
    let mut four = builder().load(&h.save().unwrap(), 4).unwrap();
    let target = SimTime(MINUTE.0 * 2);
    while h.advance(target, 1).unwrap().stop == Stop::Budget {}
    while four.advance(target, 17).unwrap().stop == Stop::Budget {}
    assert_eq!(h.get::<Counts>().unwrap().0, 2);
    assert_eq!(h.save().unwrap(), four.save().unwrap());
}

#[test]
fn joining_an_activity_after_preparation_defers_the_obsolete_decision() {
    let configured = || {
        let mut b = builder();
        game_choice::provider(&mut b, "test.distraction", |tx, e| {
            game_choice::offer(
                tx,
                e.person,
                "distraction",
                game_choice::motive(10000, 0, 0, 0, 0),
                game_choice::Wait,
            )?;
            Ok(())
        })
        .unwrap();
        b
    };
    let mut h = configured().build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person, area }).unwrap();
    h.dispatch(Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(Start).unwrap();
    h.dispatch(game_choice::ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    support::run(&mut h, SimTime(0));
    // Another actor's resolution now involves this actor in a joint activity.
    let action = h.dispatch(Involve(person)).unwrap();
    h.dispatch(Resume { person }).unwrap();
    assert_eq!(
        h.get::<Actions>().unwrap().0.get(&person).map(|a| a.id),
        Some(action),
        "a precomputed choice cancelled a new joint activity"
    );
    assert_eq!(h.get::<Counts>().unwrap().0, 0, "obsolete review ran");
    assert_eq!(
        h.get::<Minds>().unwrap().schedule.waking[&person],
        SimTime(MINUTE.0)
    );
    let mut four = configured().load(&h.save().unwrap(), 4).unwrap();
    support::run(&mut h, SimTime(MINUTE.0));
    support::run(&mut four, SimTime(MINUTE.0));
    assert_eq!(h.get::<Counts>().unwrap().0, 1);
    assert_eq!(h.save().unwrap(), four.save().unwrap());
}
