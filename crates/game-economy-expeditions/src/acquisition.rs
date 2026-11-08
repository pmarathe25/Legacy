//! Individual travelers may choose to obtain missing goods from local custody.
use crate::{MissionPhase, Missions, Objective};
use game_economy_claims::{Index as Claims, Use};
use game_economy_local_transport::{
    Endpoint, Store,
    walking::{Location, Walking},
};
use game_economy_person::Person;
use game_lots::{Movement, Place, Transaction, View};
use game_opinions::Opinions;
use rayon::prelude::*;

pub struct Context<'a> {
    pub missions: Option<&'a Missions>,
    pub people: &'a [Person],
    pub view: &'a View,
    pub stores: &'a [Store],
    pub walking: Option<&'a Walking>,
    pub opinions: Option<&'a Opinions>,
    pub claims: Option<&'a Claims<'a>>,
    pub busy: &'a [u64],
    pub max_hunger: u32,
}
pub struct Plan {
    pub actions: Vec<Transaction>,
    pub actors: Vec<u64>,
    pub uses: Vec<Use>,
}

impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        let entries = self
            .missions
            .map_or(&[][..], |missions| missions.entries.as_slice());
        if !entries.par_iter().any(|mission| {
            !mission.objective_met
                && mission
                    .delivery_preference
                    .and_then(|p| p.acquisition)
                    .is_some_and(|choice| choice.willing || choice.pressure_threshold.is_some())
        }) {
            return Ok(Plan {
                actions: vec![],
                actors: vec![],
                uses: vec![],
            });
        }
        let needs_opinions = entries.par_iter().any(|mission| {
            mission
                .delivery_preference
                .and_then(|preference| preference.acquisition)
                .is_some_and(|choice| choice.pressure_threshold.is_some())
        });
        let opinions = self
            .opinions
            .filter(|_| needs_opinions)
            .map(Opinions::index);
        let visible_stores = if self.walking.is_some() {
            &[][..]
        } else {
            self.stores
        };
        let mut store_stock: Vec<_> = visible_stores
            .par_iter()
            .flat_map_iter(|store| {
                self.view
                    .container(Place::Inventory(store.id))
                    .into_iter()
                    .flat_map(move |holder| {
                        holder
                            .goods
                            .iter()
                            .filter(|good| good.quantity > 0)
                            .map(move |good| (store.region, good.good, store.id, good.quantity))
                    })
            })
            .collect();
        store_stock.par_sort_unstable_by_key(|row| (row.0, row.1, row.2));
        let mut rows: Vec<_> = entries
            .par_iter()
            .filter_map(|mission| {
                if mission.objective_met
                    || matches!(
                        mission.phase,
                        MissionPhase::Completed | MissionPhase::Failed
                    )
                {
                    return None;
                }
                let preference = mission.delivery_preference?;
                let choice = preference.acquisition?;
                let Some(Objective::Deliver {
                    person: recipient,
                    good,
                    quantity,
                    delivered,
                }) = mission.objective
                else {
                    return None;
                };
                let person = self
                    .people
                    .binary_search_by_key(&mission.person, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])?;
                let region = person.region?;
                if person.controlled
                    || person.health == 0
                    || person.hunger > self.max_hunger
                    || self.busy.binary_search(&person.id).is_ok()
                    || (mission.phase == MissionPhase::Returning && region == mission.origin)
                {
                    return None;
                }
                let pressured = choice.pressure_threshold.is_some_and(|threshold| {
                    opinions.as_ref().is_some_and(|index| {
                        let (fear, obligation) = index.feelings(person.id, recipient);
                        fear.saturating_add(obligation) >= threshold
                    })
                });
                if !(choice.willing || pressured) {
                    return None;
                }
                let holder = self.view.container(Place::Person(person.id))?;
                let needed = quantity
                    .saturating_sub(delivered)
                    .saturating_add(preference.reserve)
                    .saturating_sub(holder.quantity(good));
                let needed = needed.min(holder.capacity.saturating_sub(holder.used));
                if needed == 0 {
                    return None;
                }
                let location = self
                    .walking
                    .map(|walking| walking.location(person.id, region));
                if matches!(location, Some(Location::Walking { .. })) {
                    return None;
                }
                let regional = Place::Stock { region, good };
                let regional_quantity = if location.is_none_or(|location| {
                    matches!(
                        location,
                        Location::At {
                            endpoint: Endpoint::Stock,
                            ..
                        }
                    )
                }) && (choice.disregard_claims
                    || self
                        .claims
                        .is_none_or(|index| index.allows(person.id, region, good)))
                {
                    self.view
                        .container(regional)
                        .map_or(0, |stock| stock.quantity(good))
                } else {
                    0
                };
                let (source, available) = if regional_quantity > 0 {
                    (regional, regional_quantity)
                } else if let Some(Location::At {
                    endpoint: Endpoint::Store { id },
                    ..
                }) = location
                {
                    let source = Place::Inventory(id);
                    let available = self.view.container(source)?.quantity(good);
                    if available == 0 {
                        return None;
                    }
                    (source, available)
                } else {
                    let start = store_stock.partition_point(|row| (row.0, row.1) < (region, good));
                    let end = store_stock.partition_point(|row| (row.0, row.1) <= (region, good));
                    if start == end {
                        return None;
                    }
                    let selected = start + (person.id % (end - start) as u64) as usize;
                    let (_, _, id, available) = store_stock[selected];
                    (Place::Inventory(id), available)
                };
                Some((person.id, region, good, needed.min(available), source))
            })
            .collect();
        rows.par_sort_unstable_by_key(|row| row.0);
        base.checked_add(rows.len() as u64)
            .ok_or("delivery acquisition action overflow")?;
        let actions = rows
            .par_iter()
            .enumerate()
            .map(|(index, &(person, _, good, quantity, source))| {
                let id = base + index as u64;
                Transaction {
                    id,
                    movements: vec![Movement {
                        item: None,
                        id,
                        from: source,
                        to: Some(Place::Person(person)),
                        good,
                        quantity,
                    }],
                    creations: vec![],
                }
            })
            .collect();
        let actors = rows.par_iter().map(|row| row.0).collect();
        let uses = rows
            .par_iter()
            .enumerate()
            .filter_map(|(index, &(person, region, good, quantity, source))| {
                matches!(source, Place::Stock { .. }).then_some(Use {
                    action: base + index as u64,
                    person,
                    region,
                    good,
                    quantity,
                    job: false,
                })
            })
            .collect();
        Ok(Plan {
            actions,
            actors,
            uses,
        })
    }
}
