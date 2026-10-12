use crate::*;
pub fn validate_runtime(
    world: &Runtime,
    store: &game_ecs::Store,
    bodies: &BTreeMap<PersonId, Body>,
    now: SimTime,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("invalid transport state".into());
    if !world.rules.valid() {
        return Err(fail());
    }
    for (passenger, group) in groups(store) {
        group.validate().map_err(|_| fail())?;
        if passenger != group.passenger
            || group.accepted_at > now
            || group.bearers.is_empty()
            || group.bearers.len() > world.rules.max_bearers
            || !group.bearers.contains(&group.anchor)
            || group.bearers.contains(&passenger)
            || group
                .bearers
                .iter()
                .any(|id| crate::group(store, *id).is_some())
        {
            return Err(fail());
        }
        let body = bodies.get(&passenger).ok_or_else(fail)?;
        for bearer in &group.bearers {
            if !bodies
                .get(bearer)
                .ok_or_else(fail)?
                .position
                .same_point(body.position)
            {
                return Err(fail());
            }
        }
    }
    for (passenger, inbox) in offer_inboxes(store) {
        inbox.validate().map_err(|_| fail())?;
        let offers = &inbox.by_bearer;
        if !bodies.contains_key(&passenger)
            || offers.is_empty()
            || offers.len() > world.rules.max_bearers
        {
            return Err(fail());
        }
        for (bearer, offer) in offers {
            if !bodies.contains_key(bearer)
                || *bearer == passenger
                || offer.bearer != *bearer
                || offer.passenger != passenger
                || offer.offered_at > now
                || offer.expires <= offer.offered_at
                || offer.expires.0 - offer.offered_at.0 != world.rules.offer_lifetime.0
            {
                return Err(fail());
            }
        }
    }
    let mut occupied = BTreeSet::new();
    for (passenger, lift) in lifts(store) {
        lift.validate().map_err(|_| fail())?;
        if passenger != lift.passenger
            || lift.started > now
            || lift.completes <= now
            || lift.bearers.is_empty()
            || lift.bearers.len() > world.rules.max_bearers
            || lift.bearers.contains(&passenger)
            || lift.completes.0.checked_sub(lift.started.0) != Some(world.rules.lift_time.0)
            || !occupied.insert(passenger)
        {
            return Err(fail());
        }
        let body = bodies.get(&passenger).ok_or_else(fail)?;
        for bearer in &lift.bearers {
            if !occupied.insert(*bearer)
                || group(store, *bearer).is_some()
                || !bodies
                    .get(bearer)
                    .ok_or_else(fail)?
                    .position
                    .same_point(body.position)
            {
                return Err(fail());
            }
        }
    }
    for body in bodies.values() {
        if body.mass.0 == 0
            || matches!(body.position, Position::Road(position) if !position.valid())
        {
            return Err(fail());
        }
    }
    for (actor, approach) in approaches(store) {
        approach.validate().map_err(|_| fail())?;
        if !bodies.contains_key(&actor)
            || group(store, actor).is_some()
            || !approach.target.valid()
            || approach.started > now
            || approach.completes <= now
        {
            return Err(fail());
        }
    }
    for (actor, target) in resume_targets(store) {
        target.validate().map_err(|_| fail())?;
        let body = bodies.get(&actor).ok_or_else(fail)?;
        if !matches!(body.position,Position::Road(point) if target.area==point.from||target.area==point.to)
        {
            return Err(fail());
        }
    }
    for (actor, paused) in paused_approaches(store) {
        paused.validate().map_err(|_| fail())?;
        let target = paused.target;
        if !target.valid() || !bodies.contains_key(&actor) || approach_job(store, actor).is_some() {
            return Err(fail());
        }
    }
    if world.responses.len() > 64 {
        return Err(fail());
    }
    for response in &world.responses {
        response.validate().map_err(|_| fail())?;
        if !bodies.contains_key(&response.passenger)
            || !bodies.contains_key(&response.bearer)
            || response.observed_at > now
            || response.reason.is_empty()
            || response.reason.len() > 256
        {
            return Err(fail());
        }
    }
    for (observer, knowledge) in support_knowledges(store) {
        knowledge.validate().map_err(|_| fail())?;
        let requests = &knowledge.by_passenger;
        if !bodies.contains_key(&observer) || requests.len() > 64 {
            return Err(fail());
        }
        for (passenger, request) in requests {
            if *passenger != request.passenger
                || !bodies.contains_key(passenger)
                || !bodies.contains_key(&request.requester)
                || request.observed_at > now
                || request.expires <= request.observed_at
            {
                return Err(fail());
            }
        }
    }
    Ok(())
}

/// The composition must not restore personal transport rows without its owner.
pub fn validate_absent(store: &game_ecs::Store) -> Result<(), Error> {
    if groups(store).next().is_some()
        || offer_inboxes(store).next().is_some()
        || lifts(store).next().is_some()
        || approaches(store).next().is_some()
        || resume_targets(store).next().is_some()
        || paused_approaches(store).next().is_some()
        || support_knowledges(store).next().is_some()
    {
        return Err(Error::InvalidSave("transport rows have no runtime".into()));
    }
    Ok(())
}
