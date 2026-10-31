//! Personal aspirations form goals and gain progress only from real work.
use game_parallel_autowork::Autowork;
pub use game_parallel_dreams::{Dream, Dreams};
use game_parallel_economy_person::Person;
use game_parallel_goals::Completion;
pub use game_parallel_goals::Goals;
use game_parallel_lot_runtime::{Events, Work};
use game_parallel_production_model::{PersonSnapshot, Recipe, person_index};
use rayon::prelude::*;
#[macro_export]
macro_rules! prepare_work {
    ($world:expr, $view:expr, $people:expr, $claims:expr, $busy:expr, $tick:expr, $preplanned:expr) => {{
        let mut unwilling: Vec<_> = $world
            .people
            .par_iter()
            .filter(|p| !p.willing_to_work)
            .map(|p| p.id)
            .collect();
        if let Some(personality) = $world
            .dynamics
            .as_ref()
            .and_then(|d| d.personality.as_ref())
        {
            unwilling.par_extend(personality.work_refusals($world.seed, $tick));
            unwilling.par_sort_unstable();
            unwilling.dedup();
        }
        let production = game_parallel_lot_projection::production(
            &$world.physical,
            &$view,
            $world.rules.max_work_hunger,
        );
        $world
            .autowork
            .as_ref()
            .map(|auto| {
                game_parallel_economy_work::Context {
                    auto,
                    production: &production,
                    people: &$people,
                    tool_requirements: &$world.work_tools,
                    view: &$view,
                    opinions: $world.opinions.as_ref(),
                    claims: $claims.as_ref(),
                    goals: $world.goals.as_ref(),
                    preplanned_goals: $preplanned.as_ref(),
                    sites: $world.work_sites.as_ref(),
                    seed: $world.seed,
                    busy: &$busy,
                    unwilling: &unwilling,
                    max_job: $world.physical.work().last().map(|row| row.job.id),
                    tick: $tick,
                }
                .prepare()
            })
            .transpose()?
    }};
}
use std::collections::HashSet;

/// Retry only goals that this person actually attempted and could not start.
pub fn blocked(
    goals: Option<&Goals>,
    auto: Option<&Autowork>,
    plan: Option<&game_parallel_economy_work::Plan>,
    people: &[PersonSnapshot],
    max_hunger: u32,
    events: &Events,
    physical_work: &[Work],
) -> Vec<Completion> {
    let (Some(goals), Some(auto), Some(plan)) = (goals, auto, plan) else {
        return Vec::new();
    };
    let mut delivered_workers: Vec<_> = if goals.plan_inputs {
        events
            .delivered
            .par_iter()
            .filter_map(|id| {
                physical_work
                    .binary_search_by_key(id, |w| w.job.id)
                    .ok()
                    .map(|i| physical_work[i].job.worker)
            })
            .collect()
    } else {
        Vec::new()
    };
    delivered_workers.par_sort_unstable();
    let mut direct: Vec<_> = auto.policies.par_iter().map(|p| p.person).collect();
    direct.par_sort_unstable();
    let mut starts: Vec<_> = plan.work.as_ref().map_or_else(Vec::new, |work| {
        work.requests
            .par_iter()
            .map(|row| (row.worker, row.id))
            .collect()
    });
    starts.par_sort_unstable();
    goals
        .policies(auto, events.tick.saturating_sub(1))
        .policies
        .into_par_iter()
        .filter_map(|policy| {
            if direct.binary_search(&policy.person).is_ok()
                || plan.occupied.binary_search(&policy.person).is_ok()
                || delivered_workers.binary_search(&policy.person).is_ok()
            {
                return None;
            }
            let person = person_index(people, policy.person).map(|i| people[i])?;
            if person.1.is_none() || person.2 == 0 || person.3 > max_hunger {
                return None;
            }
            if starts
                .binary_search_by_key(&policy.person, |row| row.0)
                .ok()
                .is_some_and(|i| events.started.binary_search(&starts[i].1).is_ok())
            {
                return None;
            }
            Some(Completion {
                person: policy.person,
                recipe: policy.recipe,
            })
        })
        .collect()
}

