use game_ecs::{Batch, ReadFootprint, ReadSet, Schema, StableId, Store};
use std::{hint::black_box, time::Instant};

fn run(rows: u64) -> u128 {
    let mut schema = Schema::new();
    schema.register::<u64>("bench/value", |_| Ok(())).unwrap();
    let mut store = Store::with_schema(schema);
    let mut initial = Batch::new();
    for id in 1..=rows {
        initial.insert(StableId(id), id);
    }
    store.apply_batch(initial).unwrap();
    let mut samples = Vec::new();
    for _ in 0..31 {
        let start = Instant::now();
        let mut reads = ReadSet::new();
        let mut admitted = ReadFootprint::new();
        let mut batch = Batch::new();
        for id in 1..=rows {
            let id = StableId(id);
            let value = *reads.read::<u64>(&store, id).unwrap();
            admitted.protect::<u64>(id);
            batch.insert(id, value + 1);
        }
        let guard = reads.bind(&store, &batch, &admitted).unwrap();
        guard.validate(&store, &batch).unwrap();
        store.apply_batch(batch).unwrap();
        black_box(&store);
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();
    samples[samples.len() / 2]
}
fn main() {
    let mut previous = None;
    println!("rows,median_ns,ns_per_row,ratio_to_previous");
    for rows in [1_000, 2_000, 4_000] {
        let nanos = run(rows);
        let ratio = previous.map_or(0.0, |before| nanos as f64 / before as f64);
        println!(
            "{rows},{nanos},{:.1},{ratio:.3}",
            nanos as f64 / rows as f64
        );
        previous = Some(nanos);
    }
}
