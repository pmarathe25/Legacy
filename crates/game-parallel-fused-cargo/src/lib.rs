//! Stage travel meals against committed person blocks.
mod gifts;
use game_parallel::World;
use game_parallel_fused_data::Block;
pub use gifts::prepare_gifts;
use rayon::prelude::*;
pub fn meals(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    occupied: &[u64],
) -> Vec<u64> {
    let Some(cargo) = &world.cargo else {
        return Vec::new();
    };
    if !cargo.auto_eat_travel && !cargo.auto_eat_local {
        return Vec::new();
    }
    let dense = positions.first().and_then(|first| {
        positions.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64)).then_some(first.0)
        })
    });
    let mut ids: Vec<_> = cargo
        .holdings
        .par_iter()
        .filter_map(|holding| {
            let p = dense
                .and_then(|first| holding.person.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < positions.len())
                .or_else(|| {
                    if dense.is_none() {
                        positions
                            .binary_search_by_key(&holding.person, |row| row.0)
                            .ok()
                    } else {
                        None
                    }
                })?;
            let (_, block, offset) = positions[p];
            let block = &blocks[block];
            (((block.journey.is_some() && cargo.auto_eat_travel)
                || (block.journey.is_none() && cargo.auto_eat_local))
                && block.health[offset] > 0
                && block.hunger[offset].saturating_add(world.rules.hunger_per_tick)
                    >= world.rules.hunger_satisfied
                && holding.food >= world.rules.food_per_meal
                && occupied.binary_search(&holding.person).is_err())
            .then_some(holding.person)
        })
        .collect();
    ids.par_sort_unstable();
    ids
}

pub fn commit(world: &mut World, meals: &[u64]) {
    if !meals.is_empty() {
        game_parallel_block_food_cargo::commit_meals(
            world.cargo.as_mut().expect("prepared cargo"),
            meals,
            world.rules.food_per_meal,
        );
    }
}

/// Filter individual acquisition choices against the committed custody and actor reservations.
pub fn seekers(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    occupied: &[u64],
    meals: &[u64],
) -> Option<Vec<u64>> {
    let cargo = world
        .cargo
        .as_ref()
        .filter(|cargo| cargo.personal_food_access)?;
    let mut holdings: Vec<_> = cargo
        .holdings
        .par_iter()
        .map(|h| (h.person, h.food))
        .collect();
    if holdings.par_windows(2).any(|p| p[0].0 > p[1].0) {
        holdings.par_sort_unstable_by_key(|h| h.0);
    }
    let meal = world.rules.food_per_meal;
    let dense_positions = positions.first().and_then(|first| {
        positions.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64)).then_some(first.0)
        })
    });
    let dense_holdings = holdings.first().and_then(|first| {
        holdings.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((holdings.len() - 1) as u64)).then_some(first.0)
        })
    });
    let mut seekers: Vec<_> = cargo
        .acquire_when_hungry
        .par_iter()
        .copied()
        .filter(|id| {
            let position = dense_positions
                .and_then(|first| id.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < positions.len())
                .or_else(|| {
                    if dense_positions.is_none() {
                        positions.binary_search_by_key(id, |row| row.0).ok()
                    } else {
                        None
                    }
                });
            let Some((_, block, slot)) = position.map(|i| positions[i]) else {
                return false;
            };
            let block = &blocks[block];
            let index = dense_holdings
                .and_then(|first| id.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < holdings.len())
                .or_else(|| {
                    if dense_holdings.is_none() {
                        holdings.binary_search_by_key(id, |h| h.0).ok()
                    } else {
                        None
                    }
                });
            let held = index.map_or(0, |i| holdings[i].1);
            block.journey.is_none()
                && block.health[slot] > 0
                && (block.work[slot] == game_parallel_fused_data::NONE
                    || block.hunger[slot]
                        > world
                            .production
                            .as_ref()
                            .map_or(0, |production| production.max_work_hunger))
                && block
                    .fatigue
                    .as_ref()
                    .is_none_or(|fatigue| fatigue.flags[slot] & 2 == 0)
                && block.hunger[slot].saturating_add(world.rules.hunger_per_tick)
                    >= world.rules.hunger_satisfied
                && held.checked_add(meal).is_some_and(|n| n <= cargo.capacity)
                && occupied.binary_search(id).is_err()
                && meals.binary_search(id).is_err()
        })
        .collect();
    if seekers.par_windows(2).any(|p| p[0] > p[1]) {
        seekers.par_sort_unstable();
    }
    Some(seekers)
}

/// Publish successful physical acquisitions after the meal debit.
pub fn commit_acquisitions(world: &mut World, persons: &[u64]) {
    if persons.is_empty() {
        return;
    }
    let additions: Vec<_> = persons
        .par_iter()
        .map(|&person| game_parallel::Holding {
            person,
            food: world.rules.food_per_meal,
        })
        .collect();
    let cargo = world.cargo.as_mut().expect("prepared cargo");
    if cargo.holdings.is_empty() {
        cargo.holdings = additions;
    } else {
        game_parallel_packing::commit(cargo, &[additions]);
    }
}

