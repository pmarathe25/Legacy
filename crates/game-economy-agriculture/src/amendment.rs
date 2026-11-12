//! Personal soil improvement consumes actual locally held material.
use crate::{Agriculture, soil::Soil};
use game_economy_claims::{Index, Use};
use game_economy_person::Person;
use game_lot_runtime::Events;
use game_lots::{Movement, Place, Transaction, View};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub person: u64,
    pub field: u64,
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
    pub accepted: bool,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Amendment {
    pub good: u32,
    pub quantity: u64,
    pub fertility_gain: u64,
    #[serde(default)]
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub events: Vec<Event>,
}
impl Clone for Amendment {
    fn clone(&self) -> Self {
        Self {
            good: self.good,
            quantity: self.quantity,
            fertility_gain: self.fertility_gain,
            choices: self.choices.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
        }
    }
}
pub struct Context<'a> {
    pub soil: &'a Soil,
    pub agriculture: &'a Agriculture,
    pub people: &'a [Person],
    pub busy: &'a [u64],
    pub nonstock: &'a [u64],
    pub occupied_fields: &'a [u64],
    pub claims: Option<&'a Index<'a>>,
    pub opinions: Option<&'a game_opinions::Opinions>,
    pub view: &'a View,
    pub max_hunger: u32,
    pub seed: u64,
    pub tick: u64,
    pub manual: Option<&'a Choice>,
}
impl Amendment {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        soil: &Soil,
    ) -> Result<(), String> {
        let mut actors: Vec<_> = self.choices.par_iter().map(|row| row.person).collect();
        actors.par_sort_unstable();
        if !goods.contains(&self.good)
            || self.quantity == 0
            || self.fertility_gain == 0
            || actors.par_windows(2).any(|p| p[0] == p[1])
            || self.choices.par_iter().any(|row| {
                !people.contains(&row.person)
                    || row
                        .pressured_by
                        .is_some_and(|id| id == row.person || !people.contains(&id))
                    || (row.fear_at_least.is_some() || row.obligation_at_least.is_some())
                        != row.pressured_by.is_some()
                    || soil
                        .fields
                        .binary_search_by_key(&row.field, |field| field.field)
                        .is_err()
            })
            || self.events.par_iter().any(|row| {
                !people.contains(&row.person)
                    || soil
                        .fields
                        .binary_search_by_key(&row.field, |field| field.field)
                        .is_err()
            })
        {
            return Err("invalid soil amendment".into());
        }
        Ok(())
    }
    pub fn prepare(&self, context: Context<'_>, base: u64) -> Result<Plan, String> {
        let Context {
            soil,
            agriculture,
            people,
            busy,
            nonstock,
            occupied_fields,
            claims,
            opinions,
            view,
            max_hunger,
            seed,
            tick,
            manual,
        } = context;
        let opinions = opinions
            .filter(|_| {
                self.choices
                    .par_iter()
                    .any(|choice| !choice.willing && choice.pressured_by.is_some())
            })
            .map(game_opinions::Opinions::index);
        let mut fields: Vec<_> = agriculture.fields.par_iter().collect();
        if fields.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            fields.par_sort_unstable_by_key(|row| row.id);
        }
        let mut rows: Vec<_> = self
            .choices
            .par_iter()
            .filter(|choice| manual.is_none_or(|selected| selected.person != choice.person))
            .chain(manual.into_par_iter())
            .filter_map(|choice| {
                let manually_selected =
                    manual.is_some_and(|selected| selected.person == choice.person);
                let person = people.get(
                    people
                        .binary_search_by_key(&choice.person, |row| row.id)
                        .ok()?,
                )?;
                let field = fields.get(
                    fields
                        .binary_search_by_key(&choice.field, |row| row.id)
                        .ok()?,
                )?;
                let fertility = soil.fields.get(
                    soil.fields
                        .binary_search_by_key(&choice.field, |row| row.field)
                        .ok()?,
                )?;
                let (fear, obligation) = choice
                    .pressured_by
                    .and_then(|target| {
                        opinions
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
                    || (person.controlled && !manually_selected)
                    || person.health == 0
                    || person.hunger > max_hunger
                    || person.region != Some(field.region)
                    || (!manually_selected && busy.binary_search(&person.id).is_ok())
                    || nonstock.binary_search(&person.id).is_ok()
                    || occupied_fields.binary_search(&field.id).is_ok()
                    || fertility.fertility == soil.capacity
                    || (!manually_selected
                        && view
                            .container(Place::Stock {
                                region: field.region,
                                good: self.good,
                            })
                            .is_none_or(|stock| stock.quantity(self.good) < self.quantity))
                    || (!choice.disregard_claims
                        && claims.is_some_and(|c| {
                            !c.allows_field(person.id, field.id, None)
                                || !c.allows(person.id, field.region, self.good)
                        }))
                {
                    return None;
                }
                Some((person.id, field.id, field.region))
            })
            .collect();
        let rank = |person: u64, field: u64| {
            let mut value = seed ^ tick.rotate_left(17) ^ field.rotate_left(31) ^ person;
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            value ^ (value >> 31)
        };
        if rows.par_windows(2).any(|pair| pair[0].1 >= pair[1].1) {
            rows.par_sort_unstable_by_key(|row| (row.1, rank(row.0, row.1), row.0));
            rows = rows
                .par_chunk_by(|a, b| a.1 == b.1)
                .map(|group| group[0])
                .collect();
        }
        base.checked_add(rows.len() as u64)
            .ok_or("action count overflow")?;
        Ok(Plan {
            rows: rows
                .into_par_iter()
                .enumerate()
                .map(|(index, (person, field, region))| {
                    let id = base + index as u64;
                    (
                        person,
                        field,
                        Transaction {
                            id,
                            movements: vec![Movement {
                                batch: None,
                                id: 1,
                                from: Place::Stock {
                                    region,
                                    good: self.good,
                                },
                                to: None,
                                good: self.good,
                                quantity: self.quantity,
                                item: None,
                            }],
                            creations: vec![],
                        },
                        Use {
                            action: id,
                            person,
                            region,
                            good: self.good,
                            quantity: self.quantity,
                            job: false,
                        },
                    )
                })
                .collect(),
        })
    }
    pub fn publish(&self, soil: &mut Soil, plan: Plan, outcomes: &Events, tick: u64) {
        let events: Vec<_> = plan
            .rows
            .par_iter()
            .map(|(person, field, action, _)| Event {
                tick,
                person: *person,
                field: *field,
                accepted: outcomes
                    .physical_actions
                    .binary_search_by_key(&action.id, |row| row.id)
                    .ok()
                    .is_some_and(|i| outcomes.physical_actions[i].accepted),
            })
            .collect();
        let mut accepted: Vec<_> = events
            .par_iter()
            .filter(|row| row.accepted)
            .map(|row| row.field)
            .collect();
        accepted.par_sort_unstable();
        soil.fields.par_iter_mut().for_each(|row| {
            if accepted.binary_search(&row.field).is_ok() {
                row.fertility = row
                    .fertility
                    .saturating_add(self.fertility_gain)
                    .min(soil.capacity);
            }
        });
        soil.amendment = Some(Self {
            good: self.good,
            quantity: self.quantity,
            fertility_gain: self.fertility_gain,
            choices: self.choices.par_iter().cloned().collect(),
            events,
        });
    }
}
pub struct Plan {
    rows: Vec<(u64, u64, Transaction, Use)>,
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
        self.rows.par_iter().map(|row| &row.2)
    }
    pub fn uses(&self) -> impl ParallelIterator<Item = Use> + '_ {
        self.rows.par_iter().map(|row| row.3)
    }
}
