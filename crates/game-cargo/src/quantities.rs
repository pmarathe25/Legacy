//! Bounded public amount choices preserve owner-provided example ordering.
use game_core::Quantity;
use std::collections::BTreeSet;

pub fn quantity_choices(
    upper_bound: Quantity,
    increment: Quantity,
    examples: impl IntoIterator<Item = Quantity>,
) -> Vec<Quantity> {
    if increment.0 == 0 {
        return Vec::new();
    }
    let mut seen = BTreeSet::new();
    examples
        .into_iter()
        .chain([Quantity(1_000), upper_bound, increment])
        .filter_map(|example| {
            let amount = example.0.min(upper_bound.0) / increment.0 * increment.0;
            (amount > 0 && seen.insert(amount)).then_some(Quantity(amount))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn examples_precede_standard_bound_and_custom_choices_with_stable_deduplication() {
        assert_eq!(
            quantity_choices(
                Quantity(1500),
                Quantity(10),
                [Quantity(253), Quantity(255), Quantity(5000)]
            ),
            vec![Quantity(250), Quantity(1500), Quantity(1000), Quantity(10)]
        );
        assert!(quantity_choices(Quantity(1500), Quantity(0), []).is_empty());
    }
}

use crate::{ActorFacts, Inventory, InventoryLocation, ReceiverFacts, SourceFacts};
use game_core::{Good, InventoryId, PersonId, SimDuration, SimTime};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuantityOperation {
    Carry {
        source: InventoryId,
    },
    Load {
        source: InventoryId,
        destination: InventoryId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuantityOffer {
    pub operation: QuantityOperation,
    pub upper_bound: Quantity,
    pub increment: Quantity,
    pub suggested: Vec<Quantity>,
}
/// Ports supply actual visible rows and owner facts. The carry callback invokes
/// Cargo's native compartment admission; examples come from the receiving owners.
pub trait QuantityFacts {
    fn actor(&self) -> ActorFacts;
    fn now(&self) -> SimTime;
    fn duration(&self) -> SimDuration;
    fn capacity(&self) -> Quantity;
    fn body_room(&self) -> Quantity;
    fn stocks(&self) -> impl Iterator<Item = &Inventory>;
    fn increment(&self, source: InventoryId) -> Quantity;
    fn source(&self, source: &Inventory, quantity: Quantity) -> SourceFacts;
    fn receiver(
        &self,
        source: InventoryId,
        destination: &Inventory,
        quantity: Quantity,
    ) -> ReceiverFacts;
    fn examples(
        &self,
        good: Good,
        bound: Quantity,
        increment: Quantity,
        source: InventoryId,
        destination: Option<InventoryId>,
    ) -> Vec<Quantity>;
    fn carry_admitted(&self, source: InventoryId, quantity: Quantity) -> bool;
}
pub fn quantity_opportunities(facts: &impl QuantityFacts) -> Vec<QuantityOffer> {
    let actor = facts.actor();
    let stocks: Vec<_> = facts.stocks().collect();
    let mut opportunities = Vec::new();
    for source in &stocks {
        let increment = facts.increment(source.id);
        let supply = facts.source(source, increment);
        if !supply.accessible || source.quantity.0 == 0 {
            continue;
        }
        if source.location != InventoryLocation::Carried(actor.actor)
            && let Ok(bound) =
                crate::pickup_upper_bound(source, supply, facts.body_room(), facts.capacity())
        {
            let suggested: Vec<_> = facts
                .examples(source.good, bound, increment, source.id, None)
                .into_iter()
                .filter(|quantity| facts.carry_admitted(source.id, *quantity))
                .collect();
            if !suggested.is_empty() {
                opportunities.push(QuantityOffer {
                    operation: QuantityOperation::Carry { source: source.id },
                    upper_bound: bound,
                    increment,
                    suggested,
                });
            }
        }
        for destination in &stocks {
            if !load_pair_eligible(actor.actor, source, destination) {
                continue;
            }
            let receiver = facts.receiver(source.id, destination, increment);
            let Ok(bound) = crate::load_upper_bound(source, destination, supply, receiver) else {
                continue;
            };
            let suggested: Vec<_> = facts
                .examples(
                    source.good,
                    bound,
                    increment,
                    source.id,
                    Some(destination.id),
                )
                .into_iter()
                .filter(|quantity| {
                    crate::prepare_load(
                        actor,
                        source,
                        destination,
                        *quantity,
                        facts.source(source, *quantity),
                        facts.receiver(source.id, destination, *quantity),
                        facts.now(),
                        facts.duration(),
                    )
                    .is_ok()
                })
                .collect();
            if !suggested.is_empty() {
                opportunities.push(QuantityOffer {
                    operation: QuantityOperation::Load {
                        source: source.id,
                        destination: destination.id,
                    },
                    upper_bound: bound,
                    increment,
                    suggested,
                });
            }
        }
    }
    opportunities
}
fn load_pair_eligible(actor: PersonId, source: &Inventory, destination: &Inventory) -> bool {
    source.id != destination.id
        && source.good == destination.good
        && (source.location == InventoryLocation::Carried(actor)
            || destination.location == InventoryLocation::Carried(actor))
}

#[cfg(test)]
mod opportunity_tests {
    use super::*;
    use std::collections::BTreeSet;
    struct Facts {
        stocks: Vec<Inventory>,
        accessible: bool,
    }
    impl QuantityFacts for Facts {
        fn actor(&self) -> ActorFacts {
            ActorFacts {
                actor: PersonId(1),
                alive: true,
                health: 100,
                independent: true,
                confined: false,
                in_combat: false,
                receiving_care: false,
                stationary: true,
                fatigue: 0,
                fatigue_limit: 100,
            }
        }
        fn now(&self) -> SimTime {
            SimTime(0)
        }
        fn duration(&self) -> SimDuration {
            SimDuration(100)
        }
        fn capacity(&self) -> Quantity {
            Quantity(1000)
        }
        fn body_room(&self) -> Quantity {
            Quantity(1000)
        }
        fn stocks(&self) -> impl Iterator<Item = &Inventory> {
            self.stocks.iter()
        }
        fn increment(&self, _: InventoryId) -> Quantity {
            Quantity(1)
        }
        fn source(&self, stock: &Inventory, _: Quantity) -> SourceFacts {
            SourceFacts {
                contact: true,
                accessible: self.accessible,
                unreserved: stock.quantity,
                record_permission: true,
            }
        }
        fn receiver(&self, _: InventoryId, _: &Inventory, _: Quantity) -> ReceiverFacts {
            ReceiverFacts {
                contact: true,
                accessible: true,
                accepted: false,
                body_room: self.body_room(),
                transport_allows: true,
            }
        }
        fn examples(
            &self,
            _: Good,
            bound: Quantity,
            increment: Quantity,
            _: InventoryId,
            _: Option<InventoryId>,
        ) -> Vec<Quantity> {
            quantity_choices(bound, increment, [])
        }
        fn carry_admitted(&self, _: InventoryId, _: Quantity) -> bool {
            false
        }
    }
    #[test]
    fn public_owner_candidates_require_access_matching_goods_and_actual_native_load() {
        let row = |id, good, quantity, location| Inventory {
            id: InventoryId(id),
            good,
            quantity: Quantity(quantity),
            capacity: Quantity(1000),
            location,
            access: BTreeSet::from([PersonId(1)]),
            last_granted: None,
            spoils_at: None,
        };
        let mut facts = Facts {
            stocks: vec![
                row(
                    1,
                    Good::Timber,
                    1000,
                    InventoryLocation::Area(game_core::AreaId(1)),
                ),
                row(2, Good::Timber, 0, InventoryLocation::Carried(PersonId(1))),
                row(3, Good::Food, 0, InventoryLocation::Carried(PersonId(1))),
            ],
            accessible: true,
        };
        let offers = quantity_opportunities(&facts);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].operation,
            QuantityOperation::Load {
                source: InventoryId(1),
                destination: InventoryId(2)
            }
        );
        assert_eq!(offers[0].suggested, vec![Quantity(1000), Quantity(1)]);
        facts.accessible = false;
        assert!(quantity_opportunities(&facts).is_empty());
    }
}
