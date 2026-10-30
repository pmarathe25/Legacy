//! Coordinate food actions without publishing the prepared generation.
mod auto_requests;
mod sharing;
use game_parallel::World;
use game_parallel_fused_data::Block;
use rayon::prelude::*;
pub struct FoodPlan {
    pub stores: Option<game_parallel_fused_food_aid::stores::Draft>,
    pub deposits: Option<game_parallel_fused_food_deposit::Draft>,
    pub builds: Option<game_parallel_fused_store_build::Draft>,
    pub diet: Option<game_parallel_fused_diet::Draft>,
    pub carried_meals: Vec<u64>,
    pub meal_debits: Option<Vec<u64>>,
    pub seekers: Option<Vec<u64>>,
    pub care_fetch_children: Vec<u64>,
    pub due_seekers: Vec<u64>,
    pub actors: Vec<u64>,
    pub aid: Option<game_parallel_fused_food_aid::AidDraft>,
    pub resolved: Vec<u64>,
    pub automatic: Option<(u64, Vec<game_parallel::FoodRequest>)>,
}
pub fn prepare(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    care: Option<&game_parallel_dependent_care::Draft>,
    planned: &[u64],
    build_restricted: &[u64],
    next: u64,
) -> Result<FoodPlan, &'static str> {
    let feeding = game_parallel_fused_dependent_care::prepare_feeding(
        world, care, blocks, positions, planned,
    );
    let fed_children = feeding.as_ref().map_or(&[][..], |f| f.children.as_slice());
    let caregivers = feeding
        .as_ref()
        .map_or(&[][..], |f| f.caregivers.as_slice());
    let fetch_children = feeding
        .as_ref()
        .map_or(&[][..], |f| f.fetch_children.as_slice());
    let fetch_caregivers = feeding
        .as_ref()
        .map_or(&[][..], |f| f.fetch_caregivers.as_slice());
    let mut base_busy = Vec::new();
    let occupied = if fed_children.is_empty() && fetch_children.is_empty() {
        planned
    } else {
        base_busy.extend_from_slice(planned);
        base_busy.extend_from_slice(fed_children);
        base_busy.extend_from_slice(fetch_children);
        if base_busy.par_windows(2).any(|p| p[0] > p[1]) {
            base_busy.par_sort_unstable();
        }
        base_busy.as_slice()
    };
    let auto_requests = auto_requests::prepare_requests(world, blocks, positions, occupied, next)?;
    let ask_actors = auto_requests
        .as_ref()
        .map_or(&[][..], |draft| draft.actors.as_slice());
    let mut aid_busy = Vec::new();
    let occupied = if ask_actors.is_empty() {
        occupied
    } else {
        aid_busy.extend_from_slice(occupied);
        aid_busy.extend_from_slice(ask_actors);
        if aid_busy.par_windows(2).any(|p| p[0] > p[1]) {
            aid_busy.par_sort_unstable();
        }
        aid_busy.as_slice()
    };
    let learned = sharing::candidates(world);
    let food_aid = game_parallel_fused_food_aid::prepare_aid(
        world, blocks, positions, occupied, next, &learned,
    );
    let resolved_food_requests = game_parallel_fused_cargo::resolve_food_requests(
        world,
        food_aid
            .as_ref()
            .map_or(&[][..], |aid| aid.gifts.as_slice()),
        next,
    );
    let aid_actors = food_aid
        .as_ref()
        .map_or(&[][..], |aid| aid.actors.as_slice());
    let mut food_actors = ask_actors.to_vec();
    food_actors.extend_from_slice(aid_actors);
    food_actors.extend_from_slice(fed_children);
    food_actors.extend_from_slice(caregivers);
    food_actors.extend_from_slice(fetch_children);
    food_actors.extend_from_slice(fetch_caregivers);
    if food_actors.par_windows(2).any(|p| p[0] > p[1]) {
        food_actors.par_sort_unstable();
    }
    let deposits = if world
        .cargo
        .as_ref()
        .is_some_and(|cargo| !cargo.food_depositors.is_empty())
    {
        let mut deposit_busy = planned.to_vec();
        deposit_busy.extend_from_slice(&food_actors);
        if deposit_busy.par_windows(2).any(|p| p[0] > p[1]) {
            deposit_busy.par_sort_unstable();
        }
        game_parallel_fused_food_deposit::prepare(world, blocks, positions, &deposit_busy, next)
    } else {
        None
    };
    if let Some(deposits) = &deposits {
        game_parallel_fused_store_build::add_actors(&mut food_actors, &deposits.actors);
    }
    let game_parallel_fused_diet::Meals {
        diet,
        carried: carried_meals,
        debits: meal_debits,
    } = game_parallel_fused_diet::meals(
        world,
        blocks,
        positions,
        planned,
        &mut food_actors,
        fed_children,
        caregivers,
    )?;
    let personal_access = world
        .cargo
        .as_ref()
        .is_some_and(|cargo| cargo.personal_food_access);
    let mut seekers = if personal_access {
        let mut busy = planned.to_vec();
        busy.extend_from_slice(&food_actors);
        if let Some(conflict) = &world.conflict {
            busy.extend(conflict.actors());
        }
        if let Some(care) = &world.care {
            busy.extend(care.actors());
        }
        if busy.par_windows(2).any(|p| p[0] > p[1]) {
            busy.par_sort_unstable();
        }
        game_parallel_fused_cargo::seekers(world, blocks, positions, &busy, &carried_meals)
    } else {
        None
    };
    let stores = seekers.as_ref().and_then(|seekers| {
        game_parallel_fused_food_aid::stores::prepare(world, blocks, positions, seekers)
    });
    if let Some(stores) = &stores {
        if let Some(seekers) = &mut seekers {
            *seekers = std::mem::take(seekers)
                .into_par_iter()
                .filter(|id| stores.actors.binary_search(id).is_err())
                .collect();
        }
        game_parallel_fused_store_build::add_actors(&mut food_actors, &stores.actors);
    }
    let reservations = (
        planned,
        build_restricted,
        &mut food_actors,
        &carried_meals[..],
        seekers.as_deref(),
    );
    let builds = game_parallel_fused_store_build::prepare_reserved(
        world,
        blocks,
        positions,
        reservations,
        next,
    )?;
    if !fetch_children.is_empty() {
        let rows = seekers
            .as_mut()
            .expect("personal access for dependent food");
        rows.extend_from_slice(fetch_children);
        if rows.par_windows(2).any(|p| p[0] > p[1]) {
            rows.par_sort_unstable();
        }
    }
    let care_fetch_children = fetch_children.to_vec();
    let mut due_seekers: Vec<_> = world.cargo.as_ref().map_or_else(Vec::new, |cargo| {
        cargo
            .food_enrollments
            .par_iter()
            .filter_map(|entry| (entry.at <= next).then_some(entry.person))
            .collect()
    });
    if due_seekers.par_windows(2).any(|p| p[0] > p[1]) {
        due_seekers.par_sort_unstable();
    }
    Ok(FoodPlan {
        deposits,
        builds,
        diet,
        stores,
        carried_meals,
        meal_debits,
        seekers,
        care_fetch_children,
        due_seekers,
        actors: food_actors,
        aid: food_aid,
        resolved: resolved_food_requests,
        automatic: auto_requests.map(|draft| (draft.next_id, draft.requests)),
    })
}

#[macro_export]
macro_rules! commit_stores {
    ($host:expr, $draft:expr) => {
        if let Some(deposits) = $draft.state.food_deposits {
            deposits.commit(&mut $host.template);
        }
        if let Some(stores) = $draft.state.food_stores {
            stores.commit(&mut $host.template, $draft.tick);
        }
    };
}
