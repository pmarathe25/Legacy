//! Translate authored object and workshop files into numeric production catalogs.
use game_production::{
    Amount, FoodProfile, Good, MaturationProfile, NUTRIENT_NAMES, Production, Recipe,
    RecipeCategory, Stock,
};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use toml::Value;
mod maturation;
mod packs;
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
            if !NUTRIENT_NAMES.contains(&key.as_str()) {
                return Err(format!("unknown nutrient: {key}"));
            }
            let value = value
                .as_integer()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or("invalid nutrient quantity")?;
            Ok((key.clone(), value))
        })
        .collect::<Result<_, String>>()?;
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
    work_unit_millis: u64,
    capacity: u64,
    food_relief_divisor: u32,
) -> Result<(), String> {
    if work_unit_millis == 0 || capacity == 0 || food_relief_divisor == 0 {
        return Err("invalid catalog conversion rules".into());
    }
    let (objects, recipes) = packs::load(objects, recipes)?;
    let mut next = production.clone();
    let mut ids = BTreeMap::new();
    let mut maturing = Vec::new();
    let mut used: HashSet<u32> = next.goods.iter().map(|good| good.id).collect();
    for good in &next.goods {
        ids.insert(good.name.to_ascii_lowercase().replace(' ', "-"), good.id);
    }
    let mut id = next.goods.iter().map(|good| good.id).max().unwrap_or(0);
    for packs::Definition {
        key,
        runtime_id,
        row: object,
        ..
    } in objects
    {
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
        let maturation =
            maturation::read(&object).map_err(|error| format!("object {key}: {error}"))?;
        if let Some(&existing) = ids.get(&key) {
            if runtime_id.is_some() {
                return Err(format!("namespaced object {key} already exists"));
            }
            let good = next
                .goods
                .iter_mut()
                .find(|good| good.id == existing)
                .expect("known good");
            if good.food.is_none() {
                good.food = food;
            }
            if let Some(profile) = maturation {
                maturing.push((existing, profile));
            }
            continue;
        }
        let assigned = if let Some(runtime_id) = runtime_id {
            runtime_id
        } else {
            id = id.checked_add(1).ok_or("good identity overflow")?;
            id
        };
        if !used.insert(assigned) {
            return Err(format!("object {key}: duplicate good identity {assigned}"));
        }
        ids.insert(key.clone(), assigned);
        if let Some(profile) = maturation {
            maturing.push((assigned, profile));
        }
        next.goods.push(Good {
            id: assigned,
            name: key,
            food,
            maturation: None,
        });
    }
    let food_ids: HashSet<_> = next
        .goods
        .iter()
        .filter(|good| good.food.is_some())
        .map(|good| good.id)
        .collect();
    for (id, profile) in maturing {
        let product = *ids
            .get(&profile.product)
            .ok_or_else(|| format!("unknown maturation product {}", profile.product))?;
        let good = next
            .goods
            .iter_mut()
            .find(|good| good.id == id)
            .expect("known preparation");
        if good.food.is_some() || !food_ids.contains(&product) {
            return Err(format!(
                "object {} must mature from an inedible preparation into food",
                good.name
            ));
        }
        good.maturation = Some(MaturationProfile {
            product,
            days: profile.days,
            min_celsius: profile.min_celsius,
            max_celsius: profile.max_celsius,
        });
    }
    let mut recipe_id = next
        .recipes
        .iter()
        .map(|recipe| recipe.id)
        .max()
        .unwrap_or(0);
    let mut required = BTreeMap::<u32, u64>::new();
    let mut recipe_ids: HashSet<_> = next.recipes.iter().map(|recipe| recipe.id).collect();
    let mut recipe_names: HashSet<_> = next
        .recipes
        .iter()
        .map(|recipe| recipe.name.clone())
        .collect();
    for packs::Definition {
        key,
        namespace,
        runtime_id,
        row,
    } in recipes
    {
        if !recipe_names.insert(key.clone()) {
            return Err(format!("recipe {key} already exists"));
        }
        let resolve = |name: &str| {
            namespace
                .as_ref()
                .filter(|_| !name.contains(':'))
                .and_then(|namespace| ids.get(&format!("{namespace}:{name}")))
                .or_else(|| ids.get(name))
                .copied()
        };
        let inputs = row
            .get("inputs")
            .and_then(Value::as_table)
            .filter(|rows| !rows.is_empty())
            .ok_or_else(|| format!("recipe {key}: invalid inputs"))?;
        let amounts = inputs
            .iter()
            .map(|(good, value)| {
                let id =
                    resolve(good).ok_or_else(|| format!("recipe {key}: unknown input {good}"))?;
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
        let outputs =
            recipe_outputs(&row, &resolve).map_err(|error| format!("recipe {key}: {error}"))?;
        let duration =
            positive(&row, "duration").map_err(|error| format!("recipe {key}: {error}"))?;
        let ticks = duration.div_ceil(work_unit_millis);
        let ticks = u32::try_from(ticks).map_err(|_| format!("recipe {key}: duration overflow"))?;
        let assigned = if let Some(runtime_id) = runtime_id {
            runtime_id
        } else {
            recipe_id = recipe_id.checked_add(1).ok_or("recipe identity overflow")?;
            recipe_id
        };
        if !recipe_ids.insert(assigned) {
            return Err(format!(
                "recipe {key}: duplicate recipe identity {assigned}"
            ));
        }
        if let Some(category) = row.get("skill") {
            let category = category
                .as_str()
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| format!("recipe {key}: invalid skill category"))?;
            next.recipe_categories.push(RecipeCategory {
                recipe: assigned,
                category: category.to_owned(),
            });
        }
        for output in &outputs {
            required
                .entry(output.good)
                .and_modify(|value| *value = (*value).max(output.quantity))
                .or_insert(output.quantity);
        }
        next.recipes.push(Recipe {
            id: assigned,
            name: key,
            ticks,
            inputs: amounts,
            outputs,
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
    next.goods.sort_unstable_by_key(|row| row.id);
    next.recipes.sort_unstable_by_key(|row| row.id);
    next.recipe_categories
        .sort_unstable_by_key(|row| row.recipe);
    *production = next;
    Ok(())
}

fn recipe_outputs(
    row: &Value,
    resolve: &impl Fn(&str) -> Option<u32>,
) -> Result<Vec<Amount>, String> {
    let amount = |name: &str, quantity| -> Result<Amount, String> {
        let good = resolve(name).ok_or_else(|| format!("unknown output {name}"))?;
        Ok(Amount { good, quantity })
    };
    let Some(outputs) = row.get("outputs") else {
        let name = row
            .get("output")
            .and_then(Value::as_str)
            .ok_or("invalid output")?;
        return Ok(vec![amount(name, positive(row, "quantity")?)?]);
    };
    if row.get("output").is_some() || row.get("quantity").is_some() {
        return Err("use either outputs or output and quantity".into());
    }
    let table = outputs
        .as_table()
        .filter(|rows| !rows.is_empty())
        .ok_or("invalid outputs")?;
    table
        .keys()
        .map(|name| amount(name, positive(outputs, name)?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_packs_atomically_and_preserves_explicit_catalog_ids() {
        use std::fs;

        let root =
            std::env::temp_dir().join(format!("legacy-content-packs-{}", std::process::id()));
        let objects = root.join("objects");
        let recipes = root.join("recipes");
        fs::create_dir_all(&objects).unwrap();
        fs::create_dir_all(&recipes).unwrap();
        let base = objects.join("base.toml");
        let first = objects.join("orchard.toml");
        let other = objects.join("extra.toml");
        let crafts = recipes.join("crafts.toml");
        fs::write(
            &base,
            "id='objects/base'\nversion=1\n[data.objects.grain]\nid=1\nname='Grain'\n",
        )
        .unwrap();
        fs::write(&first, "id='objects/orchard'\nversion=1\nnamespace='orchard'\n[data.objects.pomace]\nid=1\nruntime_id=44\nname='Pomace'\n").unwrap();
        let craft = "id='workshop/orchard'\nversion=1\nnamespace='orchard'\nrequires={'objects/orchard'=1}\n[data.recipes.grind]\nruntime_id=10\ninputs={grain=1}\noutput='pomace'\nquantity=1\nduration=1\n";
        fs::write(&crafts, craft).unwrap();
        let empty = Production {
            max_work_hunger: 3,
            goods: vec![],
            recipes: vec![],
            recipe_categories: vec![],
            inventories: vec![],
            jobs: vec![],
            events: vec![],
        };
        let compile = || {
            let mut result = empty.clone();
            augment(&mut result, &objects, &recipes, 1, 10, 1).unwrap();
            result
        };
        let first = compile();
        assert_eq!(first.goods[1].name, "orchard:pomace");
        assert_eq!(first.goods[1].id, 44);
        assert_eq!(first.recipes[0].name, "orchard:grind");
        assert_eq!(first.recipes[0].id, 10);
        assert_eq!(first.recipes[0].outputs[0].good, 44);

        fs::write(&other, "id='objects/extra'\nversion=1\nnamespace='added'\n[data.objects.ash]\nid=1\nruntime_id=19\nname='Ash'\n").unwrap();
        let mut extended = compile();
        extended.goods.retain(|good| good.id != 19);
        assert_eq!(extended, first);

        let mut unchanged = empty.clone();
        fs::write(&crafts, craft.replace("objects/orchard", "objects/missing")).unwrap();
        assert!(
            augment(&mut unchanged, &objects, &recipes, 1, 10, 1)
                .unwrap_err()
                .contains("requires missing pack")
        );
        assert_eq!(unchanged, empty);
        fs::write(&crafts, craft.replace("version=1", "version=2")).unwrap();
        assert!(
            augment(&mut unchanged, &objects, &recipes, 1, 10, 1)
                .unwrap_err()
                .contains("unsupported content pack version 2")
        );
        assert_eq!(unchanged, empty);

        for file in [base, objects.join("orchard.toml"), other, crafts] {
            fs::remove_file(file).unwrap();
        }
        fs::remove_dir(&objects).unwrap();
        fs::remove_dir(&recipes).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn rejects_authored_nutrients_the_simulation_cannot_use() {
        let object: Value =
            toml::from_str("[food]\nportion = 1\nrelief = 1\nnutrients = { enregy = 12 }").unwrap();
        assert_eq!(
            food_profile(&object, 1),
            Err("unknown nutrient: enregy".into())
        );
    }

    #[test]
    fn resolves_all_outputs_and_rejects_an_ambiguous_recipe() {
        let ids = BTreeMap::from([("bread".to_owned(), 1), ("bran".to_owned(), 2)]);
        let resolve = |name: &str| ids.get(name).copied();
        let row = |source| toml::from_str::<Value>(source).unwrap();
        assert_eq!(
            recipe_outputs(&row("outputs = { bran = 1, bread = 3 }"), &resolve).unwrap(),
            [
                Amount {
                    good: 2,
                    quantity: 1
                },
                Amount {
                    good: 1,
                    quantity: 3
                }
            ],
        );
        assert!(
            recipe_outputs(
                &row("outputs = { bran = 1 }\noutput = 'bread'\nquantity = 3"),
                &resolve
            )
            .is_err()
        );
        assert!(recipe_outputs(&row("outputs = { missing = 1 }"), &resolve).is_err());
    }
}
