//! Compare the same elapsed world across worker pools, excluding loading and reporting.
use game_economy::core_interval::{CoreHost, CoreHostSnapshot, CoreRun, CoreRunEvent};
use rayon::ThreadPoolBuilder;
use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Outcome {
    state: u64,
    events: u64,
    event_count: usize,
}

#[derive(Clone, Copy, Debug)]
enum Read {
    None,
    Person,
    Snapshot,
}

impl Read {
    fn observe(self, host: &CoreHost, person: Option<u64>) {
        match self {
            Self::None => (),
            Self::Person => {
                std::hint::black_box(person.and_then(|id| host.core().person(id)));
            }
            Self::Snapshot => {
                std::hint::black_box(host.snapshot());
            }
        }
    }
}

fn sample(
    opening: &CoreHostSnapshot,
    duration: u64,
    step_millis: u64,
    retain_preparation: bool,
    read: Read,
) -> Result<(Duration, Outcome), String> {
    let mut host = CoreHost::resume(opening.clone())?;
    let person = opening.core.people.first().map(|person| person.id);
    let (elapsed, observations) = if retain_preparation {
        host.observe_continuously(&[], |stepper| {
            observe(duration, step_millis, |duration| {
                let result = stepper.advance(duration)?;
                match read {
                    Read::None => (),
                    Read::Person => {
                        std::hint::black_box(person.and_then(|id| stepper.person(id)));
                    }
                    Read::Snapshot => {
                        std::hint::black_box(stepper.snapshot());
                    }
                }
                Ok(result)
            })
        })?
    } else {
        observe(duration, step_millis, |duration| {
            let result = host.advance_continuous(duration, &[])?;
            read.observe(&host, person);
            Ok(result)
        })?
    };
    let saved = serde_json::to_vec(&host.snapshot()).map_err(|error| error.to_string())?;
    let hash = |bytes: &[u8]| {
        let mut hash = DefaultHasher::new();
        hash.write(bytes);
        hash.finish()
    };
    Ok((
        elapsed,
        Outcome {
            state: hash(&saved),
            events: hash(format!("{observations:?}").as_bytes()),
            event_count: observations.len(),
        },
    ))
}

fn observe(
    duration: u64,
    step_millis: u64,
    mut advance: impl FnMut(u64) -> Result<CoreRun, String>,
) -> Result<(Duration, Vec<CoreRunEvent>), String> {
    let mut remaining = duration;
    let mut observations = Vec::new();
    let start = Instant::now();
    while remaining > 0 {
        let requested = remaining.min(step_millis);
        let run = advance(requested)?;
        if let Some(error) = run.failure {
            return Err(error);
        }
        if run.elapsed_millis != requested {
            return Err("world stopped before advancing the requested duration".into());
        }
        remaining -= requested;
        observations.extend(run.events);
    }
    let elapsed = start.elapsed();

    Ok((elapsed, observations))
}

fn main() -> Result<(), String> {
    let usage = "usage: elapsed_scaling SCENARIO HOURS REPEATS [POPULATION_PROFILE]";
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let (values, population) = match arguments.as_slice() {
        [_, _, _] => (arguments.as_slice(), None),
        [values @ .., profile] if values.len() == 3 => (values, Some(Path::new(profile))),
        _ => return Err(usage.into()),
    };
    let number = |index: usize| -> Result<u64, String> {
        values[index]
            .to_str()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|&number| number > 0)
            .ok_or_else(|| usage.into())
    };
    let duration = number(1)?
        .checked_mul(3_600_000)
        .ok_or("duration overflow")?;
    let repeats = number(2)?;
    let step_millis =
        std::env::var_os("LEGACY_BENCH_ADVANCE_MILLIS").map_or(Ok(60_000), |value| {
            value
                .to_str()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|&value| value > 0)
                .ok_or("invalid LEGACY_BENCH_ADVANCE_MILLIS")
        })?;
    let read = match std::env::var("LEGACY_BENCH_READ")
        .as_deref()
        .unwrap_or("none")
    {
        "none" => Read::None,
        "person" => Read::Person,
        "snapshot" => Read::Snapshot,
        _ => return Err("LEGACY_BENCH_READ must be none, person, or snapshot".into()),
    };
    let retain_preparation = std::env::var_os("LEGACY_BENCH_REPLAN").is_none();
    let scenario = Path::new(&values[0]);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut host = game_economy_cli::core_scenario_with_options(
        scenario,
        &root.join("content"),
        None,
        population,
    )?;
    host.detach_player_control()?;
    let opening = host.snapshot();
    let workers_available = std::thread::available_parallelism().map_or(1, usize::from);
    let mut expected = None;
    let mut first = None;
    println!(
        "people={} duration_millis={duration} advance_millis={step_millis} repeats={repeats} retain_preparation={retain_preparation} read={read:?}",
        opening.core.people.len()
    );
    for workers in [1, 2, 4, 6, 8]
        .into_iter()
        .filter(|&count| count <= workers_available)
    {
        let pool = ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|error| error.to_string())?;
        #[cfg(feature = "profile")]
        let profile_before = game_diagnostics::snapshot();
        let mut timings = Vec::new();
        for _ in 0..repeats {
            let (elapsed, outcome) =
                pool.install(|| sample(&opening, duration, step_millis, retain_preparation, read))?;
            if outcome != *expected.get_or_insert(outcome) {
                return Err(format!("simulation diverged at {workers} workers"));
            }
            timings.push(elapsed);
        }
        timings.sort_unstable();
        let median = timings[timings.len() / 2].as_secs_f64();
        let baseline = *first.get_or_insert(median);
        let outcome = expected.expect("positive repeat count");
        println!(
            "workers={workers} median_ms={:.3} speedup={:.2} events={} state={:016x} stream={:016x}",
            median * 1000.0,
            baseline / median,
            outcome.event_count,
            outcome.state,
            outcome.events,
        );
        #[cfg(feature = "profile")]
        for (key, after) in game_diagnostics::snapshot() {
            if key.starts_with("elapsed.") {
                let before = profile_before.get(key);
                let calls = after.calls - before.map_or(0, |row| row.calls);
                let nanoseconds = after.nanoseconds - before.map_or(0, |row| row.nanoseconds);
                println!("workers={workers} phase={key} count={calls} ns={nanoseconds}");
            }
        }
    }
    Ok(())
}
