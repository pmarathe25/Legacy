use game_core::*;
use game_packed::{Mark, Packed};
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Hot {
    rows: Packed<u64, i64>,
    meta: u64,
}
impl Resource for Hot {
    const KEY: &'static str = "journal.hot";
    fn checkpoint(&mut self) -> Checkpoint {
        Box::new((self.rows.begin(), self.meta))
    }
    fn restore(&mut self, c: Checkpoint) {
        let (mark, meta) = *c.downcast::<(Mark, u64)>().unwrap();
        self.rows.rollback(mark);
        self.meta = meta;
    }
    fn commit(&mut self, c: Checkpoint) {
        self.rows.commit(c.downcast::<(Mark, u64)>().unwrap().0);
    }
    fn validate_checkpoint(&self, c: &dyn std::any::Any) -> Result<()> {
        let old = c.downcast_ref::<(Mark, u64)>().unwrap();
        require(
            self.rows.changed_since(&old.0).all(|(_, v)| *v >= 0),
            "negative row",
        )
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Cold(u64);
impl Resource for Cold {
    const KEY: &'static str = "journal.cold";
}
#[derive(Clone, Serialize, Deserialize)]
enum Work {
    Init,
    Outer(u8),
    Child(bool),
    Leaf,
    Panic,
    Read,
}
impl Request for Work {
    const KEY: &'static str = "journal.work";
    type Response = ();
}
impl Event for Work {
    const KEY: &'static str = "journal.event";
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.plugin("journal", 1).unwrap();
    b.resource::<Hot>().unwrap();
    b.resource::<Cold>().unwrap();
    b.request::<Work>(|tx, r| {
        match r {
            Work::Init => {
                for k in [1, 4, 9] {
                    tx.edit::<Hot>()?.rows.insert(k, k as i64);
                }
                tx.edit::<Hot>()?.meta = 7;
            }
            Work::Outer(mode) => {
                tx.edit::<Hot>()?.meta = 20;
                *tx.edit::<Hot>()?.rows.get_mut(&1).unwrap() += 10;
                tx.edit::<Cold>()?.0 += 1;
                assert!(tx.attempt(Work::Child(false)).is_err());
                if mode == 1 {
                    *tx.edit::<Hot>()?.rows.get_mut(&1).unwrap() = -1;
                }
                if mode == 2 {
                    tx.attempt(Work::Panic)?;
                }
                tx.emit(Work::Read, vec![])?;
            }
            Work::Child(ok) => {
                *tx.edit::<Hot>()?.rows.get_mut(&4).unwrap() += 40;
                tx.edit::<Cold>()?.0 += 40;
                tx.call(Work::Leaf)?;
                require(ok, "failed parent")?;
            }
            Work::Leaf => {
                tx.attempt(Work::Read)?;
                tx.edit::<Hot>()?.rows.insert(9, 99);
                tx.edit::<Hot>()?.meta = 99;
                tx.allocate()?;
                tx.schedule(SimTime(10), 0, Work::Read)?;
                tx.emit(Work::Read, vec![])?;
            }
            Work::Panic => {
                tx.edit::<Hot>()?.rows.erase(&4);
                tx.edit::<Hot>()?.rows.insert(u64::MAX, 3);
                tx.edit_with_compute::<Hot, _>(|state, pool| {
                    state.rows.update(pool, |k, v| {
                        *v += 1;
                        assert_ne!(*k, u64::MAX);
                    })
                })?;
            }
            Work::Read => {}
        }
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn mixed_resources_nested_failure_validation_and_worker_panic_restore_the_host() {
    for workers in [1, 4] {
        let mut host = builder().build(5, workers).unwrap();
        host.dispatch(Work::Init).unwrap();
        let reader = host.get::<Hot>().unwrap().clone();
        let reader_wire = serde_json::to_string(&reader).unwrap();
        let before = host.save().unwrap();
        for mode in [1, 2, 2, 1] {
            assert!(host.dispatch(Work::Outer(mode)).is_err());
            assert_eq!(host.save().unwrap(), before);
            assert_eq!(serde_json::to_string(&reader).unwrap(), reader_wire);
        }
        assert!(host.query(Work::Outer(0)).is_err());
        assert_eq!(host.save().unwrap(), before);
        host.dispatch(Work::Outer(0)).unwrap();
        let h = host.get::<Hot>().unwrap();
        assert_eq!(h.meta, 20);
        assert_eq!(h.rows[&1], 11);
        assert_eq!(h.rows[&4], 4);
        assert_eq!(h.rows[&9], 9);
        assert_eq!(host.get::<Cold>().unwrap().0, 1);
        assert_eq!(host.notices().count(), 1);
        assert_eq!(serde_json::to_string(&reader).unwrap(), reader_wire);
        let saved = host.save().unwrap();
        assert_eq!(builder().load(&saved, 4).unwrap().save().unwrap(), saved);
    }
}
