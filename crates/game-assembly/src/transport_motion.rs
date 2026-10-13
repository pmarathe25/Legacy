//! Owner-prepared motion admission and factual effect bindings.
use crate::*;
use game_transport::MotionEffect;
pub(crate) use game_transport::MotionPlan;
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
        let personal = self.0.host.state.decisions.as_ref().map(|_| {
            &game_characters::decisions::mind::<game_api::Command>(&self.0.host.storage, actor)
                .expect("known mind")
                .routes
        });
        game_characters::views::navigation_routes(self.0.invocation_routes(actor), personal, || {
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
        })
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
        self.0.people().map(|person| person.id)
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
        self.apply_motion_scoped(plan, game_transport::MotionScope::Ordinary)
    }
    pub(crate) fn apply_motion_scoped(
        &mut self,
        plan: MotionPlan,
        scope: game_transport::MotionScope,
    ) -> Result<(), Error> {
        let prepared = self.prepare_motion_admission(plan, scope)?;
        let mut batch = game_ecs::Batch::new();
        let staged = prepared.stage_into(self, &mut batch, &BTreeMap::new())?;
        staged.validate_batch(self, &batch)?;
        self.host.storage.apply_batch(batch)?;
        staged.finish(self);
        Ok(())
    }
    pub(crate) fn prepare_motion_admission(
        &self,
        plan: MotionPlan,
        scope: game_transport::MotionScope,
    ) -> Result<PreparedMotionAdmission, Error> {
        self.prepare_motion_admission_preserving(plan, scope, commands::ActivityOwner::Transport)
    }
    pub(crate) fn prepare_motion_admission_preserving(
        &self,
        plan: MotionPlan,
        scope: game_transport::MotionScope,
        preserve: commands::ActivityOwner,
    ) -> Result<PreparedMotionAdmission, Error> {
        self.prepare_motion_admission_for_actors(plan, scope, preserve, &[])
    }
    pub(crate) fn prepare_motion_admission_for_actors(
        &self,
        plan: MotionPlan,
        scope: game_transport::MotionScope,
        preserve: commands::ActivityOwner,
        extra_interruptions: &[PersonId],
    ) -> Result<PreparedMotionAdmission, Error> {
        self.prepare_motion_admission_scoped_interruptions(
            plan,
            scope,
            preserve,
            extra_interruptions,
            &[],
        )
    }
    pub(crate) fn prepare_death_motion_admission(
        &self,
        plan: MotionPlan,
        scope: game_transport::MotionScope,
        actor: PersonId,
    ) -> Result<PreparedMotionAdmission, Error> {
        self.prepare_motion_admission_scoped_interruptions(
            plan,
            scope,
            commands::ActivityOwner::Transport,
            &[actor],
            &[actor],
        )
    }
    fn prepare_motion_admission_scoped_interruptions(
        &self,
        plan: MotionPlan,
        scope: game_transport::MotionScope,
        preserve: commands::ActivityOwner,
        extra_interruptions: &[PersonId],
        dying: &[PersonId],
    ) -> Result<PreparedMotionAdmission, Error> {
        let effects = plan.projection().effects().to_vec();
        let reactions = game_transport::prepare_motion_reactions(&effects, &Activities(self))?;
        let mut actors = reactions.interruptions.clone();
        let mut seen = actors.iter().copied().collect::<BTreeSet<_>>();
        let extra_finish = extra_interruptions
            .iter()
            .copied()
            .filter(|actor| seen.insert(*actor))
            .collect::<Vec<_>>();
        actors.extend(extra_finish.iter().copied());
        let interruptions = self.prepare_motion_interruptions(&actors, preserve)?;
        let final_activities = reactions
            .activities
            .iter()
            .map(|(actor, activity)| {
                let activity = match *activity {
                    game_transport::MotionActivity::Idle => Activity::Idle,
                    game_transport::MotionActivity::Lift {
                        passenger,
                        completes,
                    } => Activity::Lift {
                        passenger,
                        completes,
                    },
                };
                (*actor, activity)
            })
            .collect::<BTreeMap<_, _>>();
        let placements = reactions
            .placements
            .iter()
            .map(|placement| game_health::ShelterPlacement {
                actor: placement.actor,
                final_area: placement.area,
            })
            .collect::<Vec<_>>();
        let health = self
            .host
            .state
            .health
            .as_ref()
            .map(|runtime| {
                runtime.prepare_death_placements(
                    &self.host.storage,
                    &reactions.shelter_departures,
                    &placements,
                    &health::PlacementFacts(self),
                    dying,
                )
            })
            .transpose()?;
        let health_events = health.as_ref().map_or(0, |plan| plan.effects().len());
        let total = reactions
            .movement_events()
            .checked_add(u64::try_from(health_events).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?
            .checked_add(interruptions.event_count()?)
            .ok_or(Error::Overflow)?;
        self.host.preflight_records(0, total, 0)?;
        Ok(PreparedMotionAdmission {
            plan,
            scope,
            effects,
            reactions,
            interruptions,
            final_activities,
            placements,
            health,
            event_count: total,
            extra_finish,
        })
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
    pub(crate) fn prepare_transport_confinement_reconciliation(
        &self,
        burden: game_transport::ConfinementBurden,
    ) -> Option<MotionPlan> {
        self.host
            .state
            .transport
            .as_ref()?
            .prepare_confinement_reconciliation(
                &self.host.storage,
                self.host.clock().time,
                &Reconciliation(self),
                burden,
            )
            .ok()
            .map(MotionPlan::Confinement)
    }
    pub(crate) fn prepare_transport_reconciliation(&self) -> Option<MotionPlan> {
        self.host
            .state
            .transport
            .as_ref()?
            .reconciliation_response(
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
    pub(crate) fn prepare_automatic_transport_lift(
        &self,
        offered: game_transport::OfferedSupport,
    ) -> Result<Option<MotionPlan>, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_automatic_lift(
                &self.host.storage,
                offered,
                self.host.clock().time,
                &Carry(self),
            )
            .map(|plan| plan.map(MotionPlan::LiftStart))
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
                .and_then(|_| game_conflict::force::yielded(&self.host.storage, passenger))
                .map(|row| row.captors.clone()),
            captive_captor: self
                .host
                .state
                .combat
                .as_ref()
                .and_then(|_| game_conflict::captivity(&self.host.storage, passenger))
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
        game_transport::prepare_continuation_response(
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

struct Inputs<'a>(&'a Simulation);
impl game_transport::MotionInputs for Inputs<'_> {
    fn bodies(&self) -> impl game_transport::BodyFacts + '_ {
        self.0.transport_body_lookup()
    }
    fn journeys(&self) -> impl game_transport::JourneyFacts + '_ {
        Journeys(self.0)
    }
    fn roads(&self) -> impl game_transport::RoadFacts + '_ {
        Roads(self.0)
    }
    fn carrying(&self) -> impl game_transport::LiftRequestFacts + '_ {
        Carry(self.0)
    }
    fn activities(&self) -> impl game_transport::LiftActivityFacts + '_ {
        Activities(self.0)
    }
    fn reconciliation(&self) -> impl game_transport::ReconciliationFacts + '_ {
        Reconciliation(self.0)
    }
}

