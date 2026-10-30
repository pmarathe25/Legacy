//! Saved personal food holdings and validation.
mod requests;
mod sharing;
mod stores;
pub use game_parallel_production_model::{FoodStoreBlueprint, FoodStoreBuilder};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
pub use sharing::FoodSharingPolicy;
use std::collections::HashSet;
pub use stores::{FoodDepositPolicy, FoodStore, FoodStoreChoice, FoodStoreClaim};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cargo {
    pub capacity: u32,
    #[serde(default)]
    pub auto_eat_travel: bool,
    /// Eat held food while local before claiming regional food.
    #[serde(default)]
    pub auto_eat_local: bool,
    /// In this mode regional meals require an individual acquisition attempt.
    #[serde(default)]
    pub personal_food_access: bool,
    /// Physically distinct local stores; claims do not prevent taking.
    #[serde(default)]
    pub food_stores: Vec<FoodStore>,
    /// Independent additional personal claims, which may overlap.
    #[serde(default)]
    pub food_store_claims: Vec<FoodStoreClaim>,
    #[serde(default)]
    pub food_store_choices: Vec<FoodStoreChoice>,
    #[serde(default)]
    pub food_depositors: Vec<FoodDepositPolicy>,
    /// Content-defined construction options requiring physically held goods.
    #[serde(default)]
    pub food_store_blueprints: Vec<FoodStoreBlueprint>,
    /// Individual intentions to build a bounded number of stores from held materials.
    #[serde(default)]
    pub food_store_builders: Vec<FoodStoreBuilder>,
    /// Newborns acquire an individually changeable initial building inclination.
    #[serde(default)]
    pub inherit_store_building: bool,
    /// People who choose to seek food from their current local source when hungry.
    #[serde(default)]
    pub acquire_when_hungry: Vec<u64>,
    /// Optional individual decisions to help a known person with held food.
    #[serde(default)]
    pub food_aid: Vec<FoodAidPolicy>,
    /// Individual tendency to share with people toward whom one feels pressure.
    #[serde(default)]
    pub food_sharing: Vec<FoodSharingPolicy>,
    /// Newborns may acquire one parent's starting sharing inclination.
    #[serde(default)]
    pub inherit_food_sharing: bool,
    /// Unfulfilled requests; they do not compel the addressed person.
    #[serde(default)]
    pub food_requests: Vec<FoodRequest>,
    #[serde(default)]
    pub next_food_request: u64,
    /// Bounded individual intentions to ask a known person when hungry.
    #[serde(default)]
    pub food_askers: Vec<FoodAsker>,
    /// Schedule newborns to begin seeking food after this many ticks.
    #[serde(default)]
    pub auto_seek_age: Option<u64>,
    #[serde(default)]
    pub food_enrollments: Vec<FoodEnrollment>,
    #[serde(default)]
    pub auto_pack_travel: bool,
    #[serde(default)]
    pub holdings: Vec<Holding>,
}
/// A personal tendency to help one known person; this creates no binding obligation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodAidPolicy {
    pub giver: u64,
    pub recipient: u64,
    pub amount: u32,
    pub retain: u32,
    pub help_value: u64,
    pub threshold: u64,
    pub interval: u64,
    /// Help only in response to a pending request from this person.
    #[serde(default)]
    pub requires_request: bool,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
fn enabled() -> bool {
    true
}
/// A future individual food-seeking preference for a newborn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodEnrollment {
    pub person: u64,
    pub at: u64,
}
/// One person's tendency to ask a known person for food.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodAsker {
    pub person: u64,
    pub giver: u64,
    /// Prefer a locally present person remembered through prior help.
    #[serde(default)]
    pub choose_known: bool,
    pub amount: u32,
    pub interval: u64,
    pub lifetime: u64,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
