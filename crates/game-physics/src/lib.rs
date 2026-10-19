//! Reusable quasi-static force/travel budgets and material component definitions.
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod engine;
pub use engine::*;
mod catalog;
pub use catalog::{load, needs, validate};
mod transmissions;
pub use transmissions::transmissions;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Material {
    pub span: u32,
    pub limit: u64,
    pub efficiency: u16,
    #[serde(default)]
    pub supports: BTreeMap<GoodId, u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub force: u64,
    pub travel: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub materials: BTreeMap<GoodId, Material>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub good: GoodId,
    pub input: Option<usize>,
    pub drive: u32,
    pub load: u32,
}

/// Stable identity of a visible arrangement, independent of a particular apparatus instance.
pub fn design_key(parts: &[Part]) -> u64 {
    let mut hash = 14695981039346656037u64;
    for p in parts {
        for n in [
            p.good.0,
            p.input.map_or(0, |i| i as u64 + 1),
            u64::from(p.drive),
            u64::from(p.load),
        ] {
            for b in n.to_le_bytes() {
                hash = (hash ^ u64::from(b)).wrapping_mul(1099511628211);
            }
        }
    }
    hash
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        require(
            self.materials.values().all(|m| {
                m.span >= 2
                    && m.span <= 1000
                    && m.limit > 0
                    && m.efficiency <= 1000
                    && m.supports.values().all(|n| *n > 0 && *n <= 32)
            }),
            "invalid mechanical materials",
        )
    }
}

impl Config {
    pub fn validate_objects(&self, objects: &game_objects::Objects) -> Result<()> {
        self.validate()?;
        for (good, m) in &self.materials {
            objects.by_id(*good)?;
            for good in m.supports.keys() {
                objects.by_id(*good)?;
            }
        }
        Ok(())
    }
}
