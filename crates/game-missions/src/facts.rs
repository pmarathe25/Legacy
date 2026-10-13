//! Focused fact joins are selected by the service owner.
use crate::*;
use game_agreements::{AgreementTerm, InventoryTerms, TermsContext};
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};
pub trait AssetFacts {
    fn terms_context(&self, terms: &[AgreementTerm]) -> TermsContext;
    fn inventory_terms(&self, inventory: InventoryId) -> Option<InventoryTerms>;
    fn person_exists(&self, person: PersonId) -> bool;
    fn area_exists(&self, area: AreaId) -> bool;
    fn carrying_capacity(&self, actor: PersonId) -> Quantity;
}
pub fn mission_assets(
    objective: Option<&MissionObjective>,
    terms: &[AgreementTerm],
    supplies: &[MissionSupply],
    facts: &impl AssetFacts,
) -> MissionAssets {
    let mut assets = MissionAssets {
        terms: facts.terms_context(terms),
        ..Default::default()
    };
    let mut areas = BTreeSet::new();
    if let Some(objective) = objective {
        match objective {
            MissionObjective::Visit { destination } => {
                areas.insert(*destination);
            }
            MissionObjective::Rescue { person, return_to } => {
                areas.insert(*return_to);
                if facts.person_exists(*person) {
                    assets.people.insert(*person);
                }
            }
            MissionObjective::Defeat { person } | MissionObjective::Confine { person, .. } => {
                if facts.person_exists(*person) {
                    assets.people.insert(*person);
                }
            }
            MissionObjective::Patrol { areas: route, .. } => {
                areas.extend(route.iter().copied());
            }
            MissionObjective::Deliver {
                source,
                destination,
                ..
            } => {
                for id in [source, destination] {
                    if let Some(stock) = facts.inventory_terms(*id) {
                        assets.terms.inventories.insert(*id, stock);
                    }
                }
            }
        }
    }
    assets.areas = areas
        .into_iter()
        .filter(|id| facts.area_exists(*id))
        .collect();
    for supply in supplies {
        if let Some(stock) = facts.inventory_terms(supply.source) {
            assets.terms.inventories.insert(supply.source, stock);
        }
        assets
            .carrying_capacity
            .insert(supply.carrier, facts.carrying_capacity(supply.carrier));
    }
    assets
}
pub trait ObjectiveFacts: PresenceFacts {
    fn stock(&self, actor: PersonId, inventory: InventoryId) -> Option<StockKnowledge>;
    /// Actual target facts at a local area; visibility is decided by service.
    fn target_observation(&self, target: PersonId) -> Option<TargetObservation>;
}
impl Runtime {
    pub fn objective_knowledge(
        &self,
        actor: PersonId,
        objective: &MissionObjective,
        facts: &impl ObjectiveFacts,
    ) -> ObjectiveKnowledge {
        let stocks = if let MissionObjective::Deliver {
            source,
            destination,
            ..
        } = objective
        {
            [source, destination]
                .into_iter()
                .filter_map(|id| facts.stock(actor, *id).map(|stock| (*id, stock)))
                .collect()
        } else {
            BTreeMap::new()
        };
        let target = objective
            .target_person()
            .filter(|target| self.can_observe(actor, *target, facts))
            .and_then(|target| facts.target_observation(target));
        ObjectiveKnowledge { stocks, target }
    }
}
