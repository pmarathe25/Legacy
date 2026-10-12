//! Sparse physical carrying motion and ordered owner effects.
//! This module is a child of changes so it reuses that owner's private row guards.
use super::{PreparedTransport, Rows, TransportFinalization};
use crate::*;
use game_ecs::{Batch, StableId, Store};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MotionEffect {
    StartLift {
        actor: PersonId,
        passenger: PersonId,
        completes: SimTime,
    },
    Interrupt(PersonId),
    ClearArrival(PersonId),
    DetachDependent(PersonId),
    LeaveShelter(PersonId),
    ScheduleArrival {
        actor: PersonId,
        at: SimTime,
    },
    Departed {
        actor: PersonId,
        from: AreaId,
        to: AreaId,
        arrives: SimTime,
    },
    Arrived {
        actor: PersonId,
        area: AreaId,
    },
    LiftCancelled(Lift),
    Interrupted {
        actor: PersonId,
    },
    DependentAtArea {
        actor: PersonId,
        area: AreaId,
    },
    PutDown {
        actor: PersonId,
        area: Option<AreaId>,
    },
}
pub struct PreparedMotion {
    pub(super) transport: PreparedTransport<Vec<MotionEffect>>,
    pub(super) locations: Rows<Location>,
    pub(super) route_replacements: BTreeSet<PersonId>,
}
impl PreparedMotion {
    pub(super) fn new(runtime: &Runtime, now: SimTime) -> Self {
        let mut transport = PreparedTransport::new(runtime, Vec::new());
        transport.at = Some(now);
        Self {
            transport,
            locations: Rows::new(),
            route_replacements: BTreeSet::new(),
        }
    }
    fn replace_route(&mut self, store: &Store, actor: PersonId, after: Option<Route>) {
        self.transport.routes.set(store, actor, after);
        self.route_replacements.insert(actor);
    }
    pub fn effects(&self) -> &[MotionEffect] {
        self.transport.result()
    }
    pub fn continuations(&self, store: &Store) -> Vec<ContinuedJourney> {
        self.effects()
            .iter()
            .filter_map(|effect| {
                let MotionEffect::Arrived { actor, area } = *effect else {
                    return None;
                };
                let destination = self.transport.routes.edits.get(&actor).map_or_else(
                    || route_steps(store, actor).first().copied(),
                    |(_, after)| {
                        after
                            .as_ref()
                            .and_then(|route| route.steps.first().copied())
                    },
                )?;
                Some(ContinuedJourney {
                    actor,
                    origin: area,
                    destination,
                })
            })
            .collect()
    }
    /// Factual projected row for receiving owners to prepare their own reactions.
    pub fn projected_location<'a>(
        &'a self,
        store: &'a Store,
        actor: PersonId,
    ) -> Option<&'a Location> {
        self.locations.edits.get(&actor).map_or_else(
            || game_spatial::location(store, StableId(actor.0)),
            |(_, after)| after.as_ref(),
        )
    }
    pub fn projected_anchor<'a>(
        &'a self,
        store: &'a Store,
        actor: PersonId,
    ) -> Result<(PersonId, &'a Location), Error> {
        let mut actor = actor;
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(actor) {
                return Err(Error::Unavailable);
            }
            let location = self
                .projected_location(store, actor)
                .ok_or(Error::Unavailable)?;
            if let Location::Carried { carrier } = location {
                actor = *carrier;
            } else {
                return Ok((actor, location));
            }
        }
    }
    pub fn projected_position(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
    ) -> Result<Position, Error> {
        let mut actor = actor;
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(actor) {
                return Err(Error::Unavailable);
            }
            let location = self
                .projected_location(store, actor)
                .ok_or(Error::Unavailable)?;
            match location {
                Location::Carried { carrier } => actor = *carrier,
                other => return position_from_location(other, now),
            }
        }
    }
    fn validate_projection(&self, store: &Store, now: SimTime) -> Result<(), Error> {
        let projected_group = |actor| {
            self.transport
                .groups
                .edits
                .get(&actor)
                .map_or_else(|| group(store, actor), |(_, after)| after.as_ref())
        };
        let mut affected = self
            .locations
            .edits
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        affected.extend(self.transport.groups.edits.keys().copied());
        for actor in affected {
            let location = self
                .projected_location(store, actor)
                .ok_or(Error::Unavailable)?;
            game_spatial::validate(location)?;
            let own_group = projected_group(actor);
            match location {
                Location::Carried { carrier }
                    if own_group.is_some_and(|g| g.anchor == *carrier) => {}
                Location::Carried { .. } => return Err(Error::Unavailable),
                _ if own_group.is_some() => return Err(Error::Unavailable),
                _ => {}
            }
            if let Some(group) = own_group {
                group.validate()?;
                let pending = self
                    .transport
                    .routes
                    .edits
                    .get(&actor)
                    .map_or_else(|| route(store, actor), |(_, after)| after.as_ref());
                if pending.is_some() {
                    return Err(Error::Unavailable);
                }
                let at = self.projected_position(store, actor, now)?;
                for bearer in &group.bearers {
                    if projected_group(*bearer).is_some()
                        || !at.same_point(self.projected_position(store, *bearer, now)?)
                    {
                        return Err(Error::Unavailable);
                    }
                }
            }
        }
        Ok(())
    }
    pub fn validate(
        &self,
        runtime: &Runtime,
        store: &Store,
        bodies: &impl BodyFacts,
        now: SimTime,
    ) -> Result<(), Error> {
        self.locations.validate(store)?;
        self.transport.validate(runtime, store, bodies, now)?;
        self.validate_projection(store, now)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        bodies: &impl BodyFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        let mut envelope = MotionBatch::new(runtime, now);
        envelope.push(self)?;
        envelope.stage_into(runtime, store, bodies, now, batch)
    }
    pub fn commit(
        self,
        runtime: &mut Runtime,
        store: &mut Store,
        bodies: &impl BodyFacts,
        now: SimTime,
    ) -> Result<Vec<MotionEffect>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, bodies, now, &mut batch)?;
        finish.validate_footprint(&batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish(runtime))
    }
}
pub struct MotionFinalization {
    transport: TransportFinalization<Vec<MotionEffect>>,
    expected_groups: BTreeSet<StableId>,
    expected_lifts: BTreeSet<StableId>,
    reserved_routes: BTreeSet<StableId>,
    reserved_locations: BTreeSet<StableId>,
    preserves_locations: bool,
    route_replacements: BTreeSet<PersonId>,
    batch_key: game_ecs::BatchKey,
}
impl MotionFinalization {
    /// A review phase can retain shared local candidate identities only when
    /// its entire final Location footprint consists of this owner's no-ops.
    pub fn validate_preserving_locations(&self, batch: &Batch) -> Result<(), Error> {
        self.validate_footprint(batch)?;
        if !self.preserves_locations
            || batch
                .edited_ids::<Location>()
                .any(|id| !self.reserved_locations.contains(&id))
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    /// Call after every participant has staged and before the host admits or
    /// publishes the batch. Extra carrying edges need their own explicit plan
    /// in the envelope; a foreign component touch never authorizes an edge.
    pub fn validate_footprint(&self, batch: &Batch) -> Result<(), Error> {
        if self.batch_key != batch.key()
            || batch.edited_ids::<CarryGroup>().count() != self.expected_groups.len()
            || batch.edited_ids::<Lift>().count() != self.expected_lifts.len()
            || batch.edited_ids::<CarryGroup>().collect::<BTreeSet<_>>() != self.expected_groups
            || batch.edited_ids::<Lift>().collect::<BTreeSet<_>>() != self.expected_lifts
        {
            return Err(Error::Unavailable);
        }
        for id in &self.reserved_routes {
            if batch
                .edited_ids::<Route>()
                .filter(|edited| edited == id)
                .count()
                != 1
            {
                return Err(Error::Unavailable);
            }
        }
        for id in &self.reserved_locations {
            if batch
                .edited_ids::<Location>()
                .filter(|edited| edited == id)
                .count()
                != 1
            {
                return Err(Error::Unavailable);
            }
        }
        Ok(())
    }
    pub fn route_admission(&self, actor: PersonId) -> Option<RouteAdmission> {
        self.route_replacements
            .contains(&actor)
            .then(|| RouteAdmission {
                actor,
                batch_key: self.batch_key.clone(),
            })
    }
    pub fn finish(self, runtime: &mut Runtime) -> Vec<MotionEffect> {
        self.transport.finish(runtime)
    }
}

pub fn position_from_location(location: &Location, now: SimTime) -> Result<Position, Error> {
    match *location {
        Location::AtArea(area) => Ok(Position::Area(area)),
        Location::InTransit {
            from,
            to,
            departed,
            road_start,
            arrives,
        } => Leg {
            from,
            to,
            departed,
            road_start,
            arrives,
        }
        .position(now),
        Location::StoppedOnRoad {
            from,
            to,
            progress,
            duration,
        } => Ok(Position::Road(RoadPosition {
            from,
            to,
            progress,
            duration,
        })),
        Location::Carried { .. } => Err(Error::Unavailable),
    }
}
pub fn location_from_position(position: Position) -> Location {
    match position {
        Position::Area(area) => Location::AtArea(area),
        Position::Road(RoadPosition {
            from,
            to,
            progress,
            duration,
        }) => Location::StoppedOnRoad {
            from,
            to,
            progress,
            duration,
        },
    }
}
impl Runtime {
    pub fn birth_response(
        &self,
        store: &Store,
        child: PersonId,
        gestator: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
    ) -> Option<PreparedMotion> {
        self.prepare_birth_motion(store, child, gestator, now, bodies)
            .ok()
    }
    pub fn confinement_release_response(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
    ) -> Option<PreparedMotion> {
        self.prepare_confinement_release(store, actor, now).ok()
    }
    pub fn reconciliation_response(
        &self,
        store: &Store,
        now: SimTime,
        facts: &impl ReconciliationFacts,
    ) -> Option<PreparedReconciliation> {
        self.prepare_reconciliation(store, now, facts).ok()
    }
    pub fn prepare_stop(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
    ) -> Result<PreparedMotion, Error> {
        let mut motion = PreparedMotion::new(self, now);
        let members = motion.transport.guard_topology(store, [actor]);
        for member in members {
            motion.transport.routes.guard(store, member);
            motion.locations.guard(store, member);
            let location =
                game_spatial::location(store, StableId(member.0)).ok_or(Error::Unavailable)?;
            if let Location::InTransit { to, .. } = *location {
                let position = position_from_location(location, now)?;
                motion
                    .transport
                    .result
                    .push(MotionEffect::ClearArrival(member));
                motion.transport.approaches.guard(store, member);
                if matches!(position, Position::Road(_)) {
                    motion
                        .transport
                        .resume
                        .set(store, member, Some(ResumeTarget { area: to }));
                    if let Some(approach) = approach_job(store, member) {
                        motion.transport.approaches.set(store, member, None);
                        motion.transport.paused.set(
                            store,
                            member,
                            Some(PausedApproach {
                                target: approach.target,
                            }),
                        );
                    }
                } else {
                    motion.transport.resume.set(store, member, None);
                    motion.transport.paused.set(store, member, None);
                    motion.transport.approaches.set(store, member, None);
                }
                motion
                    .locations
                    .set(store, member, Some(location_from_position(position)));
                motion.replace_route(store, member, None);
                motion
                    .transport
                    .result
                    .extend([MotionEffect::Interrupted { actor: member }]);
            }
        }
        Ok(motion)
    }
    pub fn prepare_arrival(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
    ) -> Result<PreparedMotion, Error> {
        let approach = approach_job(store, actor).ok_or(Error::Unavailable)?;
        if approach.completes != now {
            return Err(Error::Unavailable);
        }
        let target = approach.target;
        let mut motion = PreparedMotion::new(self, now);
        let members = motion.transport.guard_topology(store, [actor]);
        for member in members {
            motion.locations.guard(store, member);
            motion.transport.routes.guard(store, member);
            motion.transport.approaches.guard(store, member);
            if let Some(approach) = approach_job(store, member)
                && approach.completes == now
            {
                if !Position::Road(approach.target).same_point(Position::Road(target)) {
                    return Err(Error::Unavailable);
                }
                motion.transport.approaches.set(store, member, None);
                motion.locations.set(
                    store,
                    member,
                    Some(location_from_position(Position::Road(approach.target))),
                );
                motion.replace_route(store, member, None);
                motion.transport.result.extend([
                    MotionEffect::ClearArrival(member),
                    MotionEffect::Interrupted { actor: member },
                ]);
            }
        }
        Ok(motion)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PutDownFacts {
    pub passenger_independent: bool,
    pub passenger_combat_restricted: bool,
}
impl Runtime {
    pub fn prepare_area_arrival(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
    ) -> Result<PreparedMotion, Error> {
        let Some(Location::InTransit { to, arrives, .. }) =
            game_spatial::location(store, StableId(actor.0))
        else {
            return Err(Error::Unavailable);
        };
        if *arrives != now || approach_job(store, actor).is_some() {
            return Err(Error::Unavailable);
        }
        let mut motion = PreparedMotion::new(self, now);
        let members = motion.transport.guard_topology(store, [actor]);
        for member in members {
            motion.locations.guard(store, member);
        }
        motion
            .locations
            .set(store, actor, Some(Location::AtArea(*to)));
        motion.transport.result.extend([
            MotionEffect::DependentAtArea { actor, area: *to },
            MotionEffect::Arrived { actor, area: *to },
        ]);
        Ok(motion)
    }
    pub fn prepare_death_motion(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
    ) -> Result<PreparedMotion, Error> {
        let mut motion = if matches!(
            game_spatial::location(store, StableId(actor.0)),
            Some(Location::Carried { .. })
        ) {
            let mut motion = PreparedMotion::new(self, now);
            motion.locations.guard(store, actor);
            motion
        } else {
            self.prepare_stop(store, actor, now)?
        };
        motion.replace_route(store, actor, None);
        let mut canceled = self.prepare_cancel_lifts(store, &[actor]);
        canceled.at = Some(now);
        let mut envelope = MotionBatch::new(self, now);
        envelope.push(PreparedMotion {
            transport: canceled
                .map_result(|lifts| lifts.into_iter().map(MotionEffect::LiftCancelled).collect()),
            locations: Rows::new(),
            route_replacements: BTreeSet::new(),
        })?;
        // Cancellation has no clock guard, so set it to the common timestamp.
        envelope.motion.transport.at = Some(now);
        motion
            .transport
            .result
            .splice(0..0, envelope.motion.transport.result.iter().cloned());
        motion
            .transport
            .lifts
            .merge(envelope.motion.transport.lifts)?;
        motion
            .transport
            .topology
            .extend(envelope.motion.transport.topology);
        Ok(motion)
    }
    pub fn prepare_completed_lift_motion(
        &self,
        store: &Store,
        passenger: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
        activities: &impl LiftActivityFacts,
    ) -> Result<PreparedLiftMotion, Error> {
        let passenger_escaping = activities.escaping(passenger).ok_or(Error::Unavailable)?;
        let lift = lift(store, passenger).ok_or(Error::Unavailable)?;
        let participants = lift.participants(passenger_escaping);
        let native = self.prepare_complete_lift(store, passenger, now, bodies)?;
        let group = native.result().clone();
        let mut motion = PreparedMotion {
            transport: native.map_result(|_| {
                participants
                    .into_iter()
                    .map(MotionEffect::Interrupt)
                    .collect()
            }),
            locations: Rows::new(),
            route_replacements: BTreeSet::new(),
        };
        for member in motion.transport.guard_topology(store, [passenger]) {
            motion.locations.guard(store, member);
        }
        motion.locations.set(
            store,
            passenger,
            Some(Location::Carried {
                carrier: group.anchor,
            }),
        );
        motion.replace_route(store, passenger, None);
        motion.transport.result.extend([
            MotionEffect::ClearArrival(passenger),
            MotionEffect::DetachDependent(passenger),
        ]);
        Ok(PreparedLiftMotion {
            motion,
            passenger,
            escaping: passenger_escaping,
        })
    }
    pub fn prepare_birth_motion(
        &self,
        store: &Store,
        child: PersonId,
        gestator: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
    ) -> Result<PreparedMotion, Error> {
        let (_, gestator_location) = game_spatial::resolved_anchor(store, StableId(gestator.0))?;
        if !matches!(gestator_location, Location::InTransit { .. }) {
            return Err(Error::Unavailable);
        }
        let native = self.prepare_newborn(store, child, gestator, now, bodies)?;
        let group = native.result().clone();
        let mut motion = PreparedMotion {
            transport: native.map_result(|_| vec![MotionEffect::ClearArrival(child)]),
            locations: Rows::new(),
            route_replacements: BTreeSet::new(),
        };
        for member in motion.transport.guard_topology(store, [child, gestator]) {
            motion.locations.guard(store, member);
        }
        motion.locations.set(
            store,
            child,
            Some(Location::Carried {
                carrier: group.anchor,
            }),
        );
        motion.replace_route(store, child, None);
        Ok(motion)
    }
    pub fn prepare_put_down(
        &self,
        store: &Store,
        actor: PersonId,
        passenger: PersonId,
        now: SimTime,
        facts: PutDownFacts,
    ) -> Result<PreparedMotion, Error> {
        let group = group(store, passenger).ok_or(Error::Unavailable)?;
        if actor != passenger && !group.bearers.contains(&actor)
            || actor == passenger
                && (!facts.passenger_independent || facts.passenger_combat_restricted)
        {
            return Err(Error::Unavailable);
        }
        let mut motion = self.prepare_stop(store, group.anchor, now)?;
        let (anchor, location) = game_spatial::resolved_anchor(store, StableId(passenger.0))?;
        let position = position_from_location(location, now)?;
        // Guard each carrier link used by Spatial's resolution, not a second location authority.
        let mut current = passenger;
        loop {
            motion.locations.guard(store, current);
            if StableId(current.0) == anchor {
                break;
            }
            let Some(Location::Carried { carrier }) =
                game_spatial::location(store, StableId(current.0))
            else {
                return Err(Error::Unavailable);
            };
            current = *carrier;
        }
        motion.transport.groups.set(store, passenger, None);
        motion
            .locations
            .set(store, passenger, Some(location_from_position(position)));
        let area = match position {
            Position::Area(area) => Some(area),
            Position::Road(_) => None,
        };
        if let Some(area) = area {
            motion.transport.result.push(MotionEffect::DependentAtArea {
                actor: passenger,
                area,
            });
        }
        motion.transport.result.push(MotionEffect::PutDown {
            actor: passenger,
            area,
        });
        Ok(motion)
    }
}

pub trait RoadFacts {
    fn duration(&self, from: AreaId, to: AreaId) -> Option<SimDuration>;
    fn known(&self, actor: PersonId, from: AreaId, to: AreaId) -> bool;
}
pub struct PreparedRoadMotion {
    motion: PreparedMotion,
    actor: PersonId,
    roads: Vec<(AreaId, AreaId, Option<SimDuration>)>,
    known: (AreaId, AreaId, bool),
}
impl PreparedRoadMotion {
    pub fn effects(&self) -> &[MotionEffect] {
        self.motion.effects()
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        bodies: &impl BodyFacts,
        roads: &impl RoadFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        if self
            .roads
            .iter()
            .any(|(from, to, before)| roads.duration(*from, *to) != *before)
            || roads.known(self.actor, self.known.0, self.known.1) != self.known.2
        {
            return Err(Error::Unavailable);
        }
        self.motion.stage_into(runtime, store, bodies, now, batch)
    }
}
pub struct RoadTrip<'a> {
    pub leg: &'a Leg,
    pub approach: Option<&'a Approach>,
}
impl Runtime {
    pub fn prepare_departure_trip(
        &self,
        store: &Store,
        actor: PersonId,
        trip: RoadTrip<'_>,
        now: SimTime,
        bodies: &impl BodyFacts,
        roads: &impl RoadFacts,
    ) -> Result<PreparedRoadMotion, Error> {
        let RoadTrip { leg, approach } = trip;
        if leg.departed != now
            || !self.can_depart(store, bodies, actor)
            || !roads.known(actor, leg.from, leg.to)
        {
            return Err(Error::Unavailable);
        }
        let duration = roads.duration(leg.from, leg.to).ok_or(Error::NoRoute)?;
        let body = bodies.body(actor).ok_or(Error::Unavailable)?;
        if !body.position.same_point(leg.position(now)?) {
            return Err(Error::Unavailable);
        }
        let actual = if let Some(start) = leg.road_start {
            remaining_duration(start, leg.from, leg.to, duration)?
        } else {
            duration
        };
        if leg.arrives.0.checked_sub(now.0) != Some(actual.0) {
            return Err(Error::Unavailable);
        }
        if approach.is_some_and(|approach| {
            approach.started != now || approach.completes <= now || approach.completes > leg.arrives
        }) {
            return Err(Error::Unavailable);
        }
        let location = Location::InTransit {
            from: leg.from,
            to: leg.to,
            departed: leg.departed,
            arrives: leg.arrives,
            road_start: leg.road_start,
        };
        game_spatial::validate(&location)?;
        let mut motion = PreparedMotion::new(self, now);
        let members = motion.transport.guard_topology(store, [actor]);
        motion
            .transport
            .guard_bodies(bodies, members.iter().copied())?;
        motion.locations.set(store, actor, Some(location.clone()));
        let deadline = approach.map_or(leg.arrives, |approach| approach.completes);
        motion.transport.result.extend([
            MotionEffect::Interrupt(actor),
            MotionEffect::ClearArrival(actor),
            MotionEffect::LeaveShelter(actor),
            MotionEffect::DetachDependent(actor),
            MotionEffect::ScheduleArrival {
                actor,
                at: deadline,
            },
            MotionEffect::Departed {
                actor,
                from: leg.from,
                to: leg.to,
                arrives: deadline,
            },
        ]);
        if approach.is_some() {
            motion.replace_route(store, actor, None);
        } else if route_steps(store, actor).first() == Some(&leg.to) && leg.road_start.is_none() {
            let remaining = route_steps(store, actor)[1..].to_vec();
            motion.replace_route(
                store,
                actor,
                (!remaining.is_empty()).then_some(Route { steps: remaining }),
            );
        } else {
            motion.transport.routes.guard(store, actor);
        }
        for member in members {
            motion.locations.guard(store, member);
            motion.transport.routes.guard(store, member);
            motion.transport.result.extend([
                MotionEffect::DetachDependent(member),
                MotionEffect::LeaveShelter(member),
            ]);
            if member == actor || group(store, member).is_some() {
                continue;
            }
            motion.locations.set(store, member, Some(location.clone()));
            motion.replace_route(store, member, None);
            motion.transport.result.extend([
                MotionEffect::Interrupt(member),
                MotionEffect::ClearArrival(member),
            ]);
            if let Some(approach) = approach {
                motion
                    .transport
                    .approaches
                    .set(store, member, Some(approach.clone()));
            }
            motion.transport.result.extend([
                MotionEffect::ScheduleArrival {
                    actor: member,
                    at: deadline,
                },
                MotionEffect::Departed {
                    actor: member,
                    from: leg.from,
                    to: leg.to,
                    arrives: deadline,
                },
            ]);
        }
        if let Some(approach) = approach {
            motion.transport.paused.set(store, actor, None);
            motion
                .transport
                .approaches
                .set(store, actor, Some(approach.clone()));
        }
        Ok(PreparedRoadMotion {
            motion,
            actor,
            roads: vec![(leg.from, leg.to, Some(duration))],
            known: (leg.from, leg.to, true),
        })
    }
}

#[cfg(test)]
#[path = "motion/tests.rs"]
mod tests;

/// One carrying transaction envelope. Independent plans compose here; shared
/// rows coalesce only when they agree on their opening and final values.
pub struct MotionBatch {
    motion: PreparedMotion,
}
impl MotionBatch {
    pub fn new(runtime: &Runtime, now: SimTime) -> Self {
        Self {
            motion: PreparedMotion::new(runtime, now),
        }
    }
    pub fn push(&mut self, next: PreparedMotion) -> Result<(), Error> {
        let a = &mut self.motion.transport;
        let b = next.transport;
        if a.rules != b.rules
            || a.at != b.at
            || (a.journal.is_some() && b.journal.is_some() && a.journal != b.journal)
            || (a.expired_inboxes.is_some()
                && b.expired_inboxes.is_some()
                && a.expired_inboxes != b.expired_inboxes)
        {
            return Err(Error::Unavailable);
        }
        if a.journal.is_none() {
            a.journal = b.journal;
        }
        if a.expired_inboxes.is_none() {
            a.expired_inboxes = b.expired_inboxes;
        }
        a.routes.merge(b.routes)?;
        a.groups.merge(b.groups)?;
        a.offers.merge(b.offers)?;
        a.lifts.merge(b.lifts)?;
        a.approaches.merge(b.approaches)?;
        a.resume.merge(b.resume)?;
        a.paused.merge(b.paused)?;
        a.knowledge.merge(b.knowledge)?;
        self.motion.locations.merge(next.locations)?;
        self.motion
            .route_replacements
            .extend(next.route_replacements);
        for (actor, body) in b.body_reads {
            if a.body_reads
                .get(&actor)
                .is_some_and(|before| *before != body)
            {
                return Err(Error::Unavailable);
            }
            a.body_reads.insert(actor, body);
        }
        a.topology.extend(b.topology);
        a.result.extend(b.result);
        Ok(())
    }
    pub fn effects(&self) -> &[MotionEffect] {
        self.motion.effects()
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        bodies: &impl BodyFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        self.motion.validate(runtime, store, bodies, now)?;
        if batch.edited_ids::<CarryGroup>().next().is_some()
            || batch.edited_ids::<Lift>().next().is_some()
        {
            return Err(Error::Unavailable);
        }
        let expected_groups = self
            .motion
            .transport
            .groups
            .edits
            .iter()
            .filter(|(_, (before, after))| before != after)
            .map(|(id, _)| StableId(id.0))
            .collect();
        let expected_lifts = self
            .motion
            .transport
            .lifts
            .edits
            .iter()
            .filter(|(_, (before, after))| before != after)
            .map(|(id, _)| StableId(id.0))
            .collect();
        let reserved_routes = self
            .motion
            .transport
            .routes
            .edits
            .keys()
            .map(|id| StableId(id.0))
            .collect();
        let reserved_locations = self
            .motion
            .locations
            .edits
            .keys()
            .map(|id| StableId(id.0))
            .collect();
        let preserves_locations = self
            .motion
            .locations
            .edits
            .values()
            .all(|(before, after)| before == after);
        let PreparedMotion {
            mut transport,
            locations,
            route_replacements,
        } = self.motion;
        let routes = std::mem::replace(&mut transport.routes, Rows::new());
        routes.reserve(batch)?;
        locations.reserve(batch)?;
        let finish = transport.stage_into(runtime, store, bodies, now, batch)?;
        Ok(MotionFinalization {
            transport: finish,
            expected_groups,
            expected_lifts,
            reserved_routes,
            reserved_locations,
            preserves_locations,
            route_replacements,
            batch_key: batch.key(),
        })
    }
}
impl Runtime {
    pub fn prepare_road_approach(
        &self,
        store: &Store,
        actor: PersonId,
        target: RoadPosition,
        now: SimTime,
        bodies: &impl BodyFacts,
        roads: &impl RoadFacts,
    ) -> Result<PreparedRoadMotion, Error> {
        if !self.can_depart(store, bodies, actor) {
            return Err(Error::Unavailable);
        }
        let start = match bodies.body(actor).ok_or(Error::Unavailable)?.position {
            Position::Area(endpoint) => RoadPosition {
                progress: if endpoint == target.from {
                    SimDuration(0)
                } else if endpoint == target.to {
                    target.duration
                } else {
                    return Err(Error::Unavailable);
                },
                ..target
            },
            Position::Road(point) => point,
        };
        let forward = roads
            .duration(target.from, target.to)
            .ok_or(Error::NoRoute)?;
        if target.duration != forward {
            return Err(Error::Unavailable);
        }
        let reverse = roads.duration(target.to, target.from);
        let (leg, approach) = approach_from(
            start,
            target,
            forward,
            reverse.unwrap_or(SimDuration(0)),
            now,
        )?;
        let mut prepared = self.prepare_departure_trip(
            store,
            actor,
            RoadTrip {
                leg: &leg,
                approach: Some(&approach),
            },
            now,
            bodies,
            roads,
        )?;
        prepared.roads = vec![
            (target.from, target.to, Some(forward)),
            (target.to, target.from, reverse),
        ];
        Ok(prepared)
    }
    pub fn prepare_road_return(
        &self,
        store: &Store,
        actor: PersonId,
        destination: AreaId,
        now: SimTime,
        bodies: &impl BodyFacts,
        roads: &impl RoadFacts,
    ) -> Result<PreparedRoadMotion, Error> {
        let Position::Road(position) = bodies.body(actor).ok_or(Error::Unavailable)?.position
        else {
            return Err(Error::Unavailable);
        };
        let from = if destination == position.to {
            position.from
        } else if destination == position.from {
            position.to
        } else {
            return Err(Error::NoRoute);
        };
        let duration = roads.duration(from, destination).ok_or(Error::NoRoute)?;
        if !self.can_depart(store, bodies, actor) {
            return Err(Error::Unavailable);
        }
        let mut stopped = self.prepare_stop(store, actor, now)?;
        let endpoint = destination == position.from && position.progress.0 == 0
            || destination == position.to && position.progress == position.duration;
        if endpoint {
            stopped
                .transport
                .result
                .push(MotionEffect::Interrupt(actor));
            let members = stopped.transport.guard_topology(store, [actor]);
            stopped
                .transport
                .guard_bodies(bodies, members.iter().copied())?;
            for member in members {
                stopped
                    .transport
                    .result
                    .push(MotionEffect::ClearArrival(member));
                stopped.transport.resume.set(store, member, None);
                stopped.transport.paused.set(store, member, None);
                if !matches!(
                    stopped.projected_location(store, member),
                    Some(Location::Carried { .. })
                ) {
                    stopped
                        .locations
                        .set(store, member, Some(Location::AtArea(destination)));
                    stopped.transport.result.push(MotionEffect::Arrived {
                        actor: member,
                        area: destination,
                    });
                }
            }
            return Ok(PreparedRoadMotion {
                motion: stopped,
                actor,
                roads: vec![(from, destination, Some(duration))],
                known: (from, destination, roads.known(actor, from, destination)),
            });
        }
        let leg = retreat(position, destination, duration, now)?;
        // Retreat historically needs the actual road, while strategic approach
        // additionally needs remembered-road permission. Preserve that rule.
        struct ReturnRoads<'a, R> {
            roads: &'a R,
        }
        impl<R: RoadFacts> RoadFacts for ReturnRoads<'_, R> {
            fn duration(&self, from: AreaId, to: AreaId) -> Option<SimDuration> {
                self.roads.duration(from, to)
            }
            fn known(&self, _: PersonId, _: AreaId, _: AreaId) -> bool {
                true
            }
        }
        let mut departure = self.prepare_departure_trip(
            store,
            actor,
            RoadTrip {
                leg: &leg,
                approach: None,
            },
            now,
            bodies,
            &ReturnRoads { roads },
        )?;
        // One owner operation deliberately replaces its stopped after-values;
        // independent plans cannot use this overwrite path.
        stopped
            .transport
            .result
            .extend(departure.motion.transport.result);
        stopped
            .locations
            .edits
            .extend(departure.motion.locations.edits);
        stopped
            .transport
            .body_reads
            .extend(departure.motion.transport.body_reads);
        stopped
            .transport
            .topology
            .extend(departure.motion.transport.topology);
        stopped
            .transport
            .routes
            .merge(departure.motion.transport.routes)?;
        for member in self.component(store, actor) {
            stopped.transport.resume.set(store, member, None);
            stopped.transport.paused.set(store, member, None);
        }
        departure.motion = stopped;
        departure.known = (from, destination, roads.known(actor, from, destination));
        Ok(departure)
    }
}

