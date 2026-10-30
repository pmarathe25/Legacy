//! Bounded attention to personal claims at a physical store.
use rayon::prelude::*;
/// Sorted candidate index. At most four claimants attend any one taking.
pub struct StoreWitnesses(Vec<(u64, u64)>);
impl StoreWitnesses {
    pub fn new(cargo: &game_parallel_cargo::Cargo) -> Self {
        let mut rows: Vec<_> = cargo
            .food_stores
            .par_iter()
            .filter_map(|store| {
                store
                    .claimant
                    .filter(|_| store.object_to_taking)
                    .map(|person| (store.id, person))
            })
            .chain(cargo.food_store_claims.par_iter().filter_map(|claim| {
                claim
                    .object_to_taking
                    .then_some((claim.store, claim.person))
            }))
            .collect();
        if rows.par_windows(2).any(|pair| pair[0] > pair[1]) {
            rows.par_sort_unstable();
        }
        Self(rows)
    }

    /// Rotate attention by incident and tick, avoiding work proportional to all claims per taking.
    pub fn candidates(&self, store: u64, taker: u64, tick: u64) -> impl Iterator<Item = u64> + '_ {
        let start = self.0.partition_point(|row| row.0 < store);
        let end = self.0.partition_point(|row| row.0 <= store);
        let count = end - start;
        let offset = if count == 0 {
            0
        } else {
            (taker.wrapping_mul(0x9e3779b97f4a7c15).wrapping_add(tick) % count as u64) as usize
        };
        (0..count.min(4)).map(move |i| self.0[start + (offset + i) % count].1)
    }
}

/// Each observer attends to at most 32 takings per tick, with rotation between ticks.
pub fn limit_attention(mut incidents: Vec<(u64, u64)>, tick: u64) -> Vec<(u64, u64)> {
    if incidents.par_windows(2).any(|pair| pair[0] > pair[1]) {
        incidents.par_sort_unstable();
    }
    incidents
        .par_chunk_by(|a, b| a.0 == b.0)
        .flat_map_iter(|group| {
            let start = (tick % group.len() as u64) as usize;
            (0..group.len().min(32)).map(move |offset| group[(start + offset) % group.len()])
        })
        .collect()
}

/// Individual claim controls for a host with compiled person positions.
#[macro_export]
macro_rules! claim_methods {
    () => {
        /// Assert or withdraw a personal claim. Other people remain free to act.
        pub fn set_food_store_claim(
            &mut self,
            person: u64,
            store: u64,
            claim: bool,
        ) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let index = self
                .positions
                .binary_search_by_key(&person, |row| row.0)
                .map_err(|_| "unknown person")?;
            let (_, block, slot) = self.positions[index];
            let actor = &self.blocks[block];
            if actor.health[slot] == 0 {
                return Err("person must be alive".into());
            }
            let cargo = self
                .template
                .cargo
                .as_mut()
                .ok_or("personal cargo is disabled")?;
            let target = cargo
                .food_stores
                .iter_mut()
                .find(|row| row.id == store)
                .ok_or("unknown food store")?;
            if claim
                && (actor.journey.is_some()
                    || self.template.regions[actor.region].id != target.region)
            {
                return Err("store must be local to assert a claim".into());
            }
            if target.claimant == Some(person) {
                if !claim {
                    target.claimant = None;
                    target.object_to_taking = false;
                }
            } else if let Some(i) = cargo
                .food_store_claims
                .iter()
                .position(|row| row.store == store && row.person == person)
            {
                if !claim {
                    cargo.food_store_claims.remove(i);
                }
            } else if claim {
                cargo.food_store_claims.push(game_parallel::FoodStoreClaim {
                    person,
                    store,
                    object_to_taking: false,
                });
            } else {
                return Err("person has no claim".into());
            }
            self.revision = revision;
            Ok(())
        }
        /// Change only this person's response to future witnessed takings.
        pub fn set_store_objection(
            &mut self,
            person: u64,
            store: u64,
            object: bool,
        ) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let index = self
                .positions
                .binary_search_by_key(&person, |row| row.0)
                .map_err(|_| "unknown person")?;
            let (_, block, slot) = self.positions[index];
            if self.blocks[block].health[slot] == 0 {
                return Err("person must be alive".into());
            }
            let cargo = self
                .template
                .cargo
                .as_mut()
                .ok_or("personal cargo is disabled")?;
            if let Some(primary) = cargo
                .food_stores
                .iter_mut()
                .find(|row| row.id == store && row.claimant == Some(person))
            {
                primary.object_to_taking = object;
            } else if let Some(claim) = cargo
                .food_store_claims
                .iter_mut()
                .find(|row| row.store == store && row.person == person)
            {
                claim.object_to_taking = object;
            } else {
                return Err("person has no claim".into());
            }
            self.revision = revision;
            Ok(())
        }
    };
}
