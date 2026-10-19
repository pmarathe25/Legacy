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
