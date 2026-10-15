//! Actual cargo owner requests: disjoint stores and a contested reservation book.
use game_cargo::*;
use game_performance::{hash, metrics};
use game_sim::Builder;
use std::time::Instant;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    b
}
fn run(count: usize, contested: bool, workers: usize) -> String {
    let mut h = builder().build(1, workers).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "ledger fixture".into(),
        })
        .unwrap();
    let inventories: Vec<_> = (0..if contested { 1 } else { count })
        .map(|_| {
            h.dispatch(Create {
                custody: Custody::Area(area),
                capacity: if contested { count as u64 } else { 1 },
            })
            .unwrap()
        })
        .collect();
    let sample = metrics::Sample::new();
    let start = Instant::now();
    let mut latencies = vec![];
    let tokens: Vec<_> = (0..count)
        .map(|i| {
            let t = Instant::now();
            let inventory = inventories[i % inventories.len()];
            let token = h
                .dispatch(ReserveSpace {
                    inventory,
                    good: FOOD,
                    quantity: 1,
                })
                .unwrap();
            latencies.push(t.elapsed().as_secs_f64() * 1000.);
            (inventory, token)
        })
        .collect();
    let reserve = start.elapsed();
    sample.report("reservations");
    let saved = h.save().unwrap();
    assert!(
        h.dispatch(ReserveSpace {
            inventory: inventories[0],
            good: FOOD,
            quantity: 1
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), saved);
    let mut loaded = builder().load(&saved, workers).unwrap();
    let start = Instant::now();
    for (index, (inventory, reservation)) in tokens.into_iter().enumerate() {
        if index.is_multiple_of(2) {
            loaded
                .dispatch(ReleaseSpace {
                    inventory,
                    reservation,
                })
                .unwrap();
        } else {
            loaded
                .dispatch(DepositReserved {
                    inventory,
                    reservation,
                    good: FOOD,
                    quantity: 1,
                })
                .unwrap();
        }
    }
    let finish = start.elapsed();
    let cargo = loaded.get::<Cargo>().unwrap();
    assert!(
        cargo
            .0
            .values()
            .all(|i| i.reserved.is_empty() && i.reserved_mass == 0)
    );
    assert_eq!(
        cargo.0.values().map(|i| i.amount(FOOD)).sum::<u64>(),
        (count / 2) as u64
    );
    latencies.sort_by(f64::total_cmp);
    let final_state = loaded.save().unwrap();
    println!(
        "reservations={count} contested={contested} inventories={} workers={workers} reserve_ms={:.3} finish_ms={:.3} command_p95_ms={:.3} command_p99_ms={:.3} save_bytes={} hash={:016x}",
        inventories.len(),
        reserve.as_secs_f64() * 1000.,
        finish.as_secs_f64() * 1000.,
        metrics::percentile(&latencies, 95),
        metrics::percentile(&latencies, 99),
        saved.len(),
        hash(final_state.as_bytes())
    );
    final_state
}
fn main() {
    metrics::metadata();
    for count in [15, 1000, 10_000, 20_000] {
        for contested in [false, true] {
            let mut reference = None;
            for workers in [1, 4] {
                for _ in 0..3 {
                    let state = run(count, contested, workers);
                    if let Some(old) = &reference {
                        assert_eq!(&state, old);
                    } else {
                        reference = Some(state);
                    }
                }
            }
        }
    }
    metrics::rss();
}
