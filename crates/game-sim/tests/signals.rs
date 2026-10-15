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
#[derive(Clone, Serialize, Deserialize)]
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
#[derive(Clone, Serialize, Deserialize)]
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

#[test]
fn typed_hooks_and_private_signals_borrow_payloads_without_json_roundtrips() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SERIALIZATIONS: AtomicUsize = AtomicUsize::new(0);
    #[derive(Clone, Deserialize)]
    struct Numeric(u64);
    impl Serialize for Numeric {
        fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
            SERIALIZATIONS.fetch_add(1, Ordering::Relaxed);
            s.serialize_u64(self.0)
        }
    }
    impl Request for Numeric {
        const KEY: &'static str = "signals.numeric.request";
        type Response = ();
    }
    impl Event for Numeric {
        const KEY: &'static str = "signals.numeric.event";
    }
    #[derive(Clone, Serialize, Deserialize)]
    struct Exercise(bool);
    impl Request for Exercise {
        const KEY: &'static str = "signals.exercise";
        type Response = ();
    }
    let mut b = Builder::new();
    b.plugin("signals", 1).unwrap();
    b.resource::<Count>().unwrap();
    b.request::<Numeric>(|_, _| Ok(())).unwrap();
    b.before::<Numeric>("check", |tx, n| {
        tx.edit::<Count>()?.completions += n.0;
        Ok(())
    })
    .unwrap();
    b.subscribe::<Numeric>("receive", |tx, n| {
        tx.edit::<Count>()?.completions += n.0;
        Ok(())
    })
    .unwrap();
    b.request::<Exercise>(|tx, r| {
        tx.call(Numeric(7))?;
        if r.0 {
            tx.emit(Numeric(11), vec![])
        } else {
            tx.notify(Numeric(11))
        }
    })
    .unwrap();
    let mut host = b.build(1, 1).unwrap();
    host.dispatch(Exercise(false)).unwrap();
    assert_eq!(SERIALIZATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(host.get::<Count>().unwrap().completions, 18);
    assert_eq!(host.notices().count(), 0);
    host.dispatch(Exercise(true)).unwrap();
    assert_eq!(SERIALIZATIONS.load(Ordering::Relaxed), 1);
    assert_eq!(host.get::<Count>().unwrap().completions, 36);
    assert_eq!(host.notices().next().unwrap().data, serde_json::json!(11));
}
