use game_choice::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize, Serializer};
use std::sync::atomic::{AtomicUsize, Ordering};
static SERIALIZATIONS: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone, Deserialize)]
struct Act(u64);
impl Serialize for Act {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        SERIALIZATIONS.fetch_add(1, Ordering::SeqCst);
        self.0.serialize(s)
    }
}
impl Request for Act {
    const KEY: &'static str = "test.temporary-act";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
struct Pick;
impl Request for Pick {
    const KEY: &'static str = "test.pick";
    type Response = String;
}
#[test]
fn temporary_offers_use_typed_calls_without_serializing_discarded_requests() {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    b.plugin("temporary-test", 1).unwrap();
    b.request::<Act>(|_, r| Ok(r.0)).unwrap();
    provider(&mut b, "test.offers", |tx, e| {
        for value in 1..10 {
            offer(
                tx,
                e.person,
                format!("act:{value}"),
                motive(0, 0, value, 0, 0),
                Act(value as u64),
            )?;
        }
        Ok(())
    })
    .unwrap();
    b.request::<Pick>(|tx, _| {
        let offers = tx.call(Collect {
            person: PersonId(1),
        })?;
        let chosen = offers.into_iter().max_by_key(|p| p.motive.score()).unwrap();
        tx.call(Run(chosen.request))?;
        Ok(chosen.key)
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    SERIALIZATIONS.store(0, Ordering::SeqCst);
    assert_eq!(h.dispatch(Pick).unwrap(), "act:9");
    assert_eq!(SERIALIZATIONS.load(Ordering::SeqCst), 0);
    h.save().unwrap();
}

static NESTED_SERIALIZATIONS: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone, Deserialize)]
struct Queued(u64);
impl Serialize for Queued {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        NESTED_SERIALIZATIONS.fetch_add(1, Ordering::SeqCst);
        self.0.serialize(s)
    }
}
impl Request for Queued {
    const KEY: &'static str = "test.queued-act";
    type Response = ();
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Delivered(u64);
impl Resource for Delivered {
    const KEY: &'static str = "test.delivered";
}
#[derive(Clone, Serialize, Deserialize)]
struct PickDeferred(bool);
impl Request for PickDeferred {
    const KEY: &'static str = "test.pick-deferred";
    type Response = String;
}
fn deferred_builder() -> Builder {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    b.plugin("deferred-test", 1).unwrap();
    b.resource::<Delivered>().unwrap();
    b.request::<Queued>(|tx, r| {
        tx.edit::<Delivered>()?.0 += r.0;
        Ok(())
    })
    .unwrap();
    provider(&mut b, "test.deferred-offers", |tx, e| {
        for value in 1..10 {
            offer(
                tx,
                e.person,
                format!("queued:{value}"),
                motive(0, value, 0, 0, 0),
                Undertake {
                    person: e.person,
                    kind: "deferred-test".into(),
                    duration: MINUTE,
                    request: Pending::immediate(Queued(value as u64)),
                },
            )?;
        }
        Ok(())
    })
    .unwrap();
    b.request::<PickDeferred>(|tx, r| {
        let offers = tx.call(Collect {
            person: PersonId(1),
        })?;
        let chosen = offers.into_iter().max_by_key(|p| p.motive.score()).unwrap();
        if r.0 {
            tx.call(Run(chosen.request))?;
        }
        Ok(chosen.key)
    })
    .unwrap();
    b
}
#[test]
fn nested_temporary_requests_serialize_only_on_commit_and_finish_after_reload() {
    let mut host = deferred_builder().build(1, 1).unwrap();
    NESTED_SERIALIZATIONS.store(0, Ordering::SeqCst);
    assert_eq!(host.dispatch(PickDeferred(false)).unwrap(), "queued:9");
    assert_eq!(NESTED_SERIALIZATIONS.load(Ordering::SeqCst), 0);
    assert_eq!(host.dispatch(PickDeferred(true)).unwrap(), "queued:9");
    assert_eq!(NESTED_SERIALIZATIONS.load(Ordering::SeqCst), 1);
    assert_eq!(host.get::<Delivered>().unwrap().0, 0);
    let mut loaded = deferred_builder().load(&host.save().unwrap(), 4).unwrap();
    host.advance(SimTime(MINUTE.0), 8).unwrap();
    loaded.advance(SimTime(MINUTE.0), 8).unwrap();
    assert_eq!(host.get::<Delivered>().unwrap().0, 9);
    assert_eq!(host.save().unwrap(), loaded.save().unwrap());
}
