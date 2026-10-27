mod support;
use game_characters::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
struct Collections(u64);
impl Resource for Collections {
    const KEY: &'static str = "test.frame-collections";
}
#[derive(Clone, Serialize, Deserialize)]
struct Count;
impl Request for Count {
    const KEY: &'static str = "test.frame-collection";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cue(PersonId);
impl Request for Cue {
    const KEY: &'static str = "test.frame-cadence-cue";
    type Response = ();
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Availability {
    closed: bool,
    performed: u64,
}
impl Resource for Availability {
    const KEY: &'static str = "test.frame-availability";
}
#[derive(Clone, Serialize, Deserialize)]
struct Close;
impl Request for Close {
    const KEY: &'static str = "test.close-frame-offer";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Fragile;
impl Request for Fragile {
    const KEY: &'static str = "test.fragile-frame-offer";
    type Response = ();
}

fn builder() -> Builder {
    let mut b = support::builder();
    b.plugin("frame-cadence-test", 1).unwrap();
    b.resource::<Collections>().unwrap();
    b.request::<Count>(|tx, _| {
        tx.edit::<Collections>()?.0 += 1;
        Ok(())
    })
    .unwrap();
    b.request::<Cue>(|tx, request| tx.notify(game_choice::Changed { person: request.0 }))
        .unwrap();
    game_choice::provider(&mut b, "test.nonurgent-opportunities", |tx, e| {
        game_choice::defer(tx, Count)?;
        for index in 0..8 {
            game_choice::offer(
                tx,
                e.person,
                format!("interest:{index}"),
                game_choice::motive(1000 - index, 0, 0, 0, 0),
                game_choice::Wait,
            )?;
        }
        Ok(())
    })
    .unwrap();
    b.subscribe::<game_decision_frames::Gather>("test.frame-cadence", |tx, _| {
        tx.call(game_decision_frames::Batch {
            people: tx.get::<Minds>()?.people.keys().copied().collect(),
        })
    })
    .unwrap();
    b
}

fn setup(window: u64) -> (Host, PersonId) {
    setup_with(builder(), window)
}

fn setup_with(builder: Builder, window: u64) -> (Host, PersonId) {
    let mut host = builder.build(7, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "frame home".into(),
        })
        .unwrap();
    let person = host
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    host.dispatch(game_transport::Place { person, area })
        .unwrap();
    host.dispatch(Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    host.dispatch(Start).unwrap();
    host.dispatch(game_choice::ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    host.dispatch(game_choice::ConfigureFrameDemand(true))
        .unwrap();
    host.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
        batch_decisions: true,
        commit_batch: 64,
        urgent_immediate: true,
        nonurgent_millis: window,
        ..Default::default()
    }))
    .unwrap();
    (host, person)
}

#[derive(Clone, Serialize, Deserialize)]
struct Sense(PersonId);
impl Request for Sense {
    const KEY: &'static str = "test.frame-urgent-sense";
    type Response = ();
}

#[derive(Clone, Serialize, Deserialize)]
struct HungerSense(PersonId);
impl Request for HungerSense {
    const KEY: &'static str = "test.frame-hunger-sense";
    type Response = ();
}

#[test]
fn deferred_exertion_crossing_hunger_invalidates_the_prepared_decision() {
    fn hunger_builder() -> Builder {
        let mut b = support::builder();
        b.plugin("frame-hunger-sense-test", 1).unwrap();
        b.resource::<Availability>().unwrap();
        b.request::<HungerSense>(|tx, request| {
            if !tx.get::<Availability>()?.closed {
                tx.edit::<Availability>()?.closed = true;
                tx.call(game_physiology::Exert {
                    person: request.0,
                    energy: 1_000_000_000,
                    fatigue: 0,
                })?;
            }
            Ok(())
        })
        .unwrap();
        game_choice::provider(&mut b, "test.hunger-sense-provider", |tx, e| {
            game_choice::defer(tx, HungerSense(e.person))?;
            let hunger = tx.get::<game_physiology::Bodies>()?.people[&e.person].hunger;
            let key = if hunger >= tx.get::<Minds>()?.rules.urgent_hunger {
                "urgent"
            } else {
                "normal"
            };
            game_choice::offer(
                tx,
                e.person,
                key,
                game_choice::motive(1000, 0, 0, 0, 0),
                game_choice::Wait,
            )?;
            Ok(())
        })
        .unwrap();
        b.subscribe::<game_decision_frames::Gather>("test.hunger-sense-cohort", |tx, _| {
            tx.call(game_decision_frames::Batch {
                people: tx.get::<Minds>()?.people.keys().copied().collect(),
            })
        })
        .unwrap();
        b
    }
    let (mut host, person) = setup_with(hunger_builder(), 4 * MINUTE.0);
    support::run(&mut host, SimTime(0));
    assert_eq!(
        host.get::<Minds>().unwrap().people[&person]
            .chosen
            .as_deref(),
        Some("urgent")
    );
    let saved = host.save().unwrap();
    assert_eq!(
        saved,
        hunger_builder().load(&saved, 4).unwrap().save().unwrap()
    );
}

#[test]
fn urgent_deferred_effect_invalidates_first_fill_before_selection() {
    fn urgent_builder() -> Builder {
        let mut b = support::builder();
        b.plugin("frame-urgent-sense-test", 1).unwrap();
        b.resource::<Availability>().unwrap();
        b.request::<Sense>(|tx, request| {
            if !tx.get::<Availability>()?.closed {
                tx.edit::<Availability>()?.closed = true;
                tx.edit::<Availability>()?.performed += 1;
                tx.notify(game_character_clock::Alert { person: request.0 })?;
            }
            Ok(())
        })
        .unwrap();
        game_choice::provider(&mut b, "test.urgent-sense-provider", |tx, e| {
            game_choice::defer(tx, Sense(e.person))?;
            let key = if tx.get::<Availability>()?.closed {
                "new"
            } else {
                "old"
            };
            game_choice::offer(
                tx,
                e.person,
                key,
                game_choice::motive(1000, 0, 0, 0, 0),
                game_choice::Wait,
            )?;
            Ok(())
        })
        .unwrap();
        b.subscribe::<game_decision_frames::Gather>("test.urgent-sense-cohort", |tx, _| {
            tx.call(game_decision_frames::Batch {
                people: tx.get::<Minds>()?.people.keys().copied().collect(),
            })
        })
        .unwrap();
        b
    }
    let (mut host, person) = setup_with(urgent_builder(), 4 * MINUTE.0);
    support::run(&mut host, SimTime(0));
    assert_eq!(host.get::<Availability>().unwrap().performed, 1);
    assert_eq!(
        host.get::<Minds>().unwrap().people[&person]
            .chosen
            .as_deref(),
        Some("new")
    );
    assert!(host.get::<Minds>().unwrap().cohort_urgency.is_empty());
    let saved = host.save().unwrap();
    let loaded = urgent_builder().load(&saved, 4).unwrap();
    assert_eq!(saved, loaded.save().unwrap());
}

#[test]
fn nonurgent_offers_are_reused_and_replay_through_frame_reload() {
    let (mut host, person) = setup(4 * MINUTE.0);
    support::run(&mut host, SimTime(0));
    assert_eq!(host.get::<Collections>().unwrap().0, 1);
    assert!(
        host.get::<Minds>().unwrap().people[&person]
            .considered
            .is_some()
    );
    let checkpoint = host.save().unwrap();
    let mut four = builder().load(&checkpoint, 4).unwrap();
    support::run(&mut host, SimTime(3 * MINUTE.0));
    support::run(&mut four, SimTime(3 * MINUTE.0));
    assert_eq!(host.save().unwrap(), four.save().unwrap());
    assert_eq!(host.get::<Collections>().unwrap().0, 1);
    support::run(&mut host, SimTime(5 * MINUTE.0));
    assert!(host.get::<Collections>().unwrap().0 >= 2);
}

#[test]
fn urgent_injury_refreshes_cached_offers_and_disabling_cleans_up() {
    let (mut host, person) = setup(4 * MINUTE.0);
    support::run(&mut host, SimTime(0));
    assert_eq!(host.get::<Collections>().unwrap().0, 1);
    host.dispatch(game_physiology::Damage { person, amount: 1 })
        .unwrap();
    support::run(&mut host, SimTime(0));
    assert!(host.get::<Collections>().unwrap().0 >= 2);
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    let before = host.save().unwrap();
    tuning.nonurgent_millis = DAY.0 + 1;
    assert!(host.dispatch(game_choice::TuneFrames(tuning)).is_err());
    assert_eq!(host.save().unwrap(), before);
    tuning.nonurgent_millis = 0;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    assert!(
        host.get::<Minds>().unwrap().people[&person]
            .considered
            .is_none()
    );
    assert!(
        host.scheduled()
            .all(|job| job.request.key() != "characters.options-expired")
    );
}

#[test]
fn prepared_cached_decision_survives_reload_before_group_resolution() {
    let (mut host, person) = setup(4 * MINUTE.0);
    support::run(&mut host, SimTime(0));
    host.dispatch(Cue(person)).unwrap();
    host.advance(SimTime(MINUTE.0), 1).unwrap();
    assert!(
        host.scheduled()
            .any(|job| job.request.key() == "characters.resolve-frame")
    );
    let mut loaded = builder().load(&host.save().unwrap(), 4).unwrap();
    support::run(&mut host, SimTime(MINUTE.0));
    support::run(&mut loaded, SimTime(MINUTE.0));
    assert_eq!(host.save().unwrap(), loaded.save().unwrap());
    assert_eq!(host.get::<Collections>().unwrap().0, 1);
}

#[test]
fn urgent_damage_after_preparation_preempts_the_cached_turn() {
    let (mut host, person) = setup(4 * MINUTE.0);
    support::run(&mut host, SimTime(0));
    host.dispatch(Cue(person)).unwrap();
    host.advance(SimTime(MINUTE.0), 1).unwrap();
    assert!(
        host.scheduled()
            .any(|job| job.request.key() == "characters.resolve-frame")
    );
    host.dispatch(game_physiology::Damage { person, amount: 1 })
        .unwrap();
    assert!(
        host.scheduled()
            .any(|job| job.request.key() == "characters.urgent-frame-resume" && job.priority == 48)
    );
    host.advance(SimTime(MINUTE.0), 1).unwrap();
    assert_eq!(host.get::<Collections>().unwrap().0, 2);
    support::run(&mut host, SimTime(MINUTE.0));
    assert_eq!(host.get::<Collections>().unwrap().0, 2);
}

#[test]
fn hunger_crossing_after_preparation_refreshes_on_the_existing_review() {
    let (mut host, person) = setup(4 * MINUTE.0);
    support::run(&mut host, SimTime(0));
    host.dispatch(Cue(person)).unwrap();
    host.advance(SimTime(MINUTE.0), 1).unwrap();
    host.dispatch(game_physiology::Exert {
        person,
        energy: 1_000_000_000,
        fatigue: 0,
    })
    .unwrap();
    let mut loaded = builder().load(&host.save().unwrap(), 4).unwrap();
    support::run(&mut host, SimTime(MINUTE.0));
    support::run(&mut loaded, SimTime(MINUTE.0));
    assert_eq!(host.get::<Collections>().unwrap().0, 2);
    assert_eq!(host.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn reloaded_cached_offers_keep_provider_identity_for_stale_rechecks() {
    fn stale_builder() -> Builder {
        let mut b = support::builder();
        b.plugin("frame-stale-cache-test", 1).unwrap();
        b.resource::<Availability>().unwrap();
        b.request::<Close>(|tx, _| {
            tx.edit::<Availability>()?.closed = true;
            Ok(())
        })
        .unwrap();
        b.request::<Fragile>(|tx, _| {
            tx.edit::<Availability>()?.performed += 1;
            Ok(())
        })
        .unwrap();
        b.request::<Cue>(|tx, request| tx.notify(game_choice::Changed { person: request.0 }))
            .unwrap();
        game_choice::provider(&mut b, "test.stale-cache-provider", |tx, e| {
            game_choice::offer(
                tx,
                e.person,
                "primary",
                game_choice::motive(1100, 0, 0, 0, 0),
                game_choice::Wait,
            )?;
            if !tx.get::<Availability>()?.closed {
                game_choice::offer(
                    tx,
                    e.person,
                    "fragile",
                    game_choice::motive(1000, 0, 0, 0, 0),
                    Fragile,
                )?;
            }
            game_choice::offer(
                tx,
                e.person,
                "fallback",
                game_choice::motive(100, 0, 0, 0, 0),
                game_choice::Wait,
            )?;
            Ok(())
        })
        .unwrap();
        b.subscribe::<game_decision_frames::Gather>("test.stale-cache-cohort", |tx, _| {
            tx.call(game_decision_frames::Batch {
                people: tx.get::<Minds>()?.people.keys().copied().collect(),
            })
        })
        .unwrap();
        b
    }
    let mut host = stale_builder().build(9, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "stale cache".into(),
        })
        .unwrap();
    let person = host
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    host.dispatch(game_transport::Place { person, area })
        .unwrap();
    host.dispatch(Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    host.dispatch(Start).unwrap();
    host.dispatch(game_choice::ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    host.dispatch(game_choice::ConfigureFrameDemand(true))
        .unwrap();
    host.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
        batch_decisions: true,
        commit_batch: 64,
        recheck_conflicts: true,
        nonurgent_millis: 4 * MINUTE.0,
        ..Default::default()
    }))
    .unwrap();
    support::run(&mut host, SimTime(0));
    assert_eq!(
        host.get::<Minds>().unwrap().people[&person]
            .chosen
            .as_deref(),
        Some("primary")
    );
    host.dispatch(Close).unwrap();
    host.dispatch(Cue(person)).unwrap();
    host.advance(SimTime(MINUTE.0), 1).unwrap();
    let mut loaded = stale_builder().load(&host.save().unwrap(), 4).unwrap();
    for simulation in [&mut host, &mut loaded] {
        support::run(simulation, SimTime(MINUTE.0));
        assert_eq!(simulation.get::<Availability>().unwrap().performed, 0);
        assert_eq!(
            simulation.get::<Minds>().unwrap().people[&person].failures,
            0
        );
        assert_eq!(
            simulation.get::<Minds>().unwrap().people[&person]
                .chosen
                .as_deref(),
            Some("fallback")
        );
        assert_eq!(
            simulation
                .get::<game_choice::Offers>()
                .unwrap()
                .frame_conflicts,
            1
        );
    }
    assert_eq!(host.save().unwrap(), loaded.save().unwrap());
}
