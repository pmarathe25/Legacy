mod support;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Circumstances {
    value: i64,
    allowed: bool,
    performed: u64,
}
impl Resource for Circumstances {
    const KEY: &'static str = "test.circumstances";
}
#[derive(Clone, Serialize, Deserialize)]
struct Attempt;
impl Request for Attempt {
    const KEY: &'static str = "test.attempt";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cue {
    person: PersonId,
    value: i64,
    allowed: bool,
}
impl Request for Cue {
    const KEY: &'static str = "test.cue";
    type Response = ();
}
#[test]
fn evidence_does_not_erase_failure_and_a_strong_motive_can_justify_retrying_immediately() {
    let mut b = support::builder();
    b.plugin("failure-test", 1).unwrap();
    b.resource::<Circumstances>().unwrap();
    b.request::<Attempt>(|tx, _| {
        require(tx.get::<Circumstances>()?.allowed, "attempt failed")?;
        tx.edit::<Circumstances>()?.performed += 1;
        Ok(())
    })
    .unwrap();
    b.request::<Cue>(|tx, r| {
        let state = tx.edit::<Circumstances>()?;
        state.value = r.value;
        state.allowed = r.allowed;
        tx.notify(game_choice::Changed { person: r.person })
    })
    .unwrap();
    game_choice::provider(&mut b, "test.opportunity", |tx, e| {
        game_choice::offer(
            tx,
            e.person,
            "uncertain-opportunity",
            game_choice::motive(0, 0, tx.get::<Circumstances>()?.value, 0, 0),
            Attempt,
        )?;
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Home".into(),
        })
        .unwrap();
    let p = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person: p, area })
        .unwrap();
    h.dispatch(game_characters::Register {
        person: p,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(Cue {
        person: p,
        value: 2,
        allowed: false,
    })
    .unwrap();
    h.dispatch(game_characters::Start).unwrap();
    support::run(&mut h, SimTime(MINUTE.0));
    assert_eq!(
        h.get::<game_characters::Minds>().unwrap().people[&p].failures,
        1
    );
    let remembered = h.get::<game_characters::Minds>().unwrap().people[&p]
        .retry_after
        .clone();
    h.dispatch(Cue {
        person: p,
        value: 2,
        allowed: true,
    })
    .unwrap();
    support::run(&mut h, SimTime(MINUTE.0));
    assert_eq!(h.get::<Circumstances>().unwrap().performed, 0);
    assert_eq!(
        h.get::<game_characters::Minds>().unwrap().people[&p].retry_after,
        remembered
    );
    h.dispatch(Cue {
        person: p,
        value: 10000,
        allowed: true,
    })
    .unwrap();
    support::run(&mut h, SimTime(MINUTE.0));
    assert_eq!(h.get::<Circumstances>().unwrap().performed, 1);
    assert!(
        h.get::<game_characters::Minds>().unwrap().people[&p]
            .retry_after
            .is_empty()
    );
}

#[derive(Clone, Serialize, Deserialize)]
struct Delayed(u8);
impl Request for Delayed {
    const KEY: &'static str = "test.delayed";
    type Response = game_choice::Outcome;
}
#[derive(Clone, Serialize, Deserialize)]
struct ResolvedAgain(game_choice::Resolved);
impl Request for ResolvedAgain {
    const KEY: &'static str = "test.repeat-outcome";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct ImmediateOutcome(bool);
impl Request for ImmediateOutcome {
    const KEY: &'static str = "test.immediate-outcome";
    type Response = game_choice::Outcome;
}

#[test]
fn reconsider_keeps_action_evidence_while_a_rejected_action_rolls_it_back() {
    for reject in [false, true] {
        let mut b = support::builder();
        b.plugin("immediate-outcome-test", 1).unwrap();
        b.resource::<Circumstances>().unwrap();
        b.request::<ImmediateOutcome>(|tx, r| {
            tx.edit::<Circumstances>()?.performed += 1;
            if r.0 {
                return Err(Error::Rejected("failed after changing evidence".into()));
            }
            Ok(game_choice::Outcome::Reconsider)
        })
        .unwrap();
        game_choice::provider(&mut b, "test.immediate-outcome", move |tx, e| {
            game_choice::offer(
                tx,
                e.person,
                "try-outcome",
                game_choice::motive(100, 0, 0, 0, 0),
                ImmediateOutcome(reject),
            )?;
            Ok(())
        })
        .unwrap();
        let mut host = b.build(1, 1).unwrap();
        host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
            .unwrap();
        let area = host
            .dispatch(game_land::CreateArea {
                name: "Home".into(),
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
        host.dispatch(game_characters::Register {
            person,
            mind: Default::default(),
        })
        .unwrap();
        host.dispatch(game_characters::Start).unwrap();
        support::run(&mut host, SimTime(MINUTE.0));
        assert_eq!(
            host.get::<Circumstances>().unwrap().performed,
            u64::from(!reject)
        );
        assert_eq!(
            host.get::<game_characters::Minds>().unwrap().people[&person].failures,
            1
        );
    }
}

#[test]
fn review_stops_after_eight_failed_selected_alternatives() {
    let mut b = support::builder();
    b.plugin("attempt-limit-test", 1).unwrap();
    b.resource::<Circumstances>().unwrap();
    b.request::<ImmediateOutcome>(|tx, _| {
        tx.edit::<Circumstances>()?.performed += 1;
        Ok(game_choice::Outcome::Reconsider)
    })
    .unwrap();
    game_choice::provider(&mut b, "test.attempt-limit", |tx, e| {
        for index in 0..9 {
            game_choice::offer(
                tx,
                e.person,
                format!("alternative-{index}"),
                game_choice::motive(100 + index, 0, 0, 0, 0),
                ImmediateOutcome(false),
            )?;
        }
        Ok(())
    })
    .unwrap();
    let mut host = b.build(1, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "Home".into(),
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
    host.dispatch(game_characters::Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    host.dispatch(game_characters::Start).unwrap();
    support::run(&mut host, SimTime(MINUTE.0));
    assert_eq!(host.get::<Circumstances>().unwrap().performed, 8);
    assert_eq!(
        host.get::<game_characters::Minds>().unwrap().people[&person].failures,
        8
    );
}
#[test]
fn delayed_rejection_and_reconsideration_are_learned_once_without_penalizing_success() {
    for mode in [0, 1, 2] {
        let builder = || {
            let mut b = support::builder();
            b.plugin("delayed-test", 1).unwrap();
            b.request::<Delayed>(|_, r| match r.0 {
                0 => Err(Error::Rejected("later rejection".into())),
                1 => Ok(game_choice::Outcome::Reconsider),
                _ => Ok(game_choice::Outcome::Performed),
            })
            .unwrap();
            b.request::<ResolvedAgain>(|tx, r| tx.notify(r.0)).unwrap();
            game_choice::provider(&mut b, "test.delayed-offer", move |tx, e| {
                game_choice::offer(
                    tx,
                    e.person,
                    "other-interest",
                    game_choice::motive(0, 50, 0, 0, 0),
                    game_choice::Wait,
                )?;
                if tx.get::<Actions>()?.0.contains_key(&e.person) {
                    return Ok(());
                }
                game_choice::offer(
                    tx,
                    e.person,
                    "delayed",
                    game_choice::motive(0, 100, 0, 0, 0),
                    game_choice::Undertake {
                        person: e.person,
                        kind: "deliberate".into(),
                        duration: HOUR,
                        request: Pending::new(Delayed(mode))?,
                    },
                )?;
                Ok(())
            })
            .unwrap();
            b
        };
        let mut h = builder().build(1, 1).unwrap();
        h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
            .unwrap();
        let area = h
            .dispatch(game_land::CreateArea {
                name: "Home".into(),
            })
            .unwrap();
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        h.dispatch(game_characters::Register {
            person: p,
            mind: Default::default(),
        })
        .unwrap();
        h.dispatch(game_characters::Start).unwrap();
        support::run(&mut h, SimTime(MINUTE.0));
        let action = h.get::<Actions>().unwrap().0[&p].id;
        support::run(&mut h, SimTime(10 * MINUTE.0));
        assert_eq!(h.get::<Actions>().unwrap().0[&p].id, action);
        assert_eq!(
            h.get::<game_characters::Minds>().unwrap().people[&p]
                .expectation
                .unwrap()
                .goal,
            100
        );
        let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
        for host in [&mut h, &mut loaded] {
            support::run(host, SimTime(HOUR.0 + MINUTE.0));
            let failures = host.get::<game_characters::Minds>().unwrap().people[&p].failures;
            assert_eq!(failures, if mode == 2 { 0 } else { 1 });
            let engagement = host.get::<game_characters::Minds>().unwrap().people[&p].engagement;
            host.dispatch(ResolvedAgain(game_choice::Resolved {
                person: p,
                action,
                outcome: game_choice::Outcome::Reconsider,
            }))
            .unwrap();
            assert_eq!(
                host.get::<game_characters::Minds>().unwrap().people[&p].failures,
                failures
            );
            assert_eq!(
                host.get::<game_characters::Minds>().unwrap().people[&p].engagement,
                engagement
            );
        }
        assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    }
}
