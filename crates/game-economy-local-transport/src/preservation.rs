//! A person's choice about where to put an actual carried good.
use super::{Choice, Context, Endpoint, walking};
use game_lots::Place;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub region: u64,
    pub good: u32,
    pub quantity: u64,
    pub cooling_value_per_degree: i32,
    #[serde(default)]
    pub keep_value: i32,
    #[serde(default)]
    pub destinations: Vec<Destination>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_cellar: Option<OwnCellar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visits: Option<VisitPreference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    pub store: u64,
    #[serde(default)]
    pub value: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnCellar {
    #[serde(default)]
    pub value: i32,
    #[serde(default)]
    pub walking_cost_per_hour: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known: Option<KnownCellar>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownCellar {
    pub store: u64,
    pub built_celsius: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisitPreference {
    pub attention: i32,
    #[serde(default)]
    pub idle: i32,
    #[serde(default)]
    pub store_value: i32,
    #[serde(default)]
    pub walking_cost_per_hour: u32,
    pub max_age_millis: std::num::NonZeroU64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed: Option<Observation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub store: u64,
    pub celsius: i16,
    pub at_millis: u128,
}

impl VisitPreference {
    fn recall(self, now: u128) -> Option<Observation> {
        let observed = self.observed?;
        (now.checked_sub(observed.at_millis)? <= u128::from(self.max_age_millis.get()))
            .then_some(observed)
    }
}

impl Preference {
    fn available(&self, context: &Context<'_>) -> Option<u64> {
        let index = context
            .people
            .binary_search_by_key(&self.person, |person| person.id)
            .ok()?;
        let person = &context.people[index];
        if person.controlled
            || person.health == 0
            || person.region != Some(self.region)
            || context.busy.binary_search(&self.person).is_ok()
        {
            return None;
        }
        let available = context
            .view
            .container(Place::Person(self.person))?
            .quantity(self.good)
            .min(self.quantity);
        (available > 0).then_some(available)
    }

    pub(super) fn choose(&self, context: &Context<'_>) -> Option<Choice> {
        let from = Place::Person(self.person);
        let available = self.available(context)?;
        let carried_celsius = (context.storage_celsius)(from);
        let own = self.own_cellar.and_then(|profile| {
            Some(Destination {
                store: profile.known?.store,
                value: profile.value,
            })
        });
        let visited = self.visits.and_then(|profile| {
            let observed = profile.recall(context.now)?;
            Some(Destination {
                store: observed.store,
                value: profile.store_value,
            })
        });
        let (score, store, quantity) = self
            .destinations
            .iter()
            .copied()
            .chain(own)
            .chain(visited)
            .filter_map(|destination| {
                let id = destination.store;
                if context.walking.is_some_and(|walking| {
                    !walking.present(self.person, self.region, Endpoint::Store { id })
                }) {
                    return None;
                }
                let to = Place::Inventory(id);
                let holder = context.view.container(to)?;
                let quantity = available.min(context.view.remaining_quantity(holder, self.good));
                let value = self.storage_value(
                    destination.value,
                    carried_celsius,
                    (context.storage_celsius)(to),
                );
                (quantity > 0).then_some((value, std::cmp::Reverse(id), quantity))
            })
            .max()?;
        (score > i64::from(self.keep_value)).then_some(Choice {
            person: self.person,
            from,
            to: Place::Inventory(store.0),
            good: self.good,
            quantity,
        })
    }

    fn storage_value(&self, base: i32, carried_celsius: i16, cellar_celsius: i16) -> i64 {
        i64::from(base)
            + (i64::from(carried_celsius) - i64::from(cellar_celsius))
                * i64::from(self.cooling_value_per_degree)
    }

    fn cellar_walk(
        &self,
        context: &Context<'_>,
        walking: &walking::Walking,
    ) -> Option<walking::Request> {
        self.available(context)?;
        let own = self.own_cellar.and_then(|profile| {
            let known = profile.known?;
            Some((
                known.store,
                known.built_celsius,
                profile.value,
                profile.walking_cost_per_hour,
            ))
        });
        let visited = self.visits.and_then(|profile| {
            let observed = profile.recall(context.now)?;
            Some((
                observed.store,
                observed.celsius,
                profile.store_value,
                profile.walking_cost_per_hour,
            ))
        });
        let carried = (context.storage_celsius)(Place::Person(self.person));
        let (_, std::cmp::Reverse(store)) = own
            .into_iter()
            .chain(visited)
            .filter_map(|(store, temperature, value, walking_cost)| {
                let endpoint = Endpoint::Store { id: store };
                let remaining = walking.millis_to(self.person, self.region, endpoint)?;
                let cost = u128::from(remaining) * u128::from(walking_cost) / 3_600_000;
                let score =
                    i128::from(self.storage_value(value, carried, temperature)) - cost as i128;
                (score > i128::from(self.keep_value)).then_some((score, std::cmp::Reverse(store)))
            })
            .max()?;
        Some(walking::Request {
            person: self.person,
            region: self.region,
            destination: Endpoint::Store { id: store },
        })
    }
}

impl Context<'_> {
    pub fn cellar_walks(&self, claimed: &[walking::Request]) -> Vec<walking::Request> {
        let Some(walking) = self.walking else {
            return Vec::new();
        };
        self.owner
            .preservation
            .par_iter()
            .filter_map(|preference| {
                claimed
                    .binary_search_by_key(&preference.person, |row| row.person)
                    .is_err()
                    .then(|| preference.cellar_walk(self, walking))
                    .flatten()
            })
            .collect()
    }
}