impl Simulation {
    pub(crate) fn transport_preflight_arrival(
        &self,
        actor: PersonId,
        at: SimTime,
    ) -> Result<(), Error> {
        game_transport::preflight_arrival_continuation(&self.host.storage, actor, at, &Roads(self))
    }
}

/// Fully prepared receiving-owner work for a physical motion publication.
pub(crate) struct PreparedMotionAdmission {
    plan: MotionPlan,
    scope: game_transport::MotionScope,
    effects: Vec<MotionEffect>,
    reactions: game_transport::PreparedMotionReactions,
    interruptions: interruptions::PreparedInterruptions,
    final_activities: BTreeMap<PersonId, Activity>,
    placements: Vec<game_health::ShelterPlacement>,
    health: Option<game_health::PreparedPlacement>,
    event_count: u64,
    extra_finish: Vec<PersonId>,
}
pub(crate) struct StagedMotionAdmission {
    scope: game_transport::MotionScope,
    effects: Vec<MotionEffect>,
    reactions: game_transport::PreparedMotionReactions,
    finish: game_transport::MotionFinalization,
    health: Option<game_health::PlacementFinalization>,
    health_effects: Vec<game_health::HealthEffect>,
    interrupted: interruptions::InterruptionFinalization,
    lineage: Option<lineage::StagedLineageWitness>,
    extra_finish: Vec<PersonId>,
}
impl PreparedMotionAdmission {
    pub(crate) fn interrupted_actors(&self) -> Vec<PersonId> {
        self.reactions
            .interruptions
            .iter()
            .chain(&self.extra_finish)
            .copied()
            .collect()
    }
    pub(crate) fn event_count(&self) -> u64 {
        self.event_count
    }
    pub(crate) fn projection(&self) -> &game_transport::PreparedMotion {
        self.plan.projection()
    }
    pub(crate) fn stage_into(
        self,
        sim: &Simulation,
        batch: &mut game_ecs::Batch,
        overrides: &BTreeMap<PersonId, Activity>,
    ) -> Result<StagedMotionAdmission, Error> {
        let Self {
            plan,
            scope,
            effects,
            reactions,
            interruptions,
            mut final_activities,
            placements,
            health,
            extra_finish,
            event_count: _,
        } = self;
        final_activities.extend(overrides.iter().map(|(actor, row)| (*actor, row.clone())));
        let mut projected_activities: BTreeMap<_, _> = interruptions
            .projected_activities()
            .map(|(actor, row)| (actor, row.clone()))
            .collect();
        projected_activities.extend(
            final_activities
                .iter()
                .map(|(actor, row)| (*actor, row.clone())),
        );
        let observed = if !placements.is_empty() {
            health
                .as_ref()
                .map(|placement| {
                    let mut facts =
                        sim.health_projected_facts(plan.projection(), &projected_activities)?;
                    // The enclosing physical origin retains its Body guard;
                    // observation consumes the same final rows it will publish.
                    for actor in &mut facts.actors {
                        let body = batch
                            .projected::<game_physiology::Body>(
                                &sim.host.storage,
                                game_ecs::StableId(actor.actor.0),
                            )?
                            .ok_or(Error::Unavailable)?;
                        actor.alive = body.vitals.alive;
                        actor.health = body.vitals.health;
                    }
                    placement.prepare_projected_observations(
                        sim.host.state.health.as_ref().unwrap(),
                        &sim.host.storage,
                        &health::PlacementFacts(sim),
                        sim.host.clock().time,
                        &facts.actors,
                        &facts.geometry,
                    )
                })
                .transpose()?
        } else {
            None
        };
        let finish = plan.stage_into(
            sim.host.state.transport.as_ref(),
            &sim.host.storage,
            &Inputs(sim),
            sim.host.clock().time,
            batch,
        )?;
        let health_effects = health
            .as_ref()
            .map(|plan| plan.effects().to_vec())
            .unwrap_or_default();
        let health = health
            .map(|plan| {
                plan.stage_into(
                    sim.host.state.health.as_ref().unwrap(),
                    &sim.host.storage,
                    &health::PlacementFacts(sim),
                    batch,
                )
            })
            .transpose()?;
        if let Some(observed) = observed {
            observed.stage_into(
                sim.host.state.health.as_ref().unwrap(),
                &sim.host.storage,
                batch,
            )?;
        }
        let interrupted = interruptions.stage_into(sim, batch, &final_activities)?;
        let mut lineage: Option<lineage::PreparedLineageWitness> = None;
        for effect in &effects {
            let event = match *effect {
                MotionEffect::Departed {
                    actor,
                    from,
                    to,
                    arrives,
                } => EventKind::Departed {
                    actor,
                    from,
                    to,
                    arrives,
                },
                MotionEffect::Arrived { actor, area } => EventKind::Arrived { actor, area },
                _ => continue,
            };
            if let Some(next) = sim.prepare_lineage_witness(&event, batch)? {
                lineage = Some(match lineage {
                    Some(previous) => previous.followed_by(next, sim)?,
                    None => next,
                });
            }
        }
        let lineage = lineage
            .map(|plan| plan.stage_into(sim, batch))
            .transpose()?;
        Ok(StagedMotionAdmission {
            scope,
            effects,
            reactions,
            finish,
            health,
            health_effects,
            interrupted,
            lineage,
            extra_finish,
        })
    }
}
impl StagedMotionAdmission {
    pub(crate) fn interruption_events_for(&self, actors: &[PersonId]) -> Vec<EventKind> {
        self.interrupted.events_for(actors)
    }
    pub(crate) fn motion_finalization(&self) -> &game_transport::MotionFinalization {
        &self.finish
    }
    /// Precomputed records in the same order as metadata finalization. These can
    /// join an enclosing host transaction before any canonical publication.
    pub(crate) fn events(&self) -> Vec<EventKind> {
        self.events_after_interruptions_for(&[])
    }
    pub(crate) fn events_after_interruptions_for(&self, actors: &[PersonId]) -> Vec<EventKind> {
        let mut events = Vec::new();
        let mut finished = actors.iter().copied().collect::<BTreeSet<_>>();
        let mut health = self.health_effects.clone();
        for effect in &self.effects {
            if let MotionEffect::Interrupt(actor) = effect
                && finished.insert(*actor)
            {
                events.extend(self.interrupted.events_for(&[*actor]));
            }
            if let Some(event) = motion_receipt(effect, &mut health) {
                events.push(event);
            }
        }
        for actor in &self.extra_finish {
            if finished.insert(*actor) {
                events.extend(self.interrupted.events_for(&[*actor]));
            }
        }
        events
    }
    pub(crate) fn validate_batch(
        &self,
        sim: &Simulation,
        batch: &game_ecs::Batch,
    ) -> Result<(), Error> {
        let Self {
            scope,
            reactions,
            finish,
            health,
            interrupted,
            ..
        } = self;
        reactions.validate(&Activities(sim))?;
        finish.validate_scope(&sim.host.storage, batch, *scope)?;
        if let Some(health) = health {
            health.validate_batch(
                sim.host.state.health.as_ref().unwrap(),
                &sim.host.storage,
                batch,
            )?;
        }
        interrupted.validate_batch(sim, batch)?;
        if let Some(lineage) = &self.lineage {
            lineage.validate(sim, batch)?;
        }
        Ok(())
    }
    pub(crate) fn finish(self, sim: &mut Simulation) {
        self.finish_inner(sim, true);
    }
    pub(crate) fn finish_metadata(self, sim: &mut Simulation) {
        self.finish_inner(sim, false);
    }
    fn finish_inner(self, sim: &mut Simulation, publish_events: bool) {
        let Self {
            effects,
            reactions,
            finish,
            health,
            health_effects: _,
            mut interrupted,
            lineage,
            extra_finish,
            ..
        } = self;
        finish.finish_optional(sim.host.state.transport.as_mut());
        if let Some(lineage) = lineage {
            lineage.finish();
        }
        let mut health_effects = health.map(|finish| finish.finish()).unwrap_or_default();
        for effect in effects {
            match effect {
                MotionEffect::StartLift { actor, completes, .. } => {
                    sim.host.state.due.entry(completes).or_default().push(Due::Physical(actor));
                }
                MotionEffect::Interrupt(actor) => {
                    if publish_events { interrupted.finish_actor(sim, actor); }
                    else { interrupted.finish_actor_metadata(sim, actor); }
                },
                MotionEffect::ClearArrival(actor) => sim.clear_transport_arrival(actor),
                MotionEffect::DetachDependent(actor) => {
                    for dependents in sim.dependent_index.values_mut() {
                        dependents.remove(&actor);
                    }
                }
                MotionEffect::LeaveShelter(actor) => {
                    if let Some(index) = health_effects.iter().position(|effect| matches!(effect, game_health::HealthEffect::ShelterLeft { actor: id, .. } if *id == actor))
                        && let game_health::HealthEffect::ShelterLeft { shelter, actor } = health_effects.remove(index)
                        && publish_events
                    {
                        sim.emit_admitted(EventKind::ShelterLeft { shelter, actor });
                    }
                }
                MotionEffect::ScheduleArrival { actor, at } => {
                    sim.host.state.due.entry(at).or_default().push(Due::Arrive(actor));
                }
                MotionEffect::Departed { actor, from, to, arrives } => { if publish_events { sim.emit_admitted(EventKind::Departed { actor, from, to, arrives }); } },
                MotionEffect::Interrupted { actor } => { if publish_events { sim.emit_admitted(EventKind::Interrupted { actor }); } },
                MotionEffect::Arrived { actor, area } => { if publish_events { sim.emit_admitted(EventKind::Arrived { actor, area }); } },
                MotionEffect::DependentAtArea { actor, area } => sim.index_dependent(actor, area),
                MotionEffect::PutDown { .. } => {}
                MotionEffect::LiftCancelled(lift) => {
                    for actor in reactions.cancelled_deadlines(lift.passenger) {
                        for due in sim.host.state.due.values_mut() {
                            due.retain(|entry| !matches!(entry, Due::Physical(id) if *id == actor));
                        }
                    }
                }
            }
        }
        for actor in extra_finish {
            if publish_events {
                interrupted.finish_actor(sim, actor);
            } else {
                interrupted.finish_actor_metadata(sim, actor);
            }
        }
    }
}

