use game_anticipation::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
#[derive(Clone, Default, Serialize, Deserialize)]
struct State {
    stock: u64,
    seen: Vec<(PersonId, u64, SimTime)>,
}
impl Resource for State {
    const KEY: &'static str = "test.state";
}
#[derive(Clone, Serialize, Deserialize)]
struct Step {
    person: PersonId,
}
impl Request for Step {
    const KEY: &'static str = "test.step";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Earlier;
impl Request for Earlier {
    const KEY: &'static str = "test.earlier";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Start;
impl Request for Start {
    const KEY: &'static str = "test.start";
    type Response = ();
}
#[derive(Default)]
struct Cache {
    entries: Vec<(PersonId, u64, SimTime)>,
    stale: u64,
    warm: u64,
}
fn builder(mode: u8) -> (Builder, Arc<Mutex<Cache>>) {
    let cache = Arc::new(Mutex::new(Cache::default()));
    let mut b = Builder::new();
    b.plugin("test", 1).unwrap();
    b.resource::<State>().unwrap();
    register(&mut b).unwrap();
    let prepared = cache.clone();
    b.subscribe::<Preview>("test.prepare", move |tx, e| {
        match mode {
            1 => return Err(Error::Rejected("advisory failure".into())),
            2 => panic!("advisory panic"),
            3 => {
                let _ = tx.edit::<State>();
                return Ok(());
            }
            4 => {
                tx.edit::<State>()?.stock += 100;
            }
            _ => {}
        }
        use rayon::prelude::*;
        let input = tx.get::<State>()?.stock;
        let entries = tx.compute().install(|| {
            e.people
                .par_iter()
                .map(|&p| (p, input, tx.now()))
                .collect::<Vec<_>>()
        });
        let mut c = prepared.lock().unwrap();
        c.warm += entries.len() as u64;
        c.entries = entries;
        Ok(())
    })
    .unwrap();
    let consumed = cache.clone();
    b.request::<Step>(move |tx, r| {
        let stock = tx.get::<State>()?.stock;
        let mut c = consumed.lock().unwrap();
        if c.entries
            .iter()
            .any(|(p, old, at)| *p == r.person && (*old != stock || *at != tx.now()))
        {
            c.stale += 1;
        }
        drop(c);
        let now = tx.now();
        tx.edit::<State>()?.seen.push((r.person, stock, now));
        tx.edit::<State>()?.stock += 1;
        if r.person == PersonId(1) {
            // New work must run ahead of later captured jobs; it cancels one of them.
            tx.schedule(now, 1, Earlier)?;
        }
        Ok(())
    })
    .unwrap();
    b.request::<Earlier>(|tx, _| {
        let cancelled = tx.get::<State>()?.seen.first().unwrap().0;
        assert_eq!(cancelled, PersonId(1));
        tx.unschedule(ScheduleId(3));
        tx.edit::<State>()?.stock += 100;
        Ok(())
    })
    .unwrap();
    b.request::<Start>(|tx, _| {
        for n in 1..=4 {
            tx.schedule(
                SimTime(10),
                50,
                Step {
                    person: PersonId(n),
                },
            )?;
        }
        tx.schedule(
            SimTime(11),
            50,
            Step {
                person: PersonId(5),
            },
        )?;
        Ok(())
    })
    .unwrap();
    (b, cache)
}
fn execute(workers: usize, width: usize, mode: u8) -> (String, Statistics, u64, u64) {
    let (b, _) = builder(mode);
    let mut host = b.build(7, workers).unwrap();
    host.dispatch(Start).unwrap();
    let saved = host.save().unwrap();
    let (b, cache) = builder(mode);
    host = b.load(&saved, workers).unwrap();
    let mut window = Window::new(width).unwrap();
    loop {
        let before = host.save().unwrap();
        let revision = host.revision();
        let now = host.now();
        window.prepare::<Step>(&host, SimTime(11), |r| r.person);
        assert_eq!(host.save().unwrap(), before);
        assert_eq!((host.now(), host.revision()), (now, revision));
        if host.advance(SimTime(11), 1).unwrap().stop == Stop::Target {
            break;
        }
    }
    assert_eq!(
        host.get::<State>()
            .unwrap()
            .seen
            .iter()
            .map(|(p, _, _)| p.0)
            .collect::<Vec<_>>(),
        vec![1, 2, 4, 5]
    );
    let c = cache.lock().unwrap();
    (host.save().unwrap(), window.statistics, c.warm, c.stale)
}
#[test]
fn preparation_preserves_order_ids_cancellation_and_complete_saves_across_workers() {
    let expected = execute(1, 0, 0).0;
    for workers in [1, 2, 4] {
        let (actual, stats, warm, stale) = execute(workers, 4, 0);
        assert_eq!(actual, expected);
        assert!(stats.batches >= 2);
        assert_eq!(stats.maximum, 4);
        assert_eq!(stats.failures, 0);
        assert!(warm >= 5);
        assert!(stale >= 1);
    }
}
#[test]
fn failures_panics_and_attempted_writes_are_advisory_and_do_not_skip_jobs() {
    let expected = execute(1, 0, 0).0;
    for mode in [1, 2, 3, 4] {
        let (actual, stats, _, _) = execute(2, 4, mode);
        assert_eq!(actual, expected);
        if mode != 3 {
            assert!(stats.failures > 0);
        }
    }
}
#[test]
fn disabled_bounded_and_future_windows_do_not_advance_the_world() {
    assert!(Window::new(129).is_err());
    let (b, _) = builder(0);
    let mut h = b.build(1, 2).unwrap();
    h.dispatch(Start).unwrap();
    let save = h.save().unwrap();
    let mut window = Window::new(2).unwrap();
    window.prepare::<Step>(&h, SimTime(9), |r| r.person);
    assert_eq!(window.statistics.batches, 0);
    window.prepare::<Step>(&h, SimTime(10), |r| r.person);
    assert_eq!(
        (window.statistics.actors, window.statistics.maximum),
        (2, 2)
    );
    window.prepare::<Step>(&h, SimTime(10), |r| r.person);
    assert_eq!(window.statistics.batches, 1);
    assert_eq!(save, h.save().unwrap());
    assert!(h.query_at(SimTime(10), Prepare { people: vec![] }).is_ok());
    h.advance(SimTime(11), 128).unwrap();
    assert!(h.query_at(SimTime(10), Prepare { people: vec![] }).is_err());
}

#[test]
fn integrated_advancement_preserves_budgets_stops_and_invalid_argument_behavior() {
    for width in [0, 1, 4] {
        for budget in [1, 2, 8] {
            let mut serial = builder(0).0.build(1, 1).unwrap();
            serial.dispatch(Start).unwrap();
            let mut prepared = builder(0).0.load(&serial.save().unwrap(), 4).unwrap();
            let mut window = Window::new(width).unwrap();
            let before = prepared.save().unwrap();
            assert!(
                window
                    .advance::<Step>(&mut prepared, SimTime(11), 0, |r| r.person)
                    .is_err()
            );
            assert_eq!(before, prepared.save().unwrap());
            loop {
                let expected = serial.advance(SimTime(11), budget).unwrap();
                let actual = window
                    .advance::<Step>(&mut prepared, SimTime(11), budget, |r| r.person)
                    .unwrap();
                assert_eq!(actual, expected);
                assert_eq!(prepared.save().unwrap(), serial.save().unwrap());
                if actual.stop == Stop::Target {
                    break;
                }
            }
        }
    }
    let (mut b, _) = builder(0);
    b.stop(|h| Ok((h.get::<State>()?.stock > 0).then(|| "inspect".into())));
    let mut serial = b.build(1, 1).unwrap();
    serial.dispatch(Start).unwrap();
    let (mut b, _) = builder(0);
    b.stop(|h| Ok((h.get::<State>()?.stock > 0).then(|| "inspect".into())));
    let mut prepared = b.load(&serial.save().unwrap(), 4).unwrap();
    let expected = serial.advance(SimTime(11), 128).unwrap();
    let actual = Window::new(4)
        .unwrap()
        .advance::<Step>(&mut prepared, SimTime(11), 128, |r| r.person)
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(prepared.save().unwrap(), serial.save().unwrap());
}
