//! Real feature composition: autonomous meals, rest, knowledge, travel and actual stock.
use game_assembly::Simulation;
use game_core::*;
use game_sim::Stop;
use std::time::Instant;
fn main() {
    let count: usize = std::env::var("LEGACY_POPULATION")
        .unwrap_or("1000".into())
        .parse()
        .unwrap();
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or("1".into())
        .parse()
        .unwrap();
    let ticks: u64 = std::env::var("LEGACY_MINUTES")
        .unwrap_or("120".into())
        .parse()
        .unwrap();
    game_performance::metrics::metadata();
    let sample = game_performance::metrics::Sample::new();
    let start = Instant::now();
    let distributed = game_performance::setting("LEGACY_DISTRIBUTED", false);
    let (mut sim, stores) = game_performance::fixture(count, workers, distributed);
    let h = sim.debug_host();
    println!(
        "areas={} inventories={} active_tasks={} shipments=0 relationships={} facts={} institutions=1 agreements=0 conflicts=0 historical_records={} plots=1 patches={}",
        h.get::<game_land::Land>().unwrap().0.len(),
        h.get::<game_cargo::Cargo>().unwrap().0.len(),
        h.get::<game_sim::Actions>().unwrap().0.len(),
        h.get::<game_relationships::Social>()
            .unwrap()
            .relationships
            .values()
            .map(|r| r.len())
            .sum::<usize>(),
        h.get::<game_knowledge::Knowledge>()
            .unwrap()
            .0
            .values()
            .map(|r| r.len())
            .sum::<usize>(),
        (count - 15) * game_performance::setting("LEGACY_HISTORY", 0usize),
        h.get::<game_ecology::Ecology>().unwrap().patches.len()
    );
    let freeze = Instant::now();
    let readers = game_performance::setting("LEGACY_HOLD_READERS", false).then(|| {
        (
            h.get::<game_physiology::Bodies>().unwrap().clone(),
            h.get::<game_characters::Minds>().unwrap().clone(),
            h.get::<game_knowledge::Knowledge>().unwrap().clone(),
            h.get::<game_transport::Positions>().unwrap().clone(),
            h.get::<game_sim::Actions>().unwrap().clone(),
            h.get::<game_property::Property>().unwrap().clone(),
        )
    });
    let freeze_ms = freeze.elapsed().as_secs_f64() * 1000.;
    let reader_hash = readers.as_ref().map(fingerprint);
    println!(
        "held_readers={} freeze_ms={freeze_ms:.6}",
        readers.is_some()
    );
    let initial = game_performance::stock(&sim, &stores);
    let init = start.elapsed();
    sample.report("initialize");
    game_performance::metrics::rss();
    let sample = game_performance::metrics::Sample::new();
    let start = Instant::now();
    let mut boundaries = vec![];
    loop {
        let t = Instant::now();
        let p = sim.advance(SimTime(ticks * MINUTE.0), 1).unwrap();
        boundaries.push(t.elapsed().as_secs_f64() * 1000.);
        if p.stop != Stop::Budget {
            break;
        }
    }
    let elapsed = start.elapsed();
    sample.report("advance");
    game_performance::metrics::rss();
    assert_eq!(
        sim.debug_host()
            .get::<game_physiology::Bodies>()
            .unwrap()
            .people
            .len(),
        count
    );
    assert!(
        sim.debug_host()
            .get::<game_physiology::Bodies>()
            .unwrap()
            .people
            .values()
            .all(|p| p.alive && p.health > 0)
    );
    assert_eq!(
        readers.as_ref().map(fingerprint),
        reader_hash,
        "committed readers changed"
    );
    let remaining = game_performance::stock(&sim, &stores);
    assert!(remaining < initial);
    if ticks == 120 {
        assert_eq!(initial - remaining, (count as u64 - 3) * 10);
    }
    boundaries.sort_by(f64::total_cmp);
    let structural = game_performance::metrics::Sample::new();
    let follow = game_performance::exercise::prepare(&mut sim, count);
    structural.report("birth_death_shipments");
    let sample = game_performance::metrics::Sample::new();
    let t = Instant::now();
    let save = sim.save().unwrap();
    let saving = t.elapsed();
    if let Ok(path) = std::env::var("LEGACY_SNAPSHOT") {
        std::fs::write(path, &save).unwrap();
    }
    let t = Instant::now();
    let mut loaded = Simulation::load(&save, workers).unwrap();
    let loading = t.elapsed();
    assert_eq!(loaded.save().unwrap(), save);
    let next = SimTime((ticks + 3) * MINUTE.0);
    while sim.advance(next, 7).unwrap().stop == Stop::Budget {}
    while loaded.advance(next, 19).unwrap().stop == Stop::Budget {}
    assert_eq!(loaded.save().unwrap(), sim.save().unwrap());
    follow.check(&sim);
    follow.check(&loaded);
    assert_eq!(
        readers.as_ref().map(fingerprint),
        reader_hash,
        "readers changed through structural churn"
    );
    sample.report("save_load_continue");
    game_performance::metrics::rss();
    let hash = save.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    println!(
        "population={count} workers={workers} minutes={ticks} init_ms={:.3} elapsed_ms={:.3} minutes_per_second={:.1} boundary_p95_ms={:.3} boundary_p99_ms={:.3} boundary_max_ms={:.3} save_ms={:.3} load_ms={:.3} save_bytes={} consumed={} hash={hash:016x}",
        init.as_secs_f64() * 1000.,
        elapsed.as_secs_f64() * 1000.,
        ticks as f64 / elapsed.as_secs_f64(),
        boundaries[boundaries.len() * 95 / 100],
        boundaries[boundaries.len() * 99 / 100],
        boundaries.last().unwrap(),
        saving.as_secs_f64() * 1000.,
        loading.as_secs_f64() * 1000.,
        save.len(),
        initial - remaining
    );
}

fn fingerprint<T: serde::Serialize>(value: &T) -> u64 {
    struct Sink(u64);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            for b in bytes {
                self.0 = (self.0 ^ u64::from(*b)).wrapping_mul(0x100000001b3);
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut sink = Sink(0xcbf29ce484222325);
    serde_json::to_writer(&mut sink, value).unwrap();
    sink.0
}
