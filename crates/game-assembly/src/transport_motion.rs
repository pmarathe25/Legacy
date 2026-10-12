//! Owner-prepared motion admission and factual effect bindings.
use crate::*;
use game_transport::{MotionEffect, PreparedMotion};
pub(crate) enum MotionPlan {
    Direct(PreparedMotion),
    Stop(game_transport::PreparedRequestedStop),
    PutDown(game_transport::PreparedPutDown),
    Journey(game_transport::PreparedJourney),
    Continuation(game_transport::PreparedContinuation),
    Road(game_transport::PreparedRoadMotion),
    Lift(game_transport::PreparedLiftMotion),
    LiftStart(game_transport::PreparedLiftStart),
    Offer(game_transport::PreparedCarryOffer),
    Reconcile(game_transport::PreparedReconciliation),
}
impl MotionPlan {
    fn projection(&self) -> &PreparedMotion {
        match self {
            Self::Direct(plan) => plan,
            Self::Stop(plan) => plan.projection(),
            Self::PutDown(plan) => plan.projection(),
            Self::Journey(plan) => plan.projection(),
            Self::Continuation(plan) => plan.projection(),
            Self::Road(plan) => plan.projection(),
            Self::Lift(plan) => plan.projection(),
            Self::LiftStart(plan) => plan.projection(),
            Self::Offer(plan) => plan.projection(),
            Self::Reconcile(plan) => plan.projection(),
        }
    }
}
struct Roads<'a>(&'a Simulation);
impl game_transport::RoadFacts for Roads<'_> {
    fn duration(&self, from: AreaId, to: AreaId) -> Option<SimDuration> {
        self.0
            .host
            .state
            .areas
            .iter()
            .find(|area| area.id == from)?
            .links
            .iter()
            .find(|link| link.destination == to)
            .map(|link| link.duration)
    }
    fn known(&self, actor: PersonId, from: AreaId, to: AreaId) -> bool {
        self.0.host.state.decisions.as_ref().is_none_or(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.0.host.storage, actor)
                .is_some_and(|mind| {
                    mind.routes
                        .get(&from)
                        .is_some_and(|links| links.contains_key(&to))
                })
        })
    }
}
struct Journeys<'a>(&'a Simulation);
impl game_transport::BodyFacts for Journeys<'_> {
    fn body(&self, actor: PersonId) -> Option<game_transport::Body> {
        game_transport::BodyFacts::body(&self.0.transport_body_lookup(), actor)
    }
}
impl game_transport::RoadFacts for Journeys<'_> {
    fn duration(&self, from: AreaId, to: AreaId) -> Option<SimDuration> {
        game_transport::RoadFacts::duration(&Roads(self.0), from, to)
    }
    fn known(&self, actor: PersonId, from: AreaId, to: AreaId) -> bool {
        game_transport::JourneyFacts::navigation(self, actor)
            .get(&from)
            .is_some_and(|links| links.contains_key(&to))
    }
}
impl game_transport::JourneyFacts for Journeys<'_> {
    fn readiness(&self, actor: PersonId) -> Option<game_transport::TravelReadiness> {
        self.0.person_index.get(&actor)?;
        let physical = self.0.host.state.physical.as_ref().map(|runtime| {
            let body = game_physiology::body(&self.0.host.storage, actor).expect("known body");
            game_transport::TravelBody {
                alive: body.vitals.alive,
                health: body.vitals.health,
                fatigue: body.vitals.fatigue,
                fatigue_limit: runtime.rules.fatigue_limit,
            }
        });
        Some(game_transport::TravelReadiness {
            independent: self.0.independent(actor),
            combat_restricted: self.0.combat_restricts(actor),
            physical,
        })
    }
    fn navigation(&self, actor: PersonId) -> game_transport::KnownRoutes {
        if let Some(routes) = self.0.invocation_routes(actor) {
            return routes;
        }
        if self.0.host.state.decisions.is_some() {
            return game_characters::decisions::mind::<game_api::Command>(
                &self.0.host.storage,
                actor,
            )
            .expect("known mind")
            .routes
            .clone();
        }
        self.0
            .host
            .state
            .areas
            .iter()
            .map(|area| {
                (
                    area.id,
                    area.links
                        .iter()
                        .map(|link| (link.destination, link.duration))
                        .collect(),
                )
            })
            .collect()
    }
    fn area_exists(&self, area: AreaId) -> bool {
        self.0.host.state.areas.iter().any(|row| row.id == area)
    }
}
struct Activities<'a>(&'a Simulation);
impl game_transport::LiftActivityFacts for Activities<'_> {
    fn escaping(&self, actor: PersonId) -> Option<bool> {
        self.0
            .host
            .storage
            .activity(actor)
            .map(|activity| matches!(activity, Activity::Escape { .. }))
    }
}
impl game_transport::LiftActionFacts for Activities<'_> {
    fn lifting(&self, actor: PersonId) -> Option<(PersonId, SimTime)> {
        match self.0.host.storage.activity(actor)? {
            Activity::Lift {
                passenger,
                completes,
            } => Some((*passenger, *completes)),
            _ => None,
        }
    }
}
struct Carry<'a>(&'a Simulation);
impl game_transport::BodyFacts for Carry<'_> {
    fn body(&self, actor: PersonId) -> Option<game_transport::Body> {
        game_transport::BodyFacts::body(&self.0.transport_body_lookup(), actor)
    }
}
impl game_transport::LiftActivityFacts for Carry<'_> {
    fn escaping(&self, actor: PersonId) -> Option<bool> {
        game_transport::LiftActivityFacts::escaping(&Activities(self.0), actor)
    }
}
impl game_transport::LiftRequestFacts for Carry<'_> {
    fn permission(&self, passenger: PersonId) -> Option<game_transport::CarryPermissionInput> {
        self.0.transport_permission_facts(passenger)
    }
    fn moving(&self, actor: PersonId) -> bool {
        self.0.transport_is_moving(actor)
    }
    fn contact(&self, bearer: PersonId, passenger: PersonId) -> bool {
        self.0.transport_same_place(bearer, passenger)
    }
}
struct Reconciliation<'a>(&'a Simulation);
impl game_transport::BodyFacts for Reconciliation<'_> {
    fn body(&self, actor: PersonId) -> Option<game_transport::Body> {
        game_transport::BodyFacts::body(&self.0.transport_body_lookup(), actor)
    }
}
impl game_transport::ReconciliationFacts for Reconciliation<'_> {
    fn actors(&self) -> impl Iterator<Item = PersonId> {
        self.0.host.state.people.iter().map(|person| person.id)
    }
    fn alive(&self, actor: PersonId) -> bool {
        game_physiology::body(&self.0.host.storage, actor).is_some_and(|body| body.vitals.alive)
    }
}
impl Simulation {
    pub(crate) fn transport_default_runtime(&self) -> game_transport::Runtime {
        game_transport::Runtime::new(game_transport::Rules::default())
            .expect("default transport configuration")
    }
    pub(crate) fn apply_motion(&mut self, plan: MotionPlan) -> Result<(), Error> {
        let effects = plan.projection().effects().to_vec();
        let interrupt = effects
            .iter()
            .filter_map(|effect| match effect {
                MotionEffect::Interrupt(actor) => Some(*actor),
                _ => None,
            })
            .collect::<Vec<_>>();
        let interruptions =
            self.prepare_interruptions(&interrupt, commands::ActivityOwner::Transport)?;
        let canceled = effects
            .iter()
            .filter_map(|effect| match effect {
                MotionEffect::LiftCancelled(lift) => Some(lift.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let lift_reactions = game_transport::prepare_lift_reactions(&canceled, &Activities(self));
        let mut final_activities = lift_reactions
            .resets()
            .map(|actor| (actor, Activity::Idle))
            .collect::<BTreeMap<_, _>>();
        final_activities.extend(effects.iter().filter_map(|effect| match effect {
            MotionEffect::StartLift {
                actor,
                passenger,
                completes,
            } => Some((
                *actor,
                Activity::Lift {
                    passenger: *passenger,
                    completes: *completes,
                },
            )),
            _ => None,
        }));
        let mut projected_activities: BTreeMap<_, _> = interruptions
            .projected_activities()
            .map(|(actor, row)| (actor, row.clone()))
            .collect();
        projected_activities.extend(
            final_activities
                .iter()
                .map(|(actor, row)| (*actor, row.clone())),
        );
        let leaving = effects
            .iter()
            .filter_map(|effect| match effect {
                MotionEffect::LeaveShelter(actor) => Some(*actor),
                _ => None,
            })
            .collect::<Vec<_>>();
        let placements = effects
            .iter()
            .filter_map(|effect| match effect {
                MotionEffect::PutDown { actor, area } => Some(game_health::ShelterPlacement {
                    actor: *actor,
                    final_area: *area,
                }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let health = self
            .host
            .state
            .health
            .as_ref()
            .map(|runtime| {
                runtime.prepare_placements(
                    &self.host.storage,
                    &leaving,
                    &placements,
                    &health::PlacementFacts(self),
                )
            })
            .transpose()?;
        let observed = if !placements.is_empty() {
            health
                .as_ref()
                .map(|placement| {
                    let facts =
                        self.health_projected_facts(plan.projection(), &projected_activities)?;
                    placement.prepare_projected_observations(
                        self.host.state.health.as_ref().unwrap(),
                        &self.host.storage,
                        &health::PlacementFacts(self),
                        self.host.clock().time,
                        &facts.actors,
                        &facts.geometry,
                    )
                })
                .transpose()?
        } else {
            None
        };
        let movement_events = effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    MotionEffect::Departed { .. }
                        | MotionEffect::Interrupted { .. }
                        | MotionEffect::Arrived { .. }
                )
            })
            .count();
        let health_events = health.as_ref().map_or(0, |plan| plan.effects().len());
        let total = u64::try_from(movement_events + health_events)
            .map_err(|_| Error::Overflow)?
            .checked_add(interruptions.event_count()?)
            .ok_or(Error::Overflow)?;
        self.host.preflight_records(0, total, 0)?;
        let fallback = self.transport_default_runtime();
        let runtime = self.host.state.transport.as_ref().unwrap_or(&fallback);
        let mut batch = game_ecs::Batch::new();
        let finish = match plan {
            MotionPlan::Continuation(plan) => plan.stage_into(
                self.host.state.transport.as_ref(),
                &self.host.storage,
                &Journeys(self),
                self.host.clock().time,
                &mut batch,
            )?,
            MotionPlan::Journey(plan) => plan.stage_into(
                self.host.state.transport.as_ref(),
                &self.host.storage,
                &Journeys(self),
                self.host.clock().time,
                &mut batch,
            )?,
            MotionPlan::Stop(plan) => plan.stage_into(
                runtime,
                &self.host.storage,
                &Journeys(self),
                self.host.clock().time,
                &mut batch,
            )?,
            MotionPlan::PutDown(plan) => plan.stage_into(
                runtime,
                &self.host.storage,
                &Journeys(self),
                self.host.clock().time,
                &mut batch,
            )?,
            MotionPlan::Direct(plan) => plan.stage_into(
                runtime,
                &self.host.storage,
                &self.transport_body_lookup(),
                self.host.clock().time,
                &mut batch,
            )?,
            MotionPlan::Road(plan) => plan.stage_into(
                runtime,
                &self.host.storage,
                &self.transport_body_lookup(),
                &Roads(self),
                self.host.clock().time,
                &mut batch,
            )?,
            MotionPlan::Offer(plan) => plan.stage_into(
                runtime,
                &self.host.storage,
                &Carry(self),
                self.host.clock().time,
                &mut batch,
            )?,
            MotionPlan::LiftStart(plan) => plan.stage_into(
                runtime,
                &self.host.storage,
                &Carry(self),
                self.host.clock().time,
                &mut batch,
            )?,
            MotionPlan::Lift(plan) => plan.stage_into(
                runtime,
                &self.host.storage,
                &self.transport_body_lookup(),
                &Activities(self),
                self.host.clock().time,
                &mut batch,
            )?,
            MotionPlan::Reconcile(plan) => plan.stage_into(
                runtime,
                &self.host.storage,
                &Reconciliation(self),
                self.host.clock().time,
                &mut batch,
            )?,
        };
        let health = health
            .map(|plan| {
                plan.stage_into(
                    self.host.state.health.as_ref().unwrap(),
                    &self.host.storage,
                    &health::PlacementFacts(self),
                    &mut batch,
                )
            })
            .transpose()?;
        if let Some(observed) = observed {
            observed.stage_into(
                self.host.state.health.as_ref().unwrap(),
                &self.host.storage,
                &mut batch,
            )?;
        }
        let mut interrupted = interruptions.stage_into(self, &mut batch, &final_activities)?;
        lift_reactions.validate(&Activities(self))?;
        finish.validate_footprint(&batch)?;
        if let Some(health) = &health {
            health.validate_footprint(&batch)?;
        }
        self.host.storage.apply_batch(batch)?;
        let mut fallback = fallback;
        finish.finish(self.host.state.transport.as_mut().unwrap_or(&mut fallback));
        let mut health_effects = health.map(|finish| finish.finish()).unwrap_or_default();
        for effect in effects {
            match effect {
                MotionEffect::StartLift { actor, completes, .. } => {
                    self.host.state.due.entry(completes).or_default().push(Due::Physical(actor));
                }
                MotionEffect::Interrupt(actor) => interrupted.finish_actor(self, actor),
                MotionEffect::ClearArrival(actor) => self.clear_transport_arrival(actor),
                MotionEffect::DetachDependent(actor) => {
                    for dependents in self.dependent_index.values_mut() {
                        dependents.remove(&actor);
                    }
                }
                MotionEffect::LeaveShelter(actor) => {
                    if let Some(index) = health_effects.iter().position(|effect| matches!(effect, game_health::HealthEffect::ShelterLeft { actor: id, .. } if *id == actor))
                        && let game_health::HealthEffect::ShelterLeft { shelter, actor } = health_effects.remove(index)
                    {
                        self.emit(EventKind::ShelterLeft { shelter, actor });
                    }
                }
                MotionEffect::ScheduleArrival { actor, at } => {
                    self.host.state.due.entry(at).or_default().push(Due::Arrive(actor));
                }
                MotionEffect::Departed { actor, from, to, arrives } => self.emit(EventKind::Departed { actor, from, to, arrives }),
                MotionEffect::Interrupted { actor } => self.emit(EventKind::Interrupted { actor }),
                MotionEffect::Arrived { actor, area } => self.emit(EventKind::Arrived { actor, area }),
                MotionEffect::DependentAtArea { actor, area } => self.index_dependent(actor, area),
                MotionEffect::PutDown { .. } => {}
                MotionEffect::LiftCancelled(lift) => {
                    for actor in lift_reactions.resets_for(lift.passenger) {
                        for due in self.host.state.due.values_mut() {
                            due.retain(|entry| !matches!(entry, Due::Physical(id) if *id == actor));
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn prepare_transport_road_return(
        &self,
        actor: PersonId,
        destination: AreaId,
    ) -> Result<MotionPlan, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_road_return(
                &self.host.storage,
                actor,
                destination,
                self.host.clock().time,
                &self.transport_body_lookup(),
                &Roads(self),
            )
            .map(MotionPlan::Road)
    }
    pub(crate) fn prepare_transport_approach(
        &self,
        actor: PersonId,
        target: game_core::RoadPosition,
    ) -> Result<MotionPlan, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_road_approach(
                &self.host.storage,
                actor,
                target,
                self.host.clock().time,
                &self.transport_body_lookup(),
                &Roads(self),
            )
            .map(MotionPlan::Road)
    }
    pub(crate) fn prepare_transport_resume(&self, actor: PersonId) -> Result<MotionPlan, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_road_resume(
                &self.host.storage,
                actor,
                self.host.clock().time,
                &self.transport_body_lookup(),
                &Roads(self),
            )
            .map(MotionPlan::Road)
    }
    pub(crate) fn prepare_transport_lift_finish(
        &self,
        actor: PersonId,
    ) -> Result<MotionPlan, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_lift_completion_by_bearer(
                &self.host.storage,
                actor,
                self.host.clock().time,
                &self.transport_body_lookup(),
                &Activities(self),
            )
            .map(MotionPlan::Lift)
    }
    pub(crate) fn prepare_transport_reconciliation(&self) -> Result<MotionPlan, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_reconciliation(
                &self.host.storage,
                self.host.clock().time,
                &Reconciliation(self),
            )
            .map(MotionPlan::Reconcile)
    }
}

impl Simulation {
    pub(crate) fn prepare_transport_journey(
        &self,
        actor: PersonId,
        destination: AreaId,
    ) -> Result<MotionPlan, Error> {
        game_transport::prepare_journey_request(
            &self.host.storage,
            game_transport::TravelRequest { actor, destination },
            &Journeys(self),
            self.host.state.transport.as_ref(),
            self.host.clock().time,
        )
        .map(MotionPlan::Journey)
    }
}

impl Simulation {
    pub(crate) fn prepare_transport_lift_start(
        &self,
        passenger: PersonId,
        bearers: BTreeSet<PersonId>,
        accepted: bool,
    ) -> Result<MotionPlan, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_lift_request(
                &self.host.storage,
                game_transport::LiftRequest {
                    passenger,
                    bearers,
                    accepted,
                },
                self.host.clock().time,
                &Carry(self),
            )
            .map(MotionPlan::LiftStart)
    }
    pub(crate) fn transport_permission_facts(
        &self,
        passenger: PersonId,
    ) -> Option<game_transport::CarryPermissionInput> {
        let body = game_physiology::body(&self.host.storage, passenger)?;
        Some(game_transport::CarryPermissionInput {
            independent: self.independent(passenger),
            combat_restricted: self.combat_restricts(passenger),
            alive: body.vitals.alive,
            health: body.vitals.health,
            incapacitation: self
                .host
                .state
                .combat
                .as_ref()
                .map(|world| world.rules.incapacitation),
            yielded_captors: self
                .host
                .state
                .force
                .as_ref()
                .and_then(|world| world.yielded.get(&passenger))
                .map(|row| row.captors.clone()),
            captive_captor: self
                .host
                .state
                .combat
                .as_ref()
                .and_then(|world| world.captivity.get(&passenger))
                .map(|row| row.captor),
        })
    }
}

impl Simulation {
    pub(crate) fn prepare_transport_continuation(
        &self,
        actor: PersonId,
        origin: AreaId,
        destination: AreaId,
    ) -> Result<MotionPlan, Error> {
        game_transport::prepare_continued_journey(
            &self.host.storage,
            game_transport::ContinuedJourney {
                actor,
                origin,
                destination,
            },
            &Journeys(self),
            self.host.state.transport.as_ref(),
            self.host.clock().time,
        )
        .map(MotionPlan::Continuation)
    }
}

impl Simulation {
    pub(crate) fn prepare_requested_transport_stop(
        &self,
        actor: PersonId,
    ) -> Result<MotionPlan, Error> {
        game_transport::prepare_stop_request(
            &self.host.storage,
            actor,
            self.host
                .state
                .transport
                .as_ref()
                .ok_or(Error::Unavailable)?,
            self.host.clock().time,
            &Journeys(self),
        )
        .map(MotionPlan::Stop)
    }
    pub(crate) fn prepare_transport_putdown(
        &self,
        actor: PersonId,
        passenger: PersonId,
    ) -> Result<MotionPlan, Error> {
        game_transport::prepare_put_down_request(
            &self.host.storage,
            game_transport::PutDownRequest { actor, passenger },
            self.host
                .state
                .transport
                .as_ref()
                .ok_or(Error::Unavailable)?,
            self.host.clock().time,
            &Journeys(self),
        )
        .map(MotionPlan::PutDown)
    }
}

impl Simulation {
    pub(crate) fn prepare_transport_offer(
        &self,
        bearer: PersonId,
        passenger: PersonId,
    ) -> Result<MotionPlan, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_offer_request(
                &self.host.storage,
                game_transport::OfferRequest { bearer, passenger },
                self.host.clock().time,
                &Carry(self),
            )
            .map(MotionPlan::Offer)
    }
}

impl Simulation {
    pub(crate) fn prepare_transport_accept(
        &self,
        passenger: PersonId,
        bearers: &[PersonId],
    ) -> Result<MotionPlan, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_accept_request(
                &self.host.storage,
                game_transport::AcceptCarryRequest {
                    passenger,
                    bearers: bearers.to_vec(),
                },
                self.host.clock().time,
                &Carry(self),
            )
            .map(MotionPlan::LiftStart)
    }
    pub(crate) fn prepare_transport_return_request(
        &self,
        actor: PersonId,
        destination: AreaId,
    ) -> Result<MotionPlan, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_return_request(
                &self.host.storage,
                actor,
                destination,
                self.host.clock().time,
                &self.transport_body_lookup(),
                &Roads(self),
            )
            .map(MotionPlan::Road)
    }
}