/// A person asks another to provide food before a deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodRequest {
    pub id: u64,
    pub requester: u64,
    pub giver: u64,
    pub amount: u32,
    pub expires_at: u64,
}
/// A giver chooses to physically hand food to one local person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoodGift {
    pub giver: u64,
    pub recipient: u64,
    pub amount: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Holding {
    pub person: u64,
    pub food: u32,
}
impl Cargo {
    pub fn validate_sorted(&self, people: &[u64]) -> Result<(), &'static str> {
        if self.capacity == 0
            || self.holdings.par_iter().any(|h| {
                h.food == 0 || h.food > self.capacity || people.binary_search(&h.person).is_err()
            })
        {
            return Err("invalid personal cargo");
        }
        if self
            .acquire_when_hungry
            .par_iter()
            .any(|id| people.binary_search(id).is_err())
        {
            return Err("invalid food seeker");
        }
        if (!self.food_aid.is_empty() && !self.personal_food_access)
            || self.food_aid.par_iter().any(|p| {
                p.giver == p.recipient
                    || p.amount == 0
                    || p.interval == 0
                    || people.binary_search(&p.giver).is_err()
                    || people.binary_search(&p.recipient).is_err()
            })
        {
            return Err("invalid food aid policy");
        }
        let mut givers: Vec<_> = self
            .food_aid
            .par_iter()
            .map(|p| (p.giver, p.recipient))
            .collect();
        givers.par_sort_unstable();
        if givers.par_windows(2).any(|p| p[0] == p[1]) {
            return Err("duplicate food aid relationship");
        }
        let mut seekers = self.acquire_when_hungry.clone();
        seekers.par_sort_unstable();
        if seekers.par_windows(2).any(|p| p[0] == p[1]) {
            return Err("duplicate food seeker");
        }
        stores::validate(self, |id| people.binary_search(&id).is_ok())?;
        sharing::validate(self, |id| people.binary_search(&id).is_ok())?;
        requests::validate(self, |id| people.binary_search(&id).is_ok())?;
        let mut ids: Vec<u64> = self.holdings.par_iter().map(|h| h.person).collect();
        ids.par_sort_unstable();
        if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("invalid personal cargo");
        }
        Ok(())
    }
    pub fn validate(&self, people: &HashSet<u64>) -> Result<(), String> {
        if self.capacity == 0 {
            return Err("cargo capacity must be positive".into());
        }
        let mut seekers = HashSet::new();
        if self
            .acquire_when_hungry
            .iter()
            .any(|id| !people.contains(id) || !seekers.insert(*id))
        {
            return Err("invalid food seeker".into());
        }
        let mut givers = HashSet::new();
        if (!self.food_aid.is_empty() && !self.personal_food_access)
            || self.food_aid.iter().any(|p| {
                p.giver == p.recipient
                    || p.amount == 0
                    || p.interval == 0
                    || !people.contains(&p.giver)
                    || !people.contains(&p.recipient)
                    || !givers.insert((p.giver, p.recipient))
            })
        {
            return Err("invalid food aid policy".into());
        }
        stores::validate(self, |id| people.contains(&id)).map_err(str::to_string)?;
        sharing::validate(self, |id| people.contains(&id)).map_err(str::to_string)?;
        requests::validate(self, |id| people.contains(&id)).map_err(str::to_string)?;
        let mut seen = HashSet::new();
        for holding in &self.holdings {
            if holding.food == 0
                || holding.food > self.capacity
                || !people.contains(&holding.person)
                || !seen.insert(holding.person)
            {
                return Err("invalid personal cargo".into());
            }
        }
        Ok(())
    }
}
impl Cargo {
    /// Validate holdings against an already sorted physical-person index.
    pub fn validate_index(
        &self,
        people: &[game_parallel_production_model::PersonSnapshot],
    ) -> Result<(), &'static str> {
        use game_parallel_production_model::person_index;
        if self.capacity == 0
            || people.par_windows(2).any(|pair| pair[0].0 >= pair[1].0)
            || self.holdings.par_iter().any(|holding| {
                holding.food == 0
                    || holding.food > self.capacity
                    || person_index(people, holding.person).is_none()
            })
        {
            return Err("invalid personal cargo");
        }
        let mut givers: Vec<_> = self
            .food_aid
            .par_iter()
            .map(|p| (p.giver, p.recipient))
            .collect();
        givers.par_sort_unstable();
        if (!self.food_aid.is_empty() && !self.personal_food_access)
            || givers.par_windows(2).any(|p| p[0] == p[1])
            || self.food_aid.par_iter().any(|p| {
                p.giver == p.recipient
                    || p.amount == 0
                    || p.interval == 0
                    || person_index(people, p.giver).is_none()
                    || person_index(people, p.recipient).is_none()
            })
        {
            return Err("invalid food aid policy");
        }
        let mut seekers = self.acquire_when_hungry.clone();
        seekers.par_sort_unstable();
        if seekers.par_windows(2).any(|p| p[0] == p[1])
            || seekers
                .par_iter()
                .any(|id| person_index(people, *id).is_none())
        {
            return Err("invalid food seeker");
        }
        stores::validate(self, |id| person_index(people, id).is_some())?;
        sharing::validate(self, |id| person_index(people, id).is_some())?;
        requests::validate(self, |id| person_index(people, id).is_some())?;
        let mut ids: Vec<_> = self.holdings.par_iter().map(|h| h.person).collect();
        if ids.par_windows(2).any(|p| p[0] > p[1]) {
            ids.par_sort_unstable();
        }
        if ids.par_windows(2).any(|p| p[0] == p[1]) {
            return Err("invalid personal cargo");
        }
        Ok(())
    }
}
