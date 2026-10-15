use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Rows(Table<u64, u64>);
impl Resource for Rows {
    const KEY: &'static str = "test.rows";
    fn validate(&self) -> Result<()> {
        require(self.0.values().all(|v| *v < 100), "bad row")
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        require(
            self.0.changed(&previous.0).all(|(_, v)| *v < 100),
            "bad row",
        )
    }
}
#[derive(Serialize, Deserialize)]
struct Change {
    key: u64,
    amount: u64,
    fail: bool,
}
impl Request for Change {
    const KEY: &'static str = "test.change";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
struct Attempt;
impl Request for Attempt {
    const KEY: &'static str = "test.attempt_rows";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
struct Panic;
impl Request for Panic {
    const KEY: &'static str = "test.panic";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.plugin("rows", 1).unwrap();
    b.resource::<Rows>().unwrap();
    b.request::<Change>(|tx, r| {
        *tx.edit::<Rows>()?.0.entry(r.key).or_default() += r.amount;
        tx.allocate()?;
        require(!r.fail, "rejected")
    })
    .unwrap();
    b.request::<Attempt>(|tx, _| {
        tx.call(Change {
            key: 1,
            amount: 10,
            fail: false,
        })?;
        assert!(
            tx.attempt(Change {
                key: 1,
                amount: 30,
                fail: true
            })
            .is_err()
        );
        assert!(
            tx.attempt(Change {
                key: 20_001,
                amount: 30,
                fail: true
            })
            .is_err()
        );
        tx.call(Change {
            key: 2,
            amount: 3,
            fail: false,
        })
    })
    .unwrap();
    b.request::<Panic>(|tx, _| {
        tx.allocate()?;
        tx.schedule(SimTime(5), 0, Attempt)?;
        tx.edit_with_compute::<Rows, _>(|state, pool| {
            state.0.update(pool, |id, v| {
                *v += 1;
                assert_ne!(*id, 789);
            })
        })?;
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn table_writes_nested_attempts_and_worker_panics_are_atomic() {
    for workers in [1, 4] {
        let mut h = builder().build(1, workers).unwrap();
        // Populate using the same owner handler; snapshots must remain isolated.
        for key in 0..5000 {
            h.dispatch(Change {
                key,
                amount: 1,
                fail: false,
            })
            .unwrap();
        }
        let original = h.save().unwrap();
        assert_eq!(h.dispatch(Panic), Err(Error::WorkerFailure));
        assert_eq!(h.save().unwrap(), original);
        assert!(
            h.dispatch(Change {
                key: 5,
                amount: 100,
                fail: false
            })
            .is_err()
        );
        assert_eq!(h.save().unwrap(), original);
        h.dispatch(Attempt).unwrap();
        assert_eq!(h.get::<Rows>().unwrap().0[&1], 11);
        assert_eq!(h.get::<Rows>().unwrap().0[&2], 4);
        assert!(!h.get::<Rows>().unwrap().0.contains_key(&20_001));
        let save = h.save().unwrap();
        assert_eq!(
            builder().load(&save, workers).unwrap().save().unwrap(),
            save
        );
        let mut corrupt: serde_json::Value = serde_json::from_str(&save).unwrap();
        corrupt["resources"]["test.rows"]["4"] = 1000.into();
        assert!(builder().load(&corrupt.to_string(), workers).is_err());
    }
}
