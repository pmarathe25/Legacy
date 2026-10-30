//! Personal choices to deposit real food, with parallel store capacity arbitration.
use game_parallel::World;
use game_parallel_fused_data::{Block, NONE};
use rayon::prelude::*;

pub struct Draft {
    pub actors: Vec<u64>,
    deposited: Vec<u64>,
    new_claims: Vec<game_parallel::FoodStoreClaim>,
    additions: Vec<(u64, u32)>,
    meal: u32,
}
pub fn prepare(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    occupied: &[u64],
    tick: u64,
) -> Option<Draft> {
    let cargo = world
        .cargo
        .as_ref()
        .filter(|cargo| !cargo.food_depositors.is_empty())?;
    let mut busy = occupied.to_vec();
    busy.extend(
        world
            .conflict
            .as_ref()
            .into_iter()
            .flat_map(|owner| owner.actors()),
    );
    busy.extend(
        world
            .care
            .as_ref()
            .into_iter()
            .flat_map(|owner| owner.actors()),
    );
    busy.par_sort_unstable();
    let mut holdings: Vec<_> = cargo
        .holdings
        .par_iter()
        .map(|row| (row.person, row.food))
        .collect();
    if holdings.par_windows(2).any(|p| p[0].0 > p[1].0) {
        holdings.par_sort_unstable();
    }
    let mut stores: Vec<_> = cargo.food_stores.par_iter().collect();
    if stores.par_windows(2).any(|p| p[0].id > p[1].id) {
        stores.par_sort_unstable_by_key(|row| row.id);
    }
    let meal = world.rules.food_per_meal;
    let mut claims: Vec<_> = cargo
        .food_depositors
        .par_iter()
        .filter_map(|policy| {
            if !policy.enabled
                || !tick.is_multiple_of(policy.interval)
                || busy.binary_search(&policy.person).is_ok()
            {
                return None;
            }
            let index = positions
                .binary_search_by_key(&policy.person, |row| row.0)
                .ok()?;
            let (_, block, slot) = positions[index];
            let actor = &blocks[block];
            let store = stores
                .binary_search_by_key(&policy.store, |row| row.id)
                .ok()
                .map(|i| stores[i])?;
            let held = holdings
                .binary_search_by_key(&policy.person, |row| row.0)
                .ok()
                .map_or(0, |i| holdings[i].1);
            (actor.journey.is_none()
                && actor.health[slot] > 0
                && actor.work[slot] == NONE
                && actor
                    .fatigue
                    .as_ref()
                    .is_none_or(|f| f.flags[slot] & 2 == 0)
                && world.regions[actor.region].id == store.region
                && held
                    .checked_sub(meal)
                    .is_some_and(|left| left >= policy.retain))
            .then_some((policy.store, policy.person, policy.claim_after_deposit))
        })
        .collect();
    if claims.is_empty() {
        return None;
    }
    if claims.par_windows(2).any(|p| p[0] > p[1]) {
        claims.par_sort_unstable();
    }
    let ranks: Vec<_> = stores
        .par_iter()
        .map(|store| {
            let start = claims.partition_point(|row| row.0 < store.id);
            let end = claims.partition_point(|row| row.0 <= store.id);
            let len = end - start;
            let count = len.min(((store.capacity - store.food) / meal) as usize);
            let offset = if count > 0 && count < len {
                (game_parallel_genetics::random(world.seed ^ 0x6465706f736974, store.id, tick)
                    % len as u64) as usize
            } else {
                0
            };
            (start, len, count, offset)
        })
        .collect();
    let additions = stores
        .par_iter()
        .zip(ranks.par_iter())
        .filter_map(|(store, &(_, _, count, _))| {
            (count > 0).then_some((store.id, count as u32 * meal))
        })
        .collect();
    let granted: Vec<_> = claims
        .par_iter()
        .enumerate()
        .filter_map(|(i, &(store, person, claim))| {
            let slot = stores.binary_search_by_key(&store, |row| row.id).unwrap();
            let (start, len, count, offset) = ranks[slot];
            let rank = game_parallel_food_select::rotated_rank(i, start, len, offset);
            (rank < count).then_some((store, person, claim))
        })
        .collect();
    let new_claims = if granted.par_iter().any(|row| row.2.is_some()) {
        let mut existing: Vec<_> = cargo
            .food_store_claims
            .par_iter()
            .map(|row| (row.store, row.person))
            .chain(
                cargo
                    .food_stores
                    .par_iter()
                    .filter_map(|row| row.claimant.map(|person| (row.id, person))),
            )
            .collect();
        if existing.par_windows(2).any(|pair| pair[0] > pair[1]) {
            existing.par_sort_unstable();
        }
        granted
            .par_iter()
            .filter_map(|&(store, person, claim)| {
                let object_to_taking = claim?;
                existing.binary_search(&(store, person)).is_err().then_some(
                    game_parallel::FoodStoreClaim {
                        person,
                        store,
                        object_to_taking,
                    },
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut deposited: Vec<_> = granted.par_iter().map(|row| row.1).collect();
    if deposited.par_windows(2).any(|p| p[0] > p[1]) {
        deposited.par_sort_unstable();
    }
    let mut actors: Vec<_> = claims.par_iter().map(|row| row.1).collect();
    if actors.par_windows(2).any(|p| p[0] > p[1]) {
        actors.par_sort_unstable();
    }
    Some(Draft {
        actors,
        deposited,
        new_claims,
        additions,
        meal,
    })
}
impl Draft {
    pub fn commit(self, world: &mut World) {
        let cargo = world.cargo.as_mut().expect("prepared deposits");
        cargo.food_store_claims.par_extend(self.new_claims);
        cargo.food_stores.par_iter_mut().for_each(|store| {
            if let Ok(i) = self.additions.binary_search_by_key(&store.id, |row| row.0) {
                store.food += self.additions[i].1;
            }
        });
        cargo.holdings = std::mem::take(&mut cargo.holdings)
            .into_par_iter()
            .filter_map(|mut holding| {
                if self.deposited.binary_search(&holding.person).is_ok() {
                    holding.food -= self.meal;
                }
                (holding.food > 0).then_some(holding)
            })
            .collect();
    }
}

#[macro_export]
macro_rules! deposit_methods {
    () => {
        /// Change this person's tendency to assert a claim after depositing.
        pub fn set_deposit_claim(
            &mut self,
            person: u64,
            preference: Option<bool>,
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
                return Err("person is dead".into());
            }
            let policy = self
                .template
                .cargo
                .as_mut()
                .and_then(|cargo| {
                    cargo
                        .food_depositors
                        .iter_mut()
                        .find(|row| row.person == person)
                })
                .ok_or("no food deposit preference")?;
            policy.claim_after_deposit = preference;
            self.revision = revision;
            Ok(())
        }
        pub fn set_food_deposit(&mut self, person: u64, enabled: bool) -> Result<(), String> {
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
                return Err("person is dead".into());
            }
            let policy = self
                .template
                .cargo
                .as_mut()
                .and_then(|cargo| {
                    cargo
                        .food_depositors
                        .iter_mut()
                        .find(|policy| policy.person == person)
                })
                .ok_or("no food deposit preference")?;
            policy.enabled = enabled;
            self.revision = revision;
            Ok(())
        }
    };
}
