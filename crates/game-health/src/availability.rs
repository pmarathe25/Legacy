//! Health action enumeration and stock judgments from factual physical inputs.
use crate::*;
use game_core::*;
use game_ecs::Store;

#[derive(Clone, Copy, Debug)]
pub enum StockPlaceFacts {
    Area {
        same_area: bool,
        actor_tactical: bool,
    },
    Carried {
        same_place: bool,
        contact_clear: bool,
    },
}
#[derive(Clone, Copy, Debug)]
pub struct StockAccessFacts {
    pub good: Good,
    pub access: bool,
    pub capable: bool,
    pub place: StockPlaceFacts,
    pub available: Quantity,
}
pub fn stock_usable(facts: StockAccessFacts, good: Good, quantity: Quantity) -> bool {
    facts.good == good
        && facts.access
        && facts.capable
        && match facts.place {
            StockPlaceFacts::Area {
                same_area,
                actor_tactical,
            } => same_area && !actor_tactical,
            StockPlaceFacts::Carried {
                same_place,
                contact_clear,
            } => same_place && contact_clear,
        }
        && (quantity.0 == 0 || facts.available >= quantity)
}

#[derive(Clone, Copy, Debug)]
pub struct RememberedStockFacts {
    pub here: bool,
    pub held: bool,
    pub available: Quantity,
    pub tool_available: bool,
}
/// Refresh physically present observations before selecting medical supplies.
pub fn planning_stocks<'a>(
    actor: PersonId,
    now: SimTime,
    stocks: impl Iterator<Item = &'a StockKnowledge>,
    mut facts: impl FnMut(&StockKnowledge) -> RememberedStockFacts,
) -> Vec<RememberedStock> {
    stocks
        .filter(|stock| {
            matches!(
                stock.good,
                Good::Water | Good::Timber | Good::Tool | Good::Herb
            )
        })
        .filter_map(|stock| {
            let physical = facts(stock);
            if stock.good == Good::Tool && !physical.tool_available {
                return None;
            }
            let mut fact = stock.clone();
            if physical.here {
                fact.quantity = physical.available;
                fact.accessible = true;
                fact.observed_at = now;
                fact.learned_at = now;
                fact.source = KnowledgeSource::Observed(actor);
            }
            Some(RememberedStock {
                fact,
                here: physical.here,
                held: physical.held,
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct CaregiverAvailability {
    pub capable: bool,
    pub observable: bool,
}

/// Whether an actor can enter a shelter from their current physical state.
pub fn can_enter_shelter(stationary: bool, carried: bool) -> bool {
    stationary && !carried
}

impl Runtime {
    pub fn available_actions(
        &self,
        store: &Store,
        actor: ActorFacts,
        planning: Option<&HealthPlanning<'_>>,
        mut caregiver: impl FnMut(PersonId) -> CaregiverAvailability,
        mut stock: impl FnMut(InventoryId) -> Result<StockFacts, Error>,
    ) -> Vec<HealthAction> {
        let Some(area) = actor.area.filter(|_| actor.present) else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        for (_, _, offer) in all_offers(store)
            .filter(|(_, _, offer)| offer.patient == actor.actor && offer.expires > actor.now)
        {
            if !caregiver(offer.caregiver).capable {
                continue;
            }
            if !offer.accepted {
                actions.push(HealthAction::AcceptTreatment {
                    caregiver: offer.caregiver,
                });
            }
            actions.push(HealthAction::RefuseTreatment {
                caregiver: offer.caregiver,
            });
        }
        for person in knowledge(store, actor.actor)
            .into_iter()
            .flat_map(|known| known.people.keys())
            .filter(|person| **person != actor.actor)
        {
            let facts = caregiver(*person);
            if facts.observable && facts.capable {
                actions.push(HealthAction::RequestTreatment { caregiver: *person });
            }
        }
        if !actor.capable {
            return actions;
        }
        if let Some(input) = planning {
            for remembered in input.stocks.iter().filter(|remembered| {
                remembered.fact.good == Good::Water && remembered.fact.accessible && remembered.here
            }) {
                if stock(remembered.fact.inventory).is_ok_and(|facts| {
                    facts.good == Good::Water
                        && facts.usable
                        && facts.available >= self.rules.drink_quantity
                }) {
                    actions.push(HealthAction::Drink {
                        source: remembered.fact.inventory,
                    });
                }
            }
            for remembered in input.stocks {
                let Some(action) =
                    self.boiling_action(store, actor.actor, remembered.fact.inventory, input)
                else {
                    continue;
                };
                let HealthAction::Boil { source, fuel, tool } = action else {
                    continue;
                };
                let facts = stock(source).and_then(|water| {
                    stock(fuel).and_then(|fuel| {
                        stock(tool).map(|tool| ActionFacts::Boil { water, fuel, tool })
                    })
                });
                if facts
                    .is_ok_and(|facts| self.prepare_action(store, actor, &action, facts).is_ok())
                {
                    actions.push(action);
                }
            }
        }
        let can_enter = actor.at_area && !actor.carried;
        for shelter in knowledge(store, actor.actor)
            .into_iter()
            .flat_map(|known| known.shelters.values())
            .filter(|shelter| {
                can_enter
                    && shelter.area == area
                    && shelter.accessible
                    && occupancy(store, actor.actor).map(|row| row.shelter) != Some(shelter.id)
                    && shelter.known_occupants.len() < shelter.capacity as usize
            })
        {
            actions.push(HealthAction::EnterShelter {
                shelter: shelter.id,
            });
        }
        if can_enter && occupancy(store, actor.actor).is_some() {
            actions.push(HealthAction::LeaveShelter);
        }
        actions
    }
    pub fn actor_busy(&self, store: &Store, actor: PersonId) -> bool {
        job(store, actor).is_some() || self.patient_busy(store, actor)
    }
    pub fn work_exerting(
        &self,
        store: &Store,
        actor: PersonId,
        can_progress: impl FnOnce(&Job) -> bool,
    ) -> bool {
        job(store, actor)
            .is_some_and(|job| !matches!(job.work, Work::Drink { .. }) && can_progress(job))
    }
}
impl Runtime {
    pub fn quantity_examples(&self, good: Good) -> Vec<Quantity> {
        match good {
            Good::Water => vec![self.rules.drink_quantity, self.rules.treatment_water],
            Good::Timber => vec![self.rules.boiling_fuel],
            Good::Herb => vec![self.rules.treatment_herbs],
            Good::Tool => vec![Quantity(1_000)],
            _ => Vec::new(),
        }
    }
}
