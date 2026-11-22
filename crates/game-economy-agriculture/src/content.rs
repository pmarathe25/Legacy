//! Compile authored crop files into stable physical cultivation definitions.
use crate::Crop;
use game_production_model::{Amount, Good};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use toml::Value;

pub fn load(
    directory: &Path,
    goods: &[Good],
    tick_millis: u64,
) -> Result<BTreeMap<String, Crop>, String> {
    if tick_millis == 0 {
        return Err("invalid crop tick duration".into());
    }
    let names: BTreeMap<_, _> = goods.iter().map(|g| (g.name.as_str(), g.id)).collect();
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    files.retain(|path| path.extension().is_some_and(|ext| ext == "toml"));
    files.sort();
    let mut definitions = BTreeMap::new();
    for path in files {
        let source =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let document: Value =
            toml::from_str(&source).map_err(|e| format!("{}: {e}", path.display()))?;
        if document
            .get("id")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
            || document
                .get("version")
                .and_then(Value::as_integer)
                .is_none_or(|v| v <= 0)
        {
            return Err(format!("{}: invalid crop header", path.display()));
        }
        let rows = document
            .get("data")
            .and_then(|v| v.get("crops"))
            .and_then(Value::as_table)
            .ok_or("missing authored crops")?;
        for (name, row) in rows {
            if definitions.insert(name.clone(), row.clone()).is_some() {
                return Err(format!("duplicate crop {name}"));
            }
        }
    }
    let mut ids = HashSet::new();
    let mut implicit_id = 0u32;
    definitions
        .into_iter()
        .map(|(name, row)| {
            let positive = |key: &str| {
                row.get(key)
                    .and_then(Value::as_integer)
                    .and_then(|v| u64::try_from(v).ok())
                    .filter(|v| *v > 0)
                    .ok_or_else(|| format!("crop {name}: invalid {key}"))
            };
            let object = |key: &str| {
                row.get(key)
                    .and_then(Value::as_str)
                    .and_then(|value| names.get(value).copied())
                    .ok_or_else(|| format!("crop {name}: unknown {key}"))
            };
            let id = if row.get("runtime_id").is_some() {
                u32::try_from(positive("runtime_id")?).map_err(|_| "crop identity overflow")?
            } else {
                implicit_id = implicit_id.checked_add(1).ok_or("crop count overflow")?;
                implicit_id
            };
            if !ids.insert(id) {
                return Err(format!("duplicate crop identity {id}"));
            }
            let seed = object("seed_object")?;
            let output = object("output_object")?;
            let mut byproducts = Vec::new();
            if let Some(byproduct_map) = row.get("byproducts") {
                for (good_name, quantity) in byproduct_map
                    .as_table()
                    .ok_or_else(|| format!("crop {name}: invalid byproducts"))?
                {
                    let good = names
                        .get(good_name.as_str())
                        .copied()
                        .filter(|id| *id != seed && *id != output)
                        .ok_or_else(|| format!("crop {name}: invalid byproduct {good_name}"))?;
                    let quantity = quantity
                        .as_integer()
                        .and_then(|value| u64::try_from(value).ok())
                        .filter(|&value| value > 0)
                        .ok_or_else(|| format!("crop {name}: invalid byproduct quantity"))?;
                    byproducts.push(Amount { good, quantity });
                }
                byproducts.sort_unstable_by_key(|amount| amount.good);
            }
            let growth_ticks = u32::try_from(positive("growth")?.div_ceil(tick_millis))
                .map_err(|_| "crop duration overflow")?;
            let crop = Crop {
                id,
                seed,
                seed_quantity: positive("seed_input")?,
                output,
                output_quantity: positive("yield_units")?,
                byproducts,
                growth_ticks,
            };
            Ok((name, crop))
        })
        .collect()
}
