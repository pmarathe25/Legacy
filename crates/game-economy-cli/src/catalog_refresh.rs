//! Add authored goods and recipes to a running or restored scenario.
use game_economy::core_interval::CoreHost;
use game_production::Production;
use std::path::Path;
use std::sync::Arc;

/// Compile the extended catalog privately and reopen the same world with it.
pub fn extend_catalog(host: &CoreHost, content: &Path) -> Result<CoreHost, String> {
    let mut snapshot = host.snapshot();
    let physical = &mut snapshot.core.physical;
    let existing = physical
        .catalog_manifest
        .as_ref()
        .ok_or("catalog additions require a save with its original content conversion settings")?;
    let mut production = Production {
        max_work_hunger: snapshot.core.rules.max_work_hunger,
        goods: Vec::new(),
        recipes: Vec::new(),
        recipe_categories: Vec::new(),
        inventories: Vec::new(),
        jobs: Vec::new(),
        events: Vec::new(),
    };
    let manifest = game_content_import::augment_catalog(
        &mut production,
        &content.join("objects"),
        &content.join("workshop"),
        existing.work_unit_millis,
        existing.food_relief_divisor,
    )?;
    manifest.includes(existing)?;
    for old in &physical.goods {
        let actual = production
            .goods
            .binary_search_by_key(&old.id, |good| good.id)
            .ok()
            .map(|index| &production.goods[index]);
        if actual != Some(old) {
            return Err(format!("catalog changed or removed good {}", old.name));
        }
    }
    for old in &physical.recipes {
        let actual = production
            .recipes
            .binary_search_by_key(&old.id, |recipe| recipe.id)
            .ok()
            .map(|index| &production.recipes[index]);
        if actual != Some(old) {
            return Err(format!("catalog changed or removed recipe {}", old.name));
        }
        let category = |categories: &[game_production::RecipeCategory]| {
            categories
                .binary_search_by_key(&old.id, |row| row.recipe)
                .ok()
        };
        let previous_category =
            category(&existing.recipe_categories).map(|index| &existing.recipe_categories[index]);
        let next_category =
            category(&manifest.recipe_categories).map(|index| &manifest.recipe_categories[index]);
        if previous_category != next_category {
            return Err(format!(
                "catalog changed skill category of recipe {}",
                old.name
            ));
        }
    }
    if let Some(skills) = &mut snapshot.skills {
        game_skill_catalog::configure(&mut skills.owner, &production.recipe_categories)?;
    }
    physical.goods = production.goods;
    physical.recipes = production.recipes;
    physical.catalog_manifest = Some(Arc::new(manifest));
    CoreHost::resume(snapshot)
}
