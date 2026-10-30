//! Physical recovery of living people, with additional recovery from actual rest.
use game_parallel_economy_interaction::PhysicalChoice;
use game_parallel_economy_person::Person;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub interval: u64,
    pub health_per_interval: u32,
    pub rest_bonus: u32,
    pub max_hunger: u32,
}

pub fn apply(
    rules: Option<&Rules>,
    people: &mut [Person],
    resting: &[u64],
    fighters: &[u64],
    choice: Option<PhysicalChoice>,
    tick: u64,
    max_health: u32,
) {
    let Some(rules) = rules.filter(|r| tick.is_multiple_of(r.interval)) else {
        return;
    };
    let manual = match choice {
        Some(PhysicalChoice::Rest { person }) => Some(person),
        _ => None,
    };
    people.par_iter_mut().for_each(|person| {
        if person.health == 0 || person.hunger > rules.max_hunger {
            return;
        }
        let resting = (manual == Some(person.id) || resting.binary_search(&person.id).is_ok())
            && fighters.binary_search(&person.id).is_err()
            && person.region.is_some();
        let amount =
            rules
                .health_per_interval
                .saturating_add(if resting { rules.rest_bonus } else { 0 });
        person.health = person.health.saturating_add(amount).min(max_health);
    });
}
