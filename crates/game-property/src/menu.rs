//! Property candidate rules over current ordinary scene facts and dated knowledge.
use crate::*;
use game_core::*;
use game_ecs::Store;

pub struct MenuFacts<'a> {
    pub actor: ActorFacts,
    pub quantity: Quantity,
    pub cargo: Option<StockFacts>,
    pub can_receive_quantity: bool,
    pub stocks: &'a [StockFacts],
    pub contacts: &'a [ContactFacts],
    pub recipient_access: &'a [(InventoryId, PersonId)],
}
impl Runtime {
    pub fn menu_admissible(&self, actor: ActorFacts) -> bool {
        actor.capable && actor.area.is_some()
    }
    pub fn available_actions(&self, store: &Store, facts: MenuFacts<'_>) -> Vec<PropertyAction> {
        if !self.menu_admissible(facts.actor) {
            return Vec::new();
        }
        let actor = facts.actor.actor;
        let area = facts.actor.area.unwrap();
        let mut actions = Vec::new();
        if let Some(known) = known_claims(store, actor) {
            for claim in known.values().take(8) {
                if !claim.recognized_by.contains(&actor) {
                    actions.push(PropertyAction::Recognize { claim: claim.id });
                }
                if !claim.opposed_by.contains(&actor) {
                    actions.push(PropertyAction::Contest { claim: claim.id });
                }
            }
        }
        for stock in facts
            .stocks
            .iter()
            .filter(|stock| stock.quantity.0 > 0 && stock.area == Some(area))
            .take(16)
        {
            if let Some(cargo) = facts.cargo {
                let room = if facts.can_receive_quantity {
                    facts.quantity
                } else {
                    Quantity(0)
                };
                if taking_possible(facts.actor, *stock, cargo, facts.quantity, room, true) {
                    actions.push(PropertyAction::Take {
                        source: stock.id,
                        destination: cargo.id,
                        quantity: facts.quantity,
                    });
                }
            }
            if !stock.actor_allowed
                && stock.custodian.is_some_and(|custodian| {
                    custodian != actor
                        && facts.contacts.iter().any(|contact| {
                            contact.actor == custodian
                                && contact.present
                                && contact.area == Some(area)
                        })
                })
            {
                actions.push(PropertyAction::RequestAccess {
                    inventory: stock.id,
                });
            }
            if stock.custodian == Some(actor) {
                for contact in facts
                    .contacts
                    .iter()
                    .filter(|contact| contact.actor != actor)
                    .take(4)
                {
                    // StockFacts reports this actor's access only; the adapter
                    // supplies per-recipient access as a separate factual input.
                    let allowed = !facts.recipient_access.iter().any(|(inventory, recipient)| {
                        *inventory == stock.id && *recipient == contact.actor
                    });
                    actions.push(PropertyAction::Share {
                        inventory: stock.id,
                        recipient: contact.actor,
                        allowed,
                    });
                }
            }
        }
        actions.truncate(40);
        for (thief, id) in pending_rows(store)
            .filter(|(thief, id)| {
                *thief != actor && incident(store, *id).is_some_and(|row| row.area == area)
            })
            .take(8)
        {
            if known_incidents(store, actor).is_some_and(|known| known.contains_key(&id)) {
                actions.push(PropertyAction::ResistTaking { thief });
            }
        }
        if let Some(known) = known_incidents(store, actor) {
            for incident in known.keys().rev().take(4) {
                for contact in facts
                    .contacts
                    .iter()
                    .filter(|contact| contact.actor != actor && contact.available)
                    .take(4)
                {
                    actions.push(PropertyAction::ReportIncident {
                        recipient: contact.actor,
                        incident: *incident,
                    });
                }
            }
        }
        actions.truncate(64);
        actions
    }
}
