use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
static ENCODED: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone, Deserialize)]
struct Work(u64);
impl Serialize for Work {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        ENCODED.fetch_add(1, Ordering::Relaxed);
        s.serialize_u64(self.0)
    }
}
impl Request for Work {
    const KEY: &'static str = "pending.work";
    type Response = ();
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct State {
    total: u64,
    reject: bool,
}
impl Resource for State {
    const KEY: &'static str = "pending.state";
}
#[derive(Clone, Serialize, Deserialize)]
enum Control {
    Seed,
    Reject(bool),
    Cancel,
}
impl Request for Control {
    const KEY: &'static str = "pending.control";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.plugin("pending", 1).unwrap();
    b.resource::<State>().unwrap();
    b.request::<Work>(|tx, r| {
        tx.edit::<State>()?.total += r.0;
        require(!tx.get::<State>()?.reject, "rejected work")
    })
    .unwrap();
    b.before::<Work>("count", |tx, r| {
        tx.edit::<State>()?.total += r.0;
        Ok(())
    })
    .unwrap();
    b.request::<Control>(|tx, r| {
        match r {
            Control::Seed => {
                tx.schedule(SimTime(5), 10, Work(3))?;
                tx.start_action(PersonId(1), "work", None, Work(7))?;
            }
            Control::Reject(reject) => tx.edit::<State>()?.reject = reject,
            Control::Cancel => tx.cancel_action(PersonId(1))?,
        }
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn pending_payloads_survive_load_and_failed_execution_without_runtime_json() {
    let mut host = builder().build(1, 1).unwrap();
    host.dispatch(Control::Seed).unwrap();
    assert_eq!(
        ENCODED.swap(0, Ordering::Relaxed),
        2,
        "validate construction once"
    );
    let snapshot = host.get::<Actions>().unwrap().clone();
    let save = host.save().unwrap();
    let mut loaded = builder().load(&save, 4).unwrap();
    assert_eq!(loaded.save().unwrap(), save);
    // Reject twice: each attempt must invoke an independent clone of the payload,
    // retain the original due job, and roll back hooks and owner effects.
    for h in [&mut host, &mut loaded] {
        h.dispatch(Control::Reject(true)).unwrap();
        let before = h.save().unwrap();
        for _ in 0..2 {
            ENCODED.store(0, Ordering::Relaxed);
            assert!(h.advance(SimTime(5), 1).is_err());
            assert_eq!(ENCODED.load(Ordering::Relaxed), 0);
            assert_eq!(h.save().unwrap(), before);
        }
        h.dispatch(Control::Reject(false)).unwrap();
        ENCODED.store(0, Ordering::Relaxed);
        h.advance(SimTime(5), 10).unwrap();
        h.dispatch(Control::Cancel).unwrap();
        assert_eq!(ENCODED.load(Ordering::Relaxed), 0);
        assert_eq!(h.get::<State>().unwrap().total, 20);
        assert!(h.get::<Actions>().unwrap().0.is_empty());
    }
    assert_eq!(host.save().unwrap(), loaded.save().unwrap());
    assert_eq!(
        snapshot.0.len(),
        1,
        "retained committed action remains intact"
    );
    for (field, value) in [
        ("key", serde_json::json!("missing.cancel")),
        ("payload", serde_json::json!({"wrong": true})),
    ] {
        let mut broken: serde_json::Value = serde_json::from_str(&save).unwrap();
        broken["resources"]["world.actions"]["1"]["cancel"][field] = value;
        assert!(builder().load(&broken.to_string(), 1).is_err());
    }
    let mut broken: serde_json::Value = serde_json::from_str(&save).unwrap();
    broken["resources"]["world.actions"]["1"]["owner"] = serde_json::json!("foreign");
    assert!(builder().load(&broken.to_string(), 1).is_err());
}
#[derive(Clone, Deserialize)]
struct Bad;
impl Serialize for Bad {
    fn serialize<S: serde::Serializer>(&self, _: S) -> std::result::Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("cannot serialize"))
    }
}
impl Request for Bad {
    const KEY: &'static str = "pending.bad";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Invalid(u8);
impl Request for Invalid {
    const KEY: &'static str = "pending.invalid";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Impostor;
impl Request for Impostor {
    const KEY: &'static str = "pending.work";
    type Response = ();
}
#[test]
fn unserializable_and_unregistered_typed_payloads_cannot_be_committed() {
    let mut b = builder();
    b.request::<Bad>(|_, _| Ok(())).unwrap();
    b.request::<Invalid>(|tx, r| match r.0 {
        0 => tx.schedule(SimTime(5), 0, Bad).map(|_| ()),
        1 => tx.start_action(PersonId(1), "bad", None, Bad),
        2 => tx.schedule(SimTime(5), 0, Impostor).map(|_| ()),
        _ => tx.start_action(PersonId(1), "alias", None, Impostor),
    })
    .unwrap();
    let mut host = b.build(1, 1).unwrap();
    let before = host.save().unwrap();
    for action in 0..4 {
        assert!(host.dispatch(Invalid(action)).is_err());
        assert_eq!(host.save().unwrap(), before);
    }
    assert!(!host.registered::<Impostor>());
    assert!(host.dispatch(Impostor).is_err());
    assert_eq!(host.save().unwrap(), before);
}
