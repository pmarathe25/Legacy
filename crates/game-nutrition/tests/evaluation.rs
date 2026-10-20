use game_core::*;
use game_nutrition::*;
use game_objects::{Food, FoodEffects};
use game_sim::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Seed(u64, bool);
impl Request for Seed {
    const KEY: &'static str = "test.seed";
    type Response = ();
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Check;
impl Request for Check {
    const KEY: &'static str = "test.check";
    type Response = ();
}
#[test]
fn prepared_evaluation_matches_independent_food_queries_across_time_and_extreme_demands() {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    b.request::<Seed>(|tx, r| {
        let s = tx.edit::<Nutrition>()?;
        s.enabled = r.1;
        let mut d = Diet::new(&s.needs, SimTime(0));
        d.requirement_per_mille = [1, 1000, 1_000_000, u64::MAX][r.0 as usize % 4];
        for (i, key) in s.needs.keys().enumerate() {
            d.reserves.insert(
                key.clone(),
                if r.0.is_multiple_of(5) {
                    u64::MAX
                } else {
                    random(r.0, i as u64, 1) % 10_000_000
                },
            );
        }
        for good in 1..=8 {
            d.learned.insert(
                GoodId(good),
                Learned {
                    nutrients: s
                        .needs
                        .keys()
                        .enumerate()
                        .map(|(i, k)| (k.clone(), random(r.0, good, i as u64) % 1000))
                        .collect(),
                    effects: Effects {
                        health: if good.is_multiple_of(2) {
                            i32::MAX
                        } else {
                            i32::MIN
                        },
                        mood: good as i32 * 17 - 90,
                        alertness: 0,
                    },
                    substance: Some("x".into()),
                },
            );
        }
        d.habits.insert(
            "x".into(),
            Habit {
                level: 1000,
                tolerance: 1000,
                last: SimTime(0),
                duration: HOUR,
            },
        );
        s.people.insert(PersonId(1), d);
        Ok(())
    })
    .unwrap();
    b.request::<Check>(|tx, _| {
        for person in [PersonId(1), PersonId(2)] {
            let evaluation = food_evaluation(tx, person);
            for good in 1..=12 {
                let good = GoodId(good);
                let food = Food {
                    portion: 1,
                    relief: if good.0.is_multiple_of(3) {
                        u64::MAX
                    } else {
                        50
                    },
                    nutrients: Default::default(),
                    effects: FoodEffects::default(),
                    shelf_days: None,
                };
                assert_eq!(
                    evaluation.appetite_for(good),
                    appetite_for(tx, person, good)
                );
                assert_eq!(
                    evaluation.preference(good, &food),
                    preference(tx, person, good, &food)
                );
                let novelty = tx.get::<Nutrition>()?.enabled
                    && tx
                        .get::<Nutrition>()?
                        .people
                        .get(&person)
                        .is_some_and(|d| !d.learned.contains_key(&good));
                assert_eq!(
                    evaluation.novelty(good),
                    if novelty {
                        appetite(tx, person).min(80) as i64
                    } else {
                        0
                    }
                );
            }
        }
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    for at in [0, MINUTE.0, HOUR.0, 14 * DAY.0, 100 * DAY.0, u64::MAX / 2] {
        h.advance(SimTime(at), 1).unwrap();
        for seed in 0..24 {
            h.dispatch(Seed(seed, !seed.is_multiple_of(7))).unwrap();
            h.query(Check).unwrap();
        }
    }
}
