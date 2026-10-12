//! A reached dependent's remembered care needs and actual physical support.
use game_core::*;
use game_transport::{Action, BodyFacts, Runtime};

pub struct RememberedRescueSignal<'a> {
    pub location: &'a Location,
    pub preference: Option<ChildhoodInterest>,
    pub urgency: i64,
    pub observed_at: SimTime,
}
/// Personal observations; `carried` supplies current stock placement.
pub trait RescueMemory {
    fn signal(&self, child: PersonId) -> Option<RememberedRescueSignal<'_>>;
    fn stocks(&self) -> impl Iterator<Item = &StockKnowledge>;
    fn carried(&self, inventory: InventoryId) -> bool;
}
#[derive(Clone, Copy)]
pub struct GuardianRescueFacts<'a> {
    pub now: SimTime,
    pub guardian_location: &'a Location,
    pub co_present: bool,
    pub moving: bool,
    pub idle_or_resting: bool,
    pub combat_restricted: bool,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub meal_size: Quantity,
    pub target_independent: bool,
    pub target_health: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RescueIntent {
    Care {
        child: PersonId,
        source: InventoryId,
    },
    Comfort {
        child: PersonId,
    },
    Rest,
    Transport(Action),
}
#[allow(
    clippy::too_many_arguments,
    reason = "separate owner rows, participant IDs, care facts, personal memory and physical facts"
)]
pub fn guardian_rescue(
    store: &game_ecs::Store,
    transport: &Runtime,
    guardian: PersonId,
    target: PersonId,
    facts: GuardianRescueFacts<'_>,
    memory: &impl RescueMemory,
    bodies: &impl BodyFacts,
) -> Option<RescueIntent> {
    if !facts.co_present
        || facts.moving
        || transport.busy(store, guardian)
        || !facts.idle_or_resting
        || facts.combat_restricted
    {
        return None;
    }
    if matches!(facts.guardian_location, Location::StoppedOnRoad { .. }) {
        if let Some(signal) = memory.signal(target).filter(|signal| {
            signal.location == facts.guardian_location && signal.observed_at == facts.now
        }) {
            if signal.urgency > 0
                && let Some(source) = memory
                    .stocks()
                    .filter(|source| {
                        source.good == Good::Food
                            && source.accessible
                            && source.quantity >= facts.meal_size
                            && memory.carried(source.inventory)
                    })
                    .min_by_key(|source| source.inventory)
            {
                return Some(RescueIntent::Care {
                    child: target,
                    source: source.inventory,
                });
            }
            if signal.preference == Some(ChildhoodInterest::Company) {
                return Some(RescueIntent::Comfort { child: target });
            }
        }
        if facts.fatigue >= facts.fatigue_limit {
            return Some(RescueIntent::Rest);
        }
        if game_transport::group(store, target)
            .is_some_and(|group| group.bearers.contains(&guardian))
            && transport.can_depart(store, bodies, guardian)
        {
            return Some(RescueIntent::Transport(Action::Resume));
        }
    }
    if !facts.target_independent || facts.target_health == 0 {
        let needs_support = game_transport::group(store, target).is_none_or(|group| {
            group
                .bearers
                .iter()
                .any(|actor| !bodies.body(*actor).is_some_and(|body| body.can_hold))
                || transport
                    .component_plan(store, bodies, target)
                    .is_ok_and(|plan| plan.overloaded.contains(&target))
        });
        if needs_support && bodies.body(guardian).is_some_and(|body| body.can_bear) {
            return Some(RescueIntent::Transport(
                if game_transport::group(store, target).is_some() {
                    Action::JoinCarry { passenger: target }
                } else {
                    Action::OfferCarry { passenger: target }
                },
            ));
        }
    }
    None
}
#[cfg(test)]
#[path = "rescue/tests.rs"]
mod tests;
