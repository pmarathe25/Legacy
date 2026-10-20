use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Scratch {
    active: bool,
    rows: Vec<u64>,
}
impl Resource for Scratch {
    const KEY: &'static str = "test.scratch";
}
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct State(Vec<u64>);
impl Resource for State {
    const KEY: &'static str = "test.state";
}
#[derive(Clone, Serialize, Deserialize)]
struct Put(u64);
impl Request for Put {
    const KEY: &'static str = "test.put";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Generate;
impl Event for Generate {
    const KEY: &'static str = "test.generate";
}
#[derive(Clone, Serialize, Deserialize)]
struct Draft;
impl Request for Draft {
    const KEY: &'static str = "test.draft";
    type Response = Vec<u64>;
}
#[derive(Clone, Serialize, Deserialize)]
struct Run;
impl Request for Run {
    const KEY: &'static str = "test.run";
    type Response = (Vec<u64>, Vec<u64>);
}
#[derive(Clone, Serialize, Deserialize)]
struct Effect(u64);
impl Request for Effect {
    const KEY: &'static str = "test.effect";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct NoticeEffect;
impl Event for NoticeEffect {
    const KEY: &'static str = "test.notice";
}
fn builder(effect: u64) -> Builder {
    let mut b = Builder::new();
    b.plugin("state", 1).unwrap();
    b.resource::<State>().unwrap();
    b.request::<Effect>(|tx, r| {
        match r.0 {
            1 => tx.edit::<State>()?.0.push(10),
            2 => {
                let id = tx.allocate()?;
                tx.edit::<State>()?.0.push(id);
            }
            3 => tx.emit(NoticeEffect, vec![])?,
            4 => tx.cancel_scheduled::<Effect>()?,
            5 => {
                tx.unschedule(ScheduleId(1));
            }
            6 => {
                let _ = tx.attempt(Effect(1));
            }
            _ => {}
        }
        Ok(())
    })
    .unwrap();
    b.plugin("scratch", 1).unwrap();
    b.resource::<Scratch>().unwrap();
    b.request::<Put>(|tx, r| {
        tx.edit::<Scratch>()?.rows.push(r.0);
        Ok(())
    })
    .unwrap();
    for (name, n) in [("a", 1), ("b", 2), ("d", 4)] {
        b.subscribe_isolated::<Generate, Scratch>(name, move |tx, _| {
            if n == 2 {
                for _ in 0..8 {
                    std::thread::yield_now();
                }
            }
            if n == 1 && effect != 0 {
                tx.call(Effect(effect))?;
            }
            tx.call(Put(n))
        })
        .unwrap();
    }
    // An ordinary subscriber is a barrier and observes all prior merged drafts.
    b.subscribe::<Generate>("c", |tx, _| {
        assert_eq!(tx.get::<Scratch>()?.rows, vec![1, 2]);
        tx.call(Put(3))
    })
    .unwrap();
    b.request::<Draft>(|tx, _| {
        tx.edit::<Scratch>()?.active = true;
        tx.notify_prepared(
            Generate,
            || Scratch {
                active: true,
                rows: vec![],
            },
            |tx, r: Scratch| {
                for row in r.rows {
                    tx.call(Put(row))?;
                }
                Ok(())
            },
        )?;
        let scratch = tx.edit::<Scratch>()?;
        scratch.active = false;
        Ok(std::mem::take(&mut scratch.rows))
    })
    .unwrap();
    b.request::<Run>(move |tx, _| {
        let count = std::sync::atomic::AtomicUsize::new(0);
        let result = tx.prepare::<Draft, Scratch, _>(Draft, |view| {
            count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(view.get::<State>()?.0.clone())
        });
        assert_eq!(
            count.load(std::sync::atomic::Ordering::Relaxed),
            if effect != 0 && tx.compute().current_num_threads() > 1 {
                2
            } else {
                1
            }
        );
        result
    })
    .unwrap();
    b
}
fn build(workers: usize, effect: u64) -> Host {
    builder(effect).build(7, workers).unwrap()
}
#[test]
fn parallel_scratch_drafts_merge_in_order_and_survive_save_load() {
    let mut serial = build(1, 0);
    let mut parallel = build(4, 0);
    assert_eq!(
        serial.dispatch(Run).unwrap(),
        parallel.dispatch(Run).unwrap()
    );
    assert_eq!(serial.get::<Scratch>().unwrap(), &Scratch::default());
    assert_eq!(serial.save().unwrap(), parallel.save().unwrap());
    // A new host has different storage and worker count, with identical subsequent state.
    let saved = parallel.save().unwrap();
    parallel = builder(0).load(&saved, 2).unwrap();
    assert_eq!(
        serial.dispatch(Run).unwrap(),
        parallel.dispatch(Run).unwrap()
    );
    assert_eq!(serial.save().unwrap(), parallel.save().unwrap());
}
#[test]
fn effects_and_caught_effect_attempts_retry_the_entire_request_serially() {
    for effect in 1..=6 {
        let mut serial = build(1, effect);
        let mut parallel = build(4, effect);
        assert_eq!(
            serial.dispatch(Run),
            parallel.dispatch(Run),
            "effect {effect}"
        );
        assert_eq!(
            serial.save().unwrap(),
            parallel.save().unwrap(),
            "effect {effect}"
        );
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Keep;
impl Request for Keep {
    const KEY: &'static str = "test.keep";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct KeepRun;
impl Request for KeepRun {
    const KEY: &'static str = "test.keep-run";
    type Response = Vec<u64>;
}
#[test]
fn retained_scratch_changes_use_serial_execution_and_query_the_resulting_state() {
    let mut worlds = vec![];
    for workers in [1, 4] {
        let mut b = builder(0);
        b.request::<Keep>(|tx, _| {
            tx.edit::<Scratch>()?.rows.push(9);
            Ok(())
        })
        .unwrap();
        b.request::<KeepRun>(|tx, _| {
            Ok(tx
                .prepare::<Keep, Scratch, _>(Keep, |view| Ok(view.get::<Scratch>()?.rows.clone()))?
                .1)
        })
        .unwrap();
        let mut h = b.build(7, workers).unwrap();
        assert_eq!(h.dispatch(KeepRun).unwrap(), vec![9]);
        worlds.push(h.save().unwrap());
    }
    assert_eq!(worlds[0], worlds[1]);
}
#[derive(Clone, Serialize, Deserialize)]
struct Noop;
impl Request for Noop {
    const KEY: &'static str = "test.noop";
    type Response = ();
}
#[test]
fn combining_draft_budgets_cannot_bypass_the_serial_dispatch_limit() {
    let mut worlds = vec![];
    for workers in [1, 4] {
        let mut b = Builder::new();
        b.plugin("limited", 1).unwrap();
        b.resource::<Scratch>().unwrap();
        b.request::<Noop>(|_, _| Ok(())).unwrap();
        for name in ["a", "b"] {
            b.subscribe_isolated::<Generate, Scratch>(name, |tx, _| {
                for _ in 0..700_000 {
                    tx.call(Noop)?;
                }
                Ok(())
            })
            .unwrap();
        }
        b.request::<Keep>(|tx, _| {
            tx.notify_prepared(Generate, Scratch::default, |_, _: Scratch| Ok(()))
        })
        .unwrap();
        b.request::<KeepRun>(|tx, _| {
            tx.prepare::<Keep, Scratch, _>(Keep, |_| Ok(()))?;
            Ok(vec![])
        })
        .unwrap();
        let mut h = b.build(7, workers).unwrap();
        assert_eq!(
            h.dispatch(KeepRun),
            Err(Error::Rejected("request dispatch limit exceeded".into()))
        );
        worlds.push(h.save().unwrap());
    }
    assert_eq!(worlds[0], worlds[1]);
}
