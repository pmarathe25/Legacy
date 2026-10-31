//! Individual work candidates filtered against provisional physical site capacity.
use game_parallel_autowork::Autowork;
use game_parallel_production_model::{PersonSnapshot, Start, person_index};
use game_parallel_work_sites::Facilities;
use rayon::prelude::*;

pub struct Selection<'a> {
    pub seed: u64,
    pub tick: u64,
    pub claims: Option<&'a game_parallel_economy_claims::Index<'a>>,
    pub classes: &'a [(u64, Option<u64>)],
    pub recipes: &'a [game_parallel_production_model::Recipe],
    pub view: &'a game_parallel_lots::View,
}
pub fn filter(
    sites: &Facilities,
    auto: &Autowork,
    people: &[PersonSnapshot],
    busy: &[u64],
    max_hunger: u32,
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
                .recipes
                .binary_search_by_key(&p.recipe, |r| r.id)
                .ok()
                .map(|i| &selection.recipes[i])?;
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
            (person.2 > 0 && person.3 <= max_hunger).then_some(Start {
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
