//! Repeated trait projection for complex profiles, using the actual owner API.
use game_core::*;
use game_sim::*;
use game_traits::*;
use std::{collections::BTreeMap, hint::black_box, time::Instant};
fn main() {
    game_performance::metrics::metadata();
    let people = game_performance::setting("LEGACY_POPULATION", 1000u64);
    let traits = game_performance::setting("LEGACY_TRAITS", 1000u64);
    let rounds = game_performance::setting("LEGACY_ROUNDS", 10u64);
    let mut definitions = Traits::default();
    for id in 0..traits {
        definitions.definitions.insert(
            format!("trait:{id:05}"),
            Trait {
                name: format!("Trait {id}"),
                minimum: -100,
                maximum: 100,
                default: 0,
                weights: BTreeMap::from([("need".into(), 3), ("risk".into(), -2)]),
            },
        );
    }
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    let mut host = b.build(1, 1).unwrap();
    host.dispatch(Install(definitions)).unwrap();
    for person in 0..people {
        host.dispatch(Set {
            person: PersonId(person + 1),
            profile: (0..traits)
                .map(|id| (format!("trait:{id:05}"), ((person + id) % 201) as i32 - 100))
                .collect(),
        })
        .unwrap();
    }
    let expected: Vec<_> = (0..people)
        .map(|person| {
            let (mut need, mut risk) = (100i64, 100i64);
            for id in 0..traits {
                let value = ((person + id) % 201) as i64 - 100;
                need = (need + value * 3 / 100).clamp(0, 1000);
                risk = (risk - value * 2 / 100).clamp(0, 1000);
            }
            (need, risk)
        })
        .collect();
    let sample = game_performance::metrics::Sample::new();
    let start = Instant::now();
    for _ in 0..rounds {
        for person in 1..=people {
            let w = black_box(host.get::<Traits>().unwrap()).weights(black_box(PersonId(person)));
            assert_eq!((w["need"], w["risk"]), expected[(person - 1) as usize]);
        }
    }
    let elapsed = start.elapsed().as_secs_f64() * 1000.;
    sample.report("trait_projection");
    game_performance::metrics::rss();
    let save = host.save().unwrap();
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    let loaded = b.load(&save, 4).unwrap();
    assert_eq!(loaded.save().unwrap(), save);
    for person in 1..=people {
        let w = loaded.get::<Traits>().unwrap().weights(PersonId(person));
        assert_eq!((w["need"], w["risk"]), expected[(person - 1) as usize]);
    }
    println!(
        "people={people} traits={traits} rounds={rounds} elapsed_ms={elapsed:.3} save_bytes={} hash={:016x}",
        save.len(),
        game_performance::hash(save.as_bytes())
    );
}
