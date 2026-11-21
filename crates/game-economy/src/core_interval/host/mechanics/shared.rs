//! Mechanisms share a physical location when all their parts occupy it together.
use super::*;
use game_economy_local_transport::{Endpoint, walking::Location};
use game_lots::{Movement, Place, Transaction};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreSharedApparatus {
    pub store: u64,
    pub apparatus: game_mechanics::Apparatus,
}

impl CoreHost {
    pub(super) fn mechanical_store_reachable(&self, person: u64, store: u64) -> bool {
        let Some(person_row) = self.core.person(person) else {
            return false;
        };
        let Some(region) = person_row.region.filter(|_| person_row.health > 0) else {
            return false;
        };
        if self.held_at(person).is_some() || self.has_tactical_engagement(person) {
            return false;
        }
        let known = self.local_transport.as_ref().is_some_and(|owner| {
            indexed(&owner.stores, store, |row| row.id).is_some_and(|row| row.region == region)
        });
        known
            && self.walking().is_none_or(|owner| {
                owner.location(person, region)
                    == Location::At {
                        region,
                        endpoint: Endpoint::Store { id: store },
                    }
            })
    }

    pub(super) fn mechanical_holder(
        &self,
        person: u64,
        apparatus: &game_mechanics::Apparatus,
    ) -> Option<Place> {
        let place = self.core.physical.item_place(apparatus.parts.first()?.id)?;
        match place {
            Place::Person(holder) if holder == person => Some(place),
            Place::Inventory(store) if self.mechanical_store_reachable(person, store) => {
                Some(place)
            }
            _ => None,
        }
    }

    pub(in crate::core_interval::host) fn shared_apparatus(
        &self,
        person: u64,
    ) -> Vec<CoreSharedApparatus> {
        let Some(owner) = &self.mechanics else {
            return Vec::new();
        };
        let now = self.core.physical.lot_instant(self.core.physical.tick());
        owner
            .world
            .apparatus()
            .par_iter()
            .filter_map(|apparatus| {
                let place = self.mechanical_holder(person, apparatus)?;
                let Place::Inventory(store) = place else {
                    return None;
                };
                parts_present(
                    self.core.physical.custody(place),
                    apparatus,
                    &owner.materials,
                    now,
                )
                .then(|| CoreSharedApparatus {
                    store,
                    apparatus: apparatus.clone(),
                })
            })
            .collect()
    }

    /// Move all parts together so that the apparatus remains usable in its new custody.
    pub fn move_controlled_apparatus(
        &mut self,
        person: u64,
        apparatus: u64,
        store: u64,
        put_down: bool,
    ) -> Result<bool, String> {
        self.controlled_person(person)?;
        if !self.mechanical_store_reachable(person, store) {
            return Err("the apparatus store is outside physical reach".into());
        }
        let busy = self.choice_busy(&[]);
        if busy.binary_search(&person).is_ok()
            || self.assisting_people().binary_search(&person).is_ok()
        {
            return Err("the controlled person is occupied".into());
        }
        if self.core.next_deadline(None)?.after_millis == 0 {
            return Err("settle the current event before moving an apparatus".into());
        }
        let owner = self.mechanics.as_ref().ok_or("mechanics is disabled")?;
        let machine =
            indexed(owner.world.apparatus(), apparatus, |row| row.id).ok_or("unknown apparatus")?;
        let cargo = Place::Person(person);
        let storage = Place::Inventory(store);
        let (from, to) = if put_down {
            (cargo, storage)
        } else {
            (storage, cargo)
        };
        let now = self.core.physical.lot_instant(self.core.physical.tick());
        if !parts_present(
            self.core.physical.custody(from),
            machine,
            &owner.materials,
            now,
        ) {
            return Err("the matching parts are not together at the source".into());
        }
        let moves = machine
            .parts
            .par_iter()
            .enumerate()
            .map(|(slot, part)| {
                let material = owner
                    .materials
                    .binary_search_by_key(&part.component, |row| row.component)
                    .expect("validated mechanical component");
                Movement {
                    id: slot as u64 + 1,
                    from,
                    to: Some(to),
                    good: owner.materials[material].good,
                    quantity: 1,
                    item: Some(part.id),
                    batch: None,
                }
            })
            .collect();
        let transfer = Transaction {
            id: 1,
            movements: moves,
            creations: Vec::new(),
        };
        let seed = self.core.seed ^ self.core.decision_time_key();
        let draft = self
            .core
            .physical
            .prepare_interval_transfer(&[transfer], seed)?;
        let moved = draft.transactions().first().is_some_and(|row| row.accepted);
        if moved {
            self.core.physical.commit_interval_consumption(draft)?;
        }
        Ok(moved)
    }
}
