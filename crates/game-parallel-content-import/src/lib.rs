//! Translate authored object and workshop files into numeric production catalogs.
use game_parallel_production::{
    Amount, FoodProfile, Good, Production, Recipe, RecipeCategory, Stock,
};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use toml::Value;

fn entries(directory: &Path, kind: &str) -> Result<BTreeMap<String, Value>, String> {
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .map(|entry| entry.map(|e| e.path()).map_err(|error| error.to_string()))
        .collect::<Result<_, _>>()?;
    paths.retain(|path| path.extension().is_some_and(|ext| ext == "toml"));
    paths.sort();
    let mut rows = BTreeMap::new();
    for path in paths {
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let document: Value =
            toml::from_str(&source).map_err(|error| format!("{}: {error}", path.display()))?;
        let header = document.as_table().ok_or("invalid content document")?;
        if header
            .get("id")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
            || header
                .get("version")
                .and_then(Value::as_integer)
                .is_none_or(|version| version <= 0)
        {
            return Err(format!("{}: invalid content header", path.display()));
        }
        let table = header
            .get("data")
            .and_then(|v| v.get(kind))
            .and_then(Value::as_table)
            .ok_or_else(|| format!("{}: missing {kind}", path.display()))?;
        for (key, value) in table {
            if rows.insert(key.clone(), value.clone()).is_some() {
                return Err(format!("duplicate {kind} key: {key}"));
            }
        }
    }
    if rows.is_empty() {
        return Err(format!("no {kind} in {}", directory.display()));
    }
    Ok(rows)
}
fn positive(row: &Value, key: &str) -> Result<u64, String> {
    row.get(key)
        .and_then(Value::as_integer)
        .and_then(|value| u64::try_from(value).ok())
        .filter(|&value| value > 0)
        .ok_or_else(|| format!("invalid {key}"))
}

fn food_profile(object: &Value, divisor: u32) -> Result<Option<FoodProfile>, String> {
    let Some(food) = object.get("food") else {
        return Ok(None);
    };
    let portion = positive(food, "portion")?;
    let relief = positive(food, "relief")?.div_ceil(u64::from(divisor));
    let hunger_relief = u32::try_from(relief).map_err(|_| "food relief overflow")?;
    let nutrients = food
        .get("nutrients")
        .and_then(Value::as_table)
        .ok_or("invalid nutrients")?
        .iter()
        .map(|(key, value)| {
            let value = value
                .as_integer()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or("invalid nutrient quantity")?;
            Ok((key.clone(), value))
        })
        .collect::<Result<_, &str>>()?;
    let health_effect = food
        .get("effects")
        .and_then(|effects| effects.get("health"))
        .and_then(Value::as_integer)
        .map(i32::try_from)
        .transpose()
        .map_err(|_| "invalid food health effect")?
        .unwrap_or(0);
    let shelf_days = food
        .get("shelf_days")
        .and_then(Value::as_integer)
        .map(u32::try_from)
        .transpose()
        .map_err(|_| "invalid shelf life")?;
    Ok(Some(FoodProfile {
        portion,
        hunger_relief,
        nutrients,
        health_effect,
        shelf_days,
    }))
}

