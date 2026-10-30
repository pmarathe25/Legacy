use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct A(u64);
impl Resource for A {
    const KEY: &'static str = "owners.a";
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct B(u64);
impl Resource for B {
    const KEY: &'static str = "owners.b";
}
#[derive(Clone, Serialize, Deserialize)]
struct Start;
impl Request for Start {
    const KEY: &'static str = "owners.start";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel;
impl Request for Cancel {
    const KEY: &'static str = "owners.cancel";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Finish(bool);
impl Request for Finish {
    const KEY: &'static str = "owners.finish";
    type Response = ();
}
fn builder(reverse: bool) -> Builder {
    let mut b = Builder::new();
    let mut plugins = ["a", "b"];
    if reverse {
        plugins.reverse();
    }
    for name in plugins {
        b.plugin(name, 1).unwrap();
        if name == "a" {
            b.resource::<A>().unwrap();
            b.request::<Start>(|tx, _| {
                tx.edit::<A>()?.0 += 1;
                tx.start_action(PersonId(1), "a", None, Cancel)
            })
            .unwrap();
            b.request::<Cancel>(|tx, _| {
                tx.edit::<A>()?.0 += 10;
                Ok(())
            })
            .unwrap();
        } else {
            b.resource::<B>().unwrap();
            b.before::<Start>("b.check", |tx, _| {
                tx.edit::<B>()?.0 += 1;
                Ok(())
            })
            .unwrap();
            b.subscribe::<ActionFinished>("b.finish", |tx, _| {
                tx.edit::<B>()?.0 += 10;
                Ok(())
            })
            .unwrap();
            b.request::<Finish>(|tx, r| {
                if r.0 {
                    tx.edit::<A>()?.0 += 100;
                }
                tx.cancel_action(PersonId(1))
            })
            .unwrap();
        }
    }
    b
}
#[test]
fn runtime_owner_numbers_are_rebound_by_names_across_registration_order() {
    let mut host = builder(false).build(1, 1).unwrap();
    host.dispatch(Start).unwrap();
    let save = host.save().unwrap();
    let mut loaded = builder(true).load(&save, 4).unwrap();
    assert_eq!(save, loaded.save().unwrap());
    for h in [&mut host, &mut loaded] {
        assert!(h.dispatch(Finish(true)).is_err());
        assert_eq!(h.save().unwrap(), save);
        h.dispatch(Finish(false)).unwrap();
        assert_eq!(h.get::<A>().unwrap().0, 11);
        assert_eq!(h.get::<B>().unwrap().0, 11);
    }
    assert_eq!(host.save().unwrap(), loaded.save().unwrap());
}
