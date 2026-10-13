//! Executable environment owner for sparse host transactions.
use super::*;
use game_world::scheduling::{ClockSpan, Order, Scheduler};
use game_world::{OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default)]
pub struct EnvironmentSystem;

#[derive(Clone, Copy, Debug)]
pub struct ReadCalendar {
    pub area: AreaId,
}
impl Request for ReadCalendar {
    type Response = Option<CalendarReading>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarReading {
    pub day: u64,
    pub year_days: u64,
    pub day_of_year: u64,
    pub temperature_c: Option<i32>,
}

#[derive(Clone, Copy, Debug)]
pub struct ReadLocalWeather {
    pub area: AreaId,
}
impl Request for ReadLocalWeather {
    type Response = Option<Weather>;
}

pub struct ReadElapsedDay;
impl Request for ReadElapsedDay {
    type Response = Option<LocalDayFacts>;
}

pub struct ObserveEnvironment {
    pub alive: bool,
    pub location: Option<Location>,
}
impl Request for ObserveEnvironment {
    type Response = Option<Observation>;
}

pub enum SystemInput {
    Initialize {
        config: Config,
        areas: Vec<AreaId>,
    },
    Advance {
        next: SimTime,
    },
    Boundary {
        limit: SimTime,
    },
    Observe {
        alive: bool,
        location: Option<Location>,
    },
    Validate {
        areas: Vec<AreaId>,
    },
    #[cfg(feature = "content")]
    AttachContent {
        document: game_content::Document<Config>,
        areas: Vec<AreaId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemOutput {
    Initialized,
    Advanced(Option<Step>),
    Boundary(SimTime),
    Observation(Option<Observation>),
    Validated,
    #[cfg(feature = "content")]
    ContentAttached,
}

impl<E: 'static> OwnerSystem<E> for EnvironmentSystem {
    type Runtime = Option<Runtime>;
    type Input = SystemInput;
    type Output = SystemOutput;

    fn execute(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        input: Self::Input,
    ) -> Result<Self::Output, Error> {
        match input {
            SystemInput::Initialize { config, areas } => {
                if tx.runtime().is_some() {
                    return Err(Error::SequenceConflict);
                }
                let runtime = Runtime::new(config, tx.now(), tx.seed())?;
                let policies = tx.iter::<LocalClimate>();
                let weather = tx.iter::<Weather>();
                let cells = paired_cells(policies, weather)?;
                runtime.validate_host_cells(
                    tx.now(),
                    tx.seed(),
                    areas.iter().copied().collect(),
                    cells.clone(),
                )?;
                let existing = cells
                    .iter()
                    .map(|(id, _, _)| AreaId(id.0))
                    .collect::<BTreeSet<_>>();
                for area in areas.into_iter().collect::<BTreeSet<_>>() {
                    if existing.contains(&area) {
                        continue;
                    }
                    let (policy, local_weather) = runtime.initial_cell(area)?;
                    tx.write::<LocalClimate>(StableId(area.0), policy)?;
                    tx.write::<Weather>(StableId(area.0), local_weather)?;
                }
                *tx.runtime_mut() = Some(runtime);
                Ok(SystemOutput::Initialized)
            }
            SystemInput::Advance { next } => {
                let Some(runtime) = tx.runtime().clone() else {
                    return Ok(SystemOutput::Advanced(None));
                };
                let step = advance_interval(
                    tx,
                    ClockSpan {
                        from: runtime.epoch(),
                        to: next,
                    },
                )?
                .ok_or(Error::Unavailable)?;
                tx.share(step.clone())?;
                Ok(SystemOutput::Advanced(Some(step)))
            }
            SystemInput::Boundary { limit } => {
                let boundary = tx
                    .runtime()
                    .as_ref()
                    .map_or(Ok(limit), |runtime| runtime.deadline(limit))?;
                Ok(SystemOutput::Boundary(boundary))
            }
            SystemInput::Observe { alive, location } => {
                let runtime = tx.runtime().clone();
                let observation = runtime.as_ref().and_then(|runtime| {
                    let location = location.as_ref()?;
                    runtime.observation_for_actor(alive, location, |area| {
                        tx.read::<Weather>(StableId(area.0)).cloned()
                    })
                });
                Ok(SystemOutput::Observation(observation))
            }
            SystemInput::Validate { areas } => {
                let runtime = tx.runtime().clone();
                let now = tx.now();
                let seed = tx.seed();
                let policies = tx.iter::<LocalClimate>();
                let weather = tx.iter::<Weather>();
                validate_host_rows(runtime.as_ref(), now, seed, areas, policies, weather)?;
                Ok(SystemOutput::Validated)
            }
            #[cfg(feature = "content")]
            SystemInput::AttachContent { document, areas } => {
                let Some(runtime) = tx.runtime().clone() else {
                    return Err(Error::Unavailable);
                };
                let policies = tx.iter::<LocalClimate>();
                let weather = tx.iter::<Weather>();
                let cells = paired_cells(policies, weather)?;
                runtime.validate_host_cells(
                    tx.now(),
                    tx.seed(),
                    areas.into_iter().collect(),
                    cells.clone(),
                )?;
                let mut attached = runtime;
                attached.attach_content_rows(document, cells)?;
                *tx.runtime_mut() = Some(attached);
                Ok(SystemOutput::ContentAttached)
            }
        }
    }
}

/// Register the environment owner and its exclusive component write authority.
pub fn register_system<S: 'static, E: 'static>(
    registry: &mut Registry<S, E>,
    slot: RuntimeSlot<S, Option<Runtime>>,
) -> Result<(), Error> {
    registry.register(EnvironmentSystem, slot)?;
    registry.owns::<EnvironmentSystem, LocalClimate>()?;
    registry.owns::<EnvironmentSystem, Weather>()?;
    registry.register_read_request::<EnvironmentSystem, ReadCalendar>(read_calendar::<E>)?;
    registry
        .register_read_request::<EnvironmentSystem, ReadLocalWeather>(read_local_weather::<E>)?;
    registry.register_read_request::<EnvironmentSystem, ReadElapsedDay>(read_elapsed_day::<E>)?;
    registry
        .register_read_request::<EnvironmentSystem, ObserveEnvironment>(observe_environment::<E>)
}

/// Register Environment's native day boundary and weather progression hooks.
pub fn register_schedule<S: 'static, E: 'static>(
    scheduler: &mut Scheduler<E>,
    registry: &mut Registry<S, E>,
    order: Order,
) -> Result<(), Error> {
    scheduler.register_owner::<EnvironmentSystem, S>(
        registry,
        order,
        Some(environment_boundary::<E>),
        advance_environment::<E>,
    )
}

fn environment_boundary<E: 'static>(
    _: &EnvironmentSystem,
    tx: &mut OwnerReadTxn<'_, '_, EnvironmentSystem, E>,
    span: ClockSpan,
) -> Result<Option<SimTime>, Error> {
    let runtime = tx.runtime().clone();
    let Some(runtime) = runtime else {
        return Ok(None);
    };
    let boundary = runtime.deadline(span.to)?;
    Ok((boundary < span.to).then_some(boundary))
}

fn advance_environment<E: 'static>(
    _: &EnvironmentSystem,
    tx: &mut OwnerTxn<'_, '_, EnvironmentSystem, E>,
    span: ClockSpan,
) -> Result<(), Error> {
    if let Some(step) = advance_interval(tx, span)? {
        tx.share(step)?;
    }
    Ok(())
}

