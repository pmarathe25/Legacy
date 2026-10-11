//! Production definitions are content; concrete sites bind real stock entities.
use super::*;
use game_content::{Document, Stamp};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecipeId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub input_good: Good,
    pub input_quantity: Quantity,
    pub output_good: Good,
    pub output_quantity: Quantity,
    pub duration: SimDuration,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub default_recipe: RecipeId,
    pub recipes: BTreeMap<RecipeId, Recipe>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definitions {
    pub catalog: Catalog,
    pub content: Stamp,
}
pub struct SiteBindings<'a> {
    pub id: SiteId,
    pub area: AreaId,
    pub input: &'a Inventory,
    pub tool: &'a Inventory,
    pub output: &'a Inventory,
    pub access: BTreeSet<PersonId>,
}
impl RecipeId {
    pub fn validate(&self) -> Result<(), Error> {
        if self.0.is_empty()
            || self.0.len() > 128
            || !self
                .0
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-/".contains(&c))
        {
            return Err(Error::InvalidConfiguration(
                "invalid workshop recipe id".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub struct RecipeView<'a> {
    pub site: &'a Site,
    pub recipe: &'a Recipe,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProductionRequirement {
    pub batches: u64,
    pub input: Quantity,
    pub output: Quantity,
    pub work: SimDuration,
}
impl Recipe {
    pub fn production_requirement(&self, output: Quantity) -> Result<ProductionRequirement, Error> {
        self.validate()?;
        let batches = output.0.div_ceil(self.output_quantity.0);
        Ok(ProductionRequirement {
            batches,
            input: Quantity(
                batches
                    .checked_mul(self.input_quantity.0)
                    .ok_or(Error::Overflow)?,
            ),
            output: Quantity(
                batches
                    .checked_mul(self.output_quantity.0)
                    .ok_or(Error::Overflow)?,
            ),
            work: SimDuration(
                batches
                    .checked_mul(self.duration.0)
                    .ok_or(Error::Overflow)?,
            ),
        })
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.input_quantity.0 == 0 || self.output_quantity.0 == 0 || self.duration.0 == 0 {
            return Err(Error::InvalidConfiguration(
                "invalid workshop recipe".into(),
            ));
        }
        Ok(())
    }
}
impl Catalog {
    pub fn validate(&self) -> Result<(), Error> {
        if self.recipes.is_empty() || !self.recipes.contains_key(&self.default_recipe) {
            return Err(Error::InvalidConfiguration(
                "missing default workshop recipe".into(),
            ));
        }
        for (id, recipe) in &self.recipes {
            if id.0.is_empty()
                || id.0.len() > 128
                || !id
                    .0
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-/".contains(&c))
            {
                return Err(Error::InvalidConfiguration(
                    "invalid workshop recipe id".into(),
                ));
            }
            recipe.validate()?;
        }
        Ok(())
    }
}
impl Definitions {
    pub fn from_content(document: Document<Catalog>) -> Result<Self, Error> {
        document.validate_header()?;
        document.data.validate()?;
        Ok(Self {
            content: document.stamp()?,
            catalog: document.data,
        })
    }
    pub fn validate(&self) -> Result<(), Error> {
        self.catalog.validate()?;
        let document = Document {
            id: self.content.id.clone(),
            version: self.content.version,
            data: self.catalog.clone(),
        };
        if document.stamp()? != self.content {
            return Err(Error::InvalidSave("changed workshop definitions".into()));
        }
        Ok(())
    }
    /// Extend an unpublished authored definition set, including test fixtures.
    /// Identity derives from content; it never consumes a host entity counter.
    pub fn declare_recipe(&mut self, recipe: Recipe) -> Result<RecipeId, Error> {
        recipe.validate()?;
        let document = Document {
            id: "workshop/inline".into(),
            version: 1,
            data: recipe.clone(),
        };
        let id = RecipeId(format!("inline/{:016x}", document.stamp()?.fingerprint));
        if let Some(previous) = self.catalog.recipes.get(&id) {
            if previous != &recipe {
                return Err(Error::InvalidConfiguration(
                    "recipe identity collision".into(),
                ));
            }
            return Ok(id);
        }
        let mut catalog = self.catalog.clone();
        catalog.recipes.insert(id.clone(), recipe);
        *self = Self::from_content(Document {
            id: self.content.id.clone(),
            version: self.content.version,
            data: catalog,
        })?;
        Ok(id)
    }
    pub fn validate_storage(&self, store: &game_ecs::Store) -> Result<(), Error> {
        self.validate()?;
        for (_, site) in sites(store) {
            let recipe = self.recipe(&site.recipe)?;
            let input = game_cargo::inventory(store, site.input).ok_or(Error::Unavailable)?;
            let output = game_cargo::inventory(store, site.output).ok_or(Error::Unavailable)?;
            let tool = game_cargo::inventory(store, site.tool).ok_or(Error::Unavailable)?;
            if input.good != recipe.input_good
                || output.good != recipe.output_good
                || tool.good != Good::Tool
            {
                return Err(Error::InvalidSave(
                    "site stocks disagree with recipe".into(),
                ));
            }
        }
        for (_, job) in jobs(store) {
            let site = site(store, job.site).ok_or(Error::Unavailable)?;
            if job.remaining > self.recipe(&site.recipe)?.duration.0 {
                return Err(Error::InvalidSave(
                    "work exceeds selected recipe duration".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn resolve_site<'a>(&'a self, site: &'a Site) -> Result<RecipeView<'a>, Error> {
        site.validate()?;
        Ok(RecipeView {
            site,
            recipe: self.recipe(&site.recipe)?,
        })
    }
    pub fn bind(&self, id: &RecipeId, facts: SiteBindings<'_>) -> Result<Site, Error> {
        let recipe = self.recipe(id)?;
        recipe.validate()?;
        if facts.input.good != recipe.input_good
            || facts.output.good != recipe.output_good
            || facts.tool.good != Good::Tool
            || facts.input.location != InventoryLocation::Area(facts.area)
            || facts.output.location != InventoryLocation::Area(facts.area)
        {
            return Err(Error::InvalidConfiguration(
                "incompatible workshop stock bindings".into(),
            ));
        }
        facts.input.room()?;
        facts.tool.room()?;
        facts.output.room()?;
        let site = Site {
            recipe: id.clone(),
            id: facts.id,
            area: facts.area,
            input: facts.input.id,
            tool: facts.tool.id,
            output: facts.output.id,
            access: facts.access,
            last_granted: None,
        };
        site.validate()?;
        Ok(site)
    }
    pub fn recipe(&self, id: &RecipeId) -> Result<&Recipe, Error> {
        self.catalog
            .recipes
            .get(id)
            .ok_or_else(|| Error::InvalidConfiguration("unknown workshop recipe".into()))
    }
}
#[cfg(test)]
#[path = "recipe_tests.rs"]
mod tests;
