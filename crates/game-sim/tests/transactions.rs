use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Counter {
    amount: u64,
}
impl Resource for Counter {
    const KEY: &'static str = "test.counter";
    fn validate(&self) -> Result<()> {
        require(self.amount < 100, "counter limit")
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Add {
    amount: u64,
    fail: bool,
}
impl Request for Add {
    const KEY: &'static str = "test.add";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
struct Schedule {
    at: SimTime,
}
impl Request for Schedule {
    const KEY: &'static str = "test.schedule";
    type Response = ScheduleId;
}
#[derive(Clone, Serialize, Deserialize)]
struct Attempt;
impl Request for Attempt {
    const KEY: &'static str = "test.attempt";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct OwnedAlternative {
    rollback: bool,
    reject_hook: bool,
}
impl Request for OwnedAlternative {
    const KEY: &'static str = "test.owned-alternative";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
struct RejectOuterAlternative;
impl Request for RejectOuterAlternative {
    const KEY: &'static str = "test.reject-outer-alternative";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct ForeignAlternative;
impl Request for ForeignAlternative {
    const KEY: &'static str = "test.foreign-alternative";
    type Response = u64;
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct HookCounter(u64);
impl Resource for HookCounter {
    const KEY: &'static str = "test.hook-counter";
}
#[derive(Serialize, Deserialize)]
struct Added {
    amount: u64,
}
impl Event for Added {
    const KEY: &'static str = "test.added";
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.plugin("counter", 1).unwrap();
    b.resource::<Counter>().unwrap();
    b.request::<Add>(|tx, r| {
        tx.edit::<Counter>()?.amount += r.amount;
        let id = tx.allocate()?;
        tx.emit(Added { amount: r.amount }, vec![PersonId(1)])?;
        require(!r.fail, "forced failure")?;
        Ok(id)
    })
    .unwrap();
    b.request::<Schedule>(|tx, r| {
        tx.schedule(
            r.at,
            10,
            Add {
                amount: 1,
                fail: false,
            },
        )
    })
    .unwrap();
    b.request::<Attempt>(|tx, _| {
        tx.call(Add {
            amount: 2,
            fail: false,
        })?;
        assert!(
            tx.attempt(Add {
                amount: 10,
                fail: true
            })
            .is_err()
        );
        tx.call(Add {
            amount: 3,
            fail: false,
        })?;
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn failures_rollback_resources_events_ids_jobs_and_validation() {
    let mut h = builder().build(1, 1).unwrap();
    let before = h.save().unwrap();
    assert!(
        h.submit(
            1,
            Add {
                amount: 5,
                fail: true
            }
        )
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
    assert!(
        h.submit(
            1,
            Add {
                amount: 100,
                fail: false
            }
        )
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
    assert_eq!(
        h.submit(
            1,
            Add {
                amount: 5,
                fail: false
            }
        )
        .unwrap(),
        1
    );
}
#[test]
fn nested_failed_attempt_does_not_damage_prior_or_later_work() {
    let mut h = builder().build(1, 1).unwrap();
    h.dispatch(Attempt).unwrap();
    assert_eq!(h.get::<Counter>().unwrap().amount, 5);
    assert_eq!(h.notices().count(), 2);
    assert_eq!(
        h.dispatch(Add {
            amount: 1,
            fail: false
        })
        .unwrap(),
        3
    );
}

#[test]
fn owned_alternative_preserves_hooks_and_rolls_back_all_nested_effects() {
    let mut b = builder();
    b.request::<OwnedAlternative>(|tx, request| {
        tx.attempt_owned_request(
            Add {
                amount: 3,
                fail: request.reject_hook,
            },
            |tx, r| {
                tx.edit::<Counter>()?.amount += r.amount;
                let id = tx.allocate()?;
                tx.emit(Added { amount: r.amount }, vec![])?;
                tx.schedule(
                    tx.now().after(MINUTE)?,
                    1,
                    Add {
                        amount: 1,
                        fail: false,
                    },
                )?;
                Ok(if request.rollback {
                    AttemptOutcome::Rollback(id)
                } else {
                    AttemptOutcome::Commit(id)
                })
            },
        )
    })
    .unwrap();
    b.request::<RejectOuterAlternative>(|tx, _| {
        tx.call(OwnedAlternative {
            rollback: false,
            reject_hook: false,
        })?;
        Err(Error::Rejected("outer failure".into()))
    })
    .unwrap();
    b.plugin("test-hook-owner", 1).unwrap();
    b.resource::<HookCounter>().unwrap();
    b.before::<Add>("test.hook", |tx, r| {
        tx.edit::<HookCounter>()?.0 += 1;
        tx.allocate()?;
        tx.schedule(
            tx.now().after(MINUTE)?,
            1,
            Add {
                amount: 1,
                fail: false,
            },
        )?;
        require(!r.fail, "offered food declined")
    })
    .unwrap();
    b.request::<ForeignAlternative>(|tx, _| {
        tx.attempt_owned_request(
            Add {
                amount: 3,
                fail: false,
            },
            |_, _| Ok(AttemptOutcome::Commit(99)),
        )
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    let before = h.save().unwrap();
    assert!(h.dispatch(ForeignAlternative).is_err());
    assert_eq!(h.save().unwrap(), before);
    assert!(h.dispatch(RejectOuterAlternative).is_err());
    assert_eq!(h.save().unwrap(), before);
    assert!(
        matches!(h.dispatch(OwnedAlternative { rollback: false, reject_hook: true }),
        Err(Error::Rejected(reason)) if reason == "offered food declined")
    );
    assert_eq!(h.save().unwrap(), before);
    assert_eq!(
        h.dispatch(OwnedAlternative {
            rollback: true,
            reject_hook: false
        })
        .unwrap(),
        3
    );
    assert_eq!(h.get::<Counter>().unwrap().amount, 0);
    assert_eq!(h.get::<HookCounter>().unwrap().0, 0);
    assert_eq!(h.notices().count(), 0);
    // The rolled-back hook and body consumed no IDs and scheduled no jobs.
    assert_eq!(
        h.dispatch(OwnedAlternative {
            rollback: false,
            reject_hook: false
        })
        .unwrap(),
        3
    );
    assert_eq!(h.get::<Counter>().unwrap().amount, 3);
    assert_eq!(h.get::<HookCounter>().unwrap().0, 1);
    assert_eq!(h.notices().count(), 1);
}
#[test]
fn retries_are_durable_and_sequence_reuse_is_rejected() {
    let mut h = builder().build(1, 1).unwrap();
    let id = h
        .submit(
            9,
            Add {
                amount: 2,
                fail: false,
            },
        )
        .unwrap();
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    assert_eq!(
        loaded
            .submit(
                9,
                Add {
                    amount: 2,
                    fail: false
                }
            )
            .unwrap(),
        id
    );
    assert_eq!(loaded.get::<Counter>().unwrap().amount, 2);
    assert_eq!(
        loaded.submit(
            9,
            Add {
                amount: 3,
                fail: false
            }
        ),
        Err(Error::SequenceConflict)
    );
}
#[test]
fn due_work_is_chunk_invariant_and_bounded() {
    let mut a = builder().build(1, 1).unwrap();
    for at in [9, 2, 8, 2, 4] {
        a.dispatch(Schedule { at: SimTime(at) }).unwrap();
    }
    let mut b = builder().load(&a.save().unwrap(), 4).unwrap();
    assert_eq!(a.advance(SimTime(10), 2).unwrap().stop, Stop::Budget);
    while a.advance(SimTime(10), 2).unwrap().stop == Stop::Budget {}
    for at in [1, 3, 5, 7, 10] {
        b.advance(SimTime(at), 100).unwrap();
    }
    assert_eq!(a.get::<Counter>().unwrap().amount, 5);
    assert_eq!(a.save().unwrap(), b.save().unwrap());
}
#[test]
fn incompatible_or_malformed_saves_are_rejected() {
    let mut h = builder().build(1, 1).unwrap();
    h.dispatch(Schedule { at: SimTime(10) }).unwrap();
    let save = h.save().unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&save).unwrap();
    value["version"] = serde_json::json!(99);
    assert!(builder().load(&value.to_string(), 1).is_err());
    value = serde_json::from_str(&save).unwrap();
    value["jobs"][0]["request"]["payload"] = serde_json::json!({"wrong":"payload"});
    assert!(builder().load(&value.to_string(), 1).is_err());
    value = serde_json::from_str(&save).unwrap();
    value["resources"]["test.counter"]["amount"] = serde_json::json!(100);
    assert!(builder().load(&value.to_string(), 1).is_err());
}
#[test]
fn registration_rejects_duplicate_owners() {
    let mut b = builder();
    assert!(b.plugin("counter", 1).is_err());
    assert!(b.resource::<Counter>().is_err());
    assert!(b.request::<Add>(|_, _| Ok(0)).is_err());
}
#[test]
fn listener_order_uses_stable_names_not_registration_order() {
    fn with_listeners(reverse: bool) -> Host {
        let mut b = builder();
        let mut names = vec!["a", "b"];
        if reverse {
            names.reverse();
        }
        for name in names {
            b.subscribe::<Added>(name, move |tx, _| {
                let c = tx.edit::<Counter>()?;
                c.amount = c.amount * 2 + if name == "a" { 1 } else { 2 };
                Ok(())
            })
            .unwrap();
        }
        b.build(1, 1).unwrap()
    }
    let mut a = with_listeners(false);
    let mut b = with_listeners(true);
    a.dispatch(Add {
        amount: 1,
        fail: false,
    })
    .unwrap();
    b.dispatch(Add {
        amount: 1,
        fail: false,
    })
    .unwrap();
    assert_eq!(a.get::<Counter>().unwrap().amount, 8);
    assert_eq!(a.save().unwrap(), b.save().unwrap());
}
#[test]
fn aliases_cannot_bypass_resource_ownership() {
    #[derive(Clone, Serialize, Deserialize)]
    struct Foreign;
    impl Request for Foreign {
        const KEY: &'static str = "foreign.edit";
        type Response = ();
    }
    let mut b = builder();
    b.plugin("foreign", 1).unwrap();
    b.request::<Foreign>(|tx, _| {
        tx.edit::<Counter>()?.amount = 9;
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    let before = h.save().unwrap();
    assert!(h.dispatch(Foreign).is_err());
    assert_eq!(before, h.save().unwrap());
    assert!(
        h.dispatch(Add {
            amount: 1,
            fail: false
        })
        .is_ok()
    );
}
#[test]
fn foreign_actions_must_be_canceled_through_the_owner() {
    #[derive(Clone, Serialize, Deserialize)]
    struct BeginAction;
    impl Request for BeginAction {
        const KEY: &'static str = "counter.begin_action";
        type Response = ();
    }
    #[derive(Clone, Serialize, Deserialize)]
    struct CancelAction;
    impl Request for CancelAction {
        const KEY: &'static str = "counter.cancel_action";
        type Response = ();
    }
    #[derive(Clone, Serialize, Deserialize)]
    struct FinishForeign;
    impl Request for FinishForeign {
        const KEY: &'static str = "foreign.finish";
        type Response = ();
    }
    #[derive(Clone, Serialize, Deserialize)]
    struct AskCancel;
    impl Request for AskCancel {
        const KEY: &'static str = "foreign.ask_cancel";
        type Response = ();
    }
    let mut b = builder();
    b.request::<BeginAction>(|tx, _| tx.start_action(PersonId(1), "counter", None, CancelAction))
        .unwrap();
    b.request::<CancelAction>(|tx, _| {
        tx.edit::<Counter>()?.amount += 1;
        Ok(())
    })
    .unwrap();
    b.plugin("foreign", 1).unwrap();
    b.request::<FinishForeign>(|tx, _| tx.finish_action(PersonId(1), "counter"))
        .unwrap();
    b.request::<AskCancel>(|tx, _| tx.cancel_action(PersonId(1)))
        .unwrap();
    let mut h = b.build(1, 1).unwrap();
    h.dispatch(BeginAction).unwrap();
    let before = h.save().unwrap();
    assert!(h.dispatch(FinishForeign).is_err());
    assert_eq!(h.save().unwrap(), before);
    h.dispatch(AskCancel).unwrap();
    assert_eq!(h.get::<Counter>().unwrap().amount, 1);
    assert!(h.get::<Actions>().unwrap().0.is_empty());
}
#[test]
fn queries_are_passive_and_effectful_handlers_are_rejected() {
    #[derive(Clone, Serialize, Deserialize)]
    struct Read;
    impl Request for Read {
        const KEY: &'static str = "counter.read";
        type Response = u64;
    }
    let mut b = builder();
    b.request::<Read>(|tx, _| Ok(tx.get::<Counter>()?.amount))
        .unwrap();
    let h = b.build(1, 1).unwrap();
    let before = h.save().unwrap();
    assert_eq!(h.query(Read).unwrap(), 0);
    assert!(
        h.query(Add {
            amount: 1,
            fail: false
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
}
