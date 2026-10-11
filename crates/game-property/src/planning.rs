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
impl Runtime {
    pub fn acquisition_eligible(&self, store: &Store, actor: ActorFacts) -> bool {
        actor.capable && !next_search(store, actor.actor).is_some_and(|at| *at > actor.now)
    }
    pub fn acquisition_source_eligible(
        &self,
        store: &Store,
        need: &AcquisitionFacts,
        stock: &StockKnowledge,
    ) -> bool {
        self.acquisition_eligible(store, need.actor)
            && stock.good == need.good
            && !stock.accessible
            && stock.quantity.0 > 0
            && stock.quantity >= need.quantity
            && Some(stock.area) == need.actor.area
    }
    /// Enumerates known feasible actions; personal motives belong to the actor.
    pub fn acquisition_opportunities(
        &self,
        store: &Store,
        need: AcquisitionFacts,
        opportunities: &[NeedOpportunity],
    ) -> Vec<PropertyAction> {
        if !self.acquisition_eligible(store, need.actor) {
            return Vec::new();
        }
        let mut actions = Vec::new();
        for opportunity in opportunities {
            let stock = &opportunity.known;
            if !self.acquisition_source_eligible(store, &need, stock) {
                continue;
            }
            if opportunity.local_custodian {
                actions.push(PropertyAction::RequestAccess {
                    inventory: stock.inventory,
                });
            }
            if need.quantity.0 > 0
                && opportunity.can_take
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
        store: &Store,
        actor: ActorFacts,
        needy: bool,
        good: Good,
        minimum: Quantity,
        stores: &[SharingStore],
    ) -> Vec<PropertyAction> {
        if !self.sharing_eligible(store, actor, needy) {
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
    pub fn sharing_eligible(&self, store: &Store, actor: ActorFacts, needy: bool) -> bool {
        actor.capable && needy && attitude(store, actor.actor).is_some_and(|a| a.sharing >= 700)
    }
}
pub struct SharingStore {
    pub stock: StockFacts,
    pub recipients_without_access: Vec<PersonId>,
}
