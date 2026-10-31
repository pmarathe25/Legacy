//! Bounded per-person search through site and kit prerequisites.
use game_parallel_economy_building::{Context, GoalNeed};
use game_parallel_goals::planning::{SupplyNeed, plan_supply};
use game_parallel_production::Production;
use rayon::prelude::*;

const MAX_SITES: usize = 16;

struct Search {
    person: u64,
    recipe: u32,
    visited: [u32; MAX_SITES],
    depth: usize,
}

pub struct Plan {
    pub builds: Vec<GoalNeed>,
    pub work: Vec<(u64, u32)>,
}

pub fn prepare(context: Context<'_>, production: &Production, pending: &[u64]) -> Plan {
    let capabilities = context.sites.capabilities();
    let mut searches: Vec<_> = context
        .goal_needs
        .par_iter()
        .map(|need| {
            let mut visited = [0; MAX_SITES];
            visited[0] = need.recipe;
            Search {
                person: need.person,
                recipe: need.recipe,
                visited,
                depth: 1,
            }
        })
        .collect();
    let mut builds = Vec::new();
    let mut work = Vec::new();
    for _ in 0..MAX_SITES {
        if searches.is_empty() {
            break;
        }
        let needs: Vec<_> = searches
            .par_iter()
            .map(|s| GoalNeed {
                person: s.person,
                recipe: s.recipe,
            })
            .collect();
        let context = Context {
            goal_needs: &needs,
            ..context
        };
        let kits = context.goal_kits();
        builds.par_extend(kits.par_iter().filter(|kit| kit.ready).map(|kit| GoalNeed {
            person: kit.person,
            recipe: kit.recipe,
        }));
        let missing: Vec<_> = kits
            .into_par_iter()
            .filter(|kit| !kit.ready && pending.binary_search(&kit.person).is_err())
            .filter_map(|kit| {
                let search = searches
                    .binary_search_by_key(&kit.person, |s| s.person)
                    .ok()
                    .map(|i| &searches[i])?;
                Some(SupplyNeed {
                    person: kit.person,
                    region: kit.region,
                    goods: kit.goods,
                    visited_sites: search.visited[..search.depth].to_vec(),
                })
            })
            .collect();
        if missing.is_empty() {
            break;
        }
        let steps = plan_supply(&missing, production, |region, recipe| {
            capabilities.permits(region, recipe)
        });
        work.par_extend(
            steps
                .par_iter()
                .filter(|step| !step.needs_site)
                .map(|step| (step.person, step.recipe)),
        );
        searches = steps
            .into_par_iter()
            .filter(|step| step.needs_site)
            .filter_map(|step| {
                let previous = searches
                    .binary_search_by_key(&step.person, |s| s.person)
                    .ok()
                    .map(|i| &searches[i])?;
                if previous.depth == MAX_SITES
                    || previous.visited[..previous.depth].contains(&step.recipe)
                {
                    return None;
                }
                let mut visited = previous.visited;
                visited[previous.depth] = step.recipe;
                Some(Search {
                    person: step.person,
                    recipe: step.recipe,
                    visited,
                    depth: previous.depth + 1,
                })
            })
            .collect();
    }
    builds.par_sort_unstable_by_key(|need| need.person);
    work.par_sort_unstable_by_key(|row| row.0);
    Plan { builds, work }
}