/// Reconciliation is an actual global movement boundary. Ordinary single-group
/// commands use BodyFacts lazily and never enumerate this catalog.
pub trait ReconciliationFacts: BodyFacts {
    fn actors(&self) -> impl Iterator<Item = PersonId>;
    fn alive(&self, actor: PersonId) -> bool;
}
pub struct PreparedReconciliation {
    motion: PreparedMotion,
    actors: Vec<(PersonId, bool)>,
}
impl PreparedReconciliation {
    pub fn effects(&self) -> &[MotionEffect] {
        self.motion.effects()
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl ReconciliationFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        let mut actors = facts
            .actors()
            .map(|actor| (actor, facts.alive(actor)))
            .collect::<Vec<_>>();
        actors.sort();
        if actors != self.actors {
            return Err(Error::Unavailable);
        }
        self.motion.stage_into(runtime, store, facts, now, batch)
    }
}
impl Runtime {
    pub fn prepare_reconciliation(
        &self,
        store: &Store,
        now: SimTime,
        facts: &impl ReconciliationFacts,
    ) -> Result<PreparedReconciliation, Error> {
        let mut actors = facts
            .actors()
            .map(|actor| (actor, facts.alive(actor)))
            .collect::<Vec<_>>();
        actors.sort();
        if actors.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(Error::Unavailable);
        }
        let plan = capacity_plan(store, facts)?;
        let mut stopped = plan.unable;
        for passenger in plan.overloaded {
            stopped.extend(
                group(store, passenger)
                    .ok_or(Error::Unavailable)?
                    .bearers
                    .iter()
                    .copied(),
            );
        }
        let mut motion = PreparedMotion::new(self, now);
        motion
            .transport
            .guard_bodies(facts, actors.iter().map(|(actor, _)| *actor))?;
        for (actor, _) in &actors {
            motion.locations.guard(store, *actor);
            let body = facts.body(*actor).ok_or(Error::Unavailable)?;
            if (!body.can_walk || body.cargo > body.capacity)
                && matches!(
                    game_spatial::location(store, StableId(actor.0)),
                    Some(Location::InTransit { .. })
                )
            {
                stopped.insert(*actor);
            }
        }
        let mut envelope = MotionBatch::new(self, now);
        let mut handled = BTreeSet::new();
        for actor in stopped {
            if handled.contains(&actor) {
                continue;
            }
            handled.extend(self.component(store, actor));
            envelope.push(self.prepare_stop(store, actor, now)?)?;
        }
        envelope.push(motion)?;
        let mut motion = envelope.motion;
        let expired = self.prepare_prune_offers(store, now);
        motion.transport.offers.merge(expired.offers)?;
        motion.transport.expired_inboxes = expired.expired_inboxes;
        for (actor, _) in resume_targets(store) {
            if !matches!(
                motion.projected_location(store, actor),
                Some(Location::StoppedOnRoad { .. })
            ) {
                motion.transport.resume.set(store, actor, None);
            }
        }
        for (actor, _) in paused_approaches(store) {
            if !matches!(
                motion.projected_location(store, actor),
                Some(Location::StoppedOnRoad { .. })
            ) {
                motion.transport.paused.set(store, actor, None);
            }
        }
        for (actor, alive) in &actors {
            if *alive
                && group(store, *actor).is_some()
                && let Position::Area(area) = motion.projected_position(store, *actor, now)?
            {
                motion.transport.result.push(MotionEffect::DependentAtArea {
                    actor: *actor,
                    area,
                });
            }
        }
        Ok(PreparedReconciliation { motion, actors })
    }
    pub fn prepare_confinement_release(
        &self,
        store: &Store,
        passenger: PersonId,
        now: SimTime,
    ) -> Result<PreparedMotion, Error> {
        let carried = group(store, passenger).ok_or(Error::Unavailable)?;
        if carried.authority != Authority::Captive {
            return Err(Error::Unavailable);
        }
        self.prepare_put_down(
            store,
            carried.anchor,
            passenger,
            now,
            PutDownFacts {
                passenger_independent: false,
                passenger_combat_restricted: false,
            },
        )
    }
}
impl Runtime {
    pub fn prepare_road_resume(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
        roads: &impl RoadFacts,
    ) -> Result<PreparedRoadMotion, Error> {
        if let Some(target) = paused_approach(store, actor) {
            return self.prepare_road_approach(store, actor, target.target, now, bodies, roads);
        }
        let Some(Location::StoppedOnRoad { to, .. }) =
            game_spatial::location(store, StableId(actor.0))
        else {
            return Err(Error::Unavailable);
        };
        let destination = resume_target(store, actor).map_or(*to, |row| row.area);
        self.prepare_road_return(store, actor, destination, now, bodies, roads)
    }
}