pub fn validate(
    goals: Option<&Goals>,
    dreams: Option<&Dreams>,
    has_work: bool,
    people: &HashSet<u64>,
    recipes: &[Recipe],
    tick: u64,
) -> Result<(), String> {
    let recipes: HashSet<_> = recipes.iter().map(|row| row.id).collect();
    if let Some(goals) = goals {
        if !has_work {
            return Err("goals require personal work planning".into());
        }
        goals
            .validate(people, &recipes, tick)
            .map_err(str::to_owned)?;
    }
    if let Some(dreams) = dreams {
        if goals.is_none() {
            return Err("aspirations require personal goals".into());
        }
        dreams
            .validate(people, &recipes, tick)
            .map_err(str::to_owned)?;
    }
    Ok(())
}
pub fn advance(
    goals: Option<&Goals>,
    dreams: Option<&Dreams>,
    work: &[Work],
    events: &Events,
    people: &[Person],
    blocked: &[Completion],
) -> Result<(Option<Goals>, Option<Dreams>), String> {
    let completed: Vec<_> = events
        .finished
        .par_iter()
        .filter_map(|id| {
            work.binary_search_by_key(id, |row| row.job.id)
                .ok()
                .map(|index| {
                    let job = &work[index].job;
                    Completion {
                        person: job.worker,
                        recipe: job.recipe,
                    }
                })
        })
        .collect();
    let mut goals = goals
        .map(|owner| {
            owner
                .prepare(&completed, blocked, events.tick)
                .map(|draft| {
                    draft.unwrap_or_else(|| {
                        let mut next = owner.clone();
                        next.events.clear();
                        next
                    })
                })
        })
        .transpose()
        .map_err(str::to_owned)?;
    let dreams = if let Some(owner) = dreams {
        let current = goals.as_ref().ok_or("aspirations require personal goals")?;
        let draft = owner
            .prepare(current, events.tick, |id| {
                people
                    .binary_search_by_key(&id, |p| p.id)
                    .ok()
                    .is_some_and(|i| people[i].health > 0 && !people[i].controlled)
            })
            .map_err(str::to_owned)?;
        if let Some(draft) = draft {
            if let Some(next) = draft.goals {
                goals = Some(next);
            }
            Some(draft.state)
        } else {
            let mut next = owner.clone();
            next.events.clear();
            Some(next)
        }
    } else {
        None
    };
    Ok((goals, dreams))
}

#[macro_export]
macro_rules! request_method {
    () => {
        /// Address a request to another individual; their later choice is independent.
        pub fn ask_work(
            &mut self,
            requester: u64,
            worker: u64,
            recipe: u32,
            lifetime: u64,
        ) -> Result<u64, String> {
            if self.campaign.as_ref().is_some_and(|c| {
                c.lineage.current == Some(requester)
                    && c.lineage.controlled_person(self.tick()) != Some(requester)
            }) {
                return Err("player control is suspended".into());
            }
            if self
                .combat
                .as_ref()
                .is_some_and(|c| c.actors().binary_search(&requester).is_ok())
            {
                return Err("combatants cannot request work during a fight".into());
            }
            let tick = self.tick();
            let expires = tick
                .checked_add(lifetime)
                .ok_or("request expiry overflow")?;
            let person = self
                .people
                .binary_search_by_key(&requester, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown requester")?;
            let region = person.region.ok_or("requester is traveling")?;
            let view = self.physical.view(tick)?;
            let production = game_parallel_lot_projection::production(
                &self.physical,
                &view,
                self.rules.max_work_hunger,
            );
            let people: Vec<_> = self
                .people
                .par_iter()
                .map(|p| (p.id, p.region, p.health, p.hunger))
                .collect();
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            let requests = self
                .autowork
                .as_mut()
                .and_then(|a| a.requests.as_mut())
                .ok_or("work requests are disabled")?;
            let id = requests.open(
                game_parallel_autowork::requests::Proposal {
                    requester,
                    worker,
                    region,
                    recipe,
                    expires,
                },
                &people,
                &production,
                tick,
            )?;
            self.epoch = epoch;
            Ok(id)
        }
    };
}
