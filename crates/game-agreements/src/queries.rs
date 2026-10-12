//! Promise matching and personal debt examples for physical cargo consumers.
use crate::*;
pub fn delivery_cargo(
    store: &Store,
    actor: PersonId,
    delivery: &game_logistics::Delivery,
) -> BTreeMap<InventoryId, Quantity> {
    let Some(source) = delivery.requirement.source else {
        return BTreeMap::new();
    };
    let mut attributed = BTreeMap::<InventoryId, Quantity>::new();
    for (_,obligation) in obligations(store).filter(|(_,o)|matches!(o.term,AgreementTerm::Delivery{debtor,source:from,destination:to,..} if debtor==actor && from==source && matches!(delivery.destination,game_logistics::Destination::Inventory(target) if target==to)) && o.performed<o.amount()){
        for (id,q) in &obligation.cargo{let total=attributed.entry(*id).or_insert(Quantity(0));total.0=total.0.saturating_add(*q);}
    }
    attributed
}
/// Dated knowledge supplies suggested debt amounts, never remote actual stock.
/// Cargo owns the eventual physical bound and admission of a chosen quantity.
pub fn known_delivery_amounts(
    store: &Store,
    actor: PersonId,
    good: Good,
    source: Option<InventoryId>,
    destination: Option<InventoryId>,
) -> Vec<Quantity> {
    let mut quantities = Vec::new();
    for agreement in known_agreements(store, actor)
        .into_iter()
        .flat_map(|known| known.values())
    {
        for o in &agreement.obligations {
            let relevant = match o.term {
                AgreementTerm::Delivery {
                    debtor,
                    source: origin,
                    destination: target,
                    ..
                } => {
                    debtor == actor
                        && destination.is_none_or(|id| id == target)
                        && source.is_some_and(|id| id == origin || o.cargo.contains_key(&id))
                }
                AgreementTerm::OutputShare {
                    debtor,
                    good: promised,
                    destination: target,
                    ..
                } => {
                    debtor == actor && promised == good && destination.is_none_or(|id| id == target)
                }
                _ => false,
            };
            if relevant {
                quantities.push(Quantity(o.amount().saturating_sub(o.performed)));
            }
        }
    }
    quantities
}
/// The first personally remembered unfinished construction promise, retaining
/// agreement and term order. Current hidden performance cannot update memory.
pub fn known_construction_obligation(
    store: &Store,
    actor: PersonId,
    project: ProjectId,
) -> Option<ObligationId> {
    known_agreements(store,actor).into_iter().flat_map(|known|known.values()).flat_map(|agreement|agreement.obligations.iter())
        .find(|o|matches!(o.term,AgreementTerm::ConstructionLabor{debtor,project:target,..} if debtor==actor && target==project) && o.performed<o.term.amount()).map(|o|o.id)
}

/// A personally visible promise, preserving the same stable cap as public views.
pub fn known_agreement(store: &Store, actor: PersonId, id: AgreementId) -> Option<&AgreementView> {
    known_agreements(store, actor)?
        .values()
        .take(64)
        .find(|view| view.id == id)
}
