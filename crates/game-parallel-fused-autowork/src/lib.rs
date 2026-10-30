//! Prepare automatic work from a committed generation and publish after validation.
use game_parallel_fused_work_experience::ask;
mod requests;
use game_parallel::World;
use game_parallel_fused_data::{Block, NONE, uniform_output};
use game_parallel_fused_metadata::Store;
use game_parallel_packed_jobs_model::Block as JobBlock;
use game_parallel_production_model::PersonSnapshot;
use game_parallel_production_packed::PackedJobs;
use rayon::prelude::*;
use std::collections::HashMap;
/// Collect age and personal work refusals for physical work and construction.
pub fn restricted(
    world: &World,
    family: Option<&game_parallel_family_lifecycle::Runtime>,
    next: u64,
) -> Vec<u64> {
    if world.autowork.is_none()
        && world.agriculture.is_none()
        && world
            .cargo
            .as_ref()
            .is_none_or(|cargo| cargo.food_store_builders.is_empty())
    {
        return Vec::new();
    }
    let mut restricted: Vec<_> = family
        .filter(|family| family.rules.min_work_age > 0)
        .map_or_else(Vec::new, |family| {
            family
                .store
                .metadata
                .par_iter()
                .enumerate()
                .filter_map(|(slot, row)| {
                    (family.store.state(slot, false).age < family.rules.min_work_age)
                        .then_some(row.id)
                })
                .collect()
        });
    if let Some(personality) = &world.personality {
        restricted.extend(personality.work_refusals(world.seed, next));
    }
    restricted
}
pub struct Draft {
    blocks: Vec<JobBlock>,
    needs: HashMap<(u64, u32), u128>,
    pub actors: Vec<u64>,
    next_id: u64,
    pub count: usize,
    request_events: Option<Vec<game_parallel_autowork::requests::Event>>,
    witnessed_refusals: Vec<u64>,
    asks: Option<ask::Draft>,
    incidents: Vec<game_parallel::PropertyTheft>,
}
pub fn prepare(
    world: &World,
    blocks: &[Block],
    jobs: &Store,
    people: &[PersonSnapshot],
    reserved: &[u64],
) -> Result<(Option<Draft>, Vec<game_parallel::GoalCompletion>), &'static str> {
    let Some(auto) = &world.autowork else {
        return Ok((None, Vec::new()));
    };
    let base = auto;
    let mut goal_policies = world
        .goals
        .as_ref()
        .map(|goals| goals.policies(auto, world.tick));
    let production = world
        .production
        .as_ref()
        .ok_or("autowork requires production")?;
    let original_goals = if world.goals.as_ref().is_some_and(|goals| goals.plan_inputs) {
        goal_policies.as_mut().map_or_else(Vec::new, |policies| {
            game_parallel_goals::planning::plan(policies, base, production, people)
        })
    } else {
        Vec::new()
    };
    let mut busy: Vec<_> = blocks
        .par_iter()
        .flat_map_iter(|block| {
            block
                .ids
                .iter()
                .zip(&block.work)
                .filter_map(|(&id, &handle)| (handle != NONE).then_some(id))
        })
        .collect();
    busy.extend_from_slice(reserved);
    let next = world.tick.checked_add(1).ok_or("tick overflow")?;
    let request_draft = base
        .requests
        .as_ref()
        .filter(|requests| !requests.pending.is_empty() || !requests.events.is_empty())
        .map(|requests| requests.prepare(people, world.opinions.as_ref(), &busy, next));
    if let Some(draft) = &request_draft {
        requests::include(base, &mut goal_policies, draft, &mut busy);
    }
    let asks = if let Some(owner) = base
        .requests
        .as_ref()
        .filter(|owner| !owner.askers.is_empty())
    {
        let mut occupied = busy.clone();
        if let Some(draft) = &request_draft {
            occupied.par_extend(draft.decisions.par_iter().map(|row| row.worker));
        }
        ask::prepare(owner, world.goals.as_ref(), people, &occupied, next)?
    } else {
        None
    };
    if let Some(asks) = &asks {
        busy.extend_from_slice(&asks.actors);
    }
    let auto = goal_policies.as_ref().unwrap_or(auto);
    let plan = auto
        .plan_validated_with_property(
            people,
            production,
            &busy,
            jobs.last().map(|row| row.id),
            world.property.as_ref(),
        )
        .map_err(|_| "invalid automatic work")?;
    let blocked = if world.goals.is_some() {
        let mut explicit: Vec<_> = base
            .policies
            .par_iter()
            .map(|policy| policy.person)
            .collect();
        explicit.par_sort_unstable();
        busy.par_sort_unstable();
        let mut granted = plan
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.actors.clone());
        if granted.par_windows(2).any(|pair| pair[0] > pair[1]) {
            granted.par_sort_unstable();
        }
        let dense = granted.first().and_then(|first| {
            granted.last().and_then(|last| {
                (last.checked_sub(*first) == Some((granted.len() - 1) as u64)).then_some(*first)
            })
        });
        let has_grant = |id: u64| {
            dense.is_some_and(|first| id >= first && id - first < granted.len() as u64)
                || (dense.is_none() && granted.binary_search(&id).is_ok())
        };
        auto.policies
            .par_iter()
            .filter(|policy| explicit.binary_search(&policy.person).is_err())
            .filter(|policy| {
                busy.binary_search(&policy.person).is_err() && !has_grant(policy.person)
            })
            .filter(|policy| {
                game_parallel_production_model::person_index(people, policy.person).is_some_and(
                    |i| {
                        let person = people[i];
                        person.1.is_some() && person.2 > 0 && person.3 <= production.max_work_hunger
                    },
                )
            })
            .map(|policy| game_parallel::GoalCompletion {
                person: policy.person,
                recipe: original_goals
                    .binary_search_by_key(&policy.person, |row| row.0)
                    .map_or(policy.recipe, |i| original_goals[i].1),
            })
            .collect()
    } else {
        Vec::new()
    };
    let (request_events, refusals) = request_draft.map_or((None, Vec::new()), |draft| {
        let (events, actors) = requests::outcomes(draft, plan.as_ref(), next);
        (Some(events), actors)
    });
    let mut witnessed_refusals: Vec<_> = base
        .requests
        .as_ref()
        .filter(|owner| owner.memory_limit > 0)
        .zip(request_events.as_ref())
        .map_or_else(Vec::new, |(owner, events)| {
            let mut refused: Vec<_> = events
                .par_iter()
                .filter(|event| {
                    matches!(
                        event.outcome,
                        game_parallel_autowork::requests::Outcome::Refused
                    )
                })
                .map(|event| event.request)
                .collect();
            refused.par_sort_unstable();
            owner
                .pending
                .par_iter()
                .filter(|request| {
                    refused.binary_search(&request.id).is_ok()
                        && game_parallel_production_model::person_index(people, request.requester)
                            .is_some_and(|i| people[i].1 == Some(request.region) && people[i].2 > 0)
                })
                .map(|request| request.id)
                .collect()
        });
    witnessed_refusals.par_sort_unstable();
    if plan.is_none() && request_events.is_none() && asks.is_none() {
        return Ok((None, blocked));
    }
    let incidents = if auto
        .policies
        .par_iter()
        .any(|policy| policy.disregard_claims)
    {
        plan.as_ref().map_or_else(Vec::new, |plan| {
            game_parallel_block_property::work_incidents::prepare(
                world,
                people,
                &plan.requests,
                next,
            )
        })
    } else {
        Vec::new()
    };
    let (blocks, needs, count, next_id, mut actors) = if let Some(plan) = plan {
        let prepared = PackedJobs::from_jobs(&[])
            .prepare_planned(
                &production.recipes,
                &production.inventories,
                &[],
                &plan.requests,
            )
            .map_err(|_| "invalid automatic work")?;
        let (blocks, needs) = prepared.into_parts();
        jobs.validate_append(&blocks)
            .map_err(|_| "automatic work metadata overflow")?;
        (blocks, needs, plan.actors.len(), plan.next_id, plan.actors)
    } else {
        (Vec::new(), HashMap::new(), 0, base.next_id, Vec::new())
    };
    actors.extend(refusals);
    if let Some(asks) = &asks {
        actors.extend_from_slice(&asks.actors);
    }
    Ok((
        Some(Draft {
            count,
            blocks,
            needs,
            actors,
            next_id,
            request_events,
            witnessed_refusals,
            asks,
            incidents,
        }),
        blocked,
    ))
}
impl Draft {
    pub fn commit(
        self,
        world: &mut World,
        blocks: &mut [Block],
        positions: &[(u64, usize, usize)],
        jobs: &mut Store,
    ) {
        let base = jobs.len();
        jobs.append_blocks(self.blocks)
            .expect("prevalidated metadata append");
        let dense_first = positions.first().and_then(|first| {
            (positions.last()?.0.checked_sub(first.0) == Some((positions.len() - 1) as u64))
                .then_some(first.0)
        });
        let mut assigned: Vec<_> = (0..self.count)
            .into_par_iter()
            .map(|i| {
                let person = jobs.row(base + i).worker;
                let index = dense_first.map_or_else(
                    || {
                        positions
                            .binary_search_by_key(&person, |row| row.0)
                            .expect("prepared worker")
                    },
                    |first| usize::try_from(person - first).expect("prepared worker"),
                );
                (positions[index].1, positions[index].2, base + i)
            })
            .collect();
        if assigned
            .par_windows(2)
            .any(|pair| (pair[0].0, pair[0].1) > (pair[1].0, pair[1].1))
        {
            assigned.par_sort_unstable_by_key(|&(block, offset, _)| (block, offset));
        }
        blocks
            .par_iter_mut()
            .enumerate()
            .for_each(|(index, block)| {
                let start = assigned.partition_point(|row| row.0 < index);
                let end = assigned.partition_point(|row| row.0 <= index);
                if start == end {
                    return;
                }
                for &(_, offset, handle) in &assigned[start..end] {
                    let row = jobs.row(handle);
                    block.work[offset] = handle;
                    block.remaining[offset] = row.remaining;
                    block.work_local[offset] =
                        block.journey.is_none() && row.region == world.regions[block.region].id;
                }
                block.uniform_output = uniform_output(&block.work, jobs);
            });
        game_parallel_production_packed::apply_needs(
            &mut world.production.as_mut().unwrap().inventories,
            &self.needs,
        );
        let auto = world.autowork.as_mut().unwrap();
        auto.next_id = self.next_id;
        if let Some(events) = self.request_events {
            requests::publish(auto, events, &self.witnessed_refusals, world.tick);
        }
        if let Some(asks) = self.asks {
            ask::publish(auto.requests.as_mut().expect("prepared requests"), asks);
        }
        game_parallel_block_property::work_incidents::publish(world, self.incidents);
    }
}
