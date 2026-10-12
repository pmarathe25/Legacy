//! Strategic itinerary requests, including the kernel without carrying enabled.
use super::motion::{MotionFinalization, PreparedMotion};
use crate::*;
use game_ecs::{Batch, StableId, Store};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TravelBody {
    pub alive: bool,
    pub health: u64,
    pub fatigue: u64,
    pub fatigue_limit: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TravelReadiness {
    pub independent: bool,
    pub combat_restricted: bool,
    pub physical: Option<TravelBody>,
}
impl TravelReadiness {
    fn ready(&self) -> bool {
        self.independent
            && !self.combat_restricted
            && self.physical.as_ref().is_none_or(|body| {
                body.alive && body.health > 0 && body.fatigue < body.fatigue_limit
            })
    }
}
pub trait JourneyFacts: BodyFacts + RoadFacts {
    fn readiness(&self, actor: PersonId) -> Option<TravelReadiness>;
    fn navigation(&self, actor: PersonId) -> KnownRoutes;
    fn area_exists(&self, area: AreaId) -> bool;
}
pub struct TravelRequest {
    pub actor: PersonId,
    pub destination: AreaId,
}
pub struct PreparedJourney {
    motion: PreparedMotion,
    actor: PersonId,
    readiness: TravelReadiness,
    navigation: KnownRoutes,
    carrying: bool,
    road: Option<(AreaId, AreaId, SimDuration)>,
}
pub fn prepare_journey_request(
    store: &Store,
    request: TravelRequest,
    facts: &impl JourneyFacts,
    runtime: Option<&Runtime>,
    now: SimTime,
) -> Result<PreparedJourney, Error> {
    let readiness = facts.readiness(request.actor).ok_or(Error::Unavailable)?;
    if !readiness.ready()
        || runtime.is_some_and(|owner| !owner.can_depart(store, facts, request.actor))
    {
        return Err(Error::Unavailable);
    }
    if !facts.area_exists(request.destination) {
        return Err(Error::UnknownArea);
    }
    let navigation = facts.navigation(request.actor);
    let replacement = prepare_route_to(store, request.actor, request.destination, &navigation)?;
    let fallback = Runtime::new(Rules::default())?;
    let owner = runtime.unwrap_or(&fallback);
    let location =
        game_spatial::location(store, StableId(request.actor.0)).ok_or(Error::Unavailable)?;
    let (motion, road) = if let Location::AtArea(from) = location {
        let to = *replacement.steps().first().ok_or(Error::AlreadyThere)?;
        let duration = facts
            .duration(*from, to)
            .filter(|duration| duration.0 > 0)
            .ok_or(Error::NoRoute)?;
        if runtime.is_some() {
            let trip = owner.prepare_itinerary_departure(store, replacement, now, facts, facts)?;
            (trip.into_motion(), Some((*from, to, duration)))
        } else {
            validate_absent(store)?;
            let arrives = SimTime(now.0.checked_add(duration.0).ok_or(Error::Overflow)?);
            let (actor, before, after) = replacement.consume_next(to)?.into_edit(store)?;
            let mut motion = PreparedMotion::new(owner, now);
            motion.transport.guard_topology(store, [actor]);
            motion.locations.set(
                store,
                actor,
                Some(Location::InTransit {
                    from: *from,
                    to,
                    departed: now,
                    arrives,
                    road_start: None,
                }),
            );
            motion.transport.routes.edits.insert(actor, (before, after));
            motion.route_replacements.insert(actor);
            motion.transport.result.extend([
                MotionEffect::Interrupt(actor),
                MotionEffect::ClearArrival(actor),
                MotionEffect::LeaveShelter(actor),
                MotionEffect::DetachDependent(actor),
                MotionEffect::ScheduleArrival { actor, at: arrives },
                MotionEffect::Departed {
                    actor,
                    from: *from,
                    to,
                    arrives,
                },
            ]);
            (motion, Some((*from, to, duration)))
        }
    } else {
        let (actor, before, after) = replacement.into_edit(store)?;
        let mut motion = PreparedMotion::new(owner, now);
        let linked = motion.transport.guard_topology(store, [actor]);
        if runtime.is_some() {
            motion.transport.guard_bodies(facts, linked)?;
        }
        motion.locations.guard(store, actor);
        motion.transport.routes.edits.insert(actor, (before, after));
        motion.route_replacements.insert(actor);
        motion.transport.result.push(MotionEffect::Interrupt(actor));
        (motion, None)
    };
    Ok(PreparedJourney {
        motion,
        actor: request.actor,
        readiness,
        navigation,
        carrying: runtime.is_some(),
        road,
    })
}
impl PreparedJourney {
    pub fn projection(&self) -> &PreparedMotion {
        &self.motion
    }
    pub fn stage_into(
        self,
        runtime: Option<&Runtime>,
        store: &Store,
        facts: &impl JourneyFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        if runtime.is_some() != self.carrying
            || facts.readiness(self.actor).as_ref() != Some(&self.readiness)
            || facts.navigation(self.actor) != self.navigation
            || self
                .road
                .is_some_and(|(from, to, duration)| facts.duration(from, to) != Some(duration))
        {
            return Err(Error::Unavailable);
        }
        let fallback = Runtime::new(Rules::default())?;
        self.motion
            .stage_into(runtime.unwrap_or(&fallback), store, facts, now, batch)
    }
}

pub struct ContinuedJourney {
    pub actor: PersonId,
    pub origin: AreaId,
    pub destination: AreaId,
}
struct Continuing<'a, F> {
    facts: &'a F,
    navigation: KnownRoutes,
}
impl<F: JourneyFacts> BodyFacts for Continuing<'_, F> {
    fn body(&self, actor: PersonId) -> Option<Body> {
        self.facts.body(actor)
    }
}
impl<F: JourneyFacts> RoadFacts for Continuing<'_, F> {
    fn duration(&self, from: AreaId, to: AreaId) -> Option<SimDuration> {
        self.facts.duration(from, to)
    }
    fn known(&self, _: PersonId, from: AreaId, to: AreaId) -> bool {
        self.navigation
            .get(&from)
            .is_some_and(|links| links.contains_key(&to))
    }
}
impl<F: JourneyFacts> JourneyFacts for Continuing<'_, F> {
    fn readiness(&self, actor: PersonId) -> Option<TravelReadiness> {
        self.facts.readiness(actor)
    }
    fn navigation(&self, _: PersonId) -> KnownRoutes {
        self.navigation.clone()
    }
    fn area_exists(&self, area: AreaId) -> bool {
        self.facts.area_exists(area)
    }
}
fn continuing_facts<'a, F: JourneyFacts>(
    store: &Store,
    request: &ContinuedJourney,
    facts: &'a F,
) -> Result<Continuing<'a, F>, Error> {
    if game_spatial::location(store, StableId(request.actor.0))
        != Some(&Location::AtArea(request.origin))
        || route_steps(store, request.actor).first() != Some(&request.destination)
    {
        return Err(Error::Unavailable);
    }
    let mut from = request.origin;
    let mut navigation = KnownRoutes::new();
    for to in route_steps(store, request.actor) {
        navigation
            .entry(from)
            .or_default()
            .insert(*to, facts.duration(from, *to).ok_or(Error::NoRoute)?);
        from = *to;
    }
    Ok(Continuing { facts, navigation })
}
pub struct PreparedContinuation {
    journey: PreparedJourney,
    request: ContinuedJourney,
}
pub fn prepare_continued_journey(
    store: &Store,
    request: ContinuedJourney,
    facts: &impl JourneyFacts,
    runtime: Option<&Runtime>,
    now: SimTime,
) -> Result<PreparedContinuation, Error> {
    let continuing = continuing_facts(store, &request, facts)?;
    let destination = *route_steps(store, request.actor)
        .last()
        .ok_or(Error::Unavailable)?;
    let mut journey = prepare_journey_request(
        store,
        TravelRequest {
            actor: request.actor,
            destination,
        },
        &continuing,
        runtime,
        now,
    )?;
    journey
        .motion
        .transport
        .result
        .retain(|effect| !matches!(effect,MotionEffect::Interrupt(actor) if *actor==request.actor));
    Ok(PreparedContinuation { journey, request })
}
impl PreparedContinuation {
    pub fn projection(&self) -> &PreparedMotion {
        self.journey.projection()
    }
    pub fn stage_into(
        self,
        runtime: Option<&Runtime>,
        store: &Store,
        facts: &impl JourneyFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        let continuing = continuing_facts(store, &self.request, facts)?;
        self.journey
            .stage_into(runtime, store, &continuing, now, batch)
    }
}

