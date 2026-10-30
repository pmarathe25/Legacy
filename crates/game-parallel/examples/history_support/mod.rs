pub mod checkpoint;
pub mod scenario;
#[cfg(test)]
mod tests;
use game_parallel::World;
use scenario::{Calendar, Date, Scenario};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub people: u64,
    pub alive: u64,
    pub hunger: u128,
    pub health: u128,
    pub regional_food: u128,
    pub in_transit: u64,
    pub shipment_food: u128,
}
impl Metrics {
    pub fn observe(world: &World) -> Self {
        let people = world
            .regions
            .iter()
            .flat_map(|r| r.people.iter())
            .chain(world.travelers.iter().map(|t| &t.person));
        let mut result = Self {
            people: 0,
            alive: 0,
            hunger: 0,
            health: 0,
            regional_food: world.regions.iter().map(|r| u128::from(r.food)).sum(),
            in_transit: world.travelers.len() as u64,
            shipment_food: world.shipments.iter().map(|s| u128::from(s.food)).sum(),
        };
        for person in people {
            result.people += 1;
            result.alive += u64::from(person.health > 0);
            result.hunger += u128::from(person.hunger);
            result.health += u128::from(person.health);
        }
        result
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sample {
    pub tick: u64,
    pub date: Date,
    pub metrics: Metrics,
    /// Snapshot differences, not fabricated engine action or birth/death events.
    pub population_delta: i128,
    pub living_delta: i128,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub version: u32,
    pub engine: String,
    pub origin_tick: u64,
    pub goal_tick: u64,
    pub calendar: Calendar,
    pub sample_every: u64,
    pub checkpoint_every: u64,
    pub max_samples: u64,
    pub samples_written: u64,
    pub max_events: u64,
    pub events_written: u64,
    pub events_omitted: u64,
    pub last_sample: Option<Sample>,
    pub world: World,
}
impl Checkpoint {
    pub fn new(scenario: Scenario, world: World, ticks: Option<u64>) -> Result<Self, String> {
        let duration = ticks.unwrap_or(scenario.ticks);
        let goal_tick = world
            .tick
            .checked_add(duration)
            .ok_or("target tick overflow")?;
        let result = Self {
            version: 1,
            engine: "game-parallel".into(),
            origin_tick: world.tick,
            goal_tick,
            calendar: scenario.calendar,
            sample_every: scenario.sample_every,
            checkpoint_every: scenario.checkpoint_every,
            max_samples: scenario.max_samples,
            samples_written: 0,
            max_events: scenario.max_events,
            events_written: 0,
            events_omitted: 0,
            last_sample: None,
            world,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.engine != "game-parallel" {
            return Err("unsupported runner checkpoint".into());
        }
        self.calendar.validate()?;
        self.world.validate()?;
        self.calendar.date(self.goal_tick)?;
        if self.origin_tick > self.world.tick
            || self.world.tick > self.goal_tick
            || self.sample_every == 0
            || self.checkpoint_every == 0
            || self.samples_written > self.max_samples
            || self.max_samples == 0
            || self
                .last_sample
                .as_ref()
                .is_some_and(|s| s.tick > self.world.tick || s.tick < self.origin_tick)
            || (self.samples_written == 0) != self.last_sample.is_none()
            || self.events_written > self.max_events
        {
            return Err("invalid runner progress or cadence".into());
        }
        Ok(())
    }
    fn sample(&mut self) -> Result<Option<Sample>, String> {
        if self.samples_written >= self.max_samples
            || self
                .last_sample
                .as_ref()
                .is_some_and(|s| s.tick == self.world.tick)
        {
            return Ok(None);
        }
        let metrics = Metrics::observe(&self.world);
        let sample = Sample {
            tick: self.world.tick,
            date: self.calendar.date(self.world.tick)?,
            population_delta: self.last_sample.as_ref().map_or(0, |s| {
                i128::from(metrics.people) - i128::from(s.metrics.people)
            }),
            living_delta: self.last_sample.as_ref().map_or(0, |s| {
                i128::from(metrics.alive) - i128::from(s.metrics.alive)
            }),
            metrics,
        };
        self.samples_written += 1;
        self.last_sample = Some(sample.clone());
        Ok(Some(sample))
    }
}
/// Stop is checked at every committed engine tick; output is published in immutable segments.
pub fn run(
    mut state: Checkpoint,
    workers: usize,
    output: &Path,
    mut stop: impl FnMut(u64) -> bool,
) -> Result<Checkpoint, String> {
    if !(1..=64).contains(&workers) {
        return Err("workers must be between 1 and 64".into());
    }
    state.validate()?;
    std::fs::create_dir(output).map_err(|e| format!("new output directory required: {e}"))?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|e| e.to_string())?;
    let mut samples = Vec::new();
    let mut events = Vec::new();
    let mut last_checkpoint = None;
    if state.world.tick == state.origin_tick
        && let Some(sample) = state.sample()?
    {
        samples.push(sample);
    }
    loop {
        let stopped = stop(state.world.tick);
        let finished = state.world.tick == state.goal_tick;
        if stopped || finished {
            if finished && let Some(sample) = state.sample()? {
                samples.push(sample);
            }
            if last_checkpoint != Some(state.world.tick) || !samples.is_empty() {
                checkpoint::publish(
                    output,
                    &state,
                    &samples,
                    &events,
                    workers,
                    if stopped { "stopped" } else { "complete" },
                )?;
            }
            return Ok(state);
        }
        // The current engine rejects tick overflow before mutating its regions.
        if let Err(error) = pool.install(|| state.world.step()) {
            if last_checkpoint != Some(state.world.tick) {
                checkpoint::publish(output, &state, &samples, &events, workers, "engine-error")?;
            }
            return Err(error.to_string());
        }
        if let Some(families) = &state.world.families {
            for receipt in families
                .events
                .iter()
                .filter(|e| e.tick == state.world.tick)
            {
                if state.events_written < state.max_events {
                    events.push(receipt.clone());
                    state.events_written += 1;
                } else {
                    state.events_omitted = state.events_omitted.saturating_add(1);
                }
            }
        }
        let elapsed = state.world.tick - state.origin_tick;
        if elapsed.is_multiple_of(state.sample_every)
            && let Some(sample) = state.sample()?
        {
            samples.push(sample);
        }
        if elapsed.is_multiple_of(state.checkpoint_every) && state.world.tick != state.goal_tick {
            checkpoint::publish(output, &state, &samples, &events, workers, "periodic")?;
            samples.clear();
            events.clear();
            last_checkpoint = Some(state.world.tick);
        }
    }
}
