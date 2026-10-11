//! Calendar/weather adapter. Consumers receive elapsed-day facts, never crops.
use crate::*;
pub use game_environment::{Calendar, Climate, Config, DayFacts, Runtime, Step, Weather};

impl Simulation {
    /// Attaches the authored definitions used while constructing this world.
    /// Changing weather/calendar in an existing simulation needs an explicit
    /// feature operation; this binding accepts identical definitions only.
    pub fn attach_environment_content(
        &mut self,
        document: game_content::Document<Config>,
    ) -> Result<(), Error> {
        self.state
            .environment
            .as_mut()
            .ok_or(Error::Unavailable)?
            .attach_content(&self.state.components, document)?;
        Ok(())
    }
    pub(crate) fn register_weather_cells(&mut self) -> Result<(), Error> {
        let Some(world) = self.state.environment.as_mut() else {
            return Ok(());
        };
        for area in &self.state.areas {
            if world
                .local_weather(&self.state.components, area.id)
                .is_none()
            {
                world.register_cell(
                    &mut self.state.components,
                    area.id,
                    game_environment::LocalClimate::default(),
                )?;
            }
        }
        Ok(())
    }
    pub(crate) fn local_weather_for_actor(&self, actor: PersonId) -> Option<&Weather> {
        let area = self.resolved_location(actor).cell()?;
        self.state
            .environment
            .as_ref()?
            .local_weather(&self.state.components, area)
    }
    pub(crate) fn environment_boundary(&self, requested: SimTime) -> Result<SimTime, Error> {
        self.state
            .environment
            .as_ref()
            .map_or(Ok(requested), |world| world.deadline(requested))
    }
    pub(crate) fn evaluate_environment(&self, next: SimTime) -> Result<Option<Step>, Error> {
        self.state
            .environment
            .as_ref()
            .map(|world| world.prepare(&self.state.components, &self.environment_queries, next))
            .transpose()
    }
    pub(crate) fn commit_environment(&mut self, output: Option<Step>) -> Result<(), Error> {
        if let Some(output) = output {
            self.state
                .environment
                .as_mut()
                .ok_or(Error::SequenceConflict)?
                .commit(
                    &mut self.state.components,
                    &mut self.environment_queries,
                    output,
                )?;
        }
        Ok(())
    }
    pub(crate) fn environment_view(&self, actor: PersonId) -> Option<EnvironmentView> {
        let world = self.state.environment.as_ref()?;
        let body = self.state.components.body(actor)?;
        if !body.vitals.alive {
            return None;
        }
        let Location::AtArea(area) = self.resolved_location(actor) else {
            return None;
        };
        let facts = world.observe_cell(&self.state.components, area)?;
        Some(EnvironmentView {
            day: facts.day,
            day_of_year: facts.day_of_year,
            season: facts.season,
            temperature_c: facts.weather.temperature,
            rain: facts.weather.rain,
        })
    }
    pub(crate) fn validate_environment(&self) -> Result<(), Error> {
        if let Some(world) = &self.state.environment {
            if world
                .cells(&self.state.components)
                .any(|(id, _, _)| !self.state.areas.iter().any(|area| area.id == id))
            {
                return Err(Error::InvalidSave("unknown weather cell area".into()));
            }
            world
                .validate(&self.state.components, self.state.time, self.state.seed)
                .map_err(|_| Error::InvalidSave("invalid calendar or climate".into()))?;
        } else if self
            .state
            .components
            .iter::<game_environment::LocalClimate>()
            .next()
            .is_some()
            || self.state.components.iter::<Weather>().next().is_some()
        {
            return Err(Error::InvalidSave(
                "weather components have no environment runtime".into(),
            ));
        }
        Ok(())
    }
}
