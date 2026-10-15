use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Log {
    values: Vec<u64>,
    cancel: Option<ScheduleId>,
}
impl Resource for Log {
    const KEY: &'static str = "batch.log";
}
#[derive(Serialize, Deserialize)]
struct Step {
    value: u64,
    fail: bool,
    spawn: bool,
    cancel: bool,
}
impl Request for Step {
    const KEY: &'static str = "batch.step";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
struct Marker(u64);
impl Request for Marker {
    const KEY: &'static str = "batch.marker";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
struct Seed {
    count: u64,
    fail: bool,
    spawn: bool,
    cancel: bool,
}
impl Request for Seed {
    const KEY: &'static str = "batch.seed";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.plugin("batch", 1).unwrap();
    b.resource::<Log>().unwrap();
    b.request::<Marker>(|tx, r| {
        tx.edit::<Log>()?.values.push(r.0);
        Ok(())
    })
    .unwrap();
    b.request::<Step>(|tx, r| {
        tx.edit::<Log>()?.values.push(r.value);
        if r.spawn {
            tx.schedule(tx.now(), 0, Marker(1000))?;
        }
        if r.cancel {
            tx.unschedule(tx.get::<Log>()?.cancel.unwrap());
        }
        require(!r.fail, "failed batch")
    })
    .unwrap();
    b.batch::<Step>(128).unwrap();
    b.request::<Seed>(|tx, r| {
        for value in 0..r.count {
            let id = tx.schedule(
                SimTime(5),
                10,
                Step {
                    value,
                    fail: r.fail && value == 5,
                    spawn: r.spawn && value == 0,
                    cancel: r.cancel && value == 0,
                },
            )?;
            if value == 1 {
                tx.edit::<Log>()?.cancel = Some(id);
            }
        }
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn batches_are_bounded_atomic_and_replay_across_workers_and_budgets() {
    let mut host = builder().build(1, 1).unwrap();
    host.dispatch(Seed {
        count: 300,
        fail: false,
        spawn: false,
        cancel: false,
    })
    .unwrap();
    let revision = host.revision();
    host.advance(SimTime(5), 1).unwrap();
    assert_eq!(host.revision(), revision + 1);
    assert_eq!(
        host.get::<Log>().unwrap().values,
        (0..128).collect::<Vec<_>>()
    );
    let mut loaded = builder().load(&host.save().unwrap(), 4).unwrap();
    while host.advance(SimTime(5), 1).unwrap().stop == Stop::Budget {}
    while loaded.advance(SimTime(5), 7).unwrap().stop == Stop::Budget {}
    assert_eq!(
        host.get::<Log>().unwrap().values,
        (0..300).collect::<Vec<_>>()
    );
    assert_eq!(host.save().unwrap(), loaded.save().unwrap());
    let mut host = builder().build(1, 1).unwrap();
    host.dispatch(Seed {
        count: 10,
        fail: true,
        spawn: false,
        cancel: false,
    })
    .unwrap();
    let before = host.save().unwrap();
    assert!(host.advance(SimTime(5), 1).is_err());
    assert_eq!(host.save().unwrap(), before);
}
#[test]
fn newly_scheduled_earlier_work_and_cancelled_members_keep_queue_order() {
    let mut host = builder().build(1, 1).unwrap();
    host.dispatch(Seed {
        count: 10,
        fail: false,
        spawn: true,
        cancel: true,
    })
    .unwrap();
    host.advance(SimTime(5), 1).unwrap();
    assert_eq!(host.get::<Log>().unwrap().values, [0]);
    while host.advance(SimTime(5), 1).unwrap().stop == Stop::Budget {}
    assert_eq!(
        host.get::<Log>().unwrap().values,
        [0, 1000, 2, 3, 4, 5, 6, 7, 8, 9]
    );
    let before = host.revision();
    host.dispatch(Step {
        value: 10,
        fail: false,
        spawn: false,
        cancel: false,
    })
    .unwrap();
    assert_eq!(
        host.revision(),
        before + 1,
        "direct requests stay individual"
    );
}
