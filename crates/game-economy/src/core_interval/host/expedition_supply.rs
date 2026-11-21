//! A traveler may seek supplies for a delivery objective.
use super::*;
use game_economy_expeditions::elapsed::{Event, Itinerary, SupplyAcquisition, SupplyNeed};
use game_economy_local_transport::{
    Endpoint,
    walking::{Location, Request},
};
use game_lots::{Movement, Place, Transaction};

pub(super) struct SupplyWalk {
    pub request: Request,
    pub first_attempt: Option<(u64, u64)>,
}

impl CoreHost {
    pub(super) fn has_expedition_supply_walks(&self) -> bool {
        self.expeditions.as_ref().is_some_and(|owner| {
            owner.itineraries.par_iter().any(|row| {
                row.supply_need()
                    .is_some_and(|need| need.choice.known_store.is_some())
            })
        })
    }

    fn expedition_supply_choices<T: Send>(
        &self,
        blocked: &[u64],
        choose: impl Fn(&Itinerary, SupplyNeed, u64, u64, bool) -> Option<T> + Sync,
    ) -> Vec<T> {
        let Some(owner) = &self.expeditions else {
            return Vec::new();
        };
        if !owner
            .itineraries
            .par_iter()
            .any(|row| row.supply_need().is_some())
        {
            return Vec::new();
        }
        let opinions = self.opinions.as_ref().map(|owner| owner.index());
        let tick = self.core.physical.tick();
        owner
            .itineraries
            .par_iter()
            .filter_map(|row| {
                let need = row.supply_need()?;
                if blocked.binary_search(&row.person).is_ok() || self.held_at(row.person).is_some()
                {
                    return None;
                }
                let slot = self
                    .core
                    .people
                    .binary_search_by_key(&row.person, |actor| actor.id)
                    .ok()?;
                let actor = &self.core.people[slot];
                if actor.controlled || actor.health == 0 {
                    return None;
                }
                let region = actor.region?;
                let cargo = self.core.physical.custody(Place::Person(row.person))?;
                let desired = need
                    .desired
                    .saturating_sub(self.available_supply(cargo, need.good, tick));
                let amount = desired.min(cargo.capacity.saturating_sub(cargo.used));
                let motivated = need.choice.willing
                    || need
                        .choice
                        .pressure_threshold
                        .zip(opinions.as_ref())
                        .is_some_and(|(minimum, opinions)| {
                            let (fear, obligation) = opinions.feelings(row.person, need.recipient);
                            fear.saturating_add(obligation) >= minimum
                        });
                choose(row, need, region, amount, motivated)
            })
            .collect()
    }

    fn available_supply(&self, cargo: &game_lots::Container, good: u32, tick: u64) -> u64 {
        let instant = self.core.physical.lot_instant(tick);
        cargo
            .lots
            .iter()
            .filter(|lot| {
                lot.good == good && lot.expires_at.is_none_or(|deadline| deadline > instant)
            })
            .map(|lot| lot.quantity)
            .sum()
    }

    pub(super) fn expedition_supply_walks(&self, blocked: &[u64]) -> Vec<SupplyWalk> {
        let Some(owner) = &self.local_transport else {
            return Vec::new();
        };
        let Some(walking) = &owner.walking else {
            return Vec::new();
        };
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let tick = self.core.physical.tick();
        self.expedition_supply_choices(blocked, |row, need, region, amount, motivated| {
            let store = need.choice.known_store?;
            let location = walking.location(row.person, region);
            if owner
                .stores
                .binary_search_by_key(&store, |row| row.id)
                .ok()
                .is_none_or(|index| owner.stores[index].region != region)
            {
                return None;
            }
            let endpoint = Endpoint::Store { id: store };
            let seek = amount > 0 && motivated;
            let (destination, first_attempt) = match location {
                Location::Walking {
                    from,
                    to: Endpoint::Stock,
                    ..
                } if from == endpoint => (Endpoint::Stock, None),
                Location::Walking { to, .. } if to == endpoint && seek => (endpoint, None),
                Location::At { endpoint: here, .. } if here == endpoint => {
                    let available = self
                        .core
                        .physical
                        .custody(Place::Inventory(store))
                        .is_some_and(|cargo| self.available_supply(cargo, need.good, tick) != 0);
                    if seek && available {
                        return None;
                    }
                    (Endpoint::Stock, None)
                }
                Location::At {
                    endpoint: Endpoint::Stock,
                    ..
                } if seek && row.visited_supply_store != Some(store) => {
                    let can_take_stock = need.choice.disregard_claims
                        || claims
                            .as_ref()
                            .is_none_or(|claims| claims.allows(row.person, region, need.good));
                    if can_take_stock
                        && self
                            .core
                            .physical
                            .custody(Place::Stock {
                                region,
                                good: need.good,
                            })
                            .is_some_and(|cargo| self.available_supply(cargo, need.good, tick) != 0)
                    {
                        return None;
                    }
                    (endpoint, Some((row.id, store)))
                }
                _ => return None,
            };
            Some(SupplyWalk {
                request: Request {
                    person: row.person,
                    region,
                    destination,
                },
                first_attempt,
            })
        })
    }