/// Exact actual action fact used by lift completion participant selection.
pub trait LiftActivityFacts {
    fn escaping(&self, actor: PersonId) -> Option<bool>;
}
impl LiftActivityFacts for BTreeMap<PersonId, bool> {
    fn escaping(&self, actor: PersonId) -> Option<bool> {
        self.get(&actor).copied()
    }
}
pub struct PreparedLiftMotion {
    motion: PreparedMotion,
    passenger: PersonId,
    escaping: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiftClosingReaction {
    Cancel(PersonId),
}
/// Closing support failures cancel the pending physical lift. Preparation and
/// admission failures preserve their original meaning; no owner state is
/// published by this policy function.
pub fn lift_closing_reaction(
    actor: PersonId,
    outcome: Result<(), Error>,
) -> Option<LiftClosingReaction> {
    outcome.err().map(|_| LiftClosingReaction::Cancel(actor))
}
impl PreparedLiftMotion {
    pub fn effects(&self) -> &[MotionEffect] {
        self.motion.effects()
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        bodies: &impl BodyFacts,
        activities: &impl LiftActivityFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        if activities.escaping(self.passenger) != Some(self.escaping) {
            return Err(Error::Unavailable);
        }
        self.motion.stage_into(runtime, store, bodies, now, batch)
    }
    pub fn commit(
        self,
        runtime: &mut Runtime,
        store: &mut Store,
        bodies: &impl BodyFacts,
        activities: &impl LiftActivityFacts,
        now: SimTime,
    ) -> Result<Vec<MotionEffect>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, bodies, activities, now, &mut batch)?;
        finish.validate_footprint(&batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish(runtime))
    }
}
impl Runtime {
    /// A new strategic route and its first departure share one native row edit;
    /// neither route installation nor removal is published early.
    pub fn prepare_itinerary_departure(
        &self,
        store: &Store,
        route: PreparedRoute,
        now: SimTime,
        bodies: &impl BodyFacts,
        roads: &impl RoadFacts,
    ) -> Result<PreparedRoadMotion, Error> {
        let actor = route.actor();
        let to = *route.steps().first().ok_or(Error::AlreadyThere)?;
        let Some(Location::AtArea(from)) = game_spatial::location(store, StableId(actor.0)) else {
            return Err(Error::Unavailable);
        };
        let duration = roads.duration(*from, to).ok_or(Error::NoRoute)?;
        let arrives = SimTime(now.0.checked_add(duration.0).ok_or(Error::Overflow)?);
        let leg = Leg {
            from: *from,
            to,
            departed: now,
            arrives,
            road_start: None,
        };
        let (_, before, after) = route.consume_next(to)?.into_edit(store)?;
        let mut plan = self.prepare_departure_trip(
            store,
            actor,
            RoadTrip {
                leg: &leg,
                approach: None,
            },
            now,
            bodies,
            roads,
        )?;
        plan.motion
            .transport
            .routes
            .edits
            .insert(actor, (before, after));
        plan.motion.route_replacements.insert(actor);
        Ok(plan)
    }
}
impl PreparedRoadMotion {
    pub fn projection(&self) -> &PreparedMotion {
        &self.motion
    }
}
impl PreparedLiftMotion {
    pub fn projection(&self) -> &PreparedMotion {
        &self.motion
    }
}
impl PreparedReconciliation {
    pub fn projection(&self) -> &PreparedMotion {
        &self.motion
    }
}

