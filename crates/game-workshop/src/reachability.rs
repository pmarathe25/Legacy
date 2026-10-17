use super::*;
use std::{cell::RefCell, collections::BTreeSet, sync::Arc};
pub type RecipeChoices = BTreeMap<GoodId, Vec<(String, Recipe)>>;
#[derive(Default)]
struct Cache {
    version: Option<game_storage::Version<String, Arc<Recipe>>>,
    choices: Arc<RecipeChoices>,
    reachable: BTreeMap<Vec<GoodId>, Arc<BTreeSet<GoodId>>>,
}
thread_local! {static CACHE:RefCell<Cache>=RefCell::new(Cache::default());}
fn cached<T>(workshop: &Workshop, f: impl FnOnce(&mut Cache) -> T) -> T {
    CACHE.with(|slot| {
        let mut cache = slot.borrow_mut();
        if cache
            .version
            .as_ref()
            .is_none_or(|v| !workshop.recipes.matches(v))
        {
            let mut choices: RecipeChoices = BTreeMap::new();
            for (key, recipe) in &workshop.recipes {
                choices
                    .entry(recipe.output)
                    .or_default()
                    .push((key.clone(), recipe.clone()));
            }
            *cache = Cache {
                version: Some(workshop.recipes.version()),
                choices: Arc::new(choices),
                reachable: BTreeMap::new(),
            };
        }
        f(&mut cache)
    })
}
impl Workshop {
    /// Deterministic recipe order, rebuilt only when actual definitions change.
    pub fn choices(&self) -> Arc<RecipeChoices> {
        cached(self, |c| c.choices.clone())
    }
    /// Conservative dependency reachability; quantities and physical capacity remain planner checks.
    pub fn reachable(&self, inputs: impl IntoIterator<Item = GoodId>) -> Arc<BTreeSet<GoodId>> {
        let mut available: BTreeSet<_> = inputs.into_iter().collect();
        let key: Vec<_> = available.iter().copied().collect();
        cached(self, |cache| {
            if let Some(found) = cache.reachable.get(&key) {
                return found.clone();
            }
            loop {
                let before = available.len();
                for choices in cache.choices.values() {
                    for (_, recipe) in choices {
                        if recipe.inputs.keys().all(|g| available.contains(g)) {
                            available.insert(recipe.output);
                        }
                    }
                }
                if before == available.len() {
                    break;
                }
            }
            let result = Arc::new(available);
            if cache.reachable.len() >= 1024 {
                cache.reachable.clear();
            }
            cache.reachable.insert(key, result.clone());
            result
        })
    }
}
