//! Isolated feature benchmark: actual daily crop logic, including preparation,
//! immutable joins, executor dispatch, staged rows, guards, and commit.
use game_agriculture::{
    Catalog, Crop, CropPhase, Field, LocalWeatherDay, Soil, WeatherFacts, World,
};
use game_core::{AreaId, Error, InventoryId, SimTime, SiteId};
use game_sim::{Execution, Executor};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::Instant,
};

const DAY: u64 = 86_400_000;
const IDLE_STEPS: u64 = 1_000;
#[derive(Clone, Copy)]
enum Mode {
    Snapshot,
    Serial,
    Ordered(usize),
}
impl Mode {
    fn name(self) -> String {
        match self {
            Self::Snapshot => "snapshot_baseline".into(),
            Self::Serial => "components_serial".into(),
            Self::Ordered(workers) => format!("components_ordered_{workers}"),
        }
    }
}
fn initial_world(fields: usize, path: &PathBuf) -> Result<World, Error> {
    let mut world = World::from_content(Catalog::read(path)?, 0)?;
    let definition = world.catalog.default_crop.clone();
    let rules = world.catalog.crops[&definition].clone();
    for index in 0..fields {
        let id = SiteId(index as u64 + 1);
        let inventory = |slot| InventoryId(index as u64 * 4 + slot);
        world.insert_field(Field {
            id,
            area: AreaId(index as u64 / 100 + 1),
            access: BTreeSet::new(),
            custodian: None,
            seed: inventory(1),
            tool: inventory(2),
            water: inventory(3),
            food: inventory(4),
        })?;
        let mut crop = Crop::new(definition.clone());
        crop.phase = CropPhase::Growing;
        crop.preparation_ms = rules.prepare_labor.0;
        crop.planting_ms = rules.plant_labor.0;
        crop.tending = (index % 1_001) as u16;
        world.insert_crop(id, crop)?;
        world.insert_soil(
            id,
            Soil {
                fertility: 500 + (index % 501) as u16,
                water: 40 + index as u64 % 21,
                capacity: 100,
            },
        )?;
    }
    world.validate(SimTime(0), Some(365))?;
    Ok(world)
}
fn weather(world: &World, days: u64) -> Vec<LocalWeatherDay> {
    let areas: BTreeSet<_> = world
        .fields()
        .map(|(_, field)| field)
        .map(|field| field.area)
        .collect();
    (1..=days)
        .map(|day| LocalWeatherDay {
            day,
            day_of_year: day % 365,
            cells: areas
                .iter()
                .map(|area| {
                    (
                        *area,
                        WeatherFacts {
                            temperature: 15 + ((day + area.0) % 16) as i32,
                            rain: 4 + (day + area.0) % 7,
                            evaporation: 4 + (day * 3 + area.0) % 5,
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        })
        .collect()
}
fn execute(
    mut world: World,
    days: &[LocalWeatherDay],
    mode: Mode,
) -> Result<(World, f64, usize), Error> {
    // Executor creation is startup work; every advance pays its full dispatch cost.
    let executor = match mode {
        Mode::Ordered(workers) => Some(Executor::new(Execution {
            workers,
            reverse_jobs: false,
        })?),
        _ => None,
    };
    let mut growth_rows = 0;
    let started = Instant::now();
    for day in days {
        let from = SimTime((day.day - 1) * DAY);
        let next = SimTime(day.day * DAY);
        match mode {
            Mode::Snapshot => {
                growth_rows += world.fields().count();
                world = world.advance(from, next, Some(day))?;
            }
            Mode::Serial => {
                let prepared = world.prepare_advance_serial(from, next, Some(day))?;
                growth_rows += prepared.growth_rows();
                world.commit_advance(prepared)?;
            }
            Mode::Ordered(_) => {
                let prepared = world.prepare_advance(from, next, Some(day), |inputs| {
                    executor
                        .as_ref()
                        .unwrap()
                        .map_ordered(inputs, |input| input.evaluate())
                })?;
                growth_rows += prepared.growth_rows();
                world.commit_advance(prepared)?;
            }
        }
    }
    let milliseconds = started.elapsed().as_secs_f64() * 1_000.;
    Ok((world, milliseconds, growth_rows))
}
fn resident_high_water_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")
            .and_then(|value| value.split_whitespace().next()?.parse().ok())
    })
}
fn check_idle(world: &World) -> Result<(), Error> {
    let mut next_world = world.clone();
    let mut callback_calls = 0;
    let started = Instant::now();
    for step in 0..IDLE_STEPS {
        let prepared =
            next_world.prepare_advance(SimTime(step), SimTime(step + 1), None, |_| {
                callback_calls += 1;
                Err(Error::Unavailable)
            })?;
        assert_eq!(prepared.growth_rows(), 0);
        next_world.commit_advance(prepared)?;
    }
    let milliseconds = started.elapsed().as_secs_f64() * 1_000.;
    assert_eq!(callback_calls, 0);
    assert_eq!(&next_world, world);
    println!(
        "mode=subday_idle fields={} jobs={} steps={IDLE_STEPS} growth_rows=0 callback_calls={callback_calls} elapsed_ms={milliseconds:.3}",
        world.fields().count(),
        world.jobs.len()
    );
    Ok(())
}
fn argument<T: std::str::FromStr>(args: &[String], index: usize, default: T) -> T {
    args.get(index).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| panic!("invalid argument {index}: {value}"))
    })
}
fn main() -> Result<(), Error> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let fields: usize = argument(&args, 0, 10_000);
    let days: u64 = argument(&args, 1, 30);
    let samples: usize = argument(&args, 2, 3);
    assert!(fields > 0 && days > 0 && samples > 0);
    let path = args.get(3).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/agriculture/crops.toml")
    });
    let initial = initial_world(fields, &path)?;
    let days = weather(&initial, days);
    check_idle(&initial)?;
    let mut expected = None;
    // This measures one isolated system, not full-game performance. The snapshot
    // mode is the current convenience API, not a timing from an older release.
    for mode in [
        Mode::Serial,
        Mode::Ordered(1),
        Mode::Ordered(2),
        Mode::Ordered(4),
        Mode::Snapshot,
    ] {
        let mut timings = Vec::new();
        let mut rows = 0;
        let mut final_world = None;
        for sample in 0..samples {
            let (world, milliseconds, evaluated) = execute(initial.clone(), &days, mode)?;
            if let Some(expected) = &expected {
                assert_eq!(&world, expected, "mode={} sample={sample}", mode.name());
            } else {
                expected = Some(world.clone());
            }
            rows = evaluated;
            timings.push(milliseconds);
            final_world = Some(world);
        }
        timings.sort_by(f64::total_cmp);
        let median = timings[timings.len() / 2];
        let final_world = final_world.unwrap();
        final_world.validate(SimTime(days.len() as u64 * DAY), Some(365))?;
        let saved = serde_json::to_vec(&final_world).unwrap();
        let restored: World = serde_json::from_slice(&saved).unwrap();
        restored.validate(SimTime(days.len() as u64 * DAY), Some(365))?;
        assert_eq!(final_world, restored);
        let growing = final_world
            .crops()
            .map(|(_, crop)| crop)
            .filter(|crop| crop.phase == CropPhase::Growing)
            .count();
        let ripe = final_world
            .crops()
            .map(|(_, crop)| crop)
            .filter(|crop| crop.phase == CropPhase::Ripe)
            .count();
        let failed = final_world
            .crops()
            .map(|(_, crop)| crop)
            .filter(|crop| crop.phase == CropPhase::Failed)
            .count();
        println!(
            "mode={} fields={fields} jobs={} days={} samples={samples} growth_rows={rows} median_ms={median:.3} min_ms={:.3} max_ms={:.3} rows_per_second={:.0} growing={growing} ripe={ripe} failed={failed} save_bytes={} process_peak_rss_kib={:?}",
            mode.name(),
            final_world.jobs.len(),
            days.len(),
            timings[0],
            timings[timings.len() - 1],
            rows as f64 / (median / 1_000.),
            saved.len(),
            resident_high_water_kib()
        );
    }
    println!("canonical_equivalence=true save_roundtrip=true scope=isolated_agriculture");
    Ok(())
}
