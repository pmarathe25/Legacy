//! Matching and conception over private lifecycle columns and immutable pedigree.
use game_parallel_families::FamilyRules;
use game_parallel_family_columns::{State, Store};
use game_parallel_family_model::{LifeInput, PhysicalView};
use game_parallel_genetics::{Sex, random};
use rayon::prelude::*;
pub type Conception = (u64, (u64, u64));
pub fn eligible(state: State, person: LifeInput, rules: &FamilyRules) -> bool {
    state.alive
        && state.reproduction_allowed
        && state.age >= rules.adult_age
        && state.age < rules.fertile_until
        && person.region.is_some()
        && person.health >= rules.minimum_health
        && person.hunger <= rules.maximum_hunger
}
pub fn partnerships(
    store: &Store,
    candidates: &[usize],
    physical: &impl PhysicalView,
    rules: &FamilyRules,
    seed: u64,
    tick: u64,
) -> Vec<(u64, u64, u64)> {
    let mut local: Vec<_> = candidates
        .par_iter()
        .filter_map(|&slot| {
            let row = &store.metadata[slot];
            let state = store.state(slot, true);
            let person = physical.read(row.id);
            (state.partner.is_none() && eligible(state, person, rules))
                .then(|| (person.region.unwrap(), slot))
        })
        .collect();
    local.par_sort_unstable_by_key(|&(region, slot)| (region, store.metadata[slot].id));
    local
        .par_chunk_by(|a, b| a.0 == b.0)
        .flat_map(|group| {
            let area = group[0].0;
            let mut females: Vec<_> = group
                .par_iter()
                .map(|&(_, slot)| slot)
                .filter(|&slot| store.metadata[slot].genome.sex == Sex::Female)
                .collect();
            let mut males: Vec<_> = group
                .par_iter()
                .map(|&(_, slot)| slot)
                .filter(|&slot| store.metadata[slot].genome.sex == Sex::Male)
                .collect();
            if females.is_empty() || males.is_empty() {
                return Vec::new();
            }
            females.par_sort_unstable_by_key(|&slot| {
                (
                    random(seed, store.metadata[slot].id, tick),
                    store.metadata[slot].id,
                )
            });
            males.par_sort_unstable_by_key(|&slot| {
                (
                    random(seed ^ 31, store.metadata[slot].id, tick),
                    store.metadata[slot].id,
                )
            });
            let proposed: Vec<_> = females
                .par_iter()
                .filter_map(|&slot| {
                    if males.is_empty() {
                        return None;
                    }
                    let mother = &store.metadata[slot];
                    let offset = (random(seed ^ 71, mother.id, tick) % males.len() as u64) as usize;
                    (0..males.len().min(32))
                        .map(|n| (offset + n) % males.len())
                        .find(|&candidate| !mother.related(&store.metadata[males[candidate]]))
                        .map(|candidate| (mother.id, store.metadata[males[candidate]].id, area))
                })
                .collect();
            game_parallel_matching::resolve(proposed, seed, tick)
        })
        .collect()
}
pub fn conceptions(
    store: &Store,
    physical: &impl PhysicalView,
    rules: &FamilyRules,
    seed: u64,
    tick: u64,
) -> Result<Vec<Conception>, &'static str> {
    store
        .metadata
        .par_iter()
        .enumerate()
        .filter_map(|(slot, mother)| {
            let state = store.state(slot, true);
            if mother.genome.sex != Sex::Female
                || state.pregnancy.is_some()
                || state.next_conception > tick
                || !eligible(state, physical.read(mother.id), rules)
            {
                return None;
            }
            let father_slot = store.locate(state.partner?)?;
            let father = &store.metadata[father_slot];
            let chance = 500 + u64::from(mother.genome.expressed(0));
            if father.genome.sex != Sex::Male
                || mother.related(father)
                || !eligible(
                    store.state(father_slot, true),
                    physical.read(father.id),
                    rules,
                )
                || physical.read(father.id).region != physical.read(mother.id).region
                || !random(seed ^ 97, mother.id, tick).is_multiple_of(rules.conception_one_in)
                || random(seed ^ 107, mother.id, tick) % 1500 >= chance
            {
                return None;
            }
            Some(
                tick.checked_add(rules.gestation)
                    .ok_or("gestation overflow")
                    .map(|due| (mother.id, (father.id, due))),
            )
        })
        .collect()
}
