//! Prepared site declarations for receiving construction assets.
use crate::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
pub struct PreparedSiteEstablishment {
    site: Site,
    recipe: Recipe,
    input: Inventory,
    tool: Inventory,
    output: Inventory,
}
pub struct SitePublication {
    recipe: Recipe,
    id: RecipeId,
    guard: ReadGuard,
}
impl Definitions {
    pub fn prepare_site_establishment(
        &self,
        store: &Store,
        recipe: &RecipeId,
        bindings: SiteBindings<'_>,
    ) -> Result<PreparedSiteEstablishment, Error> {
        if site(store, bindings.id).is_some() || store.contains(StableId(bindings.id.0)) {
            return Err(Error::Unavailable);
        }
        let recipe_row = self.recipe(recipe)?.clone();
        let input = bindings.input.clone();
        let tool = bindings.tool.clone();
        let output = bindings.output.clone();
        let site = self.bind(recipe, bindings)?;
        site.validate()?;
        Ok(PreparedSiteEstablishment {
            site,
            recipe: recipe_row,
            input,
            tool,
            output,
        })
    }
}
impl PreparedSiteEstablishment {
    pub fn stage(
        self,
        definitions: &Definitions,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<SitePublication, Error> {
        if definitions.recipe(&self.site.recipe)? != &self.recipe
            || site(store, self.site.id).is_some()
            || store.contains(StableId(self.site.id.0))
        {
            return Err(Error::Unavailable);
        }
        let id = StableId(self.site.id.0);
        let mut reads = ReadSet::new();
        reads.expect_row::<Site>(id, None);
        reads.exclusive_entity(store, id)?;
        for stock in [self.input, self.tool, self.output] {
            reads.expect_row(StableId(stock.id.0), Some(stock));
        }
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Site>(id);
        if batch.touches::<Site>(id) {
            return Err(Error::Unavailable);
        }
        let recipe_id = self.site.recipe.clone();
        batch.insert(id, self.site);
        Ok(SitePublication {
            recipe: self.recipe,
            id: recipe_id,
            guard: reads.bind(store, batch, &admitted)?,
        })
    }
}
impl SitePublication {
    pub fn validate(
        &self,
        definitions: &Definitions,
        store: &Store,
        batch: &Batch,
    ) -> Result<(), Error> {
        if definitions.recipe(&self.id)? != &self.recipe {
            return Err(Error::Unavailable);
        }
        self.guard.validate(store, batch)
    }
}
