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
}
impl<'a> Index<'a> {
    fn new(production: &'a Production) -> Self {
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
        }
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
        if *budget == 0
            || depth >= MAX_DEPTH
            || visited[..depth].contains(&recipe.id)
            || !allowed(recipe.id)
        {
            return None;
        }
        *budget -= 1;
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
            self.producers.get(&input.good).and_then(|recipes| {
                recipes.iter().find_map(|producer| {
                    let has_space = producer.outputs.iter().all(|output| {
                        inventory.stocks.iter().any(|stock| {
                            stock.good == output.good && output.quantity <= stock.capacity
                        })
                    });
                    has_space
                        .then(|| {
                            self.choose(producer, inventory, visited, depth + 1, budget, allowed)
                        })
                        .flatten()
                })
            })
        } else {
            Some(recipe.id)
        }
    }
}
/// Replace goal-derived policies with the next producible ingredient job. Returns
/// each person's original goal recipe, so blocked plans back off the right goal.
pub fn plan(
    auto: &mut Autowork,
    base: &Autowork,
    production: &Production,
    people: &[PersonSnapshot],
) -> Vec<(u64, u32)> {
    let explicit: HashSet<_> = base.policies.iter().map(|row| row.person).collect();
    let index = Index::new(production);
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
}

/// Select a bounded producible prerequisite for each person, without changing their final goal.
pub fn plan_supply(
    needs: &[SupplyNeed],
    production: &Production,
    allowed: impl Fn(u64, u32) -> bool + Sync,
) -> Vec<(u64, u32)> {
    let index = Index::new(production);
    let mut selected: Vec<_> = needs
        .par_iter()
        .filter_map(|need| {
            let inventory = index.inventory.get(&need.region)?;
            need.goods.iter().find_map(|good| {
                index.producers.get(good)?.iter().find_map(|recipe| {
                    let has_space = recipe.outputs.iter().all(|output| {
                        inventory.stocks.iter().any(|stock| {
                            stock.good == output.good && output.quantity <= stock.capacity
                        })
                    });
                    if !has_space {
                        return None;
                    }
                    let mut visited = [0; MAX_DEPTH];
                    let mut budget = MAX_VISITS;
                    index
                        .choose(recipe, inventory, &mut visited, 0, &mut budget, &|recipe| {
                            allowed(need.region, recipe)
                        })
                        .map(|recipe| (need.person, recipe))
                })
            })
        })
        .collect();
    selected.par_sort_unstable_by_key(|row| row.0);
    selected
}
