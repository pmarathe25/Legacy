//! Derive shared skill identities from data-authored recipe categories.
use game_parallel_production_model::RecipeCategory;
use game_parallel_skills::{Alias, Rule, Skills};
use std::collections::{BTreeMap, HashSet};

/// Build in a private copy so invalid catalog data cannot partially change skills.
pub fn configure(skills: &mut Skills, categories: &[RecipeCategory]) -> Result<(), String> {
    let Some(training) = skills.catalog_training.as_ref() else {
        return Ok(());
    };
    if training.completions_per_level == 0
        || training.max_level == 0
        || training.work_per_level == 0
    {
        return Err("invalid catalog skill training rules".into());
    }
    let mut groups = BTreeMap::<&str, Vec<u32>>::new();
    let mut seen = HashSet::new();
    for row in categories {
        if row.category.trim().is_empty() || !seen.insert(row.recipe) {
            return Err("invalid recipe skill category".into());
        }
        groups.entry(&row.category).or_default().push(row.recipe);
    }
    let mut rules = skills.rules.clone();
    let mut aliases = skills.aliases.clone();
    let mut rule_ids: HashSet<_> = rules.iter().map(|rule| rule.recipe).collect();
    let mut aliased: HashSet<_> = aliases.iter().map(|alias| alias.recipe).collect();
    for recipes in groups.values_mut() {
        recipes.sort_unstable();
        let anchor = recipes
            .iter()
            .copied()
            .find(|id| rule_ids.contains(id))
            .or_else(|| recipes.iter().copied().find(|id| !aliased.contains(id)));
        let Some(anchor) = anchor else {
            continue;
        };
        if rule_ids.insert(anchor) {
            rules.push(Rule {
                recipe: anchor,
                completions_per_level: training.completions_per_level,
                max_level: training.max_level,
                work_per_level: training.work_per_level,
            });
        }
        for &recipe in recipes.iter() {
            if recipe != anchor && !rule_ids.contains(&recipe) && aliased.insert(recipe) {
                aliases.push(Alias {
                    recipe,
                    canonical: anchor,
                });
            }
        }
    }
    rules.sort_unstable_by_key(|rule| rule.recipe);
    aliases.sort_unstable_by_key(|alias| alias.recipe);
    skills.rules = rules;
    skills.aliases = aliases;
    Ok(())
}