pub struct PreparedRequestedStop {
    motion: PreparedMotion,
    actor: PersonId,
    readiness: TravelReadiness,
}
pub fn prepare_stop_request(
    store: &Store,
    actor: PersonId,
    runtime: &Runtime,
    now: SimTime,
    facts: &impl JourneyFacts,
) -> Result<PreparedRequestedStop, Error> {
    let readiness = facts.readiness(actor).ok_or(Error::Unavailable)?;
    if !readiness.independent
        || !readiness.physical.as_ref().is_some_and(|body| body.alive)
        || !matches!(
            game_spatial::location(store, StableId(actor.0)),
            Some(Location::InTransit { .. })
        )
    {
        return Err(Error::Unavailable);
    }
    Ok(PreparedRequestedStop {
        motion: runtime.prepare_stop(store, actor, now)?,
        actor,
        readiness,
    })
}
impl PreparedRequestedStop {
    pub fn projection(&self) -> &PreparedMotion {
        &self.motion
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl JourneyFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        if facts.readiness(self.actor).as_ref() != Some(&self.readiness) {
            return Err(Error::Unavailable);
        }
        self.motion.stage_into(runtime, store, facts, now, batch)
    }
}
pub struct PutDownRequest {
    pub actor: PersonId,
    pub passenger: PersonId,
}
pub struct PreparedPutDown {
    motion: PreparedMotion,
    passenger: PersonId,
    permission: PutDownFacts,
}
pub fn prepare_put_down_request(
    store: &Store,
    request: PutDownRequest,
    runtime: &Runtime,
    now: SimTime,
    facts: &impl JourneyFacts,
) -> Result<PreparedPutDown, Error> {
    let readiness = facts
        .readiness(request.passenger)
        .ok_or(Error::Unavailable)?;
    let permission = PutDownFacts {
        passenger_independent: readiness.independent,
        passenger_combat_restricted: readiness.combat_restricted,
    };
    let motion =
        runtime.prepare_put_down(store, request.actor, request.passenger, now, permission)?;
    Ok(PreparedPutDown {
        motion,
        passenger: request.passenger,
        permission,
    })
}
impl PreparedPutDown {
    pub fn projection(&self) -> &PreparedMotion {
        &self.motion
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl JourneyFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        let readiness = facts.readiness(self.passenger).ok_or(Error::Unavailable)?;
        if self.permission
            != (PutDownFacts {
                passenger_independent: readiness.independent,
                passenger_combat_restricted: readiness.combat_restricted,
            })
        {
            return Err(Error::Unavailable);
        }
        self.motion.stage_into(runtime, store, facts, now, batch)
    }
}

#[cfg(test)]
#[path = "journey/tests.rs"]
mod tests;
