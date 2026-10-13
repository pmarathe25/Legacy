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
    /// Bound a requested acquisition by personal knowledge and native source eligibility.
    pub fn acquisition_need(
        &self,
        store: &Store,
        actor: ActorFacts,
        known: &StockKnowledge,
        good: Option<Good>,
        quantity: Quantity,
        destination: Option<InventoryId>,
    ) -> Option<AcquisitionFacts> {
        if good.is_some_and(|good| good != known.good) {
            return None;
        }
        let need = AcquisitionFacts {
            actor,
            good: known.good,
            destination,
            quantity: Quantity(quantity.0.min(known.quantity.0)),
        };
        self.acquisition_source_eligible(store, &need, known)
            .then_some(need)
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
    pub fn autonomous_sharing_eligible(
        &self,
        store: &Store,
        actor: ActorFacts,
        ruler: PersonId,
        needy: bool,
    ) -> bool {
        actor.actor != ruler && self.sharing_eligible(store, actor, needy)
    }
    pub fn sharing_eligible(&self, store: &Store, actor: ActorFacts, needy: bool) -> bool {
        actor.capable && needy && attitude(store, actor.actor).is_some_and(|a| a.sharing >= 700)
    }
}
pub struct SharingStore {
    pub stock: StockFacts,
    pub recipients_without_access: Vec<PersonId>,
}

/// Translate an owner-admitted acquisition action using only personal stock
/// knowledge. A remote/current physical stock is never substituted for memory.
pub fn acquisition_offer(
    action: PropertyAction,
    known: &StockKnowledge,
    good: Good,
) -> Option<game_logistics::AcquisitionOffer<PropertyAction>> {
    let (source, quantity, requires_carrying_room) = match action {
        PropertyAction::RequestAccess { inventory } => (inventory, Quantity(0), false),
        PropertyAction::Take {
            source, quantity, ..
        } => (source, quantity, true),
        _ => return None,
    };
    if known.inventory != source || known.good != good || known.quantity.0 == 0 {
        return None;
    }
    Some(game_logistics::AcquisitionOffer {
        key: source.0,
        good,
        source: Some(source),
        location: Location::AtArea(known.area),
        observed_at: known.observed_at,
        accessible: true,
        quantity: game_logistics::OfferYield::Known(quantity),
        requires_carrying_room,
        requirements: Vec::new(),
        action,
    })
}

/// A generous custodian retains two ordinary meal portions before sharing.
pub fn retention_reserve(meal_size: Quantity) -> Quantity {
    Quantity(meal_size.0.saturating_mul(2))
}