pub trait LiftActionFacts {
    fn lifting(&self, actor: PersonId) -> Option<(PersonId, SimTime)>;
}
pub struct PreparedLiftReactions {
    before: BTreeMap<PersonId, Option<(PersonId, SimTime)>>,
    by_passenger: BTreeMap<PersonId, Vec<PersonId>>,
}
pub fn prepare_lift_reactions(
    lifts: &[Lift],
    facts: &impl LiftActionFacts,
) -> PreparedLiftReactions {
    let mut before = BTreeMap::new();
    let mut by_passenger = BTreeMap::new();
    for lift in lifts {
        let mut reset = Vec::new();
        for bearer in &lift.bearers {
            let action = facts.lifting(*bearer);
            before.insert(*bearer, action);
            if action.is_some_and(|(passenger, _)| passenger == lift.passenger) {
                reset.push(*bearer);
            }
        }
        by_passenger.insert(lift.passenger, reset);
    }
    PreparedLiftReactions {
        before,
        by_passenger,
    }
}
impl PreparedLiftReactions {
    pub fn validate(&self, facts: &impl LiftActionFacts) -> Result<(), Error> {
        if self
            .before
            .iter()
            .any(|(actor, before)| facts.lifting(*actor) != *before)
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn resets(&self) -> impl Iterator<Item = PersonId> + '_ {
        self.by_passenger.values().flatten().copied()
    }
    pub fn resets_for(&self, passenger: PersonId) -> impl Iterator<Item = PersonId> + '_ {
        self.by_passenger
            .get(&passenger)
            .into_iter()
            .flatten()
            .copied()
    }
}
impl Runtime {
    pub fn prepare_lift_cancellation_motion(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
    ) -> PreparedMotion {
        let mut cancel = self.prepare_cancel_lifts(store, &[actor]);
        cancel.at = Some(now);
        PreparedMotion {
            transport: cancel
                .map_result(|lifts| lifts.into_iter().map(MotionEffect::LiftCancelled).collect()),
            locations: Rows::new(),
            route_replacements: BTreeSet::new(),
        }
    }
    pub fn prepare_lift_completion_by_bearer(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
        activities: &impl LiftActivityFacts,
    ) -> Result<PreparedLiftMotion, Error> {
        let passenger = lifts(store)
            .find(|(_, lift)| lift.bearers.contains(&actor))
            .map(|(id, _)| id)
            .ok_or(Error::Unavailable)?;
        self.prepare_completed_lift_motion(store, passenger, now, bodies, activities)
    }
}

