//! New workshop proposals from personally remembered supplies.
use crate::*;
use game_workshop::Definitions;

/// Shared spatial and custody observations for one remembered stock.
/// Amounts and accessibility remain the person's knowledge, not omniscient stock.
#[derive(Clone, Copy)]
pub struct WorkshopStock<'a> {
    pub known: &'a StockKnowledge,
    pub at_area: Option<AreaId>,
    pub custodian: Option<PersonId>,
}
#[derive(Clone, Copy)]
pub struct WorkshopProspectFacts {
    pub actor: PersonId,
    pub area: AreaId,
    pub now: SimTime,
    pub capable: bool,
    pub loading: SimDuration,
}
impl Runtime {
    pub fn workshop_prospects(
        &self,
        store: &Store,
        definitions: &Definitions,
        facts: WorkshopProspectFacts,
        stocks: &[WorkshopStock<'_>],
    ) -> Result<Vec<ProjectAction>, Error> {
        if !facts.capable
            || knowledge(store, facts.actor).is_some_and(|row| {
                row.projects.values().any(|plan| {
                    plan.sponsor == facts.actor
                        && !matches!(
                            plan.status,
                            ProjectStatus::Cancelled | ProjectStatus::Failed
                        )
                })
            })
            || requests(store, facts.actor).is_some_and(|row| row.next_search > facts.now)
        {
            return Ok(Vec::new());
        }
        let own = |stock: &&WorkshopStock<'_>| {
            stock.known.area == facts.area
                && stock.at_area == Some(facts.area)
                && stock.custodian == Some(facts.actor)
        };
        let choose = |good, minimum, different| {
            stocks
                .iter()
                .filter(own)
                .filter(|stock| {
                    stock.known.good == good
                        && stock.known.accessible
                        && stock.known.quantity.0 >= minimum
                        && different != Some(stock.known.inventory)
                })
                .min_by_key(|stock| stock.known.inventory)
                .map(|stock| stock.known.inventory)
        };
        let Some(materials) = choose(Good::Timber, 0, None) else {
            return Ok(Vec::new());
        };
        let Some(tool) = choose(Good::Tool, 1_000, None) else {
            return Ok(Vec::new());
        };
        let expected = self
            .rules
            .construction_labor
            .0
            .saturating_mul(4)
            .saturating_add(facts.loading.0.saturating_mul(8))
            .saturating_add(120_000);
        let Some(finish_by) = facts.now.0.checked_add(expected).map(SimTime) else {
            return Ok(Vec::new());
        };
        let mut result = Vec::new();
        let default = &definitions.catalog.default_recipe;
        let recipes = std::iter::once((default, definitions.recipe(default)?)).chain(
            definitions
                .catalog
                .recipes
                .iter()
                .filter(|(id, _)| *id != default),
        );
        for (id, recipe) in recipes {
            recipe.validate()?;
            let Some(input) = choose(recipe.input_good, 0, None) else {
                continue;
            };
            let output = stocks
                .iter()
                .filter(own)
                .filter(|stock| {
                    stock.known.good == recipe.output_good
                        && stock.known.inventory != tool
                        && stock.known.accessible
                        && stock.known.quantity.0 == 0
                })
                .min_by_key(|stock| stock.known.inventory)
                .map(|stock| stock.known.inventory);
            let Some(output) = output else {
                continue;
            };
            result.push(ProjectAction::Propose {
                recipe: id.clone(),
                area: facts.area,
                materials,
                tool,
                input,
                output,
                beneficiaries: BTreeSet::from([facts.actor]),
                workers: BTreeSet::from([facts.actor]),
                finish_by,
            });
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests;
