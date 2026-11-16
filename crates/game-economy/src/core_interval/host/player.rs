//! Limited observations for an explicitly controlled person.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreSeenPerson {
    pub id: u64,
    pub health: u32,
}

/// A visible injury without the simulation's hidden recovery deadline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorePlayerWound {
    pub severity: u32,
}

/// Only the controlled person's own state and physically co-located people.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorePlayerView {
    pub tick: u64,
    pub offset_millis: u64,
    pub person: Person,
    pub journey: Option<travel::Journey>,
    pub possessions: Vec<game_lots::Lot>,
    pub nearby: Vec<CoreSeenPerson>,
    pub wound: Option<CorePlayerWound>,
}

impl CoreHost {
    /// Validate control on each action; control does not grant access to other minds.
    pub fn controlled_person(&self, id: u64) -> Result<&Person, String> {
        self.core
            .people
            .binary_search_by_key(&id, |person| person.id)
            .ok()
            .map(|index| &self.core.people[index])
            .filter(|person| person.controlled)
            .ok_or_else(|| "person is not under player control".into())
    }

    /// Construct a filtered observation without cloning the world or remote state.
    pub fn player_view(&self, id: u64) -> Result<CorePlayerView, String> {
        let person = self.controlled_person(id)?;
        if person.health == 0 {
            return Err("the controlled person cannot observe while incapacitated".into());
        }
        let locations = travel::Colocation::new(self.core.travel.as_ref());
        let nearby = self
            .core
            .people
            .par_iter()
            .filter(|other| {
                other.id != person.id
                    && locations
                        .region((person.id, person.region), (other.id, other.region))
                        .is_some()
            })
            .map(|other| CoreSeenPerson {
                id: other.id,
                health: other.health,
            })
            .collect();
        let journey = self.core.travel.as_ref().and_then(|travel| {
            travel
                .journeys
                .binary_search_by_key(&id, |journey| journey.person)
                .ok()
                .map(|index| travel.journeys[index].clone())
        });
        let possessions = self
            .core
            .physical
            .custody(game_lots::Place::Person(id))
            .map_or_else(Vec::new, |holder| holder.lots.clone());
        let wound = self
            .personal_conflict
            .as_ref()
            .and_then(|owner| owner.wounds.as_ref())
            .and_then(|wounds| {
                wounds
                    .cases
                    .binary_search_by_key(&id, |case| case.person)
                    .ok()
                    .map(|index| CorePlayerWound {
                        severity: wounds.cases[index].severity,
                    })
            });
        Ok(CorePlayerView {
            tick: self.core.physical.tick(),
            offset_millis: self.core.elapsed_millis().ok_or("no physical interval")?,
            person: person.clone(),
            journey,
            possessions,
            nearby,
            wound,
        })
    }
}
