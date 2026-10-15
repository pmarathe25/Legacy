//! Synthetic protocol workload. These numbers do not establish full-game performance.
use game_core::*;
use game_sim::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Instant;
#[derive(Clone, Default, Serialize, Deserialize)]
struct Workload {
    rows: Vec<u64>,
    stock: u64,
    winners: u64,
}
impl Resource for Workload {
    const KEY: &'static str = "benchmark.workload";
}
#[derive(Clone, Serialize, Deserialize)]
struct Initialize {
    count: usize,
}
impl Request for Initialize {
    const KEY: &'static str = "benchmark.initialize";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Step;
impl Request for Step {
    const KEY: &'static str = "benchmark.step";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Claim;
impl Request for Claim {
    const KEY: &'static str = "benchmark.claim";
    type Response = bool;
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.resource::<Workload>().unwrap();
    b.request::<Initialize>(|tx, r| {
        *tx.edit::<Workload>()? = Workload {
            rows: vec![0; r.count],
            stock: 1000,
            winners: 0,
        };
        tx.schedule(SimTime(MINUTE.0), 10, Step)?;
        tx.schedule(SimTime(MINUTE.0), 20, Claim)?;
        tx.schedule(SimTime(MINUTE.0), 20, Claim)?;
        Ok(())
    })
    .unwrap();
    b.request::<Step>(|tx, _| {
        let rows = tx.get::<Workload>()?.rows.clone();
        let evaluated = tx
            .compute()
            .install(|| rows.par_iter().map(|v| v + 1).collect());
        tx.edit::<Workload>()?.rows = evaluated;
        tx.schedule(tx.now().after(MINUTE)?, 10, Step)?;
        Ok(())
    })
    .unwrap();
    b.request::<Claim>(|tx, _| {
        let state = tx.edit::<Workload>()?;
        let allowed = state.stock >= 1000;
        if allowed {
            state.stock -= 1000;
            state.winners += 1;
        }
        Ok(allowed)
    })
    .unwrap();
    b
}
fn main() {
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "CPU unavailable".into());
    println!(
        "{cpu}; package={}; arch={}; seed=1; synthetic ticks=60",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH
    );
    for count in [15, 1000, 10_000] {
        let mut canonical = None;
        for workers in [1, 4] {
            let mut host = builder().build(1, workers).unwrap();
            host.dispatch(Initialize { count }).unwrap();
            let start = Instant::now();
            while host.advance(SimTime(MINUTE.0 * 60), 32).unwrap().stop == Stop::Budget {}
            let elapsed = start.elapsed();
            let s = host.get::<Workload>().unwrap();
            assert_eq!(s.winners, 1);
            assert_eq!(s.stock, 0);
            assert!(s.rows.iter().all(|v| *v == 60));
            let save = host.save().unwrap();
            if let Some(reference) = &canonical {
                assert_eq!(&save, reference);
            } else {
                canonical = Some(save);
            }
            println!(
                "population={count} workers={workers} elapsed_ms={:.3} ns_per_person_tick={:.1}",
                elapsed.as_secs_f64() * 1000.0,
                elapsed.as_nanos() as f64 / (count as f64 * 60.0)
            );
        }
    }
}
