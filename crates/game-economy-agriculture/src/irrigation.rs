//! Individual watering of fields from finite physical regional water stocks.
use crate::{Agriculture, Phase};
use game_economy_claims::{Index, Use};
use game_economy_person::Person;
use game_lot_runtime::Events;
use game_lots::{Movement, Place, Transaction, View};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Moisture {
    pub field: u64,
    pub amount: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub person: u64,
    pub field: u64,
    pub quantity: u64,
    pub willing: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub fear_at_least: Option<u64>,
    #[serde(default)]
    pub obligation_at_least: Option<u64>,
    #[serde(default)]
    pub disregard_claims: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub field: u64,
    pub quantity: u64,
    pub accepted: bool,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Irrigation {
    pub water: u32,
    pub capacity: u64,
    pub minimum_growth: u64,
    pub evaporation: u64,
    pub moisture: Vec<Moisture>,
    #[serde(default)]
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub events: Vec<Event>,
}
pub struct Context<'a> {
    pub agriculture: &'a Agriculture,
    pub people: &'a [Person],
    pub busy: &'a [u64],
    pub claims: Option<&'a Index<'a>>,
    pub opinions: Option<&'a game_opinions::Opinions>,
    pub view: &'a View,
    pub max_hunger: u32,
}
impl Clone for Irrigation {
    fn clone(&self) -> Self {
        Self {
            water: self.water,
            capacity: self.capacity,
            minimum_growth: self.minimum_growth,
            evaporation: self.evaporation,
            moisture: self.moisture.par_iter().cloned().collect(),
            choices: self.choices.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
        }
    }
}
impl Irrigation {
    pub fn validate(
        &self,
        agriculture: &Agriculture,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        tick: u64,
    ) -> Result<(), String> {
        let fields: HashSet<_> = agriculture
            .fields
            .par_iter()
            .map(|field| field.id)
            .collect();
        let mut chosen_fields: Vec<_> =
            self.choices.par_iter().map(|choice| choice.field).collect();
        chosen_fields.par_sort_unstable();
        if !goods.contains(&self.water)
            || self.capacity == 0
            || self.minimum_growth == 0
            || self.minimum_growth > self.capacity
            || self
                .moisture
                .par_windows(2)
                .any(|p| p[0].field >= p[1].field)
            || self
                .choices
                .par_windows(2)
                .any(|p| p[0].person >= p[1].person)
            || chosen_fields.par_windows(2).any(|p| p[0] == p[1])
            || self
                .moisture
                .par_iter()
                .any(|m| !fields.contains(&m.field) || m.amount > self.capacity)
            || self.choices.par_iter().any(|p| {
                !people.contains(&p.person)
                    || p.quantity == 0
                    || p.pressured_by
                        .is_some_and(|id| id == p.person || !people.contains(&id))
                    || (p.fear_at_least.is_some() || p.obligation_at_least.is_some())
                        != p.pressured_by.is_some()
                    || self
                        .moisture
                        .binary_search_by_key(&p.field, |m| m.field)
                        .is_err()
            })
            || self.events.par_iter().any(|e| {
                e.tick > tick
                    || !people.contains(&e.person)
                    || !fields.contains(&e.field)
                    || e.quantity == 0
            })
        {
            return Err("invalid irrigation state".into());
        }
        Ok(())
    }
    /// Fields without a moisture record retain ordinary rain-fed growth.
    pub fn dry(&self, field: u64) -> bool {
        self.moisture
            .binary_search_by_key(&field, |m| m.field)
            .ok()
            .is_some_and(|i| self.moisture[i].amount < self.minimum_growth)
    }
    pub fn prepare(&self, context: Context<'_>, base: u64) -> Result<Plan, String> {
        if self.choices.is_empty() {
            return Ok(Plan { rows: Vec::new() });
        }
        let Context {
            agriculture,
            people,
            busy,
            claims,
            opinions,
            view,
            max_hunger,
        } = context;
        let feelings = opinions
            .filter(|_| {
                self.choices
                    .par_iter()
                    .any(|choice| !choice.willing && choice.pressured_by.is_some())
            })
            .map(game_opinions::Opinions::index);
        let mut fields: Vec<_> = agriculture.fields.par_iter().collect();
        if fields.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            fields.par_sort_unstable_by_key(|field| field.id);
        }
        let selected: Vec<_> = self
            .choices
            .par_iter()
            .filter_map(|choice| {
                let person = people
                    .binary_search_by_key(&choice.person, |p| p.id)
                    .ok()
                    .map(|i| &people[i])?;
                let field = fields
                    .binary_search_by_key(&choice.field, |f| f.id)
                    .ok()
                    .map(|i| fields[i])?;
                let moisture = self
                    .moisture
                    .binary_search_by_key(&choice.field, |m| m.field)
                    .ok()
                    .map(|i| &self.moisture[i])?;
                let source = view.container(Place::Stock {
                    region: field.region,
                    good: self.water,
                })?;
                let (fear, obligation) = choice
                    .pressured_by
                    .and_then(|target| {
                        feelings
                            .as_ref()
                            .map(|index| index.feelings(person.id, target))
                    })
                    .unwrap_or((0, 0));
                if (!choice.willing
                    && !choice
                        .fear_at_least
                        .is_some_and(|threshold| fear >= threshold)
                    && !choice
                        .obligation_at_least
                        .is_some_and(|threshold| obligation >= threshold))
                    || person.controlled
                    || person.health == 0
                    || person.hunger > max_hunger
                    || busy.binary_search(&person.id).is_ok()
                    || person.region != Some(field.region)
                    || field.phase != Phase::Growing
                    || (!choice.disregard_claims
                        && claims.is_some_and(|index| {
                            !index.allows(person.id, field.region, self.water)
                        }))
                {
                    return None;
                }
                let quantity = choice
                    .quantity
                    .min(self.capacity.saturating_sub(moisture.amount));
                (quantity > 0).then_some((
                    person.id,
                    field.id,
                    field.region,
                    quantity,
                    source.quantity(self.water) > 0,
                ))
            })
            .collect();
        base.checked_add(selected.len() as u64)
            .ok_or("action count overflow")?;
        let rows: Vec<_> = selected
            .into_par_iter()
            .enumerate()
            .map(|(index, (person, field, region, quantity, has_water))| {
                let id = base + index as u64;
                (
                    person,
                    field,
                    quantity,
                    Transaction {
                        id,
                        movements: vec![Movement {
                            id: 1,
                            from: Place::Stock {
                                region,
                                good: self.water,
                            },
                            to: None,
                            good: self.water,
                            quantity,
                            item: None,
                        }],
                        creations: vec![],
                    },
                    Use {
                        action: id,
                        person,
                        region,
                        good: self.water,
                        quantity,
                        job: false,
                    },
                    has_water,
                )
            })
            .collect();
        Ok(Plan { rows })
    }
    pub fn advance(&self, plan: Plan, outcomes: &Events, tick: u64) -> Self {
        let events: Vec<_> = plan
            .rows
            .par_iter()
            .map(|(person, field, quantity, action, _, _)| Event {
                tick,
                person: *person,
                field: *field,
                quantity: *quantity,
                accepted: outcomes
                    .physical_actions
                    .binary_search_by_key(&action.id, |o| o.id)
                    .ok()
                    .is_some_and(|i| outcomes.physical_actions[i].accepted),
            })
            .collect();
        let mut by_field: Vec<_> = events.par_iter().filter(|event| event.accepted).collect();
        if by_field
            .par_windows(2)
            .any(|pair| pair[0].field > pair[1].field)
        {
            by_field.par_sort_unstable_by_key(|event| event.field);
        }
        let moisture = self
            .moisture
            .par_iter()
            .map(|m| {
                let added = by_field
                    .binary_search_by_key(&m.field, |event| event.field)
                    .ok()
                    .map_or(0, |i| by_field[i].quantity);
                Moisture {
                    field: m.field,
                    amount: m
                        .amount
                        .saturating_sub(self.evaporation)
                        .saturating_add(added)
                        .min(self.capacity),
                }
            })
            .collect();
        Self {
            water: self.water,
            capacity: self.capacity,
            minimum_growth: self.minimum_growth,
            evaporation: self.evaporation,
            moisture,
            choices: self.choices.par_iter().cloned().collect(),
            events,
        }
    }
}
pub struct Plan {
    rows: Vec<(u64, u64, u64, Transaction, Use, bool)>,
}
impl Plan {
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    pub fn actors(&self) -> impl Iterator<Item = u64> + '_ {
        self.rows.iter().map(|row| row.0)
    }
    pub fn actions(&self) -> impl ParallelIterator<Item = &Transaction> {
        // An empty opening stock cannot satisfy any request in this boundary.
        // Keep the attempted event and actor reservation without staging a doomed transfer.
        self.rows.par_iter().filter(|row| row.5).map(|row| &row.3)
    }
    pub fn uses(&self) -> impl ParallelIterator<Item = Use> + '_ {
        self.rows.par_iter().filter(|row| row.5).map(|row| row.4)
    }
}
