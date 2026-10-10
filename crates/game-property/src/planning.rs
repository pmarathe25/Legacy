use crate::*;
use game_core::*;

pub struct NeedOpportunity {
    pub known: StockKnowledge,
    pub local_custodian: bool,
    pub can_take: bool,
}
pub struct AcquisitionFacts {
    pub actor: ActorFacts,
    pub good: Good,
    pub destination: Option<InventoryId>,
    pub quantity: Quantity,
}
impl World {
    /// Enumerates known feasible actions; personal motives belong to the actor.
    pub fn acquisition_opportunities(
        &self,
        need: AcquisitionFacts,
        opportunities: &[NeedOpportunity],
    ) -> Vec<PropertyAction> {
        if !need.actor.capable
            || need.quantity.0 == 0
            || self
                .next_search
                .get(&need.actor.actor)
                .is_some_and(|at| *at > need.actor.now)
        {
            return Vec::new();
        }
        let mut actions = Vec::new();
        for opportunity in opportunities {
            let stock = &opportunity.known;
            if stock.good != need.good
                || stock.accessible
                || stock.quantity < need.quantity
                || Some(stock.area) != need.actor.area
            {
                continue;
            }
            if opportunity.local_custodian {
                actions.push(PropertyAction::RequestAccess {
                    inventory: stock.inventory,
                });
            }
            if opportunity.can_take
                && let Some(destination) = need.destination
            {
                actions.push(PropertyAction::Take {
                    source: stock.inventory,
                    destination,
                    quantity: need.quantity,
                });
            }
        }
        actions
    }
    pub fn plan_sharing(
        &self,
        actor: ActorFacts,
        needy: bool,
        good: Good,
        minimum: Quantity,
        stores: &[SharingStore],
    ) -> Vec<PropertyAction> {
        if !actor.capable
            || !needy
            || !self
                .attitudes
                .get(&actor.actor)
                .is_some_and(|a| a.sharing >= 700)
        {
            return Vec::new();
        }
        stores
            .iter()
            .filter(|s| {
                s.stock.good == good
                    && s.stock.quantity >= minimum
                    && s.stock.area == actor.area
                    && s.stock.custodian == Some(actor.actor)
            })
            .flat_map(|s| {
                s.recipients_without_access
                    .iter()
                    .filter(|p| **p != actor.actor)
                    .map(|recipient| PropertyAction::Share {
                        inventory: s.stock.id,
                        recipient: *recipient,
                        allowed: true,
                    })
            })
            .collect()
    }
}
pub struct SharingStore {
    pub stock: StockFacts,
    pub recipients_without_access: Vec<PersonId>,
}
