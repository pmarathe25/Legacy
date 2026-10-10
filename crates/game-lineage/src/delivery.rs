use crate::*;
use game_core::*;
use game_logistics::{Delivery, DeliveryReceipt, Destination, Requirement};
use std::collections::BTreeMap;

/// Body/contact and personal source observations, not an overall permission or
/// willingness verdict. No remote current balance is provided to the guardian.
pub struct SupplyFacts {
    pub now: SimTime,
    pub requester_independent: bool,
    pub requester_available: bool,
    pub fresh_contact: bool,
    pub guardian_capable: bool,
    pub guardian_free: bool,
    pub known_source: Option<StockKnowledge>,
    pub source_route_known: bool,
    pub immediate_care: bool,
    pub requester_area: Option<AreaId>,
    pub source_carried: bool,
    pub room: Quantity,
    pub carrying_capacity: Quantity,
    pub cursor: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupplyAnswer {
    Accept(RequestedDelivery),
    Refuse(&'static str),
}
impl World {
    pub fn judge_supply(
        &self,
        guardian: PersonId,
        requester: PersonId,
        good: Good,
        source: InventoryId,
        quantity: Quantity,
        f: &SupplyFacts,
    ) -> Result<SupplyAnswer, Error> {
        if self.current() != Some(requester)
            || !self.guardians.contains_key(&guardian)
            || guardian == requester
            || !f.requester_independent
            || !f.requester_available
            || !f.fresh_contact
            || quantity.0 == 0
        {
            return Err(Error::Unavailable);
        }
        let refuse = |reason| Ok(SupplyAnswer::Refuse(reason));
        if !f.guardian_capable {
            return refuse("My body or confinement prevents this assistance");
        }
        if !f.guardian_free {
            return refuse("I must finish my current physical assistance first");
        }
        let Some(known) = &f.known_source else {
            return refuse("I do not know that source");
        };
        if known.inventory != source
            || known.good != good
            || !known.accessible
            || known.quantity < quantity
            || known.observed_at > f.now
            || known.learned_at > f.now
        {
            return refuse("I do not know enough usable goods at that source");
        }
        if !f.source_carried && !f.source_route_known {
            return refuse("I do not know a traversable route to that source");
        }
        if f.immediate_care && !f.source_carried && Some(known.area) != f.requester_area {
            return refuse("I will not leave while you visibly need immediate care");
        }
        if quantity > f.carrying_capacity || !f.source_carried && quantity > f.room {
            return refuse("That amount exceeds my available carrying capacity");
        }
        Ok(SupplyAnswer::Accept(RequestedDelivery {
            delivery: Delivery::new(
                Requirement {
                    good,
                    quantity,
                    source: Some(source),
                },
                Destination::Contact(requester),
            )?,
            accepted_at: f.now,
            cargo: if f.source_carried {
                BTreeMap::from([(source, quantity.0)])
            } else {
                BTreeMap::new()
            },
            last_event: f.cursor,
        }))
    }
}
/// Actual transfer/consumption facts supplied by physical stock owners. Tags
/// follow the conserved source portion even if somebody steals or consumes it.
pub enum SupplyReceipt {
    Transfer {
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        good: Good,
        quantity: Quantity,
        destination_holder: Option<PersonId>,
    },
    Lost {
        inventory: InventoryId,
        quantity: Quantity,
    },
}
impl RequestedDelivery {
    pub fn receive(
        &mut self,
        guardian: PersonId,
        sequence: EventId,
        receipt: &SupplyReceipt,
    ) -> Result<Quantity, Error> {
        if sequence.0 == 0 {
            return Err(Error::Unavailable);
        }
        if sequence.0 <= self.last_event {
            return Ok(Quantity(0));
        }
        let mut after = self.clone();
        let mut credited = Quantity(0);
        match receipt {
            SupplyReceipt::Transfer {
                actor,
                source,
                destination,
                good,
                quantity,
                destination_holder,
            } => {
                if source == destination && quantity.0 > 0 {
                    return Err(Error::Unavailable);
                }
                let tagged = after.cargo.remove(source).unwrap_or(0);
                let moved = tagged.min(quantity.0);
                if tagged > moved {
                    after.cargo.insert(*source, tagged - moved);
                }
                if *actor == guardian && *good == after.delivery.requirement.good {
                    let qualified = if Some(*source) == after.delivery.requirement.source {
                        quantity.0
                    } else {
                        moved
                    };
                    if matches!(after.delivery.destination,Destination::Contact(person) if *destination_holder==Some(person))
                    {
                        credited = Quantity(qualified.min(after.delivery.remaining.0));
                        if credited.0 > 0 {
                            after.delivery.delivered(&DeliveryReceipt {
                                sequence: sequence.0,
                                destination: after.delivery.destination,
                                good: *good,
                                quantity: credited,
                            })?;
                        }
                    } else if *destination_holder == Some(guardian) {
                        let existing = after.cargo.values().map(|q| u128::from(*q)).sum::<u128>();
                        let room =
                            u128::from(after.delivery.remaining.0).saturating_sub(existing) as u64;
                        let picked = qualified.min(room);
                        if picked > 0 {
                            *after.cargo.entry(*destination).or_default() += picked;
                        }
                    }
                }
            }
            SupplyReceipt::Lost {
                inventory,
                quantity,
            } => {
                if let Some(held) = after.cargo.get_mut(inventory) {
                    *held = held.saturating_sub(quantity.0);
                    if *held == 0 {
                        after.cargo.remove(inventory);
                    }
                }
            }
        }
        after.last_event = sequence.0;
        *self = after;
        Ok(credited)
    }
}