fn motion_receipt(
    effect: &MotionEffect,
    health: &mut Vec<game_health::HealthEffect>,
) -> Option<EventKind> {
    match *effect {
        MotionEffect::LeaveShelter(actor) => {
            let index = health.iter().position(|effect| matches!(effect, game_health::HealthEffect::ShelterLeft { actor: id, .. } if *id == actor))?;
            let game_health::HealthEffect::ShelterLeft { shelter, actor } = health.remove(index)
            else {
                unreachable!("selected shelter exit")
            };
            Some(EventKind::ShelterLeft { shelter, actor })
        }
        MotionEffect::Departed {
            actor,
            from,
            to,
            arrives,
        } => Some(EventKind::Departed {
            actor,
            from,
            to,
            arrives,
        }),
        MotionEffect::Interrupted { actor } => Some(EventKind::Interrupted { actor }),
        MotionEffect::Arrived { actor, area } => Some(EventKind::Arrived { actor, area }),
        _ => None,
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;

    #[test]
    fn composed_motion_rejects_foreign_location_before_publication() {
        let sim = Simulation::transport_fixture(1, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let foreign = sim.people().find(|person| person.id != actor).unwrap().id;
        let plan = sim.prepare_transport_journey(actor, AreaId(3)).unwrap();
        let prepared = sim
            .prepare_motion_admission(plan, game_transport::MotionScope::Ordinary)
            .unwrap();
        let before = sim.canonical_snapshot().unwrap();
        let mut batch = game_ecs::Batch::new();
        let staged = prepared
            .stage_into(&sim, &mut batch, &BTreeMap::new())
            .unwrap();
        batch.insert(game_ecs::StableId(foreign.0), Location::AtArea(AreaId(3)));
        assert_eq!(staged.validate_batch(&sim, &batch), Err(Error::Unavailable));
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }

    #[test]
    fn motion_and_lineage_witness_reject_invalid_foreign_component_together() {
        let mut sim = Simulation::lineage_fixture(1, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let Location::AtArea(area) =
            *game_spatial::location(&sim.host.storage, game_ecs::StableId(actor.0)).unwrap()
        else {
            panic!("fixture ruler starts in an area")
        };
        let destination = sim
            .host
            .state
            .areas
            .iter()
            .find(|node| node.id == area)
            .unwrap()
            .links[0]
            .destination;
        let plan = sim.prepare_transport_journey(actor, destination).unwrap();
        let prepared = sim
            .prepare_motion_admission(plan, game_transport::MotionScope::Ordinary)
            .unwrap();
        let before = sim.canonical_snapshot().unwrap();
        let opening = sim.host.transaction_opening();
        let mut batch = game_ecs::Batch::new();
        let staged = prepared
            .stage_into(&sim, &mut batch, &BTreeMap::new())
            .unwrap();
        assert!(
            game_lineage::guardians(&sim.host.storage)
                .any(|(guardian, _)| batch
                    .touches::<game_lineage::Guardian>(game_ecs::StableId(guardian.0)))
        );
        let mut invalid = game_cargo::inventories(&sim.host.storage)
            .find(|(_, stock)| matches!(stock.location, InventoryLocation::Area(_)))
            .unwrap()
            .1
            .clone();
        invalid.quantity = Quantity(invalid.capacity.0.checked_add(1).unwrap());
        batch.insert(game_ecs::StableId(invalid.id.0), invalid);
        staged.validate_batch(&sim, &batch).unwrap();
        assert!(
            sim.host
                .commit_transaction(game_world::Transaction::new(opening, batch))
                .is_err()
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }

    #[test]
    fn admitted_motion_records_do_not_replay_lineage_observations() {
        let mut sim = Simulation::lineage_fixture(2, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let Location::AtArea(area) =
            *game_spatial::location(&sim.host.storage, game_ecs::StableId(actor.0)).unwrap()
        else {
            panic!("fixture ruler starts in an area")
        };
        let destination = sim
            .host
            .state
            .areas
            .iter()
            .find(|node| node.id == area)
            .unwrap()
            .links[0]
            .destination;
        let plan = sim.prepare_transport_journey(actor, destination).unwrap();
        let prepared = sim
            .prepare_motion_admission(plan, game_transport::MotionScope::Ordinary)
            .unwrap();
        let mut batch = game_ecs::Batch::new();
        let staged = prepared
            .stage_into(&sim, &mut batch, &BTreeMap::new())
            .unwrap();
        staged.validate_batch(&sim, &batch).unwrap();
        let expected_events = staged.events();
        let first_event = sim.host.events().len();
        sim.host.storage.apply_batch(batch).unwrap();
        let guardians = game_lineage::guardians(&sim.host.storage)
            .map(|(id, row)| (id, row.clone()))
            .collect::<Vec<_>>();
        staged.finish(&mut sim);
        assert_eq!(
            sim.host.events()[first_event..]
                .iter()
                .map(|record| record.event.clone())
                .collect::<Vec<_>>(),
            expected_events
        );
        assert_eq!(
            game_lineage::guardians(&sim.host.storage)
                .map(|(id, row)| (id, row.clone()))
                .collect::<Vec<_>>(),
            guardians
        );
        assert!(sim.host.events().iter().any(|record| matches!(record.event, EventKind::Departed { actor: departing, .. } if departing == actor)));
    }

    #[test]
    fn motion_receipts_publish_with_host_and_metadata_finish_emits_nothing() {
        let mut sim = Simulation::lineage_fixture(3, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let Location::AtArea(area) =
            *game_spatial::location(&sim.host.storage, game_ecs::StableId(actor.0)).unwrap()
        else {
            panic!("fixture ruler starts in an area")
        };
        let destination = sim
            .host
            .state
            .areas
            .iter()
            .find(|node| node.id == area)
            .unwrap()
            .links[0]
            .destination;
        let plan = sim.prepare_transport_journey(actor, destination).unwrap();
        let prepared = sim
            .prepare_motion_admission(plan, game_transport::MotionScope::Ordinary)
            .unwrap();
        let opening = sim.host.transaction_opening();
        let mut batch = game_ecs::Batch::new();
        let staged = prepared
            .stage_into(&sim, &mut batch, &BTreeMap::new())
            .unwrap();
        staged.validate_batch(&sim, &batch).unwrap();
        let events = staged.events();
        let arrives = events
            .iter()
            .find_map(|event| match event {
                EventKind::Departed { arrives, .. } => Some(*arrives),
                _ => None,
            })
            .unwrap();
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.events = events;
        sim.host.commit_transaction(transaction).unwrap();
        let published = sim.host.events().to_vec();
        staged.finish_metadata(&mut sim);
        assert_eq!(sim.host.events(), published.as_slice());
        assert!(sim.host.state.due[&arrives].contains(&Due::Arrive(actor)));
    }
}
