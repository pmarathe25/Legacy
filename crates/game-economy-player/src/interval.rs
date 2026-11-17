//! Player-controlled actions on the interval host, without administrative events.
use game_economy::core_interval::{CoreHost, CorePlayerView, WoundTreatment};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Advance {
    pub elapsed_millis: u64,
    pub boundaries: u64,
    /// Earlier elapsed time stays committed if a later stage fails.
    pub failure: Option<String>,
}

/// One explicitly controlled person; every action rechecks current control.
pub struct Player<'a> {
    host: &'a mut CoreHost,
    person: u64,
}

impl<'a> Player<'a> {
    pub fn new(host: &'a mut CoreHost, person: u64) -> Result<Self, String> {
        host.controlled_person(person)?;
        Ok(Self { host, person })
    }

    pub fn view(&self) -> Result<CorePlayerView, String> {
        self.host.player_view(self.person)
    }

    /// Advance time without exposing other people's private decision events.
    pub fn advance(&mut self, millis: u64) -> Result<Advance, String> {
        self.host.controlled_person(self.person)?;
        let run = self.host.advance_continuous(millis, &[])?;
        Ok(Advance {
            elapsed_millis: run.elapsed_millis,
            boundaries: run.boundaries,
            failure: run.failure,
        })
    }

    pub fn depart(&mut self, destination: u64) -> Result<bool, String> {
        self.host.controlled_person(self.person)?;
        let result = self.host.depart(
            &[game_economy::Departure {
                person: self.person,
                destination,
            }],
            &[],
        )?;
        Ok(result.first().is_some_and(|outcome| outcome.departed))
    }

    pub fn eat(&mut self, good: u32) -> Result<bool, String> {
        self.host.controlled_person(self.person)?;
        self.host.eat_controlled(self.person, good)
    }

    pub fn set_rest(&mut self, resting: bool) -> Result<(), String> {
        self.host.set_controlled_rest(self.person, resting)
    }

    pub fn start_work(&mut self, recipe: u32, units: u32) -> Result<Option<u64>, String> {
        self.host.controlled_person(self.person)?;
        let (job, accepted) = self
            .host
            .start_controlled_work(self.person, recipe, units)?;
        Ok(accepted.then_some(job))
    }

    pub fn stop_work(&mut self, job: u64) -> Result<(), String> {
        let person = self.host.controlled_person(self.person)?;
        if person.health == 0 {
            return Err("the controlled person cannot stop work while incapacitated".into());
        }
        self.host.abandon_work(&[(job, self.person)])
    }

    pub fn stop_assisting(&mut self) -> Result<(), String> {
        let person = self.host.controlled_person(self.person)?;
        if person.health == 0 {
            return Err("the controlled person cannot stop assisting while incapacitated".into());
        }
        self.host.withdraw_shared_helpers(&[self.person])
    }

    pub fn treat_wound(&mut self, good: u32) -> Result<bool, String> {
        self.host.controlled_person(self.person)?;
        let result = self.host.treat_wounds(
            &[WoundTreatment {
                person: self.person,
                good,
            }],
            &[],
        )?;
        Ok(result
            .outcomes
            .first()
            .is_some_and(|outcome| outcome.accepted))
    }
}
