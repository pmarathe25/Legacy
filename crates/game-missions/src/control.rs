//! Individual progression from communicated objectives and narrow physical facts.
use crate::mission as stored_mission;
use crate::*;
use game_core::*;
use game_ecs::Store;
use game_logistics::{AcquisitionSource, Delivery, DeliveryFacts, KnownDestination, Step};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberActivity {
    MissionTalk,
    ArmyTalk,
    ArmyApproach,
    Eat,
    Load,
    Tell,
    Care,
    Lesson,
    WritingMemory,
    ReadingMemory,
    TeachingMemory,
    MovingRecord,
    Escape,
    Restrain,
    Unbind,
    Other,
}
impl MemberActivity {
    fn occupied(self) -> bool {
        self != Self::Other
    }
}
#[derive(Clone, Copy, Debug)]
pub struct MemberInput<'a> {
    pub actor: PersonId,
    pub now: SimTime,
    pub player: bool,
    pub in_combat: bool,
    pub area: Option<AreaId>,
    pub communication_busy: bool,
    pub activity: MemberActivity,
    pub care_urgency: i64,
    pub hunger: u64,
    pub meal_relief: u64,
    pub meal_size: Quantity,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub health: u64,
    pub max_health: u64,
    pub motives: &'a ServiceMotives,
}
#[derive(Clone, Copy, Debug)]
pub struct FoodObservation {
    pub inventory: InventoryId,
    pub good: Good,
    pub accessible: bool,
    pub quantity: Quantity,
    pub area: AreaId,
    pub carried_by_actor: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct TargetFacts {
    pub at_area: Option<AreaId>,
    pub available_area: bool,
    pub player: bool,
    pub alive: bool,
    pub can_fight: bool,
    pub captor: Option<PersonId>,
    pub trust: i32,
    pub empathy: u16,
    pub hunger: u64,
}
pub struct MemberDelivery<'a> {
    pub delivery: Delivery,
    pub destination: KnownDestination,
    pub minimum_handoff: Quantity,
    pub origin_cargo: Option<&'a BTreeMap<InventoryId, u64>>,
}
/// These methods supply knowledge and physical facts; none returns a decision.
pub trait MemberFacts<A>: AcquisitionSource<A> {
    fn food(&self) -> impl Iterator<Item = FoodObservation>;
    fn source_good(&self, inventory: InventoryId) -> Option<Good>;
    fn inventory_destination(&self, inventory: InventoryId) -> Option<KnownDestination>;
    fn actor_location(&self) -> Location;
    fn objective_knowledge(&self, objective: &MissionObjective) -> ObjectiveKnowledge;
    fn target(&self, person: PersonId) -> TargetFacts;
    fn delivery_facts(&self, request: &MemberDelivery<'_>) -> DeliveryFacts<'_, A>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberExplanation {
    Guardian,
    SupplyPermission,
    SupplyRoute,
    SourceUnknown,
    DestinationUnknown,
    DeliveryBlocked,
    RouteUnknown,
    ObjectiveUnknown,
}
impl MemberExplanation {
    pub fn text(self) -> &'static str {
        match self {
            Self::Guardian => "Withdrew to search for and care for the new or unready incarnation",
            Self::SupplyPermission => "Waiting for personally known supply permission",
            Self::SupplyRoute => {
                "Waiting for personally known qualified provisions and a usable route"
            }
            Self::SourceUnknown => "The delivery source is not personally known",
            Self::DestinationUnknown => "The delivery destination is not personally known",
            Self::DeliveryBlocked => {
                "Waiting for personally known qualified cargo, receiving room or a usable route"
            }
            Self::RouteUnknown => "No personally known usable route to the objective",
            Self::ObjectiveUnknown => {
                "The party lacks a credible observed location for its objective"
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemberAction<A> {
    Opaque(A),
    Mission(MissionAction),
    Eat(InventoryId),
    Rest,
    Carry {
        source: InventoryId,
        quantity: Quantity,
    },
    Transfer {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    Travel(game_transport::RouteIntent),
    Move {
        actor: PersonId,
        destination: AreaId,
    },
    Free(PersonId),
    Restrain(PersonId),
    Engage(PersonId),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure<A> {
    Action(MemberAction<A>),
    Explain(MemberExplanation),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberAttempt<A> {
    pub action: MemberAction<A>,
    failure: Option<Failure<A>>,
}
impl<A> MemberAttempt<A> {
    pub fn after_result(self, success: bool) -> Option<Failure<A>> {
        if success { None } else { self.failure }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberDecision<A> {
    pub attempts: Vec<MemberAttempt<A>>,
    pub explanation: Option<MemberExplanation>,
}
impl<A> Default for MemberDecision<A> {
    fn default() -> Self {
        Self {
            attempts: Vec::new(),
            explanation: None,
        }
    }
}
impl<A> MemberDecision<A> {
    fn action(action: MemberAction<A>) -> Self {
        Self {
            attempts: vec![MemberAttempt {
                action,
                failure: None,
            }],
            explanation: None,
        }
    }
    fn explain(explanation: MemberExplanation) -> Self {
        Self {
            explanation: Some(explanation),
            ..Self::default()
        }
    }
}
impl Runtime {
    pub fn member_decision<A: Clone>(
        &self,
        store: &Store,
        id: MissionId,
        input: MemberInput<'_>,
        facts: &impl MemberFacts<A>,
    ) -> MemberDecision<A> {
        let actor = input.actor;
        if input.player || input.in_combat {
            return MemberDecision::default();
        }
        let Some(m) = stored_mission(store, id) else {
            return MemberDecision::default();
        };
        let Some(member) = m
            .members
            .get(&actor)
            .filter(|m| m.status == CompanionStatus::Accepted)
        else {
            return MemberDecision::default();
        };
        let Some(area) = input.area else {
            return MemberDecision::default();
        };
        let status = knowledge(store, actor)
            .map(|row| &row.missions)
            .and_then(|known| known.get(&id))
            .map_or(m.status, |view| view.status);
        if input.motives.guardian_duty {
            let mut result = self
                .service_withdrawal(store, actor, id, input.motives)
                .map(|action| MemberDecision::action(MemberAction::Mission(action)))
                .unwrap_or_default();
            result.explanation = Some(MemberExplanation::Guardian);
            return result;
        }
        if input.communication_busy || input.activity.occupied() || input.care_urgency >= 20_000 {
            return MemberDecision::default();
        }
        if input.hunger >= input.meal_relief {
            return facts
                .food()
                .filter(|k| {
                    k.good == Good::Food
                        && k.accessible
                        && k.quantity >= input.meal_size
                        && k.area == area
                })
                .min_by_key(|k| (!k.carried_by_actor, k.inventory))
                .map(|k| MemberDecision::action(MemberAction::Eat(k.inventory)))
                .unwrap_or_default();
        }
        if input.fatigue >= input.fatigue_limit / 2 || input.health < input.max_health / 2 {
            return MemberDecision::action(MemberAction::Rest);
        }
        if let Some(action) = self.service_withdrawal(store, actor, id, input.motives) {
            return MemberDecision::action(MemberAction::Mission(action));
        }
        if matches!(status, MissionStatus::Preparing | MissionStatus::Traveling) {
            for supply in m.proposal.supplies.iter().filter(|s| s.carrier == actor) {
                let Some(good) = facts.source_good(supply.source) else {
                    return MemberDecision::explain(MemberExplanation::SupplyPermission);
                };
                let request = MemberDelivery {
                    delivery: Delivery {
                        requirement: game_logistics::Requirement {
                            good,
                            quantity: supply.quantity,
                            source: Some(supply.source),
                        },
                        destination: game_logistics::Destination::Contact(actor),
                        remaining: supply.quantity,
                        last_receipt: 0,
                    },
                    destination: KnownDestination {
                        destination: game_logistics::Destination::Contact(actor),
                        location: facts.actor_location(),
                        observed_at: input.now,
                        accessible: true,
                        room: None,
                        direct_transfer: false,
                    },
                    minimum_handoff: supply.quantity,
                    origin_cargo: member.supply_cargo.get(&supply.source),
                };
                let delivered = facts.delivery_facts(&request);
                // The route facts remain borrowed after the planner consumes its transient collections.
                let place = delivered.place.clone();
                let routes = delivered.routes;
                let at = delivered.now;
                let step = request.delivery.plan_with_acquisition(
                    delivered,
                    request.minimum_handoff,
                    facts,
                );
                if matches!(step, Step::Arrived { .. }) {
                    continue;
                }
                return delivery_decision(step, &place, routes, at, MemberExplanation::SupplyRoute);
            }
            if status == MissionStatus::Preparing {
                return MemberDecision::action(MemberAction::Rest);
            }
        }
        if status != MissionStatus::Returning
            && let MissionObjective::Deliver {
                source,
                destination,
                quantity,
            } = member.directive
        {
            let Some(good) = facts.source_good(source) else {
                return MemberDecision::explain(MemberExplanation::SourceUnknown);
            };
            let Some(goal) = facts.inventory_destination(destination) else {
                return MemberDecision::explain(MemberExplanation::DestinationUnknown);
            };
            let request = MemberDelivery {
                delivery: Delivery {
                    requirement: game_logistics::Requirement {
                        good,
                        quantity,
                        source: Some(source),
                    },
                    destination: game_logistics::Destination::Inventory(destination),
                    remaining: Quantity(quantity.0.saturating_sub(member.delivered)),
                    last_receipt: 0,
                },
                destination: goal,
                minimum_handoff: Quantity(1),
                origin_cargo: Some(&member.cargo),
            };
            let delivered = facts.delivery_facts(&request);
            let place = delivered.place.clone();
            let routes = delivered.routes;
            let at = delivered.now;
            let step =
                request
                    .delivery
                    .plan_with_acquisition(delivered, request.minimum_handoff, facts);
            return delivery_decision(step, &place, routes, at, MemberExplanation::DeliveryBlocked);
        }
        let destination = if status == MissionStatus::Returning {
            Some(m.proposal.home)
        } else {
            objective_destination(
                member,
                &member.directive,
                &facts.objective_knowledge(&member.directive),
                Some(area),
            )
        };
        let Some(destination) = destination else {
            return MemberDecision::explain(MemberExplanation::ObjectiveUnknown);
        };
        if destination != area {
            let mut result = MemberDecision::default();
            if let MissionObjective::Rescue { person, return_to } = member.directive
                && destination == return_to
            {
                let target = facts.target(person);
                if target.at_area == Some(area)
                    && target.available_area
                    && !target.player
                    && target.trust >= 0
                    && target.empathy >= 300
                    && target.hunger < input.meal_relief
                {
                    result.attempts.push(MemberAttempt {
                        action: MemberAction::Move {
                            actor: person,
                            destination: return_to,
                        },
                        failure: None,
                    });
                }
            }
            result.attempts.push(MemberAttempt {
                action: MemberAction::Move { actor, destination },
                failure: Some(Failure::Explain(MemberExplanation::RouteUnknown)),
            });
            return result;
        }
        match member.directive {
            MissionObjective::Defeat { person } => {
                let target = facts.target(person);
                if target.at_area == Some(area) && target.alive && target.can_fight {
                    MemberDecision::action(MemberAction::Engage(person))
                } else {
                    MemberDecision::default()
                }
            }
            MissionObjective::Confine { person, until } => {
                let target = facts.target(person);
                if target.at_area != Some(area) {
                    return MemberDecision::default();
                }
                if let Some(captor) = target.captor {
                    if !m.members.contains_key(&captor) {
                        return MemberDecision::default();
                    }
                    return MemberDecision::action(if input.now >= until {
                        MemberAction::Free(person)
                    } else {
                        MemberAction::Rest
                    });
                }
                if input.now < until && target.alive {
                    MemberDecision {
                        attempts: vec![MemberAttempt {
                            action: MemberAction::Restrain(person),
                            failure: target
                                .can_fight
                                .then_some(Failure::Action(MemberAction::Engage(person))),
                        }],
                        explanation: None,
                    }
                } else {
                    MemberDecision::default()
                }
            }
            MissionObjective::Rescue { person, .. } => {
                let target = facts.target(person);
                if target.at_area == Some(area) && target.captor.is_some() {
                    MemberDecision::action(MemberAction::Free(person))
                } else {
                    MemberDecision::default()
                }
            }
            _ => MemberDecision::action(MemberAction::Rest),
        }
    }
}
fn delivery_decision<A>(
    step: Step<A>,
    place: &Location,
    routes: &game_logistics::KnownRoutes,
    at: SimTime,
    blocked: MemberExplanation,
) -> MemberDecision<A> {
    let action = match step {
        Step::Acquire(a) => Some(MemberAction::Opaque(a)),
        Step::Carry { source, quantity } => Some(MemberAction::Carry { source, quantity }),
        Step::Transfer {
            source,
            destination,
            quantity,
        } => Some(MemberAction::Transfer {
            source,
            destination,
            quantity,
        }),
        Step::Travel(target) => game_transport::route_intent(place, &target, at, routes)
            .ok()
            .map(MemberAction::Travel),
        _ => None,
    };
    action
        .map(MemberDecision::action)
        .unwrap_or_else(|| MemberDecision::explain(blocked))
}
#[cfg(test)]
mod tests;
