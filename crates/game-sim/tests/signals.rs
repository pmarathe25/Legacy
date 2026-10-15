use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Count {
    completions: u64,
    fail: bool,
}
impl Resource for Count {
    const KEY: &'static str = "signals.count";
}
#[derive(Serialize, Deserialize)]
enum Operate {
    Start(bool),
    Finish,
    Cancel,
    Fail(bool),
}
impl Request for Operate {
    const KEY: &'static str = "signals.operate";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
struct Cancel(bool);
impl Request for Cancel {
    const KEY: &'static str = "signals.cancel";
    type Response = ();
}
#[test]
fn lifecycle_signals_are_atomic_private_and_not_duplicated_by_cancellation() {
    let mut b = Builder::new();
    b.plugin("signals", 1).unwrap();
    b.resource::<Count>().unwrap();
    b.request::<Cancel>(|tx, r| {
        if r.0 {
            tx.finish_action(PersonId(1), "work")?;
        }
        Ok(())
    })
    .unwrap();
    b.request::<Operate>(|tx, r| match r {
        Operate::Start(inside) => tx.start_action(PersonId(1), "work", None, Cancel(inside)),
        Operate::Finish => tx.finish_action(PersonId(1), "work"),
        Operate::Cancel => tx.cancel_action(PersonId(1)),
        Operate::Fail(fail) => {
            tx.edit::<Count>()?.fail = fail;
            Ok(())
        }
    })
    .unwrap();
    b.subscribe::<ActionFinished>("count", |tx, event| {
        assert_eq!(event.person, PersonId(1));
        let state = tx.edit::<Count>()?;
        state.completions += 1;
        require(!state.fail, "rejected lifecycle change")
    })
    .unwrap();
    let mut host = b.build(1, 1).unwrap();
    for inside in [false, true] {
        host.dispatch(Operate::Start(inside)).unwrap();
        host.dispatch(Operate::Cancel).unwrap();
    }
    assert_eq!(host.get::<Count>().unwrap().completions, 2);
    assert_eq!(host.notices().count(), 0);
    host.dispatch(Operate::Start(false)).unwrap();
    host.dispatch(Operate::Fail(true)).unwrap();
    let before = host.save().unwrap();
    assert!(host.dispatch(Operate::Finish).is_err());
    assert_eq!(host.save().unwrap(), before);
    host.dispatch(Operate::Fail(false)).unwrap();
    host.dispatch(Operate::Finish).unwrap();
    assert_eq!(host.get::<Count>().unwrap().completions, 3);
    assert_eq!(host.notices().count(), 0);
}
