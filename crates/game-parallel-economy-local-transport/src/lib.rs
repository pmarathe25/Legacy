//! Distinct local stores and personal choices to carry goods between them.
use game_parallel_economy_claims::{Index as Claims, Use};
use game_parallel_economy_person::Person;
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Movement, Place, Transaction, View};
use game_parallel_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Store {
    pub id: u64,
    pub region: u64,
    pub name: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Endpoint {
    Stock,
    Store { id: u64 },
}
impl Endpoint {
    fn place(self, region: u64, good: u32) -> Place {
        match self {
            Self::Stock => Place::Stock { region, good },
            Self::Store { id } => Place::Inventory(id),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub region: u64,
    pub source: Endpoint,
    pub destination: Endpoint,
    pub good: u32,
    pub quantity: u64,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
    #[serde(default)]
    pub disregard_claims: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub from: Place,
    pub to: Place,
    pub good: u32,
    pub quantity: u64,
    pub moved: bool,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalTransport {
    pub stores: Vec<Store>,
    #[serde(default)]
    pub policies: Vec<Policy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
}
impl Clone for LocalTransport {
    fn clone(&self) -> Self {
        Self {
            stores: self.stores.par_iter().cloned().collect(),
            policies: self.policies.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
        }
    }
}
impl LocalTransport {
    pub fn normalize(&mut self) {
        self.stores.par_sort_unstable_by_key(|store| store.id);
        self.policies
            .par_sort_unstable_by_key(|policy| policy.person);
    }
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        view: &View,
        tick: u64,
    ) -> Result<(), String> {
        let regions: HashSet<_> = view
            .containers
            .par_iter()
            .filter_map(|row| match row.place {
                Place::Stock { region, .. } => Some(region),
                _ => None,
            })
            .collect();
        let endpoint = |end: Endpoint, region, good| match end {
            Endpoint::Stock => view.container(end.place(region, good)).is_some(),
            Endpoint::Store { id } => self
                .stores
                .binary_search_by_key(&id, |store| store.id)
                .ok()
                .is_some_and(|i| self.stores[i].region == region),
        };
        if self.stores.par_windows(2).any(|w| w[0].id >= w[1].id)
            || self.stores.par_iter().any(|s| s.id == 0 || s.name.is_empty() || !regions.contains(&s.region) || view.container(Place::Inventory(s.id)).is_none())
            || view.containers.par_iter().any(|c| matches!(c.place, Place::Inventory(id) if self.stores.binary_search_by_key(&id, |s| s.id).is_err()))
            || self.policies.par_windows(2).any(|w| w[0].person >= w[1].person)
            || self.policies.par_iter().any(|p| !people.contains(&p.person) || !goods.contains(&p.good) || p.quantity == 0
                || p.source == p.destination || !endpoint(p.source, p.region, p.good) || !endpoint(p.destination, p.region, p.good)
                || p.pressured_by.is_some() != p.pressure_threshold.is_some() || p.pressure_threshold == Some(0)
                || p.pressured_by.is_some_and(|id| id == p.person || !people.contains(&id)))
            || self.events.par_iter().any(|e| {
                let holder = Place::Person(e.person);
                let other = if e.from == holder { Some(e.to) } else if e.to == holder { Some(e.from) } else { None };
                e.tick > tick || !people.contains(&e.person) || !goods.contains(&e.good) || e.quantity == 0
                    || other.is_none_or(|place| match place {
                        Place::Inventory(id) => self.stores.binary_search_by_key(&id, |store| store.id).is_err(),
                        Place::Stock { good, .. } => good != e.good || view.container(place).is_none(),
                        _ => true,
                    })
            })
        { return Err("invalid local transport state".into()); }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct Manual {
    pub person: u64,
    pub store: u64,
    pub good: u32,
    pub quantity: u64,
    pub deposit: bool,
}
struct Choice {
    person: u64,
    from: Place,
    to: Place,
    good: u32,
    quantity: u64,
}
pub struct Plan {
    pub actions: Vec<Transaction>,
    pub actors: Vec<u64>,
    pub uses: Vec<Use>,
    rows: Vec<Choice>,
}
pub struct Context<'a> {
    pub owner: &'a LocalTransport,
    pub people: &'a [Person],
    pub view: &'a View,
    pub opinions: Option<&'a Opinions>,
    pub claims: Option<&'a Claims<'a>>,
    pub busy: &'a [u64],
    pub max_hunger: u32,
    pub manual: Option<Manual>,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        if self.manual.is_none()
            && self
                .owner
                .policies
                .par_iter()
                .all(|policy| !policy.willing && policy.pressure_threshold.is_none())
        {
            return Ok(Plan {
                actions: vec![],
                actors: vec![],
                uses: vec![],
                rows: vec![],
            });
        }
        let opinions = self
            .owner
            .policies
            .par_iter()
            .any(|policy| policy.pressure_threshold.is_some())
            .then(|| self.opinions.map(Opinions::index))
            .flatten();
        let mut rows: Vec<_> =
            self.owner
                .policies
                .par_iter()
                .filter_map(|p| {
                    let person = self
                        .people
                        .binary_search_by_key(&p.person, |person| person.id)
                        .ok()
                        .map(|i| &self.people[i])?;
                    let pressured = p.pressured_by.zip(p.pressure_threshold).is_some_and(
                        |(other, threshold)| {
                            opinions.as_ref().is_some_and(|index| {
                                let (fear, obligation) = index.feelings(person.id, other);
                                fear.saturating_add(obligation) >= threshold
                            })
                        },
                    );
                    if person.controlled
                        || person.health == 0
                        || person.region != Some(p.region)
                        || person.hunger > self.max_hunger
                        || self.busy.binary_search(&person.id).is_ok()
                        || (!p.willing && !pressured)
                    {
                        return None;
                    }
                    let holder = self.view.container(Place::Person(person.id))?;
                    let carrying = holder.quantity(p.good);
                    let (from, to, quantity) = if carrying == 0 {
                        if !p.disregard_claims
                            && matches!(p.source, Endpoint::Stock)
                            && self
                                .claims
                                .is_some_and(|claims| !claims.allows(person.id, p.region, p.good))
                        {
                            return None;
                        }
                        let source = p.source.place(p.region, p.good);
                        let stock = self.view.container(source)?;
                        (
                            source,
                            Place::Person(person.id),
                            p.quantity
                                .min(stock.quantity(p.good))
                                .min(holder.capacity.saturating_sub(holder.used)),
                        )
                    } else {
                        let destination = p.destination.place(p.region, p.good);
                        let store = self.view.container(destination)?;
                        (
                            Place::Person(person.id),
                            destination,
                            p.quantity
                                .min(carrying)
                                .min(store.capacity.saturating_sub(store.used)),
                        )
                    };
                    (quantity > 0).then_some(Choice {
                        person: person.id,
                        from,
                        to,
                        good: p.good,
                        quantity,
                    })
                })
                .collect();
        if let Some(manual) = self.manual {
            let person = self
                .people
                .binary_search_by_key(&manual.person, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown person")?;
            let store = self
                .owner
                .stores
                .binary_search_by_key(&manual.store, |s| s.id)
                .ok()
                .map(|i| &self.owner.stores[i])
                .ok_or("unknown local store")?;
            if !person.controlled
                || person.health == 0
                || person.region != Some(store.region)
                || manual.good == 0
                || manual.quantity == 0
                || self.view.container(Place::Person(person.id)).is_none()
            {
                return Err(
                    "local carrying requires a living controlled person at the store".into(),
                );
            }
            let holder = Place::Person(person.id);
            let inventory = Place::Inventory(store.id);
            let (from, to) = if manual.deposit {
                (holder, inventory)
            } else {
                (inventory, holder)
            };
            rows.push(Choice {
                person: person.id,
                from,
                to,
                good: manual.good,
                quantity: manual.quantity,
            });
        }
        rows.par_sort_unstable_by_key(|row| row.person);
        base.checked_add(rows.len() as u64)
            .ok_or("local carrying action overflow")?;
        let actions = rows
            .par_iter()
            .enumerate()
            .map(|(i, row)| {
                let id = base + i as u64;
                Transaction {
                    id,
                    movements: vec![Movement {
                        id,
                        from: row.from,
                        to: Some(row.to),
                        good: row.good,
                        quantity: row.quantity,
                    }],
                    creations: vec![],
                }
            })
            .collect();
        let uses = rows
            .par_iter()
            .enumerate()
            .filter_map(|(i, row)| match row.from {
                Place::Stock { region, .. } => self.claims.map(|_| Use {
                    action: base + i as u64,
                    person: row.person,
                    region,
                    good: row.good,
                    quantity: row.quantity,
                    job: false,
                }),
                _ => None,
            })
            .collect();
        Ok(Plan {
            actors: rows.par_iter().map(|row| row.person).collect(),
            actions,
            uses,
            rows,
        })
    }
}
impl Plan {
    pub fn publish(self, tick: u64, events: &Events) -> Vec<Event> {
        if self.rows.is_empty() {
            return Vec::new();
        }
        let accepted: HashSet<_> = events
            .physical_actions
            .par_iter()
            .filter(|row| row.accepted)
            .map(|row| row.id)
            .collect();
        self.rows
            .into_par_iter()
            .zip(self.actions.into_par_iter())
            .map(|(row, action)| Event {
                tick,
                person: row.person,
                from: row.from,
                to: row.to,
                good: row.good,
                quantity: row.quantity,
                moved: accepted.contains(&action.id),
            })
            .collect()
    }
}
