//! Reciprocal links to physical placement and the current lifting action.
use crate::*;
pub trait TransportReferences {
    fn lifting(&self, actor: PersonId) -> Option<(PersonId, SimTime)>;
    fn lifters(&self) -> impl Iterator<Item = (PersonId, PersonId, SimTime)>;
}
pub fn validate_composition(
    runtime: &Runtime,
    store: &game_ecs::Store,
    bodies: &BTreeMap<PersonId, Body>,
    references: &impl TransportReferences,
    now: SimTime,
) -> Result<(), Error> {
    validate_runtime(runtime, store, bodies, now)?;
    let fail = || Error::InvalidSave("invalid physical transport references".into());
    for actor in bodies.keys() {
        let location =
            game_spatial::location(store, game_ecs::StableId(actor.0)).ok_or_else(fail)?;
        if let Location::Carried { carrier } = *location {
            if !group(store, *actor).is_some_and(|group| group.anchor == carrier)
                || !route_steps(store, *actor).is_empty()
            {
                return Err(fail());
            }
        } else if group(store, *actor).is_some() {
            return Err(fail());
        }
    }
    for (passenger, lift) in lifts(store) {
        for bearer in &lift.bearers {
            if references.lifting(*bearer) != Some((passenger, lift.completes)) {
                return Err(fail());
            }
        }
    }
    for (actor, passenger, completes) in references.lifters() {
        if !lift(store, passenger)
            .is_some_and(|lift| lift.bearers.contains(&actor) && lift.completes == completes)
        {
            return Err(fail());
        }
    }
    Ok(())
}

pub trait MotionReferences {
    fn actors(&self) -> impl Iterator<Item = PersonId>;
    fn arrivals(&self) -> impl Iterator<Item = (PersonId, SimTime)>;
    fn alive(&self, actor: PersonId) -> Option<bool>;
    fn physical_enabled(&self) -> bool;
    fn area_exists(&self, area: AreaId) -> bool;
    fn duration(&self, from: AreaId, to: AreaId) -> Option<SimDuration>;
}
pub fn validate_motion(
    store: &game_ecs::Store,
    facts: &impl MotionReferences,
    now: SimTime,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("invalid transport movement references".into());
    let actors = facts.actors().collect::<BTreeSet<_>>();
    let mut arrivals = BTreeMap::new();
    for (actor, at) in facts.arrivals() {
        if !actors.contains(&actor) || arrivals.insert(actor, at).is_some() {
            return Err(fail());
        }
    }
    for actor in actors {
        let location =
            game_spatial::location(store, game_ecs::StableId(actor.0)).ok_or_else(fail)?;
        game_spatial::validate(location).map_err(|_| fail())?;
        let arrival = arrivals.get(&actor).copied();
        match *location {
            Location::AtArea(area) => {
                if !facts.area_exists(area)
                    || !route_steps(store, actor).is_empty()
                    || arrival.is_some()
                {
                    return Err(fail());
                }
            }
            Location::InTransit {
                from,
                to,
                departed,
                arrives,
                road_start,
            } => {
                let duration = facts
                    .duration(from, to)
                    .filter(|duration| duration.0 > 0)
                    .ok_or_else(fail)?;
                let expected = if let Some(point) = road_start {
                    if facts.duration(point.from, point.to) != Some(point.duration) {
                        return Err(fail());
                    }
                    let leg = retreat(point, to, duration, SimTime(0)).map_err(|_| fail())?;
                    if leg.from != from {
                        return Err(fail());
                    }
                    leg.arrives.0
                } else {
                    duration.0
                };
                let dead = facts.physical_enabled() && !facts.alive(actor).ok_or_else(fail)?;
                let deadline =
                    approach_job(store, actor).map_or(arrives, |approach| approach.completes);
                if departed > now
                    || arrives.0.checked_sub(departed.0) != Some(expected)
                    || (!dead && (arrives <= now || arrival != Some(deadline)))
                    || (dead && arrival.is_some())
                {
                    return Err(fail());
                }
            }
            Location::StoppedOnRoad {
                from,
                to,
                progress,
                duration,
            } => {
                if facts.duration(from, to) != Some(duration)
                    || progress > duration
                    || !route_steps(store, actor).is_empty()
                    || arrival.is_some()
                {
                    return Err(fail());
                }
            }
            Location::Carried { carrier } => {
                if carrier == actor
                    || game_spatial::location(store, game_ecs::StableId(carrier.0)).is_none()
                    || !route_steps(store, actor).is_empty()
                    || arrival.is_some()
                    || !group(store, actor).is_some_and(|group| group.anchor == carrier)
                {
                    return Err(fail());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
