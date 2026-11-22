//! Provenance of the authored packs from which a saved catalog was compiled.
use crate::RecipeCategory;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub id: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogManifest {
    pub work_unit_millis: u64,
    pub food_relief_divisor: u32,
    pub packs: Vec<Pack>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipe_categories: Vec<RecipeCategory>,
}

impl CatalogManifest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.work_unit_millis == 0
            || self.food_relief_divisor == 0
            || self.packs.is_empty()
            || self
                .recipe_categories
                .windows(2)
                .any(|pair| pair[0].recipe >= pair[1].recipe)
            || self
                .recipe_categories
                .iter()
                .any(|row| row.recipe == 0 || row.category.trim().is_empty())
            || self.packs.windows(2).any(|pair| pair[0].id >= pair[1].id)
            || self.packs.iter().any(|pack| {
                pack.id.trim().is_empty()
                    || pack.version == 0
                    || pack.requires.windows(2).any(|pair| pair[0] >= pair[1])
                    || pack.requires.iter().any(|id| {
                        id == &pack.id
                            || self
                                .packs
                                .binary_search_by(|candidate| candidate.id.cmp(id))
                                .is_err()
                    })
            })
        {
            return Err("invalid catalog manifest");
        }
        Ok(())
    }

    pub fn includes(&self, previous: &Self) -> Result<(), String> {
        if self.work_unit_millis != previous.work_unit_millis
            || self.food_relief_divisor != previous.food_relief_divisor
        {
            return Err("catalog conversion settings changed".into());
        }
        for old in &previous.packs {
            let actual = self
                .packs
                .binary_search_by(|pack| pack.id.cmp(&old.id))
                .ok()
                .map(|index| &self.packs[index]);
            if actual != Some(old) {
                return Err(format!("catalog pack {} changed or is missing", old.id));
            }
        }
        Ok(())
    }
}