/// Validate everything in a private copy before publishing any catalog changes.
pub fn augment(
    production: &mut Production,
    objects: &Path,
    recipes: &Path,
    tick_millis: u64,
    capacity: u64,
    food_relief_divisor: u32,
) -> Result<(), String> {
    if tick_millis == 0 || capacity == 0 || food_relief_divisor == 0 {
        return Err("invalid catalog conversion rules".into());
    }
    let objects = entries(objects, "objects")?;
    let recipes = entries(recipes, "recipes")?;
    let mut next = production.clone();
    let mut ids = BTreeMap::new();
    let mut used: HashSet<u32> = next.goods.iter().map(|good| good.id).collect();
    for good in &next.goods {
        ids.insert(good.name.to_ascii_lowercase().replace(' ', "-"), good.id);
    }
    let mut id = next.goods.iter().map(|good| good.id).max().unwrap_or(0);
    for (key, object) in objects {
        positive(&object, "id").map_err(|error| format!("object {key}: {error}"))?;
        if object
            .get("name")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(format!("object {key}: invalid name"));
        }
        let food = food_profile(&object, food_relief_divisor)
            .map_err(|error| format!("object {key}: {error}"))?;
        if let Some(&existing) = ids.get(&key) {
            let good = next
                .goods
                .iter_mut()
                .find(|good| good.id == existing)
                .expect("known good");
            if good.food.is_none() {
                good.food = food;
            }
            continue;
        }
        id = id.checked_add(1).ok_or("good identity overflow")?;
        if !used.insert(id) {
            return Err("duplicate good identity".into());
        }
        ids.insert(key.clone(), id);
        next.goods.push(Good {
            id,
            name: key,
            food,
        });
    }
    let mut recipe_id = next
        .recipes
        .iter()
        .map(|recipe| recipe.id)
        .max()
        .unwrap_or(0);
    let mut required = BTreeMap::<u32, u64>::new();
    for (key, row) in recipes {
        let inputs = row
            .get("inputs")
            .and_then(Value::as_table)
            .filter(|rows| !rows.is_empty())
            .ok_or_else(|| format!("recipe {key}: invalid inputs"))?;
        let amounts = inputs
            .iter()
            .map(|(good, value)| {
                let id = *ids
                    .get(good)
                    .ok_or_else(|| format!("recipe {key}: unknown input {good}"))?;
                let quantity = value
                    .as_integer()
                    .and_then(|v| u64::try_from(v).ok())
                    .filter(|&v| v > 0)
                    .ok_or_else(|| format!("recipe {key}: invalid input quantity"))?;
                Ok(Amount { good: id, quantity })
            })
            .collect::<Result<Vec<_>, String>>()?;
        for input in &amounts {
            required
                .entry(input.good)
                .and_modify(|value| *value = (*value).max(input.quantity))
                .or_insert(input.quantity);
        }
        let output = row
            .get("output")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("recipe {key}: invalid output"))?;
        let good = *ids
            .get(output)
            .ok_or_else(|| format!("recipe {key}: unknown output {output}"))?;
        let quantity =
            positive(&row, "quantity").map_err(|error| format!("recipe {key}: {error}"))?;
        let duration =
            positive(&row, "duration").map_err(|error| format!("recipe {key}: {error}"))?;
        let ticks = duration.div_ceil(tick_millis);
        let ticks = u32::try_from(ticks).map_err(|_| format!("recipe {key}: duration overflow"))?;
        recipe_id = recipe_id.checked_add(1).ok_or("recipe identity overflow")?;
        if let Some(category) = row.get("skill") {
            let category = category
                .as_str()
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| format!("recipe {key}: invalid skill category"))?;
            next.recipe_categories.push(RecipeCategory {
                recipe: recipe_id,
                category: category.to_owned(),
            });
        }
        required
            .entry(good)
            .and_modify(|value| *value = (*value).max(quantity))
            .or_insert(quantity);
        next.recipes.push(Recipe {
            id: recipe_id,
            name: key,
            ticks,
            inputs: amounts,
            outputs: vec![Amount { good, quantity }],
        });
    }
    for inventory in &mut next.inventories {
        let mut existing: HashSet<_> = inventory.stocks.iter().map(|stock| stock.good).collect();
        for stock in &mut inventory.stocks {
            if let Some(&minimum) = required.get(&stock.good) {
                stock.capacity = stock.capacity.max(minimum);
            }
        }
        for good in &next.goods {
            if existing.insert(good.id) {
                inventory.stocks.push(Stock {
                    good: good.id,
                    quantity: 0,
                    capacity: capacity.max(required.get(&good.id).copied().unwrap_or(0)),
                });
            }
        }
    }
    *production = next;
    Ok(())
}