impl PreparedRoadMotion {
    pub(super) fn into_motion(self) -> PreparedMotion {
        self.motion
    }
}
impl Runtime {
    /// Complete one deadline's arrival actors in the caller's stable schedule
    /// order. Coupled endpoints publish together without reordering outsiders.
    pub fn prepare_arrivals(
        &self,
        store: &Store,
        actors: &[PersonId],
        now: SimTime,
    ) -> Result<PreparedMotion, Error> {
        let mut motion = PreparedMotion::new(self, now);
        let mut seen = BTreeSet::new();
        for actor in actors {
            if !seen.insert(*actor) {
                return Err(Error::Unavailable);
            }
            let linked = motion.transport.guard_topology(store, [*actor]);
            for member in linked {
                motion.locations.guard(store, member);
                motion.transport.routes.guard(store, member);
            }
            if let Some(approach) = approach_job(store, *actor) {
                if approach.completes != now {
                    return Err(Error::Unavailable);
                }
                motion.transport.approaches.set(store, *actor, None);
                motion.locations.set(
                    store,
                    *actor,
                    Some(location_from_position(Position::Road(approach.target))),
                );
                motion.replace_route(store, *actor, None);
                motion
                    .transport
                    .result
                    .push(MotionEffect::Interrupted { actor: *actor });
            } else if let Some(Location::InTransit { to, arrives, .. }) =
                game_spatial::location(store, StableId(actor.0))
            {
                if *arrives != now {
                    return Err(Error::Unavailable);
                }
                motion
                    .locations
                    .set(store, *actor, Some(Location::AtArea(*to)));
                motion.transport.result.extend([
                    MotionEffect::DependentAtArea {
                        actor: *actor,
                        area: *to,
                    },
                    MotionEffect::Arrived {
                        actor: *actor,
                        area: *to,
                    },
                ]);
            }
        }
        Ok(motion)
    }
}

