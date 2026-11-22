//! Distinct local stores and personal choices to carry goods between them.
pub mod goal;
pub mod preservation;
pub mod walking;
use game_economy_claims::{Index as Claims, Use};
use game_economy_person::Person;
use game_lot_runtime::Events;
use game_lots::{Movement, Place, Transaction, View};
use game_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Store {
    pub id: u64,
    pub region: u64,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<StoreStructure>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreStructure {
    pub site: u64,
    pub celsius: i16,
    pub intact: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Endpoint {
    Stock,
    Store {
        id: u64,
    },
    /// The acting person's own carried goods, never another person's custody.
    Person,
}
impl Endpoint {
    fn place(self, region: u64, good: u32, person: u64) -> Place {
        match self {
            Self::Stock => Place::Stock { region, good },
            Self::Store { id } => Place::Inventory(id),
            Self::Person => Place::Person(person),
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
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalTransport {
    pub stores: Vec<Store>,
    #[serde(default)]
    pub policies: Vec<Policy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preservation: Vec<preservation::Preference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub goal_fetchers: Vec<goal::Preference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
    /// Optional physical travel between the regional stock and named stores.
    #[serde(default)]
    pub walking: Option<walking::Walking>,
}
impl Clone for LocalTransport {
    fn clone(&self) -> Self {
        Self {
            stores: self.stores.par_iter().cloned().collect(),
            policies: self.policies.par_iter().cloned().collect(),
            preservation: self.preservation.par_iter().cloned().collect(),
            goal_fetchers: self.goal_fetchers.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
            walking: self.walking.clone(),
        }
    }
}
impl LocalTransport {
    pub fn has_choices(&self) -> bool {
        !self.policies.is_empty()
            || !self.goal_fetchers.is_empty()
            || !self.preservation.is_empty()
    }

    pub fn remember_built_cellars(&mut self, stores: &[Store]) {
        if self.preservation.is_empty() {
            return;
        }
        let mut built: Vec<_> = stores.par_iter().filter_map(|store| {
            let structure = store.structure?;
            Some(((structure.builder?, store.region, store.id), structure.celsius))
        }).collect();
        built.par_sort_unstable_by_key(|row| row.0);
        self.preservation.par_iter_mut().for_each(|preference| {
            let Some(profile) = &mut preference.own_cellar else { return };
            let key = (preference.person, preference.region);
            let end = built.partition_point(|row| (row.0.0, row.0.1) <= key);
            if let Some(((builder, region, store), built_celsius)) = end.checked_sub(1).map(|i| built[i])
                && (builder, region) == key
            {
                profile.known = Some(preservation::KnownCellar { store, built_celsius });
            }
        });
    }

    pub fn remember_visited_stores(
        &mut self,
        arrivals: &[walking::Event],
        now: u128,
        temperature: impl Fn(Place) -> i16 + Sync,
    ) {
        if self.preservation.is_empty() {
            return;
        }
        let updates: Vec<_> = arrivals.par_iter().filter_map(|arrival| {
            let Endpoint::Store { id } = arrival.destination else { return None };
            if arrival.outcome != walking::Outcome::Arrived {
                return None;
            }
            let index = self.preservation.binary_search_by_key(&arrival.person, |row| row.person).ok()?;
            let preference = &self.preservation[index];
            let profile = preference.visits?;
            if profile.attention <= profile.idle {
                return None;
            }
            let store = &self.stores[self.stores.binary_search_by_key(&id, |row| row.id).ok()?];
            (preference.region == store.region).then(|| (index, preservation::Observation {
                store: id,
                celsius: temperature(Place::Inventory(id)),
                at_millis: now,
            }))
        }).collect();
        for (index, observed) in updates {
            self.preservation[index].visits.as_mut().expect("prepared observer").observed = Some(observed);
        }
    }

    fn valid_goal_search(&self, search: &goal::SearchPreference) -> bool {
        let Some(first) = self
            .stores
            .binary_search_by_key(&search.store, |store| store.id)
            .ok()
            .map(|index| &self.stores[index])
        else {
            return false;
        };
        self.walking.is_some()
            && search.valid_clock()
            && search.max_visits > 0
            && search.max_visits <= 32
            && search.alternatives.len() < 32
            && (search.max_visits as usize) <= search.alternatives.len() + 1
            && search.examined <= search.max_visits
            && search.active_recipe != Some(0)
            && (search.active_recipe.is_some() || search.target.is_none())
            && search.target.is_none_or(|target| {
                target == search.store || search.alternatives.contains(&target)
            })
            && search.alternatives.iter().enumerate().all(|(index, id)| {
                *id != search.store
                    && !search.alternatives[..index].contains(id)
                    && self
                        .stores
                        .binary_search_by_key(id, |store| store.id)
                        .ok()
                        .is_some_and(|i| self.stores[i].region == first.region)
            })
    }
    /// Clone the transport owner while a replacement walking generation is staged separately.
    pub fn clone_without_walking(&self) -> Self {
        Self {
            stores: self.stores.par_iter().cloned().collect(),
            policies: self.policies.par_iter().cloned().collect(),
            preservation: self.preservation.par_iter().cloned().collect(),
            goal_fetchers: self.goal_fetchers.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
            walking: None,
        }
    }
    pub fn normalize(&mut self) {
        self.stores.par_sort_unstable_by_key(|store| store.id);
        self.policies
            .par_sort_unstable_by_key(|policy| policy.person);
        self.preservation.par_sort_unstable_by_key(|policy| policy.person);
        self.goal_fetchers
            .par_sort_unstable_by_key(|policy| policy.person);
        if let Some(walking) = &mut self.walking {
            walking.normalize();
        }
    }
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        view: &View,
        tick: u64,
        now: u128,
    ) -> Result<(), String> {
        let regions: HashSet<_> = view
            .regions().iter().copied().collect();
        let endpoint = |end: Endpoint, region, good, person| match end {
            Endpoint::Stock => view.container(end.place(region, good, person)).is_some(),
            Endpoint::Person => view.container(Place::Person(person)).is_some(),
            Endpoint::Store { id } => self
                .stores
                .binary_search_by_key(&id, |store| store.id)
                .ok()
                .is_some_and(|i| self.stores[i].region == region),
        };
        if self.stores.par_windows(2).any(|w| w[0].id >= w[1].id)
            || self.stores.par_iter().any(|s| s.id == 0 || s.name.is_empty() || !regions.contains(&s.region) || view.container(Place::Inventory(s.id)).is_none()
                || s.structure.and_then(|row| row.builder).is_some_and(|builder| !people.contains(&builder)))
            || view.containers.par_iter().any(|c| matches!(c.place, Place::Inventory(id) if self.stores.binary_search_by_key(&id, |s| s.id).is_err()))
            || self.policies.par_windows(2).any(|w| w[0].person >= w[1].person)
            || self.preservation.par_windows(2).any(|w| w[0].person >= w[1].person)
            || self.preservation.par_iter().any(|p| {
                let mut stores = HashSet::new();
                !people.contains(&p.person) || !goods.contains(&p.good) || p.quantity == 0 || !regions.contains(&p.region)
                    || self.policies.binary_search_by_key(&p.person, |row| row.person).is_ok()
                    || p.destinations.is_empty() && p.own_cellar.is_none() && p.visits.is_none()
                    || p.visits.is_some_and(|profile| {
                        self.walking.as_ref().is_none_or(|walking| walking.elapsed.is_none())
                            || profile.observed.is_some_and(|observed| {
                                observed.at_millis > now
                                    || !endpoint(Endpoint::Store { id: observed.store }, p.region, p.good, p.person)
                            })
                    })
                    || p.own_cellar.and_then(|profile| profile.known).is_some_and(|known| {
                        self.stores.binary_search_by_key(&known.store, |row| row.id)
                            .ok().map(|index| &self.stores[index]).is_none_or(|store| {
                                store.region != p.region || store.structure.is_none_or(|structure| {
                                    structure.builder != Some(p.person) || structure.celsius != known.built_celsius
                                })
                            })
                    })
                    || p.destinations.iter().any(|row| {
                        !stores.insert(row.store)
                            || !endpoint(Endpoint::Store { id: row.store }, p.region, p.good, p.person)
                    })
            })
            || self.goal_fetchers.par_windows(2).any(|w| w[0].person >= w[1].person)
            || self.goal_fetchers.par_iter().any(|p| !people.contains(&p.person) || p.max_quantity == 0
                || p.search.as_ref().is_some_and(|search| !self.valid_goal_search(search)))
            || self.policies.par_iter().any(|p| !people.contains(&p.person) || !goods.contains(&p.good) || p.quantity == 0
                || p.source == p.destination || !endpoint(p.source, p.region, p.good, p.person) || !endpoint(p.destination, p.region, p.good, p.person)
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opportunity {
    pub person: u64,
    pub from: Place,
    pub to: Place,
    pub good: u32,
    pub quantity: u64,
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
    pub now: u128,
    pub owner: &'a LocalTransport,
    pub people: &'a [Person],
    pub view: &'a View,
    pub opinions: Option<&'a Opinions>,
    pub claims: Option<&'a Claims<'a>>,
    pub busy: &'a [u64],
    pub max_hunger: u32,
    pub manual: Option<Manual>,
    pub walking: Option<&'a walking::Walking>,
    pub storage_celsius: &'a (dyn Fn(Place) -> i16 + Sync),
}
impl Context<'_> {
    fn personal_choice(
        &self,
        p: &Policy,
        opinions: Option<&game_opinions::Index<'_>>,
    ) -> Option<Choice> {
        let person = self
            .people
            .binary_search_by_key(&p.person, |person| person.id)
            .ok()
            .map(|i| &self.people[i])?;
        let pressured =
            p.pressured_by
                .zip(p.pressure_threshold)
                .is_some_and(|(other, threshold)| {
                    opinions.is_some_and(|index| {
                        let (fear, obligation) = index.feelings(person.id, other);
                        fear.saturating_add(obligation) >= threshold
                    })
                });
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
        let pickup = if p.destination == Endpoint::Person {
            carrying < p.quantity
        } else if p.source == Endpoint::Person {
            false
        } else {
            carrying == 0
        };
        if (p.destination == Endpoint::Person && !pickup)
            || (p.source == Endpoint::Person && carrying == 0)
        {
            return None;
        }
        let endpoint = if pickup { p.source } else { p.destination };
        if self
            .walking
            .is_some_and(|walking| !walking.present(person.id, p.region, endpoint))
        {
            return None;
        }
        let (from, to, quantity) = if pickup {
            if !p.disregard_claims
                && matches!(p.source, Endpoint::Stock)
                && self
                    .claims
                    .is_some_and(|claims| !claims.allows(person.id, p.region, p.good))
            {
                return None;
            }
            let source = p.source.place(p.region, p.good, person.id);
            let stock = self.view.container(source)?;
            (
                source,
                Place::Person(person.id),
                (if p.destination == Endpoint::Person {
                    p.quantity.saturating_sub(carrying)
                } else {
                    p.quantity
                })
                .min(stock.quantity(p.good))
                .min(holder.capacity.saturating_sub(holder.used)),
            )
        } else {
            let destination = p.destination.place(p.region, p.good, person.id);
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
    }

    /// Actors with a currently executable personal carrying choice.
    pub fn ready_people(&self, candidates: &[u64]) -> Vec<u64> {
        if candidates.is_empty()
            || (self.owner.policies.is_empty() && self.owner.preservation.is_empty())
        {
            return Vec::new();
        }
        let needs_opinions = candidates.par_iter().any(|person| {
            self.owner
                .policies
                .binary_search_by_key(person, |p| p.person)
                .ok()
                .is_some_and(|i| self.owner.policies[i].pressure_threshold.is_some())
        });
        let opinions = needs_opinions
            .then(|| self.opinions.map(Opinions::index))
            .flatten();
        candidates
            .par_iter()
            .filter_map(|person| {
                self.owner
                    .policies
                    .binary_search_by_key(person, |p| p.person)
                    .ok()
                    .and_then(|index| {
                        self.personal_choice(&self.owner.policies[index], opinions.as_ref())
                    })
                    .or_else(|| {
                        let index = self
                            .owner
                            .preservation
                            .binary_search_by_key(person, |p| p.person)
                            .ok()?;
                        self.owner.preservation[index].choose(self)
                    })
                    .map(|choice| choice.person)
            })
            .collect()
    }

    fn direct_choices(&self) -> Vec<Choice> {
        let opinions = self
            .owner
            .policies
            .par_iter()
            .any(|policy| policy.pressure_threshold.is_some())
            .then(|| self.opinions.map(Opinions::index))
            .flatten();
        self.owner
            .policies
            .par_iter()
            .filter_map(|p| self.personal_choice(p, opinions.as_ref()))
            .chain(
                self.owner
                    .preservation
                    .par_iter()
                    .filter_map(|p| p.choose(self)),
            )
            .collect()
    }

    /// Direct policy opportunities without physical transactions; excludes manual requests.
    pub fn direct_opportunities(&self) -> Vec<Opportunity> {
        self.opportunities(None)
    }

    pub fn opportunities(&self, goals: Option<&goal::Context<'_>>) -> Vec<Opportunity> {
        self.choices_with_goals(goals)
            .into_par_iter()
            .map(|row| Opportunity {
                person: row.person,
                from: row.from,
                to: row.to,
                good: row.good,
                quantity: row.quantity,
            })
            .collect()
    }

    pub fn prepare_with_goals(&self, base: u64, goals: &goal::Context<'_>) -> Result<Plan, String> {
        finish(self.choices_with_goals(Some(goals)), base, self.claims)
    }

    fn choices_with_goals(&self, goals: Option<&goal::Context<'_>>) -> Vec<Choice> {
        let mut choices = self.direct_choices();
        if let Some(goals) = goals {
            let selected: HashSet<_> = choices.par_iter().map(|row| row.person).collect();
            choices.par_extend(
                goals
                    .directed_choices()
                    .into_par_iter()
                    .filter(|row| !selected.contains(&row.person)),
            );
        }
        choices
    }

    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        if self.manual.is_none()
            && self.owner.preservation.is_empty()
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
        let mut rows = if self.manual.is_some() { Vec::new() } else { self.direct_choices() };
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
                || self.walking.is_some_and(|walking| {
                    !walking.present(person.id, store.region, Endpoint::Store { id: store.id })
                })
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
        finish(rows, base, self.claims)
    }
}
fn finish(mut rows: Vec<Choice>, base: u64, claims: Option<&Claims<'_>>) -> Result<Plan, String> {
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
                    batch: None,
                    item: None,
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
            Place::Stock { region, .. } => claims.map(|_| Use {
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
