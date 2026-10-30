use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct State(Vec<u64>);
impl Resource for State {
    const KEY: &'static str = "test.executor-state";
}
#[derive(Clone, Serialize, Deserialize)]
struct Work(u64);
impl Request for Work {
    const KEY: &'static str = "test.executor-work";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Start;
impl Request for Start {
    const KEY: &'static str = "test.executor-start";
    type Response = ();
}
fn builder(resident: bool) -> Builder {
    let mut b = Builder::new();
    b.plugin("test.executor", 1).unwrap();
    b.resource::<State>().unwrap();
    b.request::<Start>(|tx, _| {
        tx.schedule(tx.now(), 20, Work(0))?;
        Ok(())
    })
    .unwrap();
    b.request::<Work>(move |tx, r| {
        assert_eq!(tx.compute().current_thread_index().is_some(), resident);
        let id = tx.allocate()?;
        tx.edit::<State>()?.0.extend([r.0, id]);
        assert_ne!(r.0, 99, "worker panic");
        if r.0 < 3 {
            tx.schedule(tx.now().after(SimDuration(1))?, 20, Work(r.0 + 1))?;
        }
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn residing_on_the_compute_pool_preserves_commits_budgets_and_panic_rollback() {
    let mut serial = builder(false).build(7, 1).unwrap();
    let mut resident = builder(true).build(7, 4).unwrap();
    serial.dispatch(Start).unwrap();
    resident.dispatch(Start).unwrap();
    let executor = resident.executor();
    loop {
        let a = serial.advance(SimTime(10), 1).unwrap();
        let b = executor
            .install(|| resident.advance(SimTime(10), 1))
            .unwrap();
        assert_eq!(a, b);
        if a.stop == Stop::Target {
            break;
        }
    }
    assert_eq!(serial.save().unwrap(), resident.save().unwrap());
    let saved = resident.save().unwrap();
    assert_eq!(
        executor.install(|| resident.dispatch(Work(99))),
        Err(Error::WorkerFailure)
    );
    assert_eq!(resident.save().unwrap(), saved);
}
