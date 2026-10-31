//! Individual work candidates filtered against provisional physical site capacity.
use game_parallel_autowork::{Autowork, Plan};
use game_parallel_production_model::{PersonSnapshot, Start, person_index};
use game_parallel_work_sites::Facilities;
use rayon::prelude::*;

#[derive(Clone, Copy)]
pub struct Selection<'a> {
    pub seed: u64,
    pub tick: u64,
    pub claims: Option<&'a game_parallel_economy_claims::Index<'a>>,
    pub classes: &'a [(u64, Option<u64>)],
    pub production: &'a game_parallel_production::Production,
    pub view: &'a game_parallel_lots::View,
    pub max_job: Option<u64>,
}
fn filter(
    sites: &Facilities,
    auto: &Autowork,
    people: &[PersonSnapshot],
    busy: &[u64],
    selection: Selection<'_>,
) -> Autowork {
    let candidates: Vec<_> = auto
        .policies
        .par_iter()
        .filter_map(|p| {
            if !p.enabled || busy.binary_search(&p.person).is_ok() {
                return None;
            }
            let person = person_index(people, p.person).map(|i| people[i])?;
            let region = person.1?;
            let recipe = selection
                .production
                .recipes
                .binary_search_by_key(&p.recipe, |r| r.id)
                .ok()
                .map(|i| &selection.production.recipes[i])?;
            if !recipe.inputs.iter().all(|input| {
                selection
                    .view
                    .container(game_parallel_lots::Place::Stock {
                        region,
                        good: input.good,
                    })
                    .is_some_and(|stock| stock.quantity(input.good) >= input.quantity)
            }) {
                return None;
            }
            (person.2 > 0 && person.3 <= selection.production.max_work_hunger).then_some(Start {
                id: p.person,
                worker: p.person,
                region,
                recipe: p.recipe,
                units: 1,
            })
        })
        .collect();
    let disregards = super::site_disregards(auto, selection.claims, selection.classes);
    let plan = sites.prepare_access(
        &candidates,
        &game_parallel_work_sites::access::Access {
            classes: selection.classes,
            disregards: &disregards,
        },
        selection.seed,
        selection.tick,
    );
    let policies = auto
        .policies
        .par_iter()
        .filter(|p| {
            sites
                .requirements
                .binary_search_by_key(&p.recipe, |r| r.recipe)
                .is_err()
                || plan.permits(p.person)
        })
        .cloned()
        .collect();
    Autowork {
        next_id: auto.next_id,
        policies,
        requests: None,
    }
}

/// Reconsider site losers when an earlier provisional winner cannot reserve work inputs.
/// The bounded rounds never publish intermediate reservations.
pub fn plan(
    sites: &Facilities,
    auto: &Autowork,
    people: &[PersonSnapshot],
    busy: &[u64],
    selection: Selection<'_>,
) -> Result<Option<Plan>, String> {
    const ROUNDS: usize = 4;
    let mut excluded = busy.to_vec();
    for round in 0..ROUNDS {
        let filtered = filter(sites, auto, people, &excluded, selection);
        let work =
            filtered.plan_validated(people, selection.production, &excluded, selection.max_job)?;
        if round + 1 == ROUNDS {
            return Ok(work);
        }
        let mut granted = work
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.actors.clone());
        granted.par_sort_unstable();
        let denied: Vec<_> = filtered
            .policies
            .par_iter()
            .filter(|policy| {
                sites
                    .requirements
                    .binary_search_by_key(&policy.recipe, |r| r.recipe)
                    .is_ok()
                    && granted.binary_search(&policy.person).is_err()
            })
            .map(|policy| policy.person)
            .collect();
        if denied.is_empty() {
            return Ok(work);
        }
        excluded.par_extend(denied);
        excluded.par_sort_unstable();
        excluded.dedup();
    }
    unreachable!("bounded site admission returns in its final round")
}