fn advance_interval<E: 'static>(
    tx: &mut OwnerTxn<'_, '_, EnvironmentSystem, E>,
    span: ClockSpan,
) -> Result<Option<Step>, Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(None);
    };
    let policies = tx.iter::<LocalClimate>();
    let weather = tx.iter::<Weather>();
    let cells = paired_cells(policies, weather)?;
    let step = runtime.advance_cells(span.from, span.to, tx.seed(), cells)?;
    for (area, weather) in step.weather_updates() {
        tx.write::<Weather>(StableId(area.0), weather)?;
    }
    let mut advanced = runtime;
    advanced.finish_step(&step)?;
    *tx.runtime_mut() = Some(advanced);
    Ok(Some(step))
}

fn read_calendar<E: 'static>(
    _: &EnvironmentSystem,
    tx: &mut OwnerReadTxn<'_, '_, EnvironmentSystem, E>,
    request: ReadCalendar,
) -> Result<Option<CalendarReading>, Error> {
    let Some((day, year_days, day_of_year)) = tx.runtime().as_ref().map(|runtime| {
        let observation = runtime.observe();
        (
            observation.day,
            runtime.calendar.year_days(),
            observation.day_of_year,
        )
    }) else {
        return Ok(None);
    };
    let temperature_c = tx
        .read::<Weather>(StableId(request.area.0))
        .map(|weather| weather.temperature);
    Ok(Some(CalendarReading {
        day,
        year_days,
        day_of_year,
        temperature_c,
    }))
}

fn read_local_weather<E: 'static>(
    _: &EnvironmentSystem,
    tx: &mut OwnerReadTxn<'_, '_, EnvironmentSystem, E>,
    request: ReadLocalWeather,
) -> Result<Option<Weather>, Error> {
    if tx.runtime().is_none() {
        return Ok(None);
    }
    Ok(tx.read::<Weather>(StableId(request.area.0)).cloned())
}

fn read_elapsed_day<E: 'static>(
    _: &EnvironmentSystem,
    tx: &mut OwnerReadTxn<'_, '_, EnvironmentSystem, E>,
    _: ReadElapsedDay,
) -> Result<Option<LocalDayFacts>, Error> {
    Ok(tx
        .fact::<Step>()
        .and_then(|step| step.elapsed_local_day.clone()))
}

fn observe_environment<E: 'static>(
    _: &EnvironmentSystem,
    tx: &mut OwnerReadTxn<'_, '_, EnvironmentSystem, E>,
    request: ObserveEnvironment,
) -> Result<Option<Observation>, Error> {
    let runtime = tx.runtime().clone();
    let Some(runtime) = runtime else {
        return Ok(None);
    };
    let Some(location) = request.location.as_ref() else {
        return Ok(None);
    };
    Ok(
        runtime.observation_for_actor(request.alive, location, |area| {
            tx.read::<Weather>(StableId(area.0)).cloned()
        }),
    )
}
