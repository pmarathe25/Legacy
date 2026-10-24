mod support;
use game_characters::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Work {
    reviews: u64,
    completed: u64,
    launches: u64,
}
impl Resource for Work {
    const KEY: &'static str = "test.attention-work";
}
#[derive(Clone, Serialize, Deserialize)]
struct Step(PersonId);
impl Request for Step {
    const KEY: &'static str = "test.attention-step";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Finish(PersonId);
impl Request for Finish {
    const KEY: &'static str = "test.attention-finish";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel(ScheduleId);
impl Request for Cancel {
    const KEY: &'static str = "test.attention-cancel";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Continue(PersonId);
impl Request for Continue {
    const KEY: &'static str = "test.attention-continue";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
struct Evidence(PersonId);
#[derive(Clone, Serialize, Deserialize)]
struct UrgentEvidence(PersonId);
impl Request for UrgentEvidence {
    const KEY: &'static str = "test.attention-urgent-evidence";
    type Response = ();
}
impl Request for Evidence {
    const KEY: &'static str = "test.attention-evidence";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = support::builder();
    b.plugin("attention-test", 1).unwrap();
    b.resource::<Work>().unwrap();
    b.request::<Step>(|tx, r| {
        let until = tx.now().after(MINUTE)?;
        let job = tx.schedule(until, 20, Finish(r.0))?;
        tx.start_action(r.0, "craft", Some(until), Cancel(job))?;
        tx.edit::<Work>()?.launches += 1;
        Ok(())
    })
    .unwrap();
    b.request::<Cancel>(|tx, r| {
        tx.unschedule(r.0);
        Ok(())
    })
    .unwrap();
    b.request::<Finish>(|tx, r| {
        let action = tx.get::<Actions>()?.0[&r.0].id;
        tx.finish_action(r.0, "craft")?;
        tx.edit::<Work>()?.completed += 1;
        tx.notify(game_choice::GoalReady {
            person: r.0,
            action,
            key: "work:test".into(),
            request: Pending::immediate(Continue(r.0)),
        })
    })
    .unwrap();
    b.request::<Continue>(|tx, r| {
        if tx.get::<Work>()?.completed >= 2 {
            return Ok(false);
        }
        tx.call(Step(r.0))?;
        Ok(true)
    })
    .unwrap();
    b.request::<Evidence>(|tx, r| tx.notify(game_choice::Changed { person: r.0 }))
        .unwrap();
    b.request::<UrgentEvidence>(|tx, r| tx.notify(Alert { person: r.0 }))
        .unwrap();
    game_choice::serial_provider(&mut b, "test.attention", |tx, e| {
        tx.edit::<Work>()?.reviews += 1;
        if tx.get::<Work>()?.completed < 2 {
            game_choice::offer(
                tx,
                e.person,
                "work:test",
                game_choice::motive(10000, 0, 0, 0, 0),
                Step(e.person),
            )?;
        }
        Ok(())
    })
    .unwrap();
    b
}
fn world(execution: bool) -> (Host, PersonId) {
    world_with_busy_attention(execution, false)
}
fn world_with_busy_attention(execution: bool, busy_attention: bool) -> (Host, PersonId) {
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let p = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person: p, area })
        .unwrap();
    if busy_attention {
        h.dispatch(Configure(Rules {
            busy_attention: true,
            ..Default::default()
        }))
        .unwrap();
    }
    h.dispatch(Register {
        person: p,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(ConfigureAttention(Some(AttentionRates {
        ordinary: SimDuration(5 * MINUTE.0),
        evidence: SimDuration(2 * MINUTE.0),
    })))
    .unwrap();
    h.dispatch(game_choice::ConfigureExecution(execution))
        .unwrap();
    h.dispatch(Start).unwrap();
    support::run(&mut h, SimTime(MINUTE.0));
    assert_eq!(h.get::<Work>().unwrap().reviews, 1);
    (h, p)
}
#[test]
fn busy_attention_finishes_work_and_admits_idle_choices_without_waiting_for_the_floor() {
    let (mut h, p) = world_with_busy_attention(false, true);
    for _ in 0..100 {
        h.dispatch(Evidence(p)).unwrap();
    }
    support::run(&mut h, SimTime(MINUTE.0 + MINUTE.0 / 2));
    assert_eq!(
        h.get::<Work>().unwrap().reviews,
        1,
        "busy chatter coalesces"
    );
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        support::run(host, SimTime(2 * MINUTE.0));
        assert_eq!(host.get::<Work>().unwrap().completed, 1);
        assert_eq!(host.get::<Work>().unwrap().reviews, 2);
        assert_eq!(host.get::<Work>().unwrap().launches, 2);
        support::run(host, SimTime(3 * MINUTE.0));
        assert_eq!(host.get::<Work>().unwrap().completed, 2);
        assert!(!host.get::<Actions>().unwrap().0.contains_key(&p));
        let before = host.get::<Work>().unwrap().reviews;
        for _ in 0..100 {
            host.dispatch(Evidence(p)).unwrap();
        }
        support::run(host, SimTime(3 * MINUTE.0));
        assert_eq!(
            host.get::<Work>().unwrap().reviews,
            before + 1,
            "idle opportunity stays prompt"
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn urgent_external_evidence_interrupts_busy_work_before_its_ordinary_attention_deadline() {
    let (mut h, p) = world_with_busy_attention(false, true);
    h.dispatch(Evidence(p)).unwrap();
    let now = SimTime(MINUTE.0 + MINUTE.0 / 2);
    support::run(&mut h, now);
    assert_eq!(h.get::<Work>().unwrap().reviews, 1);
    assert!(h.get::<Actions>().unwrap().0.contains_key(&p));
    h.dispatch(UrgentEvidence(p)).unwrap();
    support::run(&mut h, now);
    assert_eq!(h.get::<Work>().unwrap().reviews, 2);
}
#[test]
fn work_steps_advance_on_completion_without_postponing_the_planning_deadline() {
    let (mut h, p) = world(true);
    assert_eq!(
        h.advance(SimTime(2 * MINUTE.0), 1).unwrap().stop,
        Stop::Budget
    );
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        support::run(host, SimTime(2 * MINUTE.0));
        assert_eq!(host.get::<Work>().unwrap().reviews, 1);
        assert_eq!(host.get::<Work>().unwrap().launches, 2);
        assert_eq!(
            host.get::<Actions>().unwrap().0[&p].until,
            Some(SimTime(3 * MINUTE.0))
        );
        support::run(host, SimTime(3 * MINUTE.0));
        assert_eq!(host.get::<Work>().unwrap().completed, 2);
        assert_eq!(
            host.get::<Work>().unwrap().reviews,
            2,
            "goal exhaustion prompts planning now"
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn evidence_uses_its_earlier_rate_and_injury_advances_that_pending_review() {
    let (mut h, p) = world(false);
    for _ in 0..100 {
        h.dispatch(Evidence(p)).unwrap();
    }
    support::run(&mut h, SimTime(2 * MINUTE.0));
    assert_eq!(h.get::<Work>().unwrap().reviews, 1);
    h.dispatch(game_physiology::Damage {
        person: p,
        amount: 1,
    })
    .unwrap();
    support::run(&mut h, SimTime(2 * MINUTE.0));
    assert_eq!(h.get::<Work>().unwrap().reviews, 2);
    support::run(&mut h, SimTime(3 * MINUTE.0));
    assert_eq!(
        h.get::<Work>().unwrap().reviews,
        2,
        "old evidence callback was replaced"
    );
}
#[test]
fn abandoning_or_disabling_execution_invalidates_an_already_queued_continuation() {
    for abandon in [false, true] {
        let (mut h, p) = world(true);
        h.advance(SimTime(2 * MINUTE.0), 1).unwrap();
        if abandon {
            h.dispatch(Abandon { person: p }).unwrap();
        } else {
            h.dispatch(game_choice::ConfigureExecution(false)).unwrap();
        }
        support::run(&mut h, SimTime(2 * MINUTE.0));
        assert_eq!(h.get::<Work>().unwrap().launches, 1);
        assert!(!h.get::<Actions>().unwrap().0.contains_key(&p));
    }
}
#[test]
fn attention_reconfiguration_moves_pending_deadlines_and_rejects_incompatible_modes() {
    let (mut h, p) = world(false);
    h.dispatch(Evidence(p)).unwrap();
    support::run(&mut h, SimTime(2 * MINUTE.0));
    h.dispatch(ConfigureAttention(Some(AttentionRates {
        ordinary: MINUTE,
        evidence: MINUTE,
    })))
    .unwrap();
    support::run(&mut h, SimTime(2 * MINUTE.0));
    assert_eq!(h.get::<Work>().unwrap().reviews, 2);
    h.dispatch(game_choice::ConfigureExecution(true)).unwrap();
    let before = h.save().unwrap();
    assert!(h.dispatch(ConfigureAttention(None)).is_err());
    assert_eq!(h.save().unwrap(), before);
    assert!(
        h.dispatch(game_choice::ConfigureFrames {
            interval: Some(MINUTE)
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
    h.dispatch(game_choice::ConfigureExecution(false)).unwrap();
    h.dispatch(ConfigureAttention(None)).unwrap();
    support::run(&mut h, SimTime(2 * MINUTE.0));
}

#[test]
fn enabling_busy_attention_releases_an_idle_actor_from_an_old_pending_floor() {
    let (mut h, p) = world(false);
    h.dispatch(Evidence(p)).unwrap();
    support::run(&mut h, SimTime(2 * MINUTE.0));
    assert_eq!(h.get::<Work>().unwrap().reviews, 1);
    assert!(!h.get::<Actions>().unwrap().0.contains_key(&p));
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        host.dispatch(ConfigureBusyAttention(true)).unwrap();
        support::run(host, SimTime(2 * MINUTE.0));
        assert_eq!(host.get::<Work>().unwrap().reviews, 2);
        assert_eq!(host.get::<Work>().unwrap().launches, 2);
        assert!(host.get::<Minds>().unwrap().rules.busy_attention);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
