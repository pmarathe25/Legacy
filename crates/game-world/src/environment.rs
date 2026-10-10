//! Calendar/weather adapter. Consumers receive elapsed-day facts, never crops.
use crate::*;
pub use game_environment::{Calendar, Climate, Config, DayFacts, Step, Weather, World};

impl Simulation {
    /// Attaches the authored definitions used while constructing this world.
    /// Changing weather/calendar in an existing simulation needs an explicit
    /// feature operation; this binding accepts identical definitions only.
    pub fn attach_environment_content(
        &mut self,
        document: game_content::Document<Config>,
    ) -> Result<(), Error> {
        let current = self.state.environment.as_ref().ok_or(Error::Unavailable)?;
        if current.calendar != document.data.calendar || current.climate != document.data.climate {
            return Err(Error::InvalidConfiguration(
                "environment content differs from constructed world".into(),
            ));
        }
        let world = World::from_content(document, self.state.time, self.state.seed)?;
        self.state.environment = Some(world);
        Ok(())
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
            .map(|world| world.prepare(next))
            .transpose()
    }
    pub(crate) fn commit_environment(&mut self, output: Option<Step>) -> Result<(), Error> {
        if let Some(output) = output {
            self.state
                .environment
                .as_mut()
                .ok_or(Error::SequenceConflict)?
                .commit(output)?;
        }
        Ok(())
    }
    pub(crate) fn environment_view(&self, actor: PersonId) -> Option<EnvironmentView> {
        let world = self.state.environment.as_ref()?;
        let body = self.state.physical.as_ref()?.actors.get(&actor)?;
        if !body.vitals.alive {
            return None;
        }
        let Location::AtArea(_) = self.resolved_location(actor) else {
            return None;
        };
        let facts = world.observe();
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
            world
                .validate(self.state.time, self.state.seed)
                .map_err(|_| Error::InvalidSave("invalid calendar or climate".into()))?;
        }
        Ok(())
    }
}
