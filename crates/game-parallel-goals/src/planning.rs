//! Bounded local input planning for autonomous goals.
use game_parallel_autowork::Autowork;
use game_parallel_production::{Inventory, Production, Recipe};
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

const MAX_DEPTH: usize = 16;
const MAX_VISITS: usize = 64;

struct Index<'a> {
    recipes: HashMap<u32, &'a Recipe>,
    producers: HashMap<u32, Vec<&'a Recipe>>,
    inventory: HashMap<u64, &'a Inventory>,
    review: u64,
}
impl<'a> Index<'a> {
    fn new(production: &'a Production, review: u64) -> Self {
        let recipes = production
            .recipes
            .iter()
            .map(|recipe| (recipe.id, recipe))
            .collect();
        let mut producers: HashMap<u32, Vec<&Recipe>> = HashMap::new();
        for recipe in &production.recipes {
            for output in &recipe.outputs {
                producers.entry(output.good).or_default().push(recipe);
            }
        }
        for recipes in producers.values_mut() {
            recipes.sort_unstable_by_key(|recipe| recipe.id);
        }
        let inventory = production
            .inventories
            .iter()
            .map(|inventory| (inventory.region, inventory))
            .collect();
        Self {
            recipes,
            producers,
            inventory,
            review,
        }
    }
    fn rotated<'b, T>(&self, rows: &'b [T]) -> impl Iterator<Item = &'b T> {
        let start = if rows.is_empty() {
            0
        } else {
            (self.review % rows.len() as u64) as usize
        };
        rows[start..].iter().chain(rows[..start].iter())
    }
    fn choose(
        &self,
        recipe: &Recipe,
        inventory: &Inventory,
        visited: &mut [u32; MAX_DEPTH],
        depth: usize,
        budget: &mut usize,
        allowed: &impl Fn(u32) -> bool,
    ) -> Option<u32> {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        if depth >= MAX_DEPTH || visited[..depth].contains(&recipe.id) || !allowed(recipe.id) {
            return None;
        }
        visited[depth] = recipe.id;
        let missing = recipe
            .inputs
            .iter()
            .filter(|input| {
                inventory
                    .stocks
                    .iter()
                    .find(|stock| stock.good == input.good)
                    .is_none_or(|stock| stock.quantity < input.quantity)
            })
            .min_by_key(|input| input.good);
        if let Some(input) = missing {
            for producer in self.rotated(self.producers.get(&input.good)?) {
                if *budget == 0 {
                    break;
                }
                let has_space = producer.outputs.iter().all(|output| {
                    inventory
                        .stocks
                        .iter()
                        .any(|stock| stock.good == output.good && output.quantity <= stock.capacity)
                });
                if !has_space {
                    *budget -= 1;
                    continue;
                }
                if let Some(next) =
                    self.choose(producer, inventory, visited, depth + 1, budget, allowed)
                {
                    return Some(next);
                }
            }
            None
        } else {
            Some(recipe.id)
        }
    }
    fn choose_step(
        &self,
        recipe: &Recipe,
        inventory: &Inventory,
        visited: &mut [u32; MAX_DEPTH],
        depth: usize,
        budget: &mut usize,
        constraints: &SiteConstraints<'_, impl Fn(u32) -> bool>,
    ) -> Option<Step> {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        if depth >= MAX_DEPTH || visited[..depth].contains(&recipe.id) {
            return None;
        }
        if !(constraints.allowed)(recipe.id) {
            return (!constraints.visited_sites.contains(&recipe.id))
                .then_some(Step::Site(recipe.id));
        }
        visited[depth] = recipe.id;
        let missing = recipe
            .inputs
            .iter()
            .filter(|input| {
                inventory
                    .stocks
                    .iter()
                    .find(|stock| stock.good == input.good)
                    .is_none_or(|stock| stock.quantity < input.quantity)
            })
            .min_by_key(|input| input.good);
        let Some(input) = missing else {
            return Some(Step::Work(recipe.id));
        };
        let mut fallback = None;
        for producer in self.rotated(self.producers.get(&input.good)?) {
            if *budget == 0 {
                break;
            }
            let has_space = producer.outputs.iter().all(|output| {
                inventory
                    .stocks
                    .iter()
                    .any(|stock| stock.good == output.good && output.quantity <= stock.capacity)
            });
            if !has_space {
                *budget -= 1;
                continue;
            }
            match self.choose_step(producer, inventory, visited, depth + 1, budget, constraints) {
                Some(step @ Step::Work(_)) => return Some(step),
                Some(step @ Step::Site(_)) if fallback.is_none() => fallback = Some(step),
                _ => {}
            }
        }
        fallback
    }
}
#[derive(Clone, Copy)]
enum Step {
    Work(u32),
    Site(u32),
}
struct SiteConstraints<'a, F> {
    allowed: &'a F,
    visited_sites: &'a [u32],
}
/// Replace goal-derived policies with the next producible ingredient job. Returns
/// each person's original goal recipe, so blocked plans back off the right goal.
pub fn plan(
    auto: &mut Autowork,
    base: &Autowork,
    production: &Production,
    people: &[PersonSnapshot],
) -> Vec<(u64, u32)> {
    plan_at(auto, base, production, people, 0)
}
/// Rotate bounded alternatives between reviews so later recipes can be considered.
pub fn plan_at(
    auto: &mut Autowork,
    base: &Autowork,
    production: &Production,
    people: &[PersonSnapshot],
    review: u64,
) -> Vec<(u64, u32)> {
    let explicit: HashSet<_> = base.policies.par_iter().map(|row| row.person).collect();
    let index = Index::new(production, review);
    let mut originals: Vec<_> = auto
        .policies
        .par_iter_mut()
        .filter_map(|policy| {
            if explicit.contains(&policy.person) || !policy.enabled {
                return None;
            }
            let original = policy.recipe;
            let mut visited = [0; MAX_DEPTH];
            let mut budget = MAX_VISITS;
            if let Some(region) = person_index(people, policy.person).and_then(|i| people[i].1)
                && let (Some(recipe), Some(inventory)) =
                    (index.recipes.get(&original), index.inventory.get(&region))
                && let Some(next) =
                    index.choose(recipe, inventory, &mut visited, 0, &mut budget, &|_| true)
            {
                policy.recipe = next;
            }
            Some((policy.person, original))
        })
        .collect();
    if originals.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        originals.par_sort_unstable_by_key(|row| row.0);
    }
    originals
}