pub trait LiftRequestFacts: BodyFacts + LiftActivityFacts {
    fn permission(&self, passenger: PersonId) -> Option<CarryPermissionInput>;
    fn moving(&self, actor: PersonId) -> bool;
    fn contact(&self, bearer: PersonId, passenger: PersonId) -> bool;
}
pub struct LiftRequest {
    pub passenger: PersonId,
    pub bearers: BTreeSet<PersonId>,
    pub accepted: bool,
}
pub struct PreparedLiftStart {
    motion: PreparedMotion,
    passenger: PersonId,
    permission: CarryPermissionInput,
    escaping: bool,
    bearers: BTreeSet<PersonId>,
}
impl Runtime {
    pub fn prepare_lift_request(
        &self,
        store: &Store,
        request: LiftRequest,
        now: SimTime,
        facts: &impl LiftRequestFacts,
    ) -> Result<PreparedLiftStart, Error> {
        let permission = facts
            .permission(request.passenger)
            .ok_or(Error::Unavailable)?;
        if request.accepted
            && (!permission.independent || !permission.alive || permission.combat_restricted)
        {
            return Err(Error::Unavailable);
        }
        let escaping = facts
            .escaping(request.passenger)
            .ok_or(Error::Unavailable)?;
        if facts.moving(request.passenger)
            || request
                .bearers
                .iter()
                .any(|bearer| facts.moving(*bearer) || !facts.contact(*bearer, request.passenger))
        {
            return Err(Error::Unavailable);
        }
        let native = self.prepare_begin_lift(
            store,
            request.passenger,
            request.bearers.clone(),
            Permission::from_facts(&request.bearers, request.accepted, &permission),
            now,
            facts,
        )?;
        let lift = native.result().clone();
        let mut effects = lift
            .participants(escaping)
            .into_iter()
            .map(MotionEffect::Interrupt)
            .collect::<Vec<_>>();
        effects.extend(lift.bearers.iter().map(|actor| MotionEffect::StartLift {
            actor: *actor,
            passenger: lift.passenger,
            completes: lift.completes,
        }));
        let mut motion = PreparedMotion {
            transport: native.map_result(|_| effects),
            locations: Rows::new(),
            route_replacements: BTreeSet::new(),
        };
        for actor in motion.transport.guard_topology(
            store,
            request.bearers.iter().copied().chain([request.passenger]),
        ) {
            motion.locations.guard(store, actor);
        }
        Ok(PreparedLiftStart {
            motion,
            passenger: request.passenger,
            permission,
            escaping,
            bearers: request.bearers,
        })
    }
}
impl PreparedLiftStart {
    pub fn projection(&self) -> &PreparedMotion {
        &self.motion
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl LiftRequestFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        if facts.permission(self.passenger).as_ref() != Some(&self.permission)
            || facts.escaping(self.passenger) != Some(self.escaping)
            || facts.moving(self.passenger)
            || self
                .bearers
                .iter()
                .any(|bearer| facts.moving(*bearer) || !facts.contact(*bearer, self.passenger))
        {
            return Err(Error::Unavailable);
        }
        self.motion.stage_into(runtime, store, facts, now, batch)
    }
}

