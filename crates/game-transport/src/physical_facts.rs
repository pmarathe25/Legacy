//! Transport interprets factual body limits and work state.
use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarryWork {
    Idle,
    Rest,
    Lifting,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyInput {
    pub position: Position,
    pub mass: Quantity,
    pub cargo: Quantity,
    pub capacity: Quantity,
    pub alive: bool,
    pub health: u64,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub independent: bool,
    pub combat_restricted: bool,
    pub carried: bool,
    pub work: CarryWork,
    pub incapacitation: Option<u64>,
}
impl Body {
    pub fn from_facts(facts: BodyInput) -> Self {
        let can_walk = facts.alive
            && facts.health > 0
            && facts.fatigue < facts.fatigue_limit
            && facts.independent
            && !facts.combat_restricted
            && !facts.carried;
        Self {
            position: facts.position,
            mass: facts.mass,
            cargo: facts.cargo,
            capacity: facts.capacity,
            can_hold: facts.alive
                && facts.health > 0
                && facts
                    .incapacitation
                    .is_none_or(|limit| facts.health > limit),
            can_walk,
            can_bear: can_walk
                && matches!(
                    facts.work,
                    CarryWork::Idle | CarryWork::Rest | CarryWork::Lifting
                ),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn facts() -> BodyInput {
        BodyInput {
            position: Position::Area(AreaId(1)),
            mass: Quantity(60_000),
            cargo: Quantity(0),
            capacity: Quantity(40_000),
            alive: true,
            health: 100,
            fatigue: 0,
            fatigue_limit: 100,
            independent: true,
            combat_restricted: false,
            carried: false,
            work: CarryWork::Idle,
            incapacitation: Some(25),
        }
    }
    #[test]
    fn movement_bearing_and_holding_keep_their_distinct_actual_limits() {
        let ordinary = facts();
        assert!(Body::from_facts(ordinary.clone()).can_bear);
        let mut exhausted = ordinary.clone();
        exhausted.fatigue = exhausted.fatigue_limit;
        let body = Body::from_facts(exhausted);
        assert!(!body.can_walk);
        assert!(!body.can_bear);
        assert!(body.can_hold);
        let mut working = ordinary.clone();
        working.work = CarryWork::Other;
        let body = Body::from_facts(working);
        assert!(body.can_walk);
        assert!(!body.can_bear);
        let mut incapacitated = ordinary.clone();
        incapacitated.health = 25;
        assert!(!Body::from_facts(incapacitated).can_hold);
        let mut carried = ordinary.clone();
        carried.carried = true;
        let body = Body::from_facts(carried);
        assert!(!body.can_walk);
        assert!(body.can_hold);
        let mut child = ordinary.clone();
        child.independent = false;
        assert!(!Body::from_facts(child).can_walk);
        let mut confined = ordinary.clone();
        confined.combat_restricted = true;
        assert!(!Body::from_facts(confined).can_walk);
        let mut dead = ordinary;
        dead.alive = false;
        let body = Body::from_facts(dead);
        assert!(!body.can_walk);
        assert!(!body.can_hold);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarryPermissionInput {
    pub independent: bool,
    pub combat_restricted: bool,
    pub alive: bool,
    pub health: u64,
    pub incapacitation: Option<u64>,
    pub yielded_captors: Option<BTreeSet<PersonId>>,
    pub captive_captor: Option<PersonId>,
}
impl Permission {
    pub fn from_facts(
        bearers: &BTreeSet<PersonId>,
        accepted: bool,
        facts: &CarryPermissionInput,
    ) -> Self {
        Self {
            passenger_accepted: accepted,
            unable_to_consent: !facts.alive
                || facts.health == 0
                || facts
                    .incapacitation
                    .is_some_and(|limit| facts.health <= limit)
                || !facts.independent,
            yielded: facts
                .yielded_captors
                .as_ref()
                .is_some_and(|captors| bearers.is_subset(captors)),
            captive: facts
                .captive_captor
                .is_some_and(|captor| bearers.contains(&captor)),
            newborn: false,
        }
    }
}

pub fn area_leg(
    from: AreaId,
    to: AreaId,
    duration: SimDuration,
    now: SimTime,
) -> Result<Leg, Error> {
    if from.0 == 0 || to.0 == 0 || from == to || duration.0 == 0 {
        return Err(Error::NoRoute);
    }
    Ok(Leg {
        from,
        to,
        departed: now,
        arrives: SimTime(now.0.checked_add(duration.0).ok_or(Error::Overflow)?),
        road_start: None,
    })
}

pub fn exerting(location: &Location, lifting: bool) -> bool {
    matches!(location, Location::InTransit { .. }) || lifting
}