/// A person's alternative physical supplies for an unfinished plan.
pub struct SupplyNeed {
    pub person: u64,
    pub region: u64,
    pub goods: Vec<u32>,
    pub visited_sites: Vec<u32>,
}

pub struct SupplyStep {
    pub person: u64,
    pub recipe: u32,
    pub needs_site: bool,
}

/// Select a bounded producible prerequisite for each person, without changing their final goal.
pub fn plan_supply(
    needs: &[SupplyNeed],
    production: &Production,
    review: u64,
    allowed: impl Fn(u64, u64, u32) -> bool + Sync,
) -> Vec<SupplyStep> {
    let index = Index::new(production, review);
    let mut selected: Vec<_> = needs
        .par_iter()
        .filter_map(|need| {
            let inventory = index.inventory.get(&need.region)?;
            let mut fallback = None;
            let mut budget = MAX_VISITS;
            for good in index.rotated(&need.goods) {
                if budget == 0 {
                    break;
                }
                budget -= 1;
                let Some(producers) = index.producers.get(good) else {
                    continue;
                };
                for recipe in index.rotated(producers) {
                    if budget == 0 {
                        break;
                    }
                    let has_space = recipe.outputs.iter().all(|output| {
                        inventory.stocks.iter().any(|stock| {
                            stock.good == output.good && output.quantity <= stock.capacity
                        })
                    });
                    if !has_space {
                        budget -= 1;
                        continue;
                    }
                    let mut visited = [0; MAX_DEPTH];
                    let site_allowed = |recipe| allowed(need.person, need.region, recipe);
                    let constraints = SiteConstraints {
                        allowed: &site_allowed,
                        visited_sites: &need.visited_sites,
                    };
                    match index.choose_step(
                        recipe,
                        inventory,
                        &mut visited,
                        0,
                        &mut budget,
                        &constraints,
                    ) {
                        Some(Step::Work(recipe)) => {
                            return Some(SupplyStep {
                                person: need.person,
                                recipe,
                                needs_site: false,
                            });
                        }
                        Some(Step::Site(recipe)) if fallback.is_none() => {
                            fallback = Some(SupplyStep {
                                person: need.person,
                                recipe,
                                needs_site: true,
                            })
                        }
                        _ => {}
                    }
                }
            }
            fallback
        })
        .collect();
    selected.par_sort_unstable_by_key(|row| row.person);
    selected
}
