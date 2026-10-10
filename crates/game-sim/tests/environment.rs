use game_core::{Error, SimDuration, SimTime};
use game_environment::{Config, DayFacts, Step, Weather};
use game_sim::*;
use serde::{Deserialize, Serialize};

const WEATHER: ModuleId = ModuleId(9);
const EXTRA: ModuleId = ModuleId(2);
const PREPARE: PhaseId = PhaseId(0);
const COMMIT: PhaseId = PhaseId(1);
const PERIOD: u64 = 210_000;
fn key(module: ModuleId, phase: PhaseId) -> PhaseKey {
    PhaseKey { module, phase }
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DayRecord {
    day: u64,
    year_day: u64,
    weather: Weather,
}
impl From<DayFacts> for DayRecord {
    fn from(facts: DayFacts) -> Self {
        Self {
            day: facts.day,
            year_day: facts.day_of_year,
            weather: facts.weather,
        }
    }
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Row {
    key: u64,
    units: u64,
    remainder: u64,
}
// Deliberately not Clone: the driver never copies the containing world.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct World {
    clock: Clock,
    seed: u64,
    environment: Option<game_environment::World>,
    rows: Vec<Row>,
    days: Vec<DayRecord>,
    marks: Vec<SimTime>,
    observed_weather_day: Option<u64>,
}
impl ClockWorld for World {
    fn clock(&self) -> Clock {
        self.clock
    }
    fn publish_clock(&mut self, clock: Clock) {
        self.clock = clock;
    }
}
fn world(weather: bool, extra: bool) -> World {
    let config = Config {
        calendar: game_environment::Calendar {
            day: SimDuration(600_000),
            ..Config::default().calendar
        },
        ..Config::default()
    };
    World {
        clock: Clock::default(),
        seed: 17,
        environment: weather.then(|| game_environment::World::new(config, SimTime(0), 17).unwrap()),
        rows: if extra {
            (0..512)
                .rev()
                .map(|key| Row {
                    key,
                    units: 0,
                    remainder: 0,
                })
                .collect()
        } else {
            Vec::new()
        },
        days: Vec::new(),
        marks: Vec::new(),
        observed_weather_day: None,
    }
}
fn engine(weather: bool, extra: bool, execution: Execution, reverse: bool) -> Engine<World, u8> {
    let mut modules = Vec::new();
    if weather {
        let mut module = Module::new(WEATHER)
            .phase(PREPARE, [])
            .phase(COMMIT, [key(WEATHER, PREPARE)]);
        module.boundary = Some(|world: &World, interval| {
            world
                .environment
                .as_ref()
                .unwrap()
                .boundary(interval.from, interval.to)
        });
        modules.push(module);
    }
    if extra {
        let mut after = vec![key(EXTRA, PREPARE)];
        if weather {
            after.push(key(WEATHER, COMMIT));
        }
        let mut module = Module::new(EXTRA).phase(PREPARE, []).phase(COMMIT, after);
        module.boundary = Some(|_: &World, interval| {
            Ok(SimTime((interval.from.0 / PERIOD + 1) * PERIOD).min(interval.to))
        });
        modules.push(module);
    }
    if reverse {
        modules.reverse();
    }
    Engine::new(modules, execution).unwrap()
}
struct Prepared {
    interval: Interval,
    phases: Vec<PhaseKey>,
    weather: Option<Step>,
    rows: Option<Vec<Row>>,
}
fn prepare(
    world: &World,
    interval: Interval,
    phases: &[PhaseKey],
    executor: &Executor,
) -> Result<Prepared, Error> {
    let mut weather = None;
    let mut rows = None;
    for phase in phases {
        if *phase == key(WEATHER, PREPARE) {
            weather = Some(world.environment.as_ref().unwrap().advance(
                interval.from,
                interval.to,
                world.seed,
            )?);
        } else if *phase == key(EXTRA, PREPARE) {
            let input: Vec<_> = world
                .rows
                .iter()
                .map(|row| {
                    (
                        row.key,
                        (
                            row.units,
                            row.remainder,
                            row.key % 7 + 1,
                            interval.elapsed().0,
                        ),
                    )
                })
                .collect();
            rows = Some(
                executor
                    .map_keyed(&input, |(units, remainder, rate, elapsed)| {
                        let amount =
                            u128::from(*elapsed) * u128::from(*rate) + u128::from(*remainder);
                        let value = units
                            .checked_add(
                                u64::try_from(amount / 60_000).map_err(|_| Error::Overflow)?,
                            )
                            .ok_or(Error::Overflow)?;
                        Ok((value, (amount % 60_000) as u64))
                    })?
                    .into_iter()
                    .map(|(key, (units, remainder))| Row {
                        key,
                        units,
                        remainder,
                    })
                    .collect(),
            );
        }
    }
    Ok(Prepared {
        interval,
        phases: phases.to_vec(),
        weather,
        rows,
    })
}
impl Transaction<World, u8> for Prepared {
    fn commit(mut self, world: &mut World) -> Option<u8> {
        assert_eq!(world.clock.time, self.interval.to);
        for phase in self.phases {
            if phase == key(WEATHER, COMMIT) {
                let step = self.weather.take().unwrap();
                if let Some(day) = step.elapsed_day {
                    world.days.push(day.into());
                }
                world.environment = Some(step.world);
            } else if phase == key(EXTRA, COMMIT) {
                world.rows = self.rows.take().unwrap();
                // The declared dependency makes closing weather visible here.
                world.observed_weather_day =
                    world.environment.as_ref().map(|world| world.weather_day);
                if self.interval.to.0.is_multiple_of(PERIOD) {
                    world.marks.push(self.interval.to);
                }
            }
        }
        None
    }
}
fn reference(
    mut environment: game_environment::World,
    target: SimTime,
) -> (game_environment::World, Vec<DayRecord>) {
    let mut at = SimTime(0);
    let mut days = Vec::new();
    while at < target {
        let next = environment.boundary(at, target).unwrap();
        let step = environment.advance(at, next, 17).unwrap();
        if let Some(day) = step.elapsed_day {
            days.push(day.into());
        }
        environment = step.world;
        at = next;
    }
    (environment, days)
}

#[test]
fn a_real_module_plugs_in_without_engine_feature_imports_or_world_cloning() {
    let mut world = world(true, true);
    let (expected, days) = reference(
        world.environment.as_ref().unwrap().clone(),
        SimTime(9_000_123),
    );
    let engine = engine(true, true, Execution::default(), false);
    engine
        .advance(&mut world, SimDuration(9_000_123), prepare)
        .unwrap();
    assert_eq!(world.environment, Some(expected));
    assert_eq!(world.days, days);
    assert_eq!(world.days.len(), 15);
    assert_eq!(world.observed_weather_day, Some(15));
    assert_eq!(
        world.marks,
        (1..=42)
            .map(|mark| SimTime(mark * PERIOD))
            .collect::<Vec<_>>()
    );
    for row in &world.rows {
        let amount = 9_000_123 * (row.key % 7 + 1);
        assert_eq!(row.units, amount / 60_000);
        assert_eq!(row.remainder, amount % 60_000);
    }
}

#[test]
fn worker_chunk_registration_order_save_restore_and_passive_queries_preserve_facts() {
    let target = 9_000_123;
    let mut baseline = world(true, true);
    engine(true, true, Execution::default(), false)
        .advance(&mut baseline, SimDuration(target), prepare)
        .unwrap();
    let expected = serde_json::to_vec(&baseline).unwrap();
    for workers in [1, 4] {
        for reverse in [false, true] {
            let mut world = world(true, true);
            let engine = engine(
                true,
                true,
                Execution {
                    workers,
                    reverse_jobs: reverse,
                },
                reverse,
            );
            while world.clock.time.0 < target {
                let before = serde_json::to_vec(&world).unwrap();
                for _ in 0..3 {
                    std::hint::black_box((&world.environment, &world.days, engine.phases()));
                }
                assert_eq!(before, serde_json::to_vec(&world).unwrap());
                let remaining = target - world.clock.time.0;
                engine
                    .advance(&mut world, SimDuration(remaining.min(37_001)), prepare)
                    .unwrap();
                // Actual serialization/restoration of the containing state,
                // excluding the noncanonical publication counter.
                world = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
            }
            assert_eq!(serde_json::to_vec(&world).unwrap(), expected);
        }
    }
}

#[test]
fn either_module_can_be_absent_without_an_engine_branch_or_missing_dependency() {
    for weather in [false, true] {
        for extra in [false, true] {
            let mut world = world(weather, extra);
            engine(weather, extra, Execution::default(), true)
                .advance(&mut world, SimDuration(1_200_001), prepare)
                .unwrap();
            assert_eq!(world.clock.time, SimTime(1_200_001));
            assert_eq!(world.environment.is_some(), weather);
            assert_eq!(world.rows.is_empty(), !extra);
            assert_eq!(world.days.len(), if weather { 2 } else { 0 });
            assert_eq!(world.marks.len(), if extra { 5 } else { 0 });
        }
    }
}
