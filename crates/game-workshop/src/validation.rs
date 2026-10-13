//! Workshop save invariants over canonical rows and supplied actor facts.
use crate::*;
use game_ecs::Store;
use std::collections::BTreeMap;
#[derive(Clone, Copy)]
pub struct ActorSaveFacts {
    pub alive: bool,
    pub active_site: Option<SiteId>,
    pub maximum_craft_rate: u64,
}
pub struct SaveFacts {
    pub areas: BTreeSet<AreaId>,
    pub actors: BTreeMap<PersonId, ActorSaveFacts>,
}
pub fn validate_store(
    store: &Store,
    definitions: &Definitions,
    facts: &SaveFacts,
) -> Result<(), Error> {
    let bad = || Error::InvalidSave("inconsistent workshop sites or jobs".into());
    for (id, declaration) in sites(store) {
        if id != declaration.id
            || !facts.areas.contains(&declaration.area)
            || definitions.resolve_site(declaration).is_err()
            || declaration
                .access
                .iter()
                .any(|actor| !facts.actors.contains_key(actor))
        {
            return Err(bad());
        }
        for stock in [declaration.input, declaration.tool, declaration.output] {
            if game_cargo::inventory(store, stock)
                .is_none_or(|stock| stock.location != InventoryLocation::Area(declaration.area))
            {
                return Err(bad());
            }
        }
        if game_cargo::inventory(store, declaration.tool)
            .is_none_or(|stock| stock.good != Good::Tool)
        {
            return Err(bad());
        }
    }
    for (actor, work) in jobs(store) {
        let actor = facts.actors.get(&actor).ok_or_else(bad)?;
        if !actor.alive
            || work.tool.is_some_and(|id| {
                game_cargo::inventory(store, id).is_none_or(|stock| stock.good != Good::Tool)
            })
            || !matches!(work.rate.denominator, 1_000 | 1_000_000)
            || work.body_rate.denominator != 1_000
            || !(250..=2_000).contains(&work.body_rate.numerator)
            || work.work_remainder >= work.rate.denominator
            || !site(store, work.site).is_some_and(|declaration| {
                definitions
                    .resolve_site(declaration)
                    .is_ok_and(|view| work.remaining <= view.recipe.duration.0)
            })
        {
            return Err(bad());
        }
        let numerator =
            u128::from(work.rate.numerator) * u128::from(work.body_rate.denominator) * 1_000;
        let denominator = u128::from(work.rate.denominator) * u128::from(work.body_rate.numerator);
        if denominator == 0
            || !numerator.is_multiple_of(denominator)
            || numerator / denominator < 1_000
            || numerator / denominator > u128::from(actor.maximum_craft_rate)
        {
            return Err(bad());
        }
    }
    for (id, actor) in &facts.actors {
        if let Some(active) = actor.active_site
            && job(store, *id).is_none_or(|work| work.site != active || work.remaining == 0)
        {
            return Err(bad());
        }
    }
    Ok(())
}
