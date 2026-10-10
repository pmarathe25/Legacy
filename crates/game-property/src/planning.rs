use crate::*;
use game_core::*;

pub struct NeedOpportunity {
    pub known: StockKnowledge,
    pub local_custodian: bool,
    pub can_take: bool,
}
pub struct NeedFacts {
    pub actor: ActorFacts,
    pub needed: bool,
    pub good: Good,
    pub honesty: u16,
    pub risk_tolerance: u16,
    pub destination: InventoryId,
    pub quantity: Quantity,
}
impl World {
    pub fn plan_need(
        &self,
        need: NeedFacts,
        opportunities: &[NeedOpportunity],
    ) -> Option<PropertyAction> {
        if !need.actor.capable
            || !need.needed
            || self
                .next_search
                .get(&need.actor.actor)
                .is_some_and(|at| *at > need.actor.now)
        {
            return None;
        }
        for opportunity in opportunities {
            let stock = &opportunity.known;
            if stock.good != need.good
                || stock.accessible
                || stock.quantity < need.quantity
                || Some(stock.area) != need.actor.area
            {
                continue;
            }
            if need.honesty >= 400 || need.risk_tolerance < 600 {
                if opportunity.local_custodian {
                    return Some(PropertyAction::RequestAccess {
                        inventory: stock.inventory,
                    });
                }
            } else if opportunity.can_take {
                return Some(PropertyAction::Take {
                    source: stock.inventory,
                    destination: need.destination,
                    quantity: need.quantity,
                });
            }
        }
        None
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
