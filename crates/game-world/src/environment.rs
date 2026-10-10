//! Calendar/weather adapter. Consumers receive elapsed-day facts, never crops.
use crate::*;
pub use game_environment::{Calendar, Climate, Config, DayFacts, Step, Weather, World};

impl Simulation {
    pub(crate) fn environment_boundary(&self, requested: SimTime) -> SimTime {
        self.state.environment.as_ref().map_or(requested, |world| {
            world
                .boundary(self.state.time, requested)
                .expect("validated environmental interval")
        })
    }
    pub(crate) fn evaluate_environment(&self, next: SimTime) -> Result<Option<Step>, Error> {
        self.state
            .environment
            .as_ref()
            .map(|world| world.advance(self.state.time, next, self.state.seed))
            .transpose()
    }
    pub(crate) fn commit_environment(&mut self, output: Option<World>) {
        if let Some(output) = output {
            self.state.environment = Some(output);
        }
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
        Some(EnvironmentView {
            day: world.weather_day,
            day_of_year: world.calendar.day_of_year(self.state.time),
            season: world.calendar.season(self.state.time) as u8,
            temperature_c: world.weather.temperature,
            rain: world.weather.rain,
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