/// Attach individual food-choice commands to the fused host.
#[macro_export]
macro_rules! food_choice_methods {
    () => {
        /// Atomically hand personal food to local recipients. Each participant appears at most once.
        pub fn give_food_batch(
            &mut self,
            requests: &[game_parallel::FoodGift],
        ) -> Result<(), String> {
            if requests.is_empty() {
                return Ok(());
            }
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            let cargo = game_parallel_fused_cargo::prepare_gifts(
                &self.template,
                &self.blocks,
                &self.positions,
                requests,
            )?;
            let resolved = game_parallel_fused_cargo::resolve_food_requests(
                &self.template,
                requests,
                self.template.tick,
            );
            self.template.cargo = Some(cargo);
            game_parallel_fused_cargo::commit_food_requests(&mut self.template, &resolved);
            if let Some(opinions) = self
                .template
                .opinions
                .as_mut()
                .filter(|owner| owner.tracks_help())
            {
                let pairs: Vec<_> = requests
                    .iter()
                    .map(|gift| (gift.recipient, gift.giver))
                    .collect();
                opinions.learn_help(&pairs);
            }
            self.revision = revision;
            Ok(())
        }
        /// Change one person's autonomous decision to seek food from a local source.
        pub fn set_food_seeking(&mut self, person: u64, enabled: bool) -> Result<(), String> {
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
            let cargo = self
                .template
                .cargo
                .as_mut()
                .filter(|cargo| cargo.personal_food_access)
                .ok_or("personal food access is disabled")?;
            cargo
                .food_enrollments
                .retain(|entry| entry.person != person);
            cargo.acquire_when_hungry.sort_unstable();
            match (cargo.acquire_when_hungry.binary_search(&person), enabled) {
                (Err(index), true) => cargo.acquire_when_hungry.insert(index, person),
                (Ok(index), false) => {
                    cargo.acquire_when_hungry.remove(index);
                }
                _ => {}
            }
            self.revision = revision;
            Ok(())
        }
    };
}

/// Publish prepared disjoint aid after other food custody changes.
pub fn commit_aid(world: &mut World, gifts: &[game_parallel::FoodGift]) {
    gifts::apply(world.cargo.as_mut().expect("prepared cargo"), gifts);
    if let Some(opinions) = world.opinions.as_mut().filter(|owner| owner.tracks_help()) {
        let pairs: Vec<_> = gifts
            .par_iter()
            .map(|gift| (gift.recipient, gift.giver))
            .collect();
        opinions.learn_help(&pairs);
    }
}

/// Remove only requests whose deadlines passed or whose food was actually delivered.
pub fn commit_food_requests(world: &mut World, ids: &[u64]) {
    if ids.is_empty() {
        return;
    }
    let cargo = world.cargo.as_mut().expect("prepared cargo");
    cargo.food_requests = cargo
        .food_requests
        .par_iter()
        .filter(|request| ids.binary_search(&request.id).is_err())
        .copied()
        .collect();
}

pub struct SocialFoodDraft {
    pub gifts: Vec<game_parallel::FoodGift>,
    pub resolved: Vec<u64>,
    pub automatic: Option<(u64, Vec<game_parallel::FoodRequest>)>,
    pub due_seekers: Vec<u64>,
}
/// Publish aid, request resolution and maturation choices at the tick boundary.
pub fn commit_social_food(world: &mut World, draft: SocialFoodDraft) {
    if !draft.gifts.is_empty() {
        commit_aid(world, &draft.gifts);
    }
    commit_food_requests(world, &draft.resolved);
    if let Some((next_id, requests)) = draft.automatic {
        let cargo = world.cargo.as_mut().expect("prepared cargo");
        cargo.food_requests.par_extend(requests);
        cargo.next_food_request = next_id;
    }
    if !draft.due_seekers.is_empty() {
        let cargo = world.cargo.as_mut().expect("prepared cargo");
        cargo.food_enrollments = cargo
            .food_enrollments
            .par_iter()
            .filter(|entry| draft.due_seekers.binary_search(&entry.person).is_err())
            .copied()
            .collect();
        cargo.acquire_when_hungry.par_extend(draft.due_seekers);
        if cargo
            .acquire_when_hungry
            .par_windows(2)
            .any(|p| p[0] > p[1])
        {
            cargo.acquire_when_hungry.par_sort_unstable();
        }
    }
}

/// Find expired requests and those satisfied by actual deliveries.
pub fn resolve_food_requests(
    world: &World,
    gifts: &[game_parallel::FoodGift],
    tick: u64,
) -> Vec<u64> {
    let Some(cargo) = world
        .cargo
        .as_ref()
        .filter(|cargo| !cargo.food_requests.is_empty())
    else {
        return Vec::new();
    };
    let mut gifts: Vec<_> = gifts.par_iter().collect();
    if gifts
        .par_windows(2)
        .any(|p| (p[0].giver, p[0].recipient) > (p[1].giver, p[1].recipient))
    {
        gifts.par_sort_unstable_by_key(|g| (g.giver, g.recipient));
    }
    let mut ids: Vec<_> = cargo
        .food_requests
        .par_iter()
        .filter(|request| {
            request.expires_at < tick
                || gifts
                    .binary_search_by_key(&(request.giver, request.requester), |g| {
                        (g.giver, g.recipient)
                    })
                    .ok()
                    .is_some_and(|i| gifts[i].amount >= request.amount)
        })
        .map(|request| request.id)
        .collect();
    if ids.par_windows(2).any(|p| p[0] > p[1]) {
        ids.par_sort_unstable();
    }
    ids
}
