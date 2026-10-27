//! Measure a bounded physiology-owned meal wave, including stock and listeners.
use game_core::*;
use game_physiology::{ConsumeEdible, ConsumeSelfMeals};
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone, Serialize, Deserialize)]
struct Ordered(Vec<ConsumeEdible>);
impl Request for Ordered {
    const KEY: &'static str = "performance.ordered-self-meals";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    b.plugin("meal-wave-benchmark", 1).unwrap();
    b.request::<Ordered>(|tx, r| {
        for meal in r.0 {
            tx.call(meal)?;
        }
        Ok(())
    })
    .unwrap();
    b
}
fn main() {
    let count = game_performance::setting("LEGACY_POPULATION", 64usize);
    let rounds = game_performance::setting("LEGACY_ROUNDS", 5usize);
    assert!((1..=128).contains(&count) && rounds > 0);
    let mut source = builder().build(71, 1).unwrap();
    source
        .dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = source
        .dispatch(game_land::CreateArea {
            name: "meal benchmark".into(),
        })
        .unwrap();
    let inventory = source
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: (count as u64) * 1000,
        })
        .unwrap();
    source
        .dispatch(game_cargo::Deposit {
            inventory,
            good: game_cargo::FOOD,
            quantity: (count as u64) * 10,
        })
        .unwrap();
    let mut meals = Vec::new();
    for _ in 0..count {
        let person = source
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        source
            .dispatch(game_transport::Place { person, area })
            .unwrap();
        meals.push(ConsumeEdible {
            person,
            inventory,
            good: game_cargo::FOOD,
            quantity: 10,
        });
    }
    let before = source.save().unwrap();
    let mut wave_reference = None;
    let mut ordered_reference = None;
    for round in 0..rounds {
        let mut modes = vec![
            ("ordered", 1, usize::MAX),
            ("wave-serial", 1, usize::MAX),
            ("wave-serial", 12, usize::MAX),
            ("wave-parallel", 4, 32),
            ("wave-parallel", 12, 32),
        ];
        if round % 2 == 1 {
            modes.reverse();
        }
        for (mode, workers, threshold) in modes {
            let mut host = builder().load(&before, workers).unwrap();
            let start = Instant::now();
            if mode == "ordered" {
                host.dispatch(Ordered(meals.clone())).unwrap();
            } else {
                host.dispatch(ConsumeSelfMeals {
                    meals: meals.clone(),
                    minimum_parallel: threshold,
                })
                .unwrap();
            }
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            let saved = host.save().unwrap();
            let reference = if mode == "ordered" {
                &mut ordered_reference
            } else {
                &mut wave_reference
            };
            if let Some(expected) = reference {
                assert_eq!(&saved, expected, "worker or threshold changed results");
            } else {
                *reference = Some(saved);
            }
            println!(
                "round={round} people={count} mode={mode} workers={workers} elapsed_ms={elapsed:.6}"
            );
        }
    }
}