    pub(super) fn acquire_expedition_supplies(
        &mut self,
        blocked: &[u64],
    ) -> Result<(Vec<u64>, Vec<Event>), String> {
        if self.expeditions.as_ref().is_none_or(|owner| {
            !owner
                .itineraries
                .par_iter()
                .any(|row| row.supply_need().is_some())
        }) {
            return Ok((Vec::new(), Vec::new()));
        }
        let blocked = self.local_walk_exclusions(blocked);
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let tick = self.core.physical.tick();
        let mut picks: Vec<_> =
            self.expedition_supply_choices(&blocked, |row, need, region, amount, motivated| {
                if amount == 0 || !motivated {
                    return None;
                }
                let from = match self.local_location(row.person) {
                    Some(Location::At {
                        endpoint: Endpoint::Store { id },
                        ..
                    }) if need.choice.known_store == Some(id) => Place::Inventory(id),
                    _ if self.regional_stock_reachable(row.person, region)
                        && (need.choice.disregard_claims
                            || claims.as_ref().is_none_or(|claims| {
                                claims.allows(row.person, region, need.good)
                            })) =>
                    {
                        Place::Stock {
                            region,
                            good: need.good,
                        }
                    }
                    _ => return None,
                };
                let stock = self.core.physical.custody(from)?;
                let quantity = self.available_supply(stock, need.good, tick).min(amount);
                (quantity != 0).then_some((
                    row.person,
                    SupplyAcquisition {
                        itinerary: row.id,
                        region,
                        good: need.good,
                        quantity,
                        from,
                    },
                ))
            });
        if picks.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        picks.par_sort_unstable_by_key(|(person, _)| *person);
        let actions: Vec<_> = picks
            .par_iter()
            .enumerate()
            .map(|(index, &(person, row))| {
                let id = index as u64 + 1;
                Transaction {
                    id,
                    movements: vec![Movement {
                        id,
                        from: row.from,
                        to: Some(Place::Person(person)),
                        good: row.good,
                        quantity: row.quantity,
                        batch: None,
                        item: None,
                    }],
                    creations: Vec::new(),
                }
            })
            .collect();
        let uses: Vec<_> = picks
            .par_iter()
            .enumerate()
            .filter_map(|(index, &(person, row))| {
                matches!(row.from, Place::Stock { .. }).then_some(game_economy_claims::Use {
                    action: index as u64 + 1,
                    person,
                    region: row.region,
                    good: row.good,
                    quantity: row.quantity,
                    job: false,
                })
            })
            .collect();
        let draft = self
            .core
            .physical
            .prepare_interval_transfer(&actions, self.core.seed ^ self.core.decision_time_key())?;
        let paid: Vec<_> = draft
            .transactions()
            .par_iter()
            .filter_map(|receipt| receipt.accepted.then(|| picks[(receipt.id - 1) as usize].1))
            .collect();
        let tools = self.stage_tools(&[], None, draft.item_updates())?;
        let writing = self.stage_written_page_removals(draft.item_updates());
        let claims = self
            .claims
            .as_ref()
            .filter(|_| !uses.is_empty())
            .map(|owner| {
                let mut owner = owner.clone();
                let mut opinions = self.opinions.clone();
                game_economy_claims::publish_transactions(
                    &mut owner,
                    &uses,
                    draft.transactions(),
                    &self.core.people,
                    &mut opinions,
                    tick,
                    Some(&|person, region| self.regional_stock_reachable(person, region)),
                );
                (owner, opinions)
            });
        self.core.physical.commit_interval_consumption(draft)?;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(writing) = writing {
            self.writing = Some(writing);
        }
        if let Some((claims, opinions)) = claims {
            self.claims = Some(claims);
            self.opinions = opinions;
        }
        self.work_checked_revision = None;
        let events = self
            .expeditions
            .as_ref()
            .expect("supply itineraries")
            .supply_events(&paid, self.core.simulated_millis());
        Ok((
            picks.into_iter().map(|(person, _)| person).collect(),
            events,
        ))
    }
}
