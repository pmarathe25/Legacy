use game_core::*;
use game_nutrition::*;
use game_sim::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct FailedMeal {
    person: PersonId,
    good: GoodId,
}
impl Request for FailedMeal {
    const KEY: &'static str = "test.failed-meal";
    type Response = ();
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Meals {
    kept: PersonId,
    reverted: PersonId,
    good: GoodId,
}
impl Request for Meals {
    const KEY: &'static str = "test.meals";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_objects::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.request::<FailedMeal>(|tx, r| {
        tx.call(Meal {
            person: r.person,
            good: r.good,
            quantity: 10,
        })?;
        Err(Error::Rejected("reject after changing diet".into()))
    })
    .unwrap();
    b.request::<Meals>(|tx, r| {
        tx.call(Meal {
            person: r.kept,
            good: r.good,
            quantity: 10,
        })?;
        assert!(
            tx.attempt(FailedMeal {
                person: r.reverted,
                good: r.good
            })
            .is_err()
        );
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn changing_one_diet_preserves_neighbours_nested_rollback_and_save_load() {
    let mut h = builder().build(1, 1).unwrap();
    let objects = game_objects::standard().unwrap();
    let grain = objects.resolve("grain").unwrap();
    h.dispatch(game_objects::Install(objects)).unwrap();
    h.dispatch(Enable).unwrap();
    for p in 1..=40 {
        h.dispatch(Register {
            person: PersonId(p),
        })
        .unwrap();
    }
    let before = h.get::<Nutrition>().unwrap().people.clone();
    h.dispatch(Meals {
        kept: PersonId(13),
        reverted: PersonId(14),
        good: grain,
    })
    .unwrap();
    for p in 1..=40 {
        if p != 13 {
            assert_eq!(
                h.get::<Nutrition>().unwrap().people[&PersonId(p)],
                before[&PersonId(p)]
            );
        }
    }
    assert_eq!(h.get::<Nutrition>().unwrap().people[&PersonId(13)].meals, 1);
    assert_eq!(before[&PersonId(13)].meals, 0);
    let save = h.save().unwrap();
    assert_eq!(builder().load(&save, 4).unwrap().save().unwrap(), save);
}
