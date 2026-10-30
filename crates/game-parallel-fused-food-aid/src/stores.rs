//! Physically contend for separate stores without treating a claim as a lock.
use game_parallel::World;
use game_parallel_fused_data::Block;
use rayon::prelude::*;

pub struct Draft {
    pub actors: Vec<u64>,
    pub acquired: Vec<u64>,
    debits: Vec<(u64, u32)>,
    witnessed: Vec<(u64, u64)>,
    reconsider: Vec<u64>,
}
pub fn prepare(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    seekers: &[u64],
) -> Option<Draft> {
    let cargo = world.cargo.as_ref()?.food_store_choices.as_slice();
    if cargo.is_empty() || seekers.is_empty() {
        return None;
    }
    let stores = &world.cargo.as_ref()?.food_stores;
    let mut store_index: Vec<_> = stores.par_iter().collect();
    if store_index.par_windows(2).any(|p| p[0].id > p[1].id) {
        store_index.par_sort_unstable_by_key(|store| store.id);
    }
    let dense_seekers = seekers.first().and_then(|first| {
        seekers.last().and_then(|last| {
            (last.checked_sub(*first) == Some((seekers.len() - 1) as u64)).then_some(*first)
        })
    });
    let dense_positions = positions.first().and_then(|first| {
        positions.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64)).then_some(first.0)
        })
    });
    let mut claims: Vec<_> = cargo
        .par_iter()
        .filter_map(|choice| {
            if !dense_seekers.is_some_and(|first| {
                choice.person >= first && choice.person - first < seekers.len() as u64
            }) && (dense_seekers.is_some() || seekers.binary_search(&choice.person).is_err())
            {
                return None;
            }
            let store = store_index
                .binary_search_by_key(&choice.store, |row| row.id)
                .ok()
                .map(|i| store_index[i])?;
            let index = dense_positions
                .and_then(|first| choice.person.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&i| i < positions.len())
                .or_else(|| {
                    if dense_positions.is_none() {
                        positions
                            .binary_search_by_key(&choice.person, |row| row.0)
                            .ok()
                    } else {
                        None
                    }
                })?;
            let (_, block, _) = positions[index];
            (blocks[block].journey.is_none()
                && world.regions[blocks[block].region].id == store.region)
                .then_some((choice.store, choice.person))
        })
        .collect();
    if claims.is_empty() {
        return None;
    }
    if claims.par_windows(2).any(|p| p[0] > p[1]) {
        claims.par_sort_unstable();
    }
    let meal = world.rules.food_per_meal;
    let ranks: Vec<_> = store_index
        .par_iter()
        .map(|store| {
            let start = claims.partition_point(|row| row.0 < store.id);
            let end = claims.partition_point(|row| row.0 <= store.id);
            let len = end - start;
            let count = len.min((store.food / meal) as usize);
            let offset = if count > 0 && count < len {
                (game_parallel_genetics::random(world.seed, store.id, world.tick) % len as u64)
                    as usize
            } else {
                0
            };
            (start, len, count, offset)
        })
        .collect();
    let debits: Vec<_> = store_index
        .par_iter()
        .zip(ranks.par_iter())
        .filter_map(|(store, &(_, _, count, _))| {
            (count > 0).then_some((store.id, (count as u32) * meal))
        })
        .collect();
    let granted: Vec<_> = claims
        .par_iter()
        .enumerate()
        .filter_map(|(index, &(id, person))| {
            let slot = store_index.binary_search_by_key(&id, |row| row.id).unwrap();
            let (first, len, count, offset) = ranks[slot];
            let rank = game_parallel_food_select::rotated_rank(index, first, len, offset);
            (rank < count).then_some((store_index[slot], person))
        })
        .collect();
    let witnessed = if world.opinions.is_some() {
        let witnesses =
            game_parallel_store_witnesses::StoreWitnesses::new(world.cargo.as_ref().unwrap());
        granted
            .par_iter()
            .flat_map_iter(|(store, person)| {
                witnesses
                    .candidates(store.id, *person, world.tick)
                    .filter_map(|owner| {
                        if owner == *person {
                            return None;
                        }
                        let index = positions.binary_search_by_key(&owner, |row| row.0).ok()?;
                        let (_, block, offset) = positions[index];
                        (blocks[block].journey.is_none()
                            && blocks[block].health[offset] > 0
                            && world.regions[blocks[block].region].id == store.region)
                            .then_some((owner, *person))
                    })
            })
            .collect()
    } else {
        Vec::new()
    };
    let witnessed = game_parallel_store_witnesses::limit_attention(witnessed, world.tick);
    let mut actors: Vec<_> = claims.par_iter().map(|row| row.1).collect();
    if actors.par_windows(2).any(|p| p[0] > p[1]) {
        actors.par_sort_unstable();
    }
    let mut acquired: Vec<_> = granted.par_iter().map(|row| row.1).collect();
    if acquired.par_windows(2).any(|p| p[0] > p[1]) {
        acquired.par_sort_unstable();
    }
    let mut reconsider: Vec<_> = cargo
        .par_iter()
        .filter(|choice| {
            choice.reconsider_on_failure
                && actors.binary_search(&choice.person).is_ok()
                && acquired.binary_search(&choice.person).is_err()
        })
        .map(|choice| choice.person)
        .collect();
    if reconsider.par_windows(2).any(|pair| pair[0] > pair[1]) {
        reconsider.par_sort_unstable();
    }
    Some(Draft {
        reconsider,
        actors,
        acquired,
        debits,
        witnessed,
    })
}
impl Draft {
    pub fn commit(self, world: &mut World, tick: u64) {
        let cargo = world.cargo.as_mut().expect("prepared stores");
        if !self.reconsider.is_empty() {
            cargo.food_store_choices = std::mem::take(&mut cargo.food_store_choices)
                .into_par_iter()
                .filter(|choice| self.reconsider.binary_search(&choice.person).is_err())
                .collect();
        }
        cargo.food_stores.par_iter_mut().for_each(|store| {
            if let Ok(i) = self.debits.binary_search_by_key(&store.id, |row| row.0) {
                store.food -= self.debits[i].1;
            }
        });
        if let Some(opinions) = &mut world.opinions {
            opinions.witness_thefts(&self.witnessed, tick);
        }
    }
}
