//! Individual material-backed store construction, staged without shared writes.
use game_parallel::{FoodStore, World};
use game_parallel_fused_data::{Block, NONE};
use rayon::prelude::*;

pub struct Draft {
    pub actors: Vec<u64>,
    material: Vec<(u64, u32, u64)>,
    stores: Vec<FoodStore>,
    preferences: Vec<(u64, u64, bool, bool, u64)>,
}

pub fn prepare(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    occupied: &[u64],
    tick: u64,
) -> Result<Option<Draft>, &'static str> {
    let Some(cargo) = world
        .cargo
        .as_ref()
        .filter(|cargo| !cargo.food_store_builders.is_empty())
    else {
        return Ok(None);
    };
    let Some(goods) = world.goods_cargo.as_ref() else {
        return Ok(None);
    };
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
    if busy.par_windows(2).any(|pair| pair[0] > pair[1]) {
        busy.par_sort_unstable();
    }
    let mut holdings: Vec<_> = goods.holdings.par_iter().collect();
    if holdings
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        holdings.par_sort_unstable_by_key(|row| row.person);
    }
    let mut blueprints: Vec<_> = cargo.food_store_blueprints.par_iter().collect();
    if blueprints
        .par_windows(2)
        .any(|pair| pair[0].id > pair[1].id)
    {
        blueprints.par_sort_unstable_by_key(|row| row.id);
    }
    let mut candidates: Vec<_> = cargo
        .food_store_builders
        .par_iter()
        .filter_map(|policy| {
            if !policy.enabled
                || policy.remaining == 0
                || !tick.is_multiple_of(policy.interval)
                || busy.binary_search(&policy.person).is_ok()
            {
                return None;
            }
            let (_, block, slot) = positions[positions
                .binary_search_by_key(&policy.person, |row| row.0)
                .ok()?];
            let actor = &blocks[block];
            if actor.journey.is_some()
                || actor.health[slot] == 0
                || actor.work[slot] != NONE
                || actor
                    .fatigue
                    .as_ref()
                    .is_some_and(|f| f.flags[slot] & 2 != 0)
            {
                return None;
            }
            let blueprint = blueprints[blueprints
                .binary_search_by_key(&policy.blueprint, |row| row.id)
                .ok()?];
            let holder = holdings[holdings
                .binary_search_by_key(&policy.person, |row| row.person)
                .ok()?];
            if !holder
                .items
                .iter()
                .any(|item| item.good == blueprint.good && item.quantity >= blueprint.quantity)
            {
                return None;
            }
            Some((
                policy.person,
                world.regions[actor.region].id,
                blueprint.good,
                blueprint.quantity,
                blueprint.capacity,
                policy.object_to_taking,
                policy.use_after_build,
                policy.deposit_after_build,
                policy.interval,
            ))
        })
        .collect();
    if candidates.is_empty() {
        return Ok(None);
    }
    if candidates.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        candidates.par_sort_unstable_by_key(|row| row.0);
    }
    let max = cargo
        .food_stores
        .par_iter()
        .map(|store| store.id)
        .max()
        .unwrap_or(0);
    let first = max.checked_add(1).ok_or("food store id overflow")?;
    first
        .checked_add((candidates.len() - 1) as u64)
        .ok_or("food store id overflow")?;
    let stores = candidates
        .par_iter()
        .enumerate()
        .map(
            |(index, &(person, region, _, _, capacity, object_to_taking, _, _, _))| FoodStore {
                id: first + index as u64,
                region,
                food: 0,
                capacity,
                claimant: Some(person),
                object_to_taking,
            },
        )
        .collect();
    let preferences = candidates
        .par_iter()
        .enumerate()
        .map(|(index, row)| (row.0, first + index as u64, row.6, row.7, row.8))
        .collect();
    let actors = candidates.par_iter().map(|row| row.0).collect();
    let material = candidates
        .par_iter()
        .map(|row| (row.0, row.2, row.3))
        .collect();
    Ok(Some(Draft {
        actors,
        material,
        stores,
        preferences,
    }))
}

/// Merge disjoint action reservations into sorted actor order.
pub fn add_actors(actors: &mut Vec<u64>, additional: &[u64]) {
    actors.extend_from_slice(additional);
    if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
        actors.par_sort_unstable();
    }
}

/// Prior actor reservations, including food actions selected for this tick.
pub type Reservations<'a> = (
    &'a [u64],
    &'a [u64],
    &'a mut Vec<u64>,
    &'a [u64],
    Option<&'a [u64]>,
);
/// Reserve builders after prior decisions and food seeking have chosen their actors.
pub fn prepare_reserved(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    reservations: Reservations<'_>,
    tick: u64,
) -> Result<Option<Draft>, &'static str> {
    if world
        .cargo
        .as_ref()
        .is_none_or(|cargo| cargo.food_store_builders.is_empty())
    {
        return Ok(None);
    }
    let (planned, restricted, food_actors, eaters, seekers) = reservations;
    let mut busy = planned.to_vec();
    busy.extend_from_slice(restricted);
    busy.extend_from_slice(food_actors);
    busy.extend_from_slice(eaters);
    if let Some(seekers) = seekers {
        busy.extend_from_slice(seekers);
    }
    if busy.par_windows(2).any(|pair| pair[0] > pair[1]) {
        busy.par_sort_unstable();
    }
    let builds = prepare(world, blocks, positions, &busy, tick)?;
    if let Some(builds) = &builds {
        food_actors.extend_from_slice(&builds.actors);
        if food_actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            food_actors.par_sort_unstable();
        }
    }
    Ok(builds)
}

