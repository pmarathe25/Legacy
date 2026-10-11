//! Compare sparse typed iteration with the previous all-ID probing approach.
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::{hint::black_box, time::Instant};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Foreign(u64);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Sparse(u64);

fn main() {
    println!("foreign_entities,matching_rows,repeats,typed_us,all_id_probe_us");
    for foreign in [0, 1_000, 10_000, 100_000] {
        let mut schema = Schema::new();
        schema
            .register::<Foreign>("bench/foreign", |_| Ok(()))
            .unwrap();
        schema
            .register::<Sparse>("bench/sparse", |_| Ok(()))
            .unwrap();
        let mut store = Store::with_schema(schema);
        for id in 1..=foreign {
            store.insert_or_spawn(StableId(id), Foreign(id)).unwrap();
        }
        for id in [foreign + 3, foreign + 1, foreign + 2] {
            store.insert_or_spawn(StableId(id), Sparse(id)).unwrap();
        }
        let expected = (foreign + 1..=foreign + 3).sum::<u64>();
        // `cargo test --all-targets` also runs harness-free benchmarks. Keep
        // that debug smoke check short; measure with the release benchmark.
        let repeats = if cfg!(debug_assertions) { 1 } else { 1_000 };
        let start = Instant::now();
        for _ in 0..repeats {
            assert_eq!(
                black_box(&store)
                    .iter::<Sparse>()
                    .map(|(_, row)| row.0)
                    .sum::<u64>(),
                expected
            );
        }
        let typed = start.elapsed();
        let start = Instant::now();
        for _ in 0..repeats {
            assert_eq!(
                black_box(&store)
                    .ids()
                    .filter_map(|id| store.get::<Sparse>(id))
                    .map(|row| row.0)
                    .sum::<u64>(),
                expected
            );
        }
        println!(
            "{foreign},3,{repeats},{},{}",
            typed.as_micros(),
            start.elapsed().as_micros()
        );
    }
}
