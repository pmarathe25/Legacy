use game_body::{Kind, Nourish, NourishMany};
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
struct Hooks(u64);
impl Resource for Hooks {
    const KEY: &'static str = "test.nourishment-hooks";
}
#[derive(Clone, Serialize, Deserialize)]
struct Hook;
impl Request for Hook {
    const KEY: &'static str = "test.nourishment-hook";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Scalar(Vec<Nourish>);
impl Request for Scalar {
    const KEY: &'static str = "test.nourishment-scalar";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Reject(NourishMany);
impl Request for Reject {
    const KEY: &'static str = "test.nourishment-reject";
    type Response = ();
}

fn builder(hooked: bool) -> Builder {
    let mut b = Builder::new();
    game_soma::plugin(&mut b).unwrap();
    b.plugin("nourishment-test", 1).unwrap();
    b.resource::<Hooks>().unwrap();
    b.request::<Hook>(|tx, _| {
        tx.edit::<Hooks>()?.0 += 1;
        Ok(())
    })
    .unwrap();
    b.request::<Scalar>(|tx, r| {
        for effect in r.0 {
            tx.call(effect)?;
        }
        Ok(())
    })
    .unwrap();
    b.request::<Reject>(|tx, r| {
        tx.call(r.0)?;
        Err(Error::Rejected("rollback batch".into()))
    })
    .unwrap();
    if hooked {
        b.before::<Nourish>("test.hook", |tx, _| tx.call(Hook))
            .unwrap();
    }
    b
}
fn effect(person: PersonId, i: usize) -> Nourish {
    Nourish {
        person,
        relief: (i % 17) as u64,
        health: (i % 7) as i32 - 3,
        alertness: (i % 5) as i32 - 2,
    }
}

#[test]
fn disjoint_batch_matches_scalar_across_workers_and_reload() {
    let mut source = builder(false).build(42, 1).unwrap();
    let mut people = vec![];
    for i in 0..500 {
        let person = source
            .dispatch(game_soma::Create {
                kind: if i % 2 == 0 {
                    Kind::Mortal
                } else {
                    Kind::Guardian
                },
                born: SimTime(0),
            })
            .unwrap();
        source
            .dispatch(game_body::Exert {
                person,
                energy: (i as u64 + 1) * 10_000,
                fatigue: 100_000,
            })
            .unwrap();
        people.push(person);
    }
    let checkpoint = source.save().unwrap();
    let effects: Vec<_> = people
        .iter()
        .enumerate()
        .map(|(i, &p)| effect(p, i))
        .collect();
    let mut scalar = builder(false).load(&checkpoint, 1).unwrap();
    scalar.dispatch(Scalar(effects.clone())).unwrap();
    let expected = scalar.save().unwrap();
    for workers in [1, 4, 12] {
        let mut host = builder(false).load(&checkpoint, workers).unwrap();
        host.dispatch(NourishMany {
            effects: effects.clone(),
            minimum_parallel: 1,
        })
        .unwrap();
        assert_eq!(host.save().unwrap(), expected);
        assert_eq!(
            builder(false)
                .load(&host.save().unwrap(), 4)
                .unwrap()
                .save()
                .unwrap(),
            expected
        );
    }
}

#[test]
fn scalar_hooks_duplicates_deaths_and_failures_preserve_order_and_rollback() {
    for hooked in [false, true] {
        let mut source = builder(hooked).build(9, 1).unwrap();
        let first = source
            .dispatch(game_soma::Create {
                kind: Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        let second = source
            .dispatch(game_soma::Create {
                kind: Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        let checkpoint = source.save().unwrap();
        let effects = vec![
            effect(first, 1),
            effect(first, 2),
            Nourish {
                person: second,
                relief: 0,
                health: -1000,
                alertness: 0,
            },
        ];
        let mut scalar = builder(hooked).load(&checkpoint, 1).unwrap();
        scalar.dispatch(Scalar(effects.clone())).unwrap();
        let mut batch = builder(hooked).load(&checkpoint, 4).unwrap();
        batch
            .dispatch(NourishMany {
                effects: effects.clone(),
                minimum_parallel: 1,
            })
            .unwrap();
        assert_eq!(batch.save().unwrap(), scalar.save().unwrap());
        assert!(!batch.get::<game_soma::Bodies>().unwrap().people[&second].alive);
        assert_eq!(batch.get::<Hooks>().unwrap().0, if hooked { 3 } else { 0 });
        let before = source.save().unwrap();
        assert!(
            source
                .dispatch(Reject(NourishMany {
                    effects,
                    minimum_parallel: 1
                }))
                .is_err()
        );
        assert_eq!(source.save().unwrap(), before);
        assert!(
            source
                .dispatch(NourishMany {
                    effects: vec![effect(first, 0), effect(PersonId(u64::MAX), 0)],
                    minimum_parallel: 1
                })
                .is_err()
        );
        assert_eq!(source.save().unwrap(), before);
    }
}

#[test]
fn unique_hooked_and_fatal_effects_take_the_scalar_path() {
    for (hooked, fatal) in [(true, false), (false, true)] {
        let mut source = builder(hooked).build(11, 1).unwrap();
        let first = source
            .dispatch(game_soma::Create {
                kind: Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        let second = source
            .dispatch(game_soma::Create {
                kind: Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        let effects = vec![
            effect(first, 1),
            Nourish {
                person: second,
                relief: 0,
                health: if fatal { -1000 } else { 1 },
                alertness: 0,
            },
        ];
        let before = source.save().unwrap();
        let mut expected = builder(hooked).load(&before, 1).unwrap();
        expected.dispatch(Scalar(effects.clone())).unwrap();
        source
            .dispatch(NourishMany {
                effects,
                minimum_parallel: 1,
            })
            .unwrap();
        assert_eq!(source.save().unwrap(), expected.save().unwrap());
        assert_eq!(source.get::<Hooks>().unwrap().0, if hooked { 2 } else { 0 });
        assert_eq!(
            source.get::<game_soma::Bodies>().unwrap().people[&second].alive,
            !fatal
        );
    }
}

#[test]
fn a_later_failure_rolls_back_the_fast_path_rows() {
    let mut source = builder(false).build(13, 4).unwrap();
    let mut effects = vec![];
    for i in 0..300 {
        let person = source
            .dispatch(game_soma::Create {
                kind: Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        effects.push(effect(person, i));
    }
    let before = source.save().unwrap();
    assert!(
        source
            .dispatch(Reject(NourishMany {
                effects,
                minimum_parallel: 1
            }))
            .is_err()
    );
    assert_eq!(source.save().unwrap(), before);
}
