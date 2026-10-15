use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Log(Vec<u64>);
impl Resource for Log {
    const KEY: &'static str = "logs.values";
}
#[derive(Clone, Serialize, Deserialize)]
struct Add(u64);
impl Request for Add {
    const KEY: &'static str = "logs.add";
    type Response = ();
}
impl Event for Add {
    const KEY: &'static str = "logs.added";
}
#[derive(Clone, Serialize, Deserialize)]
struct Branch {
    staged: ScheduleId,
    committed: ScheduleId,
    fail: bool,
}
impl Request for Branch {
    const KEY: &'static str = "logs.branch";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Run {
    committed: ScheduleId,
    panic: bool,
}
impl Request for Run {
    const KEY: &'static str = "logs.run";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Seed;
impl Request for Seed {
    const KEY: &'static str = "logs.seed";
    type Response = ScheduleId;
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.plugin("logs", 1).unwrap();
    b.resource::<Log>().unwrap();
    b.request::<Add>(|tx, r| {
        tx.edit::<Log>()?.0.push(r.0);
        Ok(())
    })
    .unwrap();
    b.request::<Seed>(|tx, _| tx.schedule(SimTime(10), 10, Add(0)))
        .unwrap();
    b.request::<Branch>(|tx, r| {
        tx.unschedule(r.staged);
        tx.unschedule(r.committed);
        tx.schedule(SimTime(10), 10, Add(99))?;
        tx.emit(Add(99), vec![])?;
        if r.fail {
            tx.attempt(Branch { fail: false, ..r })?;
        }
        require(
            !r.fail,
            "rollback parent including successful nested attempt",
        )
    })
    .unwrap();
    b.request::<Run>(|tx, r| {
        let staged = tx.schedule(SimTime(10), 10, Add(1))?;
        tx.emit(Add(1), vec![])?;
        assert!(
            tx.attempt(Branch {
                staged,
                committed: r.committed,
                fail: true
            })
            .is_err()
        );
        // Cancelling an unknown future identity cannot cancel later work.
        tx.unschedule(ScheduleId(staged.0 + 1));
        let following = tx.schedule(SimTime(10), 10, Add(2))?;
        assert_eq!(
            following.0,
            staged.0 + 1,
            "failed branches restore identities"
        );
        let discarded = tx.schedule(SimTime(1), 0, Add(88))?;
        tx.unschedule(discarded);
        tx.emit(Add(2), vec![])?;
        assert!(!r.panic, "rollback entire transaction");
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn append_logs_restore_nested_cancellations_notices_and_identities() {
    for workers in [1, 4] {
        let mut host = builder().build(1, workers).unwrap();
        let committed = host.dispatch(Seed).unwrap();
        let before = host.save().unwrap();
        assert_eq!(
            host.dispatch(Run {
                committed,
                panic: true
            }),
            Err(Error::WorkerFailure)
        );
        assert_eq!(host.save().unwrap(), before);
        host.dispatch(Run {
            committed,
            panic: false,
        })
        .unwrap();
        let save = host.save().unwrap();
        let mut loaded = builder().load(&save, 2).unwrap();
        for h in [&mut host, &mut loaded] {
            h.advance(SimTime(10), 100).unwrap();
            assert_eq!(h.get::<Log>().unwrap().0, [0, 1, 2]);
            let notices: Vec<_> = h.notices().map(|n| (n.sequence, n.data.clone())).collect();
            assert_eq!(
                notices,
                [(1, serde_json::json!(1)), (2, serde_json::json!(2))]
            );
        }
        assert_eq!(host.save().unwrap(), loaded.save().unwrap());
    }
}
