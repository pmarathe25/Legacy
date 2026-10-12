//! Natural acquisition exposes ordinary material offers to the logistics planner.
use crate::*;
use game_ecs::Store;
use game_logistics::{
    AcquisitionOffer, OfferYield, Prerequisite, PrerequisitePlacement, Requirement,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcquisitionAction {
    Resource(EcologyAction),
    Explore { destination: AreaId },
}

impl Runtime {
    pub fn acquisition_offers(
        &self,
        store: &Store,
        need: AcquisitionNeed,
        facts: GatherFacts,
        local_actions: &[EcologyAction],
        known_areas: &[AreaId],
    ) -> Vec<AcquisitionOffer<AcquisitionAction>> {
        if facts.now != need.now || facts.area != need.area || facts.carrying_room != need.room {
            return Vec::new();
        }
        let local = self
            .knowledge(store, need.actor)
            .into_iter()
            .flat_map(|knowledge| knowledge.known_resources.values())
            .filter(|known| known.area == need.area && known.kind.good() == need.good)
            .map(|known| {
                let quantity = Quantity(need.quantity.0.min(need.room.0).min(known.available.0));
                LocalAcquisition {
                    resource: known.id,
                    can_prepare: self
                        .gathering_conditions(
                            store,
                            need.actor,
                            known.id,
                            quantity,
                            !known.accessible,
                            facts,
                        )
                        .is_ok(),
                    can_request: local_actions.iter().any(|action| {
                        matches!(action, EcologyAction::RequestAccess { resource } if *resource == known.id)
                    }),
                }
            })
            .collect::<Vec<_>>();
        let mut offers = self
            .acquisition_prospects(store, need, &local)
            .into_iter()
            .map(|prospect| AcquisitionOffer {
                key: prospect.resource.0,
                good: need.good,
                source: None,
                location: Location::AtArea(prospect.area),
                observed_at: prospect.observed_at,
                accessible: true,
                quantity: OfferYield::Known(prospect.quantity),
                requires_carrying_room: matches!(prospect.action, EcologyAction::Gather { .. }),
                requirements: if prospect.needs_tool {
                    vec![Prerequisite {
                        requirement: Requirement {
                            good: Good::Tool,
                            quantity: Quantity(1_000),
                            source: None,
                        },
                        placement: PrerequisitePlacement::CarriedInOneLot,
                    }]
                } else {
                    Vec::new()
                },
                action: AcquisitionAction::Resource(prospect.action),
            })
            .collect::<Vec<_>>();
        offers.extend(
            self.exploration_areas(store, need.actor, need.area, need.now, known_areas)
                .into_iter()
                .map(|(destination, inspected)| AcquisitionOffer {
                    key: destination.0,
                    good: need.good,
                    source: None,
                    location: Location::AtArea(destination),
                    observed_at: inspected,
                    accessible: true,
                    quantity: OfferYield::Unknown,
                    requires_carrying_room: false,
                    requirements: Vec::new(),
                    action: AcquisitionAction::Explore { destination },
                }),
        );
        offers
    }
}

#[cfg(test)]
mod tests;
