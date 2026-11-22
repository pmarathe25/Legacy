//! Individual work candidates filtered against provisional physical site capacity.
use game_autowork::{Autowork, Plan};
use game_production_model::{PersonSnapshot, Start, person_index};
use game_work_sites::Facilities;
use rayon::prelude::*;

#[derive(Clone, Copy)]
pub struct Selection<'a> {
    pub seed: u64,
    pub tick: u64,
    pub claims: Option<&'a game_economy_claims::Index<'a>>,
    pub classes: &'a [(u64, Option<u64>)],
    pub production: &'a game_production::Production,
    pub view: &'a game_lots::View,
    pub max_job: Option<u64>,
}
fn filter(
    sites: &Facilities,
    auto: &Autowork,
    people: &[PersonSnapshot],
    busy: &[u64],
    selection: Selection<'_>,
) -> Autowork {
    let ambiguous = auto.ambiguous_people();
    let candidates: Vec<_> = auto
        .policies
        .par_iter()
        .filter_map(|p| {
            if !p.enabled
                || busy.binary_search(&p.person).is_ok()
                || ambiguous.binary_search(&p.person).is_ok()
            {
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
                    .container(game_lots::Place::Stock {
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
        &game_work_sites::access::Access {
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
            ambiguous.binary_search(&p.person).is_err()
                && (sites
                    .requirements
                    .binary_search_by_key(&p.recipe, |r| r.recipe)
                    .is_err()
                    || plan.permits(p.person))
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

#[cfg(test)]
mod tests {
    use super::*;
    use game_autowork::Policy;
    use game_lots::{Balance, ContainerView, Place, View};
    use game_production::{Amount, Inventory, Production, Recipe, Stock};
    use game_work_sites::{Requirement, Site};

    #[test]
    fn unresolved_recipes_do_not_reserve_someone_elses_site() {
        let people = vec![(1, Some(1), 100, 0), (2, Some(1), 100, 0)];
        let recipe = |id| Recipe {
            id,
            name: format!("work-{id}"),
            ticks: 1,
            inputs: vec![Amount {
                good: 1,
                quantity: 1,
            }],
            outputs: vec![Amount {
                good: 2,
                quantity: 1,
            }],
        };
        let production = Production {
            max_work_hunger: 20,
            goods: vec![],
            recipes: vec![recipe(1), recipe(2)],
            recipe_categories: vec![],
            inventories: vec![Inventory {
                region: 1,
                stocks: vec![
                    Stock {
                        good: 1,
                        quantity: 10,
                        capacity: 10,
                    },
                    Stock {
                        good: 2,
                        quantity: 0,
                        capacity: 10,
                    },
                ],
            }],
            jobs: vec![],
            events: vec![],
        };
        let policy = |person, recipe| Policy {
            person,
            recipe,
            enabled: true,
            disregard_claims: false,
        };
        let auto = Autowork {
            next_id: 1,
            policies: vec![policy(1, 1), policy(1, 2), policy(2, 1)],
            requests: None,
        };
        let sites = Facilities {
            next_id: 2,
            sites: vec![Site {
                id: 1,
                region: 1,
                kind: "bench".into(),
                capacity: 1,
            }],
            requirements: vec![
                Requirement {
                    recipe: 1,
                    kind: "bench".into(),
                    slots: 1,
                },
                Requirement {
                    recipe: 2,
                    kind: "bench".into(),
                    slots: 1,
                },
            ],
            bindings: vec![],
        };
        let view = View {
            region_ids: None,
            boundary: 0,
            revision: 0,
            containers: vec![ContainerView {
                place: Place::Stock { region: 1, good: 1 },
                capacity: 10,
                used: 10,
                goods: vec![Balance {
                    good: 1,
                    quantity: 10,
                }],
            }],
        };
        let selection = Selection {
            seed: 1,
            tick: 1,
            claims: None,
            classes: &[],
            production: &production,
            view: &view,
            max_job: None,
        };
        for workers in [1, 4] {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let result = plan(&sites, &auto, &people, &[], selection)
                        .unwrap()
                        .unwrap();
                    assert_eq!(result.actors, vec![2]);
                    assert_eq!(result.requests[0].worker, 2);
                });
        }
    }
}
