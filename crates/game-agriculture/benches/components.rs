//! Isolated feature benchmark: actual daily crop logic, including preparation,
//! immutable joins, executor dispatch, staged rows, guards, and commit.
use game_agriculture::{
    Catalog, Crop, CropPhase, Field, LocalWeatherDay, Queries, Runtime, Soil, WeatherFacts,
    register_components,
};
use game_core::{AreaId, Error, InventoryId, SimTime, SiteId};
use game_ecs::{Schema, Snapshot, Store};
use game_sim::{Execution, Executor};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::Instant,
};

const DAY: u64 = 86_400_000;
const IDLE_STEPS: u64 = 1_000;
#[derive(Clone, Copy)]
enum Mode {
    Serial,
    Ordered(usize),
}
impl Mode {
    fn name(self) -> String {
        match self {
            Self::Serial => "components_serial".into(),
            Self::Ordered(workers) => format!("components_ordered_{workers}"),
        }
    }
}
/// Benchmark composition owns the store. Runtime has no component authority;
/// query metadata is bound outside the saved data and rebuilt after restoration.
struct Fixture {
    runtime: Runtime,
    store: Store,
    queries: Queries,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Saved {
    runtime: Runtime,
    components: Snapshot,
}
fn schema() -> Result<Schema, Error> {
    let mut schema = Schema::new();
    register_components(&mut schema)?;
    game_land::register_components(&mut schema)?;
    Ok(schema)
}
impl Fixture {
    fn saved(&self) -> Result<Saved, Error> {
        Ok(Saved {
            runtime: self.runtime.clone(),
            components: self.store.snapshot()?,
        })
    }
    fn restore(saved: Saved) -> Result<Self, Error> {
        let mut store = Store::restore(schema()?, saved.components)?;
        let queries = Runtime::bind(&mut store);
        Ok(Self {
            runtime: saved.runtime,
            store,
            queries,
        })
    }
    fn validate(&self, now: SimTime) -> Result<(), Error> {
        self.runtime.validate(&self.store, now, Some(365))
    }
}
fn initial_world(fields: usize, path: &PathBuf, reverse: bool) -> Result<Fixture, Error> {
    let runtime = Runtime::from_content(Catalog::read(path)?, 0)?;
    let mut store = Store::with_schema(schema()?);
    let definition = runtime.catalog.default_crop.clone();
    let rules = &runtime.catalog.crops[&definition];
    let mut indices: Vec<_> = (0..fields).collect();
    if reverse {
        indices.reverse();
    }
    for index in indices {
        let id = SiteId(index as u64 + 1);
        let inventory = |slot| InventoryId(index as u64 * 4 + slot);
        runtime.insert_field(
            &mut store,
            Field {
                id,
                area: AreaId(index as u64 / 100 + 1),
                access: BTreeSet::new(),
                custodian: None,
                seed: inventory(1),
                tool: inventory(2),
                water: inventory(3),
                food: inventory(4),
            },
        )?;
        let mut crop = Crop::new(definition.clone());
        crop.phase = CropPhase::Growing;
        crop.preparation_ms = rules.prepare_labor.0;
        crop.planting_ms = rules.plant_labor.0;
        crop.tending = (index % 1_001) as u16;
        runtime.insert_crop(&mut store, id, crop)?;
        runtime.insert_soil(
            &mut store,
            id,
            Soil {
                fertility: 500 + (index % 501) as u16,
                water: 40 + index as u64 % 21,
                capacity: 100,
            },
        )?;
    }
    let queries = Runtime::bind(&mut store);
    let fixture = Fixture {
        runtime,
        store,
        queries,
    };
    fixture.validate(SimTime(0))?;
    Ok(fixture)
}
fn weather(world: &Fixture, days: u64) -> Vec<LocalWeatherDay> {
    let areas: BTreeSet<_> = world
        .runtime
        .fields(&world.store)
        .map(|(_, field)| field.area)
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
    mut world: Fixture,
    days: &[LocalWeatherDay],
    mode: Mode,
) -> Result<(Fixture, f64, usize), Error> {
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
        let prepared = match mode {
            Mode::Serial => world.runtime.prepare_advance_serial(
                &world.store,
                &world.queries,
                from,
                next,
                Some(day),
            )?,
            Mode::Ordered(_) => world.runtime.prepare_advance(
                &world.store,
                &world.queries,
                from,
                next,
                Some(day),
                |inputs| {
                    executor
                        .as_ref()
                        .unwrap()
                        .map_ordered(inputs, |input| input.evaluate())
                },
            )?,
        };
        growth_rows += prepared.growth_rows();
        world.runtime.commit_advance(&mut world.store, prepared)?;
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
fn check_idle(world: &Fixture) -> Result<(), Error> {
    let before = world.saved()?;
    let mut next_world = Fixture::restore(before.clone())?;
    let mut callback_calls = 0;
    let started = Instant::now();
    for step in 0..IDLE_STEPS {
        let prepared = next_world.runtime.prepare_advance(
            &next_world.store,
            &next_world.queries,
            SimTime(step),
            SimTime(step + 1),
            None,
            |_| {
                callback_calls += 1;
                Err(Error::Unavailable)
            },
        )?;
        assert_eq!(prepared.growth_rows(), 0);
        next_world
            .runtime
            .commit_advance(&mut next_world.store, prepared)?;
    }
    let milliseconds = started.elapsed().as_secs_f64() * 1_000.;
    assert_eq!(callback_calls, 0);
    assert_eq!(next_world.saved()?, before);
    println!(
        "mode=subday_idle fields={} jobs={} steps={IDLE_STEPS} growth_rows=0 callback_calls={callback_calls} elapsed_ms={milliseconds:.3}",
        world.runtime.fields(&world.store).count(),
        world.runtime.cultivations(&world.store).count()
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
    assert!(fields > 0 && days > 0 && days <= u64::MAX / DAY && samples > 0);
    let path = args.get(3).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/agriculture/crops.toml")
    });
    let initial = initial_world(fields, &path, false)?;
    let days = weather(&initial, days);
    check_idle(&initial)?;
    let opening = initial.saved()?;
    let mut expected = None;
    // This measures one isolated system, not full-game performance. Component
    // allocation and save conversion are outside the measured daily pipeline.
    for mode in [
        Mode::Serial,
        Mode::Ordered(1),
        Mode::Ordered(2),
        Mode::Ordered(4),
    ] {
        let mut timings = Vec::new();
        let mut rows = 0;
        let mut final_world = None;
        for sample in 0..samples {
            let fixture = initial_world(fields, &path, sample % 2 == 1)?;
            assert_eq!(fixture.saved()?, opening);
            let (world, milliseconds, evaluated) = execute(fixture, &days, mode)?;
            let canonical = world.saved()?;
            if let Some(expected) = &expected {
                assert_eq!(&canonical, expected, "mode={} sample={sample}", mode.name());
            } else {
                expected = Some(canonical);
            }
            rows = evaluated;
            timings.push(milliseconds);
            final_world = Some(world);
        }
        timings.sort_by(f64::total_cmp);
        let median = timings[timings.len() / 2];
        let final_world = final_world.unwrap();
        let now = SimTime(days.len() as u64 * DAY);
        final_world.validate(now)?;
        let canonical = final_world.saved()?;
        let saved = serde_json::to_vec(&canonical).unwrap();
        assert_eq!(
            saved,
            serde_json::to_vec(expected.as_ref().unwrap()).unwrap()
        );
        let restored = Fixture::restore(serde_json::from_slice(&saved).unwrap())?;
        restored.validate(now)?;
        assert_eq!(canonical, restored.saved()?);
        let phase_count = |phase| {
            final_world
                .runtime
                .crops(&final_world.store)
                .filter(|(_, crop)| crop.phase == phase)
                .count()
        };
        let growing = phase_count(CropPhase::Growing);
        let ripe = phase_count(CropPhase::Ripe);
        let failed = phase_count(CropPhase::Failed);
        println!(
            "mode={} fields={fields} jobs={} days={} samples={samples} growth_rows={rows} median_ms={median:.3} min_ms={:.3} max_ms={:.3} rows_per_second={:.0} growing={growing} ripe={ripe} failed={failed} save_bytes={} process_peak_rss_kib={:?}",
            mode.name(),
            final_world.runtime.cultivations(&final_world.store).count(),
            days.len(),
            timings[0],
            timings[timings.len() - 1],
            rows as f64 / (median / 1_000.),
            saved.len(),
            resident_high_water_kib()
        );
    }
    println!(
        "canonical_equivalence=true reverse_allocation_equivalence=true save_roundtrip=true scope=isolated_agriculture shared_store=true"
    );
    Ok(())
}
