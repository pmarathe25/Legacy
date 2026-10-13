use game_api::*;
use game_assembly::*;
use std::time::Instant;

fn main() {
    let maximum_population = std::env::args()
        .find_map(|argument| {
            argument
                .strip_prefix("--max-population=")
                .map(|value| value.parse::<usize>().expect("maximum population"))
        })
        .unwrap_or(usize::MAX);
    assert!(maximum_population >= 15);
    println!(
        "kernel synthetic fixture; seed=1; build=release; rustc={}",
        command("rustc", &["--version"])
    );
    println!(
        "os={} cpu={} memory={}",
        command("uname", &["-sr"]),
        std::fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .find(|l| l.starts_with("model name"))
            .unwrap_or("unknown"),
        std::fs::read_to_string("/proc/meminfo")
            .unwrap_or_default()
            .lines()
            .next()
            .unwrap_or("unknown")
    );
    let available = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(64);
    let mut workers = vec![1, 2, 4, available];
    workers.sort_unstable();
    workers.dedup();
    for population in [15, 1_000, 10_000, 20_000]
        .into_iter()
        .filter(|population| *population <= maximum_population)
    {
        let mut expected = None;
        for count in &workers {
            let mut samples = Vec::new();
            let mut profile = Profile::default();
            for _ in 0..5 {
                let mut sim = Simulation::kernel_fixture(
                    1,
                    population,
                    Execution {
                        workers: *count,
                        reverse_jobs: false,
                    },
                )
                .unwrap();
                let state = sim.canonical_state();
                let source = game_cargo::inventories(state.components)
                    .map(|(id, _)| id)
                    .last()
                    .unwrap();
                let intents = state
                    .people()
                    .skip(3)
                    .map(|p| TransferIntent {
                        actor: p.id,
                        source,
                        destination: p.inventory,
                        quantity: Quantity(1_000),
                    })
                    .collect();
                sim.debug_schedule_transfers(SimTime(60_000), intents)
                    .unwrap();
                let start = Instant::now();
                sim.advance(SimDuration(60 * 60_000)).unwrap();
                let _view = std::hint::black_box(sim.observe());
                samples.push(start.elapsed().as_secs_f64());
                let checksum = serde_json::to_vec(&sim.canonical_state()).unwrap();
                if let Some(expected) = &expected {
                    assert_eq!(expected, &checksum);
                } else {
                    expected = Some(checksum);
                }
                profile = sim.profile().clone();
                assert_eq!(
                    game_cargo::inventories(sim.canonical_state().components)
                        .map(|(_, inventory)| inventory)
                        .map(|i| i.quantity.0)
                        .sum::<u64>(),
                    1_000
                );
            }
            samples.sort_by(f64::total_cmp);
            let middle = samples[2];
            println!(
                "people={population} areas=3 inventories={} transfer_requests={} workers={count} seed=1 samples=5 median_ms={:.3} p95_ms={:.3} sim_minutes_per_second={:.1} evaluation_ms={:.3} arbitration_ms={:.3} commit_ms={:.3} boundaries={} rss={}",
                population + 1,
                population - 3,
                middle * 1_000.0,
                samples[4] * 1_000.0,
                60.0 / middle,
                profile.evaluation_ns as f64 / 1e6,
                profile.arbitration_ns as f64 / 1e6,
                profile.commit_ns as f64 / 1e6,
                profile.boundaries,
                std::fs::read_to_string("/proc/self/status")
                    .unwrap_or_default()
                    .lines()
                    .find(|l| l.starts_with("VmHWM"))
                    .unwrap_or("unknown")
            );
        }
    }
    for population in [15, 1_000, 10_000]
        .into_iter()
        .filter(|population| *population <= maximum_population)
    {
        for contested in [false, true] {
            let mut expected = None;
            for count in &workers {
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(*count)
                    .build()
                    .unwrap();
                let mut samples = Vec::new();
                for _ in 0..5 {
                    let start = Instant::now();
                    let outcome = benchmark::custom_run(population, &pool, contested, false, 10);
                    samples.push(start.elapsed().as_secs_f64() * 1e3);
                    if let Some(expected) = &expected {
                        assert_eq!(expected, &outcome);
                    } else {
                        expected = Some(outcome);
                    }
                }
                samples.sort_by(f64::total_cmp);
                println!(
                    "synthetic storage=typed people={population} areas=16 sparse_edges={} creations=10 removals=10 decision_candidates=64 workers={count} contested={contested} seed=1 median_ms={:.3} p95_ms={:.3}",
                    population * 2,
                    samples[2],
                    samples[4]
                );
            }
        }
    }
    let session = game_session::Session::spawn(
        Simulation::kernel_fixture(
            1,
            10_000,
            Execution {
                workers: 4,
                reverse_jobs: false,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let run = session.run(SimDuration(365 * 86_400_000)).unwrap();
    let mut times = Vec::new();
    for _ in 0..100 {
        let start = Instant::now();
        std::hint::black_box(session.observe().unwrap());
        times.push(start.elapsed().as_secs_f64() * 1e3);
    }
    let start = Instant::now();
    session.pause().unwrap();
    let pause_ms = start.elapsed().as_secs_f64() * 1e3;
    assert_eq!(run.wait().unwrap().reason, StopReason::Paused);
    times.sort_by(f64::total_cmp);
    println!(
        "host people=10000 workers=4 observation_p95_ms={:.3} observation_p99_ms={:.3} pause_ms={pause_ms:.3}",
        times[94], times[98]
    );
}
fn command(name: &str, args: &[&str]) -> String {
    std::process::Command::new(name)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|_| "unknown".into())
}