pub struct OfferRequest {
    pub bearer: PersonId,
    pub passenger: PersonId,
}
pub struct PreparedCarryOffer {
    motion: PreparedMotion,
    bearer: PersonId,
    passenger: PersonId,
}
impl Runtime {
    pub fn prepare_offer_request(
        &self,
        store: &Store,
        request: OfferRequest,
        now: SimTime,
        facts: &impl LiftRequestFacts,
    ) -> Result<PreparedCarryOffer, Error> {
        now.0
            .checked_add(self.rules.lift_time.0)
            .ok_or(Error::Overflow)?;
        if facts.moving(request.bearer)
            || facts.moving(request.passenger)
            || !facts.contact(request.bearer, request.passenger)
        {
            return Err(Error::Unavailable);
        }
        let native = self.prepare_offer(store, request.bearer, request.passenger, now, facts)?;
        let mut motion = PreparedMotion {
            transport: native.map_result(|_| Vec::new()),
            locations: Rows::new(),
            route_replacements: BTreeSet::new(),
        };
        for actor in motion
            .transport
            .guard_topology(store, [request.bearer, request.passenger])
        {
            motion.locations.guard(store, actor);
        }
        Ok(PreparedCarryOffer {
            motion,
            bearer: request.bearer,
            passenger: request.passenger,
        })
    }
}
impl PreparedCarryOffer {
    pub fn projection(&self) -> &PreparedMotion {
        &self.motion
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl LiftRequestFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MotionFinalization, Error> {
        if facts.moving(self.bearer)
            || facts.moving(self.passenger)
            || !facts.contact(self.bearer, self.passenger)
        {
            return Err(Error::Unavailable);
        }
        self.motion.stage_into(runtime, store, facts, now, batch)
    }
}

pub struct AcceptCarryRequest {
    pub passenger: PersonId,
    pub bearers: Vec<PersonId>,
}
#[derive(Clone, Copy)]
pub enum OfferedSupport {
    Single {
        bearer: PersonId,
        passenger: PersonId,
    },
    Joined {
        passenger: PersonId,
    },
}
impl Runtime {
    /// A physical offer remains available when voluntary support is not yet
    /// authorized or cannot carry the load. Only other errors end the command.
    pub fn prepare_automatic_lift(
        &self,
        store: &Store,
        offered: OfferedSupport,
        now: SimTime,
        facts: &impl LiftRequestFacts,
    ) -> Result<Option<PreparedLiftStart>, Error> {
        let (passenger, bearers) = match offered {
            OfferedSupport::Single { bearer, passenger } => (passenger, BTreeSet::from([bearer])),
            OfferedSupport::Joined { passenger } => {
                (passenger, self.join_bearers(store, facts, passenger, now))
            }
        };
        let input = facts.permission(passenger).ok_or(Error::Unavailable)?;
        if Permission::from_facts(&bearers, false, &input)
            .authority()
            .is_err()
        {
            return Ok(None);
        }
        match self.prepare_lift_request(
            store,
            LiftRequest {
                passenger,
                bearers,
                accepted: false,
            },
            now,
            facts,
        ) {
            Ok(plan) => Ok(Some(plan)),
            Err(Error::Unavailable) => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn prepare_accept_request(
        &self,
        store: &Store,
        request: AcceptCarryRequest,
        now: SimTime,
        facts: &impl LiftRequestFacts,
    ) -> Result<PreparedLiftStart, Error> {
        let bearers = request.bearers.iter().copied().collect::<BTreeSet<_>>();
        if bearers.len() != request.bearers.len() {
            return Err(Error::Unavailable);
        }
        self.prepare_lift_request(
            store,
            LiftRequest {
                passenger: request.passenger,
                bearers,
                accepted: true,
            },
            now,
            facts,
        )
    }
    pub fn prepare_return_request(
        &self,
        store: &Store,
        actor: PersonId,
        destination: AreaId,
        now: SimTime,
        bodies: &impl BodyFacts,
        roads: &impl RoadFacts,
    ) -> Result<PreparedRoadMotion, Error> {
        if !matches!(
            game_spatial::resolve(store, StableId(actor.0)),
            Ok(Location::StoppedOnRoad { .. })
        ) {
            return Err(Error::Unavailable);
        }
        self.prepare_road_return(store, actor, destination, now, bodies, roads)
    }
}