impl Draft {
    /// Commit after trade snapshots have been published; their actors are disjoint.
    pub fn commit(self, world: &mut World) {
        let goods = world
            .goods_cargo
            .as_mut()
            .expect("prepared construction goods");
        goods.holdings.par_iter_mut().for_each(|holding| {
            if let Ok(index) = self
                .material
                .binary_search_by_key(&holding.person, |row| row.0)
            {
                let (_, good, quantity) = self.material[index];
                let item = holding
                    .items
                    .iter_mut()
                    .find(|item| item.good == good)
                    .expect("reserved material");
                item.quantity -= quantity;
                holding.items.retain(|item| item.quantity > 0);
            }
        });
        goods.holdings = std::mem::take(&mut goods.holdings)
            .into_par_iter()
            .filter(|holding| !holding.items.is_empty())
            .collect();
        let cargo = world.cargo.as_mut().expect("prepared construction cargo");
        cargo.food_store_builders.par_iter_mut().for_each(|policy| {
            if self.actors.binary_search(&policy.person).is_ok() {
                policy.remaining -= 1;
            }
        });
        let mut source_people: Vec<_> = cargo
            .food_store_choices
            .par_iter()
            .map(|row| row.person)
            .collect();
        source_people.par_sort_unstable();
        cargo.food_store_choices.par_iter_mut().for_each(|choice| {
            if let Ok(index) = self
                .preferences
                .binary_search_by_key(&choice.person, |row| row.0)
            {
                let (_, store, use_store, _, _) = self.preferences[index];
                if use_store {
                    choice.store = store;
                    choice.reconsider_on_failure = true;
                }
            }
        });
        cargo
            .food_store_choices
            .par_extend(self.preferences.par_iter().filter_map(
                |&(person, store, use_store, _, _)| {
                    (use_store && source_people.binary_search(&person).is_err()).then_some(
                        game_parallel::FoodStoreChoice {
                            person,
                            store,
                            reconsider_on_failure: true,
                        },
                    )
                },
            ));
        let mut deposit_people: Vec<_> = cargo
            .food_depositors
            .par_iter()
            .map(|row| row.person)
            .collect();
        deposit_people.par_sort_unstable();
        cargo.food_depositors.par_iter_mut().for_each(|policy| {
            if let Ok(index) = self
                .preferences
                .binary_search_by_key(&policy.person, |row| row.0)
            {
                let (_, store, _, deposit, _) = self.preferences[index];
                if deposit {
                    policy.store = store;
                }
            }
        });
        cargo
            .food_depositors
            .par_extend(self.preferences.par_iter().filter_map(
                |&(person, store, _, deposit, interval)| {
                    (deposit && deposit_people.binary_search(&person).is_err()).then_some(
                        game_parallel::FoodDepositPolicy {
                            person,
                            store,
                            retain: 0,
                            claim_after_deposit: None,
                            interval,
                            enabled: true,
                        },
                    )
                },
            ));
        cargo.food_stores.par_extend(self.stores);
    }
}

/// Newborns may acquire a parent's personal tendency, not their store or claim.
pub fn inherit_birth_preferences(
    cargo: &mut game_parallel::Cargo,
    births: &[(u64, [u64; 2], usize)],
) {
    cargo.inherit_sharing(births);
    if !cargo.inherit_store_building || births.is_empty() {
        return;
    }
    let mut parents: Vec<_> = cargo.food_store_builders.par_iter().collect();
    if parents
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        parents.par_sort_unstable_by_key(|row| row.person);
    }
    let children: Vec<_> = births
        .par_iter()
        .filter_map(|&(person, ids, count)| {
            if count == 0 {
                return None;
            }
            let parent = ids[(person % count as u64) as usize];
            let index = parents
                .binary_search_by_key(&parent, |row| row.person)
                .ok()?;
            let mut policy = parents[index].clone();
            policy.person = person;
            policy.remaining = 1;
            Some(policy)
        })
        .collect();
    cargo.food_store_builders.par_extend(children);
}

#[macro_export]
macro_rules! builder_methods {
    () => {
        /// Change only this person's construction intention.
        pub fn set_store_building(&mut self, person: u64, enabled: bool) -> Result<(), String> {
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
                        .food_store_builders
                        .iter_mut()
                        .find(|row| row.person == person)
                })
                .ok_or("no store construction intention")?;
            policy.enabled = enabled;
            self.revision = revision;
            Ok(())
        }
    };
}
