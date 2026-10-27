mod support;
use game_characters::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Serialize, Deserialize)]
struct Cue(PersonId);
impl Request for Cue {
    const KEY: &'static str = "test.tier-cue";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct AbortUrgency(PersonId);
impl Request for AbortUrgency {
    const KEY: &'static str = "test.tier-abort-urgency";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct StartLong(PersonId);
impl Request for StartLong {
    const KEY: &'static str = "test.tier-start-long";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct CancelLong;
impl Request for CancelLong {
    const KEY: &'static str = "test.tier-cancel-long";
    type Response = ();
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Pulse(bool);
impl Resource for Pulse {
    const KEY: &'static str = "test.tier-pulse";
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct PersistentPulse(bool);
impl Resource for PersistentPulse {
    const KEY: &'static str = "test.tier-persistent-pulse";
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct CrossAlerts(Option<(PersonId, PersonId)>);
impl Resource for CrossAlerts {
    const KEY: &'static str = "test.tier-cross-alerts";
}
#[derive(Clone, Serialize, Deserialize)]
struct ArmCross(PersonId, PersonId);
impl Request for ArmCross {
    const KEY: &'static str = "test.tier-arm-cross";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct FireOther(PersonId);
impl Request for FireOther {
    const KEY: &'static str = "test.tier-fire-other";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct ArmPulse;
impl Request for ArmPulse {
    const KEY: &'static str = "test.tier-arm-pulse";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct ArmPersistentPulse;
impl Request for ArmPersistentPulse {
    const KEY: &'static str = "test.tier-arm-persistent-pulse";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct FirePulse(PersonId);
impl Request for FirePulse {
    const KEY: &'static str = "test.tier-fire-pulse";
    type Response = ();
}

fn builder(full: Arc<AtomicUsize>, fast: Arc<AtomicUsize>) -> Builder {
    let mut b = support::builder();
    b.plugin("test.two-tier", 1).unwrap();
    b.resource::<Pulse>().unwrap();
    b.resource::<PersistentPulse>().unwrap();
    b.resource::<CrossAlerts>().unwrap();
    b.request::<ArmCross>(|tx, r| {
        tx.edit::<CrossAlerts>()?.0 = Some((r.0, r.1));
        Ok(())
    })
    .unwrap();
    b.request::<FireOther>(|tx, r| tx.notify(Alert { person: r.0 }))
        .unwrap();
    b.request::<ArmPulse>(|tx, _| {
        tx.edit::<Pulse>()?.0 = true;
        Ok(())
    })
    .unwrap();
    b.request::<FirePulse>(|tx, request| {
        if !tx.get::<PersistentPulse>()?.0 {
            tx.edit::<Pulse>()?.0 = false;
        }
        tx.notify(Alert { person: request.0 })
    })
    .unwrap();
    b.request::<ArmPersistentPulse>(|tx, _| {
        tx.edit::<PersistentPulse>()?.0 = true;
        tx.edit::<Pulse>()?.0 = true;
        Ok(())
    })
    .unwrap();
    b.request::<Cue>(|tx, cue| tx.notify(game_choice::Changed { person: cue.0 }))
        .unwrap();
    b.request::<AbortUrgency>(|tx, cue| {
        tx.notify(Alert { person: cue.0 })?;
        Err(Error::Rejected("test rollback".into()))
    })
    .unwrap();
    b.request::<CancelLong>(|_, _| Ok(())).unwrap();
    b.request::<StartLong>(|tx, request| {
        tx.start_action(
            request.0,
            "test-long",
            Some(tx.now().after(SimDuration(10 * MINUTE.0))?),
            CancelLong,
        )?;
        tx.notify(game_choice::Changed { person: request.0 })
    })
    .unwrap();
    game_choice::deliberative_provider(&mut b, "test.full-discovery", move |tx, e| {
        full.fetch_add(1, Ordering::SeqCst);
        let score = if tx.get::<Actions>()?.0.contains_key(&e.person) {
            -1
        } else {
            1
        };
        game_choice::offer(
            tx,
            e.person,
            "full-interest",
            game_choice::motive(score, 0, 0, 0, 0),
            game_choice::Wait,
        )?;
        Ok(())
    })
    .unwrap();
    game_choice::provider(&mut b, "test.safety", move |tx, e| {
        fast.fetch_add(1, Ordering::SeqCst);
        if tx.get::<Pulse>()?.0 {
            game_choice::defer(tx, FirePulse(e.person))?;
        }
        if let Some((a, b)) = tx.get::<CrossAlerts>()?.0 {
            if e.person == a {
                game_choice::defer(tx, FireOther(b))?;
            }
            if e.person == b {
                game_choice::defer(tx, FireOther(a))?;
            }
        }
        let score = if tx.get::<Actions>()?.0.contains_key(&e.person) {
            -1
        } else {
            2
        };
        game_choice::offer(
            tx,
            e.person,
            "safety",
            game_choice::motive(score, 0, 0, 0, 0),
            game_choice::Wait,
        )?;
        Ok(())
    })
    .unwrap();
    game_autonomy_frames::register(&mut b).unwrap();
    b
}

fn setup(minimum: usize) -> (Host, PersonId, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    setup_with_review(minimum, MINUTE)
}
fn setup_with_review(
    minimum: usize,
    review: SimDuration,
) -> (Host, PersonId, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let full = Arc::new(AtomicUsize::new(0));
    let fast = Arc::new(AtomicUsize::new(0));
    let mut host = builder(full.clone(), fast.clone()).build(7, 1).unwrap();
    host.dispatch(Configure(Rules {
        review,
        ..Default::default()
    }))
    .unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "tier home".into(),
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
        minimum_cohort: minimum,
        commit_batch: 64,
        urgent_immediate: true,
        full_review_millis: 4 * MINUTE.0,
        ..Default::default()
    }))
    .unwrap();
    (host, person, full, fast)
}

#[test]
fn fast_reviews_preserve_deadline_while_urgent_evidence_is_immediate() {
    for minimum in [0, 2] {
        let (mut host, person, full, fast) = setup(minimum);
        support::run(&mut host, SimTime(0));
        let first = host.get::<Minds>().unwrap().people[&person]
            .frame_tier
            .unwrap();
        assert_eq!(first.full_due, Some(SimTime(4 * MINUTE.0)));
        let baseline = full.load(Ordering::SeqCst);
        assert!(baseline > 0);
        host.dispatch(Cue(person)).unwrap();
        support::run(&mut host, SimTime(MINUTE.0));
        assert_eq!(full.load(Ordering::SeqCst), baseline);
        assert!(fast.load(Ordering::SeqCst) >= 2);
        let before = host.save().unwrap();
        assert!(host.dispatch(AbortUrgency(person)).is_err());
        assert_eq!(host.save().unwrap(), before);
        host.dispatch(game_physiology::Damage { person, amount: 1 })
            .unwrap();
        let marked = host.get::<Minds>().unwrap().people[&person]
            .frame_tier
            .unwrap();
        assert!(marked.generation > first.generation);
        assert_eq!(marked.full_due, first.full_due);
        let old_fast = fast.load(Ordering::SeqCst);
        support::run(&mut host, SimTime(MINUTE.0));
        assert!(fast.load(Ordering::SeqCst) > old_fast);
        assert_eq!(full.load(Ordering::SeqCst), baseline);
        support::run(&mut host, SimTime(4 * MINUTE.0));
        assert!(full.load(Ordering::SeqCst) > baseline);
        assert!(
            host.get::<Minds>().unwrap().people[&person]
                .frame_tier
                .unwrap()
                .full_due
                .unwrap()
                > SimTime(4 * MINUTE.0)
        );
    }
}

#[test]
fn urgent_cue_after_preparation_preempts_the_group_and_replays_after_reload() {
    let (mut host, person, full, _) = setup(0);
    support::run(&mut host, SimTime(0));
    let full_count = full.load(Ordering::SeqCst);
    host.dispatch(Cue(person)).unwrap();
    host.advance(SimTime(MINUTE.0), 1).unwrap();
    assert!(
        host.scheduled()
            .any(|j| j.request.key() == "characters.resolve-frame")
    );
    host.dispatch(game_physiology::Damage { person, amount: 1 })
        .unwrap();
    assert!(
        host.scheduled()
            .any(|j| j.request.key() == "characters.urgent-frame-resume" && j.priority == 48)
    );
    let save = host.save().unwrap();
    let mut loaded = builder(Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)))
        .load(&save, 4)
        .unwrap();
    support::run(&mut host, SimTime(MINUTE.0));
    support::run(&mut loaded, SimTime(MINUTE.0));
    assert_eq!(host.save().unwrap(), loaded.save().unwrap());
    assert_eq!(full.load(Ordering::SeqCst), full_count);
}

#[test]
fn a_long_action_does_not_prevent_a_full_review_at_its_deadline() {
    let (mut host, person, full, _) = setup(0);
    support::run(&mut host, SimTime(0));
    let initial = full.load(Ordering::SeqCst);
    host.dispatch(StartLong(person)).unwrap();
    support::run(&mut host, SimTime(2 * MINUTE.0));
    assert_eq!(host.get::<Actions>().unwrap().0[&person].kind, "test-long");
    assert_eq!(full.load(Ordering::SeqCst), initial);
    support::run(&mut host, SimTime(4 * MINUTE.0));
    assert!(full.load(Ordering::SeqCst) > initial);
}

#[test]
fn full_deadline_is_not_rounded_to_the_slower_ordinary_review_grid() {
    let (mut host, person, full, _) = setup_with_review(0, SimDuration(12 * HOUR.0));
    support::run(&mut host, SimTime(0));
    let initial = full.load(Ordering::SeqCst);
    assert!(initial > 0);
    host.dispatch(StartLong(person)).unwrap();
    support::run(&mut host, SimTime(2 * MINUTE.0));
    assert_eq!(host.get::<Actions>().unwrap().0[&person].kind, "test-long");
    assert_eq!(full.load(Ordering::SeqCst), initial);
    support::run(&mut host, SimTime(4 * MINUTE.0));
    assert!(full.load(Ordering::SeqCst) > initial);
}

#[test]
fn new_evidence_wakes_fast_review_before_a_later_full_deadline() {
    let (mut host, person, full, fast) = setup(0);
    support::run(&mut host, SimTime(MINUTE.0));
    let full_before = full.load(Ordering::SeqCst);
    let fast_before = fast.load(Ordering::SeqCst);
    assert!(
        host.get::<Minds>().unwrap().people[&person]
            .frame_tier
            .unwrap()
            .full_due
            .unwrap()
            > SimTime(MINUTE.0)
    );
    host.dispatch(Cue(person)).unwrap();
    support::run(&mut host, SimTime(2 * MINUTE.0));
    assert!(fast.load(Ordering::SeqCst) > fast_before);
    assert_eq!(full.load(Ordering::SeqCst), full_before);
}

#[test]
fn urgent_evidence_during_an_urgent_review_schedules_another_review() {
    let (mut host, person, _, fast) = setup(0);
    support::run(&mut host, SimTime(0));
    host.dispatch(ArmPulse).unwrap();
    host.dispatch(game_physiology::Damage { person, amount: 1 })
        .unwrap();
    assert!(
        host.scheduled()
            .any(|j| j.request.key() == "characters.urgent-frame-resume")
    );
    let before = fast.load(Ordering::SeqCst);
    host.advance(SimTime(0), 1).unwrap();
    assert!(!host.get::<Pulse>().unwrap().0);
    assert!(
        host.scheduled()
            .any(|j| j.request.key() == "characters.urgent-frame-resume")
    );
    support::run(&mut host, SimTime(0));
    assert!(fast.load(Ordering::SeqCst) >= before + 2);
    assert!(
        !host
            .scheduled()
            .any(|j| j.request.key() == "characters.urgent-frame-resume")
    );
}

#[test]
fn persistent_urgent_evidence_yields_to_the_next_frame() {
    let (mut host, person, _, fast) = setup(0);
    support::run(&mut host, SimTime(0));
    host.dispatch(ArmPersistentPulse).unwrap();
    host.dispatch(game_physiology::Damage { person, amount: 1 })
        .unwrap();
    let before = fast.load(Ordering::SeqCst);
    assert_eq!(host.advance(SimTime(0), 100).unwrap().stop, Stop::Target);
    assert!(fast.load(Ordering::SeqCst) <= before + 3);
    assert!(host.get::<Pulse>().unwrap().0);
    assert!(
        host.get::<Minds>()
            .unwrap()
            .schedule
            .waking
            .get(&person)
            .is_some_and(|at| *at > SimTime(0))
    );
    assert_eq!(
        host.advance(SimTime(MINUTE.0), 100).unwrap().stop,
        Stop::Target
    );
}

#[test]
fn cross_alerts_between_two_people_cannot_starve_the_clock() {
    let (mut host, first, _, _) = setup(0);
    support::run(&mut host, SimTime(0));
    let area = game_transport::area(host.get().unwrap(), first).unwrap();
    let second = host
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    host.dispatch(game_transport::Place {
        person: second,
        area,
    })
    .unwrap();
    host.dispatch(Register {
        person: second,
        mind: Default::default(),
    })
    .unwrap();
    support::run(&mut host, SimTime(0));
    host.dispatch(ArmCross(first, second)).unwrap();
    host.dispatch(game_physiology::Damage {
        person: first,
        amount: 1,
    })
    .unwrap();
    assert_eq!(host.advance(SimTime(0), 100).unwrap().stop, Stop::Target);
    assert!(
        host.get::<Minds>()
            .unwrap()
            .schedule
            .waking
            .iter()
            .any(|(_, at)| *at > SimTime(0))
    );
    assert_eq!(
        host.advance(SimTime(MINUTE.0), 100).unwrap().stop,
        Stop::Target
    );
}

#[test]
fn changing_or_disabling_the_interval_invalidates_the_old_deadline() {
    let (mut host, person, _, _) = setup(0);
    support::run(&mut host, SimTime(0));
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    let before = host.save().unwrap();
    tuning.meal_arbitration = true;
    assert!(host.dispatch(game_choice::TuneFrames(tuning)).is_err());
    assert_eq!(host.save().unwrap(), before);
    tuning.meal_arbitration = false;
    tuning.full_review_millis = 2 * MINUTE.0;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    support::run(&mut host, SimTime(0));
    assert_eq!(
        host.get::<Minds>().unwrap().people[&person]
            .frame_tier
            .unwrap()
            .full_due,
        Some(SimTime(2 * MINUTE.0))
    );
    tuning.full_review_millis = 0;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    assert!(
        host.get::<Minds>().unwrap().people[&person]
            .frame_tier
            .unwrap()
            .full_due
            .is_none()
    );
}
