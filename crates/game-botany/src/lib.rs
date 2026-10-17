//! Wild plant survival, seed banks, reproduction, and finite standing produce.
mod growth;
mod stand;
use game_core::*;
use game_cultivars::{Lot, Strain};
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
pub use stand::{Plant, Seed, Stand};
use std::collections::{BTreeMap, VecDeque};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Botany {
    pub enabled: bool,
    pub stands: Records<u64, Stand>,
    pub births: u64,
    pub deaths: u64,
}
impl Resource for Botany {
    const KEY: &'static str = "botany.stands";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "botany.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Changed {
    pub patch: u64,
    pub living: u64,
    pub seeds: u64,
    pub births: u64,
    pub deaths: u64,
    pub generation: u32,
    pub mean_cold: i64,
}
impl Event for Changed {
    const KEY: &'static str = "botany.changed";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("botany", 1)?;
    b.resource::<Botany>()?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Botany>()?.enabled = true;
        let patches: Vec<_> = tx
            .get::<game_ecology::Ecology>()?
            .patches
            .iter()
            .map(|(id, p)| (*id, p.clone()))
            .collect();
        for (patch, plant) in patches {
            let e = tx.get::<game_ecology::Ecology>()?;
            let r = e.renewal.get(&patch);
            let request = game_ecology::PlantGrowth {
                patch,
                plant,
                reserved: 0,
                minimum_warmth: r.map(|r| r.minimum_warmth),
                minimum_rain: r.map_or(0, |r| r.minimum_rain),
            };
            stand::ensure(tx, &request)?;
        }
        Ok(())
    })?;
    b.request::<game_ecology::PlantGrowth>(growth::advance)?;
    b.request::<game_ecology::PlantOrigin>(|tx, r| {
        if !tx.get::<Botany>()?.enabled {
            return Ok(vec![]);
        }
        if !tx.get::<Botany>()?.stands.contains_key(&r.patch) {
            let ecology = tx.get::<game_ecology::Ecology>()?;
            let plant = ecology
                .patches
                .get(&r.patch)
                .cloned()
                .ok_or_else(|| Error::Missing("patch".into()))?;
            let rule = ecology.renewal.get(&r.patch);
            let request = game_ecology::PlantGrowth {
                patch: r.patch,
                plant,
                reserved: 0,
                minimum_warmth: rule.map(|r| r.minimum_warmth),
                minimum_rain: rule.map_or(0, |r| r.minimum_rain),
            };
            if !stand::ensure(tx, &request)? {
                return Ok(vec![]);
            }
        }
        tx.edit::<Botany>()?
            .stands
            .get_mut(&r.patch)
            .unwrap()
            .take(r.quantity)
    })?;
    b.validate_save(|h| {
        for (id, s) in &h.get::<Botany>()?.stands {
            require(
                s.plants.len() <= 32
                    && s.plants.iter().all(|p| p.id > 0 && p.id <= s.sequence)
                    && s.plants
                        .iter()
                        .map(|p| p.id)
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == s.plants.len()
                    && s.fruit
                        .values()
                        .try_fold(0u64, |n, l| n.checked_add(l.quantity))
                        == h.get::<game_ecology::Ecology>()?
                            .patches
                            .get(id)
                            .map(|p| p.stock),
                "wild plant stock mismatch",
            )?;
        }
        Ok(())
    });
    Ok(())
}
