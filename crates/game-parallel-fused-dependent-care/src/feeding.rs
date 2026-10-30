//! Feed a dependent during selected care using real food.
use game_parallel::World;
use game_parallel_dependent_care::Draft;
use game_parallel_fused_data::Block;
use rayon::prelude::*;
pub struct Feeding {
    pub caregivers: Vec<u64>,
    pub children: Vec<u64>,
    pub fetch_caregivers: Vec<u64>,
    pub fetch_children: Vec<u64>,
}
pub fn prepare_feeding(
    world: &World,
    care: Option<&Draft>,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    occupied: &[u64],
) -> Option<Feeding> {
    let rules = &world.caregiving.as_ref()?.rules;
    if !rules.feed_from_cargo && !rules.fetch_local_food {
        return None;
    }
    let cargo = world
        .cargo
        .as_ref()
        .filter(|cargo| cargo.personal_food_access)?;
    let care = care?;
    let mut holdings: Vec<_> = cargo
        .holdings
        .par_iter()
        .map(|h| (h.person, h.food))
        .collect();
    if holdings.par_windows(2).any(|p| p[0].0 > p[1].0) {
        holdings.par_sort_unstable_by_key(|h| h.0);
    }
    let held = |id| {
        holdings
            .binary_search_by_key(&id, |h| h.0)
            .ok()
            .map_or(0, |i| holdings[i].1)
    };
    let choices: Vec<_> = care
        .events
        .par_iter()
        .filter_map(|event| {
            let parent = event.caregiver?;
            if occupied.binary_search(&event.child).is_ok()
                || held(event.child) >= world.rules.food_per_meal
            {
                return None;
            }
            let index = positions
                .binary_search_by_key(&event.child, |row| row.0)
                .ok()?;
            let (_, block, slot) = positions[index];
            let child = &blocks[block];
            if child.health[slot] == 0
                || child.hunger[slot].saturating_add(world.rules.hunger_per_tick)
                    < world.rules.hunger_satisfied
            {
                return None;
            }
            let held_food = held(parent);
            let personal = rules.feed_from_cargo && held_food >= world.rules.food_per_meal;
            let fetch = rules.fetch_local_food
                && child.journey.is_none()
                && held_food
                    .checked_add(world.rules.food_per_meal)
                    .is_some_and(|n| n <= cargo.capacity);
            (personal || fetch).then_some((parent, event.child, personal))
        })
        .collect();
    if choices.is_empty() {
        return None;
    }
    let mut caregivers: Vec<_> = choices
        .par_iter()
        .filter_map(|&(parent, _, own)| own.then_some(parent))
        .collect();
    let mut children: Vec<_> = choices
        .par_iter()
        .filter_map(|&(_, child, own)| own.then_some(child))
        .collect();
    let mut fetch_caregivers: Vec<_> = choices
        .par_iter()
        .filter_map(|&(parent, _, own)| (!own).then_some(parent))
        .collect();
    let mut fetch_children: Vec<_> = choices
        .par_iter()
        .filter_map(|&(_, child, own)| (!own).then_some(child))
        .collect();
    caregivers.par_sort_unstable();
    children.par_sort_unstable();
    fetch_caregivers.par_sort_unstable();
    fetch_children.par_sort_unstable();
    Some(Feeding {
        caregivers,
        children,
        fetch_caregivers,
        fetch_children,
    })
}
