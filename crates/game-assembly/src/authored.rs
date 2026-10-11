//! Startup binds validated authored plans to allocated host identities.
use crate::*;
use game_openings::Placement;

pub(crate) struct Content {
    pub openings: game_openings::Definitions,
    pub workshops: game_workshop::Definitions,
}
impl Content {
    pub fn resolve(config: &scenarios::Config) -> Result<Self, Error> {
        let openings = game_openings::Definitions::from_content(config.openings.resolve()?)?;
        let workshops = game_workshop::Definitions::from_content(config.workshops.resolve()?)?;
        for settlement in openings.catalog.settlements.values() {
            for workshop in &settlement.workshops {
                let recipe = workshops.recipe(&game_workshop::RecipeId(workshop.recipe.clone()))?;
                let stock = |key: &str| {
                    settlement
                        .stocks
                        .iter()
                        .find(|stock| stock.key == key)
                        .expect("validated stock reference")
                };
                if stock(&workshop.input).good != recipe.input_good
                    || stock(&workshop.output).good != recipe.output_good
                    || stock(&workshop.tool).good != Good::Tool
                    || stock(&workshop.materials).good != Good::Timber
                {
                    return Err(Error::InvalidConfiguration(
                        "opening stocks do not match selected recipe".into(),
                    ));
                }
            }
        }
        Ok(Self {
            openings,
            workshops,
        })
    }
}
pub(crate) fn opening_source() -> game_content::DefinitionSource<game_openings::Catalog> {
    game_content::DefinitionSource::File {
        path: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/openings/temperate.toml"),
    }
}
pub(crate) fn workshop_source() -> game_content::DefinitionSource<game_workshop::Catalog> {
    game_content::DefinitionSource::File {
        path: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/workshop/temperate.toml"),
    }
}
pub(crate) fn fixture_workshops() -> Result<game_workshop::Definitions, Error> {
    game_workshop::Definitions::from_content(game_content::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/workshop/fixtures.toml"),
    )?)
}
impl Simulation {
    #[cfg(test)]
    pub(crate) fn fixture_recipe(
        &mut self,
        input: InventoryId,
        output: InventoryId,
        input_quantity: Quantity,
        output_quantity: Quantity,
        duration: SimDuration,
    ) -> Result<game_workshop::RecipeId, Error> {
        let input_good = game_cargo::inventory(&self.host.storage, input)
            .ok_or(Error::Unavailable)?
            .good;
        let output_good = game_cargo::inventory(&self.host.storage, output)
            .ok_or(Error::Unavailable)?
            .good;
        let id = self
            .host
            .state
            .workshops
            .declare_recipe(game_workshop::Recipe {
                input_good,
                input_quantity,
                output_good,
                output_quantity,
                duration,
            })?;
        if self.host.state.content.contains_key("workshops") {
            self.host.state.content.insert(
                "workshops".into(),
                self.host.state.workshops.content.clone(),
            );
        }
        Ok(id)
    }

    pub(crate) fn bind_authored_stocks(
        &mut self,
        plan: &game_openings::AuthoredPlan,
        settlement: &population::Settlement,
        first_food: Option<InventoryId>,
    ) -> Result<BTreeMap<String, InventoryId>, Error> {
        let mut ids = BTreeMap::new();
        for stock in &plan.stocks {
            let holders: Vec<_> = match &stock.definition.placement {
                Placement::Area { .. } => vec![None],
                Placement::Resident { index } => vec![Some(settlement.residents[*index])],
                Placement::EachResident => settlement.residents.iter().copied().map(Some).collect(),
            };
            for holder in holders {
                let location = match &stock.definition.placement {
                    Placement::Area { area } => InventoryLocation::Area(settlement.areas[area]),
                    _ => InventoryLocation::Carried(holder.expect("authored holder")),
                };
                let access = stock
                    .definition
                    .access
                    .resolve(&settlement.residents, holder)?;
                let id = if let ("food", Some(id)) = (stock.definition.key.as_str(), first_food) {
                    let row = game_cargo::inventory_mut(&mut self.host.storage, id)
                        .ok_or(Error::Unavailable)?;
                    row.good = stock.definition.good;
                    row.location = location;
                    row.quantity = stock.quantity;
                    row.capacity = stock.capacity;
                    row.access = access;
                    id
                } else {
                    self.add_inventory(
                        stock.definition.good,
                        location,
                        stock.quantity,
                        stock.capacity,
                        access,
                    )?
                };
                if holder.is_none() {
                    ids.insert(stock.definition.key.clone(), id);
                }
                // Custody is bound later, when the agreement owner is installed.
                if let Some(holder) = holder {
                    ids.insert(format!("{}/{}", stock.definition.key, holder.0), id);
                }
            }
        }
        Ok(ids)
    }
    pub(crate) fn bind_authored_custody(
        &mut self,
        plan: &game_openings::AuthoredPlan,
        settlement: &population::Settlement,
    ) -> Result<(), Error> {
        for definition in &plan.definition.stocks {
            let holders: Vec<_> = match definition.placement {
                Placement::Area { .. } => vec![None],
                Placement::Resident { index } => vec![Some(settlement.residents[index])],
                Placement::EachResident => settlement.residents.iter().copied().map(Some).collect(),
            };
            for holder in holders {
                let id = if let Some(holder) = holder {
                    settlement.stocks[&format!("{}/{}", definition.key, holder.0)]
                } else {
                    settlement.stocks[&definition.key]
                };
                let value = definition
                    .custodian
                    .resolve(&settlement.residents, holder)?;
                let custody = &mut self.host.state.agreements.as_mut().unwrap().custodians;
                if let Some(value) = value {
                    custody.insert(id, value);
                } else {
                    custody.remove(&id);
                }
            }
        }
        Ok(())
    }
    pub(crate) fn bind_authored_resources(
        &mut self,
        plan: &game_openings::AuthoredPlan,
        settlement: &population::Settlement,
    ) -> Result<(), Error> {
        for definition in &plan.definition.resources {
            let id = self.debug_add_resource(
                definition.kind,
                settlement.areas[&definition.area],
                definition.capacity,
                definition.capacity,
                definition.daily_growth,
                definition.access.resolve(&settlement.residents, None)?,
                Some(settlement.residents[definition.custodian]),
                definition.contamination,
            )?;
            self.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .resource_mut(&mut self.host.storage, id)
                .unwrap()
                .labor_per_unit = definition.labor_per_unit;
        }
        Ok(())
    }
    pub(crate) fn bind_authored_workshop(
        &mut self,
        definition: &game_openings::WorkshopDefinition,
        settlement: &population::Settlement,
    ) -> Result<SiteId, Error> {
        let id = SiteId(self.host.allocate_id()?.0);
        let selected = game_workshop::RecipeId(definition.recipe.clone());
        let site = self.host.state.workshops.bind(
            &selected,
            game_workshop::SiteBindings {
                id,
                area: settlement.areas[&definition.area],
                input: game_cargo::inventory(
                    &self.host.storage,
                    settlement.stocks[&definition.input],
                )
                .expect("authored input"),
                tool: game_cargo::inventory(
                    &self.host.storage,
                    settlement.stocks[&definition.tool],
                )
                .expect("authored tool"),
                output: game_cargo::inventory(
                    &self.host.storage,
                    settlement.stocks[&definition.output],
                )
                .expect("authored output"),
                access: definition.access.resolve(&settlement.residents, None)?,
            },
        )?;
        game_workshop::insert_site(&mut self.host.storage, site)?;
        Ok(id)
    }
}
