//! A person's choice about where to put an actual carried good.
use super::{Choice, Context, Endpoint};
use game_lots::Place;
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
    pub destinations: Vec<Destination>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    pub store: u64,
    #[serde(default)]
    pub value: i32,
}

impl Preference {
    pub(super) fn choose(&self, context: &Context<'_>) -> Option<Choice> {
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
        let from = Place::Person(self.person);
        let available = context
            .view
            .container(from)?
            .quantity(self.good)
            .min(self.quantity);
        if available == 0 {
            return None;
        }
        let carried_celsius = (context.storage_celsius)(from);
        let (score, store, quantity) = self
            .destinations
            .iter()
            .filter_map(|destination| {
                let id = destination.store;
                if context.walking.is_some_and(|walking| {
                    !walking.present(self.person, self.region, Endpoint::Store { id })
                }) {
                    return None;
                }
                let to = Place::Inventory(id);
                let holder = context.view.container(to)?;
                let quantity = available.min(holder.capacity.saturating_sub(holder.used));
                let cooler_by =
                    i64::from(carried_celsius) - i64::from((context.storage_celsius)(to));
                let value = i64::from(destination.value)
                    + cooler_by * i64::from(self.cooling_value_per_degree);
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
}
