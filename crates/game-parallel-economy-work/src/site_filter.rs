//! Individual work candidates filtered against provisional physical site capacity.
use game_parallel_autowork::Autowork;
use game_parallel_production_model::{PersonSnapshot, Start, person_index};
use game_parallel_work_sites::Facilities;
use rayon::prelude::*;

pub fn filter(
    sites: &Facilities,
    auto: &Autowork,
    people: &[PersonSnapshot],
    busy: &[u64],
    max_hunger: u32,
    seed: u64,
    tick: u64,
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
            (person.2 > 0 && person.3 <= max_hunger).then_some(Start {
                id: p.person,
                worker: p.person,
                region,
                recipe: p.recipe,
                units: 1,
            })
        })
        .collect();
    let plan = sites.prepare(&candidates, seed, tick);
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
