//! The coordinator supplies actual bodies and commits pure transport effects.
//! Carrying changes physical support, never ownership, kinship, or government.
use crate::*;
pub use game_transport::{
    Action, Authority, Body, CarryGroup, Position, RoadPosition, Rules, Runtime, View,
};

pub(crate) struct TransportBodyLookup<'a> {
    sim: &'a Simulation,
    now: SimTime,
}
struct GrowingTransportBodies<'a>(&'a Simulation);
impl game_transport::GrowingBodyFacts for GrowingTransportBodies<'_> {
    fn body_at(&self, actor: PersonId, at: SimTime) -> Option<Body> {
        self.0.person_index.get(&actor)?;
        self.0
            .host
            .state
            .transport
            .as_ref()?
            .measured_body(
                &self.0.host.storage,
                actor,
                at,
                &TransportMeasurements(self.0),
            )
            .ok()
    }
}
impl game_transport::BodyFacts for TransportBodyLookup<'_> {
    fn body(&self, actor: PersonId) -> Option<Body> {
        self.sim.person_index.get(&actor)?;
        self.sim
            .host
            .state
            .transport
            .as_ref()?
            .measured_body(
                &self.sim.host.storage,
                actor,
                self.now,
                &TransportMeasurements(self.sim),
            )
            .ok()
    }
}

struct TransportMeasurements<'a>(&'a Simulation);
impl game_transport::MeasuredBodyFacts for TransportMeasurements<'_> {
    fn measurements(
        &self,
        actor: PersonId,
        at: SimTime,
    ) -> Result<game_transport::BodyMeasurements, Error> {
        self.0
            .host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let body = game_physiology::body(&self.0.host.storage, actor).ok_or(Error::Unavailable)?;
        let limits = self.0.body_rules(actor);
        let age = self
            .0
            .host
            .state
            .development
            .as_ref()
            .map(|runtime| -> Result<_, Error> {
                Ok((
                    SimDuration(
                        game_lifecycle::life(&self.0.host.storage, actor)
                            .ok_or(Error::Unavailable)?
                            .age_at(at)?,
                    ),
                    runtime.rules.adulthood,
                ))
            })
            .transpose()?;
        let cargo = game_cargo::carried_quantity_rows(
            self.0
                .inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .map(|id| {
                    game_cargo::inventory(&self.0.host.storage, *id).expect("indexed cargo row")
                }),
        )?;
        let work = match self
            .0
            .host
            .storage
            .activity(actor)
            .ok_or(Error::Unavailable)?
        {
            Activity::Idle => game_transport::CarryWork::Idle,
            Activity::Rest => game_transport::CarryWork::Rest,
            Activity::Lift { .. } => game_transport::CarryWork::Lifting,
            _ => game_transport::CarryWork::Other,
        };
        Ok(game_transport::BodyMeasurements {
            age,
            cargo,
            capacity: limits.carrying_capacity,
            alive: body.vitals.alive,
            health: body.vitals.health,
            fatigue: body.vitals.fatigue,
            fatigue_limit: limits.fatigue_limit,
            independent: self.0.independent(actor),
            combat_restricted: self.0.combat_restricts(actor),
            work,
            incapacitation: self
                .0
                .host
                .state
                .combat
                .as_ref()
                .map(|runtime| runtime.rules.incapacitation),
        })
    }
}

struct RescueMemoryAdapter<'a> {
    signals: Option<&'a BTreeMap<PersonId, game_characters::decisions::ChildSignal>>,
    stocks: Option<&'a BTreeMap<InventoryId, StockKnowledge>>,
    carried: Option<&'a [InventoryId]>,
}
impl game_lineage::RescueMemory for RescueMemoryAdapter<'_> {
    fn signal(&self, child: PersonId) -> Option<game_lineage::RememberedRescueSignal<'_>> {
        self.signals?
            .get(&child)
            .map(|signal| game_lineage::RememberedRescueSignal {
                location: &signal.location,
                preference: signal.preference,
                urgency: signal.urgency,
                observed_at: signal.observed_at,
            })
    }
    fn stocks(&self) -> impl Iterator<Item = &StockKnowledge> {
        self.stocks.into_iter().flat_map(|stocks| stocks.values())
    }
    fn carried(&self, inventory: InventoryId) -> bool {
        self.carried.is_some_and(|ids| ids.contains(&inventory))
    }
}

impl Simulation {
    pub fn initialize_transport(&mut self, rules: Rules) -> Result<(), Error> {
        if self.host.state.transport.is_some() || self.host.state.physical.is_none() {
            return Err(Error::Unavailable);
        }
        self.host.state.transport = Some(Runtime::new(rules)?);
        Ok(())
    }
    pub fn transport_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::lineage_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "transport-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.initialize_transport(Rules {
            adult_mass: Quantity(6_000),
            newborn_mass: Quantity(1_000),
            lift_time: SimDuration(6_000),
            ..Rules::default()
        })?;
        Ok(sim)
    }
    pub(crate) fn resolved_location(&self, actor: PersonId) -> Location {
        game_spatial::resolve(&self.host.storage, game_ecs::StableId(actor.0))
            .expect("validated physical location")
    }

    pub(crate) fn transport_position(&self, actor: PersonId) -> Option<Position> {
        if !self.person_index.contains_key(&actor) {
            return None;
        }
        location_position(&self.resolved_location(actor), self.host.clock().time).ok()
    }
    pub(crate) fn transport_same_place(&self, a: PersonId, b: PersonId) -> bool {
        game_transport::same_place(
            &self.host.storage,
            a,
            b,
            self.host.clock().time,
            &TransportReviewFacts(self),
        )
    }
    pub(crate) fn transport_is_moving(&self, actor: PersonId) -> bool {
        game_spatial::resolve(&self.host.storage, game_ecs::StableId(actor.0))
            .is_ok_and(|location| matches!(location, Location::InTransit { .. }))
    }
    pub(crate) fn transport_exerting(&self, actor: PersonId) -> bool {
        let location = game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0))
            .expect("known location");
        let lifting = self.host.state.physical.is_some()
            && matches!(
                self.host.storage.activity(actor),
                Some(Activity::Lift { .. })
            );
        game_transport::exerting(location, lifting)
    }
    #[cfg(test)]
    fn transport_bodies_at(&self, now: SimTime) -> Result<BTreeMap<PersonId, Body>, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .measured_bodies(
                &self.host.storage,
                self.people().map(|person| person.id),
                now,
                &TransportMeasurements(self),
            )
    }
    #[cfg(test)]
    pub(crate) fn transport_bodies(&self) -> Result<BTreeMap<PersonId, Body>, Error> {
        self.transport_bodies_at(self.host.clock().time)
    }
    pub(crate) fn transport_body_lookup(&self) -> TransportBodyLookup<'_> {
        TransportBodyLookup {
            sim: self,
            now: self.host.clock().time,
        }
    }
    fn apply_transport_change<T>(
        &mut self,
        prepared: game_transport::PreparedTransport<T>,
    ) -> Result<T, Error> {
        let mut batch = game_ecs::Batch::new();
        let finish = prepared.stage_into(
            self.host
                .state
                .transport
                .as_ref()
                .ok_or(Error::Unavailable)?,
            &self.host.storage,
            &self.transport_body_lookup(),
            self.host.clock().time,
            &mut batch,
        )?;
        finish.validate_batch(&self.host.storage, &batch)?;
        game_transport::require_no_location_edits(&batch)?;
        self.host.storage.apply_batch(batch)?;
        Ok(finish.finish(
            self.host
                .state
                .transport
                .as_mut()
                .expect("prevalidated transport runtime"),
        ))
    }
    pub(crate) fn transport_can_depart(&self, actor: PersonId) -> bool {
        self.host.state.transport.as_ref().is_none_or(|runtime| {
            runtime.can_depart(&self.host.storage, &self.transport_body_lookup(), actor)
        })
    }
    #[cfg(test)]
    pub(crate) fn transport_body_load(&self, actor: PersonId) -> Quantity {
        self.host
            .state
            .transport
            .as_ref()
            .map_or(Quantity(0), |runtime| {
                runtime.body_load(&self.host.storage, &self.transport_body_lookup(), actor)
            })
    }
    pub(crate) fn transport_transfer_capacity(&self, actor: PersonId, amount: u64) -> bool {
        if self.host.state.transport.is_none() {
            return true;
        }
        game_transport::transfer_capacity(
            &self.host.storage,
            &self.transport_body_lookup(),
            actor,
            Quantity(amount),
        )
        .unwrap_or(false)
    }
    pub(crate) fn transport_carrying_room(&self, actor: PersonId) -> Quantity {
        let fallback = self.transport_default_runtime();
        let owner = self.host.state.transport.as_ref().unwrap_or(&fallback);
        struct Measured<'a> {
            owner: &'a Runtime,
            sim: &'a Simulation,
        }
        impl game_transport::BodyFacts for Measured<'_> {
            fn body(&self, actor: PersonId) -> Option<Body> {
                self.owner
                    .measured_body(
                        &self.sim.host.storage,
                        actor,
                        self.sim.host.clock().time,
                        &TransportMeasurements(self.sim),
                    )
                    .ok()
            }
        }
        owner
            .carrying_room(&self.host.storage, &Measured { owner, sim: self }, actor)
            .unwrap_or(Quantity(0))
    }
    pub(crate) fn transport_boundary(&self, limit: SimTime) -> Result<SimTime, Error> {
        self.host
            .state
            .transport
            .as_ref()
            .map_or(Ok(limit), |owner| {
                owner.growth_boundary(
                    &self.host.storage,
                    self.host.clock().time,
                    limit,
                    &GrowingTransportBodies(self),
                )
            })
    }
    pub(crate) fn clear_transport_arrival(&mut self, actor: PersonId) {
        for due in self.host.state.due.values_mut() {
            due.retain(|d| !matches!(d,Due::Arrive(id) if *id==actor));
        }
        self.host.state.due.retain(|_, due| !due.is_empty());
    }
    pub(crate) fn transport_death(&mut self, actor: PersonId) {
        let fallback = self.transport_default_runtime();
        let owner = self.host.state.transport.as_ref().unwrap_or(&fallback);
        let plan = owner
            .prepare_death_motion(&self.host.storage, actor, self.host.clock().time)
            .expect("canonical death motion facts");
        self.apply_motion(crate::transport_motion::MotionPlan::Direct(plan))
            .expect("guarded transport death reaction");
    }
    pub(crate) fn transport_retreat(
        &mut self,
        actor: PersonId,
        destination: AreaId,
    ) -> Result<(), Error> {
        let plan = self.prepare_transport_road_return(actor, destination)?;
        self.apply_motion(plan)
    }
    pub(crate) fn transport_guardian_rescue(
        &self,
        guardian: PersonId,
        target: PersonId,
    ) -> Option<Command> {
        let transport = self.host.state.transport.as_ref()?;
        let physical = self.host.state.physical.as_ref()?;
        let body = game_physiology::body(&self.host.storage, guardian)?;
        let location = self.resolved_location(guardian);
        let mind = game_characters::decisions::mind::<Command>(&self.host.storage, guardian);
        let memory = RescueMemoryAdapter {
            signals: mind.map(|mind| &mind.child_signals),
            stocks: game_characters::decisions::stocks(&self.host.storage, guardian)
                .map(|stocks| &stocks.known),
            carried: self
                .inventory_index
                .get(&InventoryLocation::Carried(guardian))
                .map(Vec::as_slice),
        };
        let facts = game_lineage::GuardianRescueFacts {
            now: self.host.clock().time,
            guardian_location: &location,
            co_present: self.transport_same_place(guardian, target),
            moving: self.transport_is_moving(guardian),
            idle_or_resting: matches!(
                self.host.storage.activity(guardian)?,
                Activity::Idle | Activity::Rest
            ),
            combat_restricted: self.combat_restricts(guardian),
            fatigue: body.vitals.fatigue,
            fatigue_limit: self.body_rules(guardian).fatigue_limit,
            meal_size: physical.rules.meal_size,
            target_independent: self.independent(target),
            target_health: game_physiology::body(&self.host.storage, target)?
                .vitals
                .health,
        };
        game_lineage::guardian_rescue(
            &self.host.storage,
            transport,
            guardian,
            target,
            facts,
            &memory,
            &self.transport_body_lookup(),
        )
        .map(|intent| match intent {
            game_lineage::RescueIntent::Care { child, source } => Command::Care { child, source },
            game_lineage::RescueIntent::Comfort { child } => Command::Comfort { child },
            game_lineage::RescueIntent::Rest => Command::Rest,
            game_lineage::RescueIntent::Transport(action) => Command::Transport { action },
        })
    }
    pub(crate) fn reconcile_transport(&mut self) {
        if self.host.state.transport.is_none() {
            return;
        }
        let Some(plan) = self.prepare_transport_reconciliation() else {
            return;
        };
        self.apply_motion(plan)
            .expect("guarded transport reconciliation");
    }
    pub(crate) fn complete_transport_lift(&mut self, actor: PersonId) {
        let outcome = self
            .prepare_transport_lift_finish(actor)
            .and_then(|plan| self.apply_motion(plan));
        if let Some(game_transport::LiftClosingReaction::Cancel(actor)) =
            game_transport::lift_closing_reaction(actor, outcome)
        {
            self.interrupt_transport_lift(actor);
        }
    }
    pub(crate) fn interrupt_transport_lift(&mut self, actor: PersonId) {
        let Some(owner) = self.host.state.transport.as_ref() else {
            return;
        };
        let plan = owner.prepare_lift_cancellation_motion(
            &self.host.storage,
            actor,
            self.host.clock().time,
        );
        self.apply_motion(crate::transport_motion::MotionPlan::Direct(plan))
            .expect("guarded lift action reactions");
    }
    #[cfg(test)]
    pub(crate) fn transport_approach_road(
        &mut self,
        actor: PersonId,
        target: RoadPosition,
    ) -> Result<(), Error> {
        let plan = self.prepare_transport_approach(actor, target)?;
        self.apply_motion(plan)
    }
    pub(crate) fn transport_command(
        &mut self,
        actor: PersonId,
        action: &Action,
    ) -> Result<(), Error> {
        self.transport_command_scoped(actor, action, game_transport::MotionScope::Ordinary)
    }
    fn transport_command_scoped(
        &mut self,
        actor: PersonId,
        action: &Action,
        scope: game_transport::MotionScope,
    ) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        if self.host.state.transport.is_none() || !self.person_index.contains_key(&actor) {
            return Err(Error::Unavailable);
        }
        match action {
            Action::OfferCarry { passenger } => {
                let change = self.prepare_transport_offer(actor, *passenger)?;
                self.apply_motion_scoped(change, scope)?;
                if let Some(plan) =
                    self.prepare_automatic_transport_lift(game_transport::OfferedSupport::Single {
                        bearer: actor,
                        passenger: *passenger,
                    })?
                {
                    self.apply_motion_scoped(plan, scope)?;
                }
            }
            Action::AcceptCarry { bearers } => {
                let plan = self.prepare_transport_accept(actor, bearers)?;
                self.apply_motion_scoped(plan, scope)?;
            }
            Action::RefuseCarry { bearer } => {
                let change = self
                    .host
                    .state
                    .transport
                    .as_ref()
                    .unwrap()
                    .prepare_refusal(&self.host.storage, actor, *bearer)?;
                self.apply_transport_change(change)?;
            }
            Action::JoinCarry { passenger } => {
                let change = self.prepare_transport_offer(actor, *passenger)?;
                self.apply_motion_scoped(change, scope)?;
                if let Some(plan) =
                    self.prepare_automatic_transport_lift(game_transport::OfferedSupport::Joined {
                        passenger: *passenger,
                    })?
                {
                    self.apply_motion_scoped(plan, scope)?;
                }
            }
            Action::PutDown { passenger } | Action::WithdrawSupport { passenger } => {
                let plan = self.prepare_transport_putdown(actor, *passenger)?;
                self.apply_motion_scoped(plan, scope)?;
            }
            Action::Stop => {
                let plan = self.prepare_requested_transport_stop(actor)?;
                self.apply_motion_scoped(plan, scope)?;
            }
            Action::Resume => {
                let plan = self.prepare_transport_resume(actor)?;
                self.apply_motion_scoped(plan, scope)?;
            }
            Action::ReturnToArea { destination } => {
                let plan = self.prepare_transport_return_request(actor, *destination)?;
                self.apply_motion_scoped(plan, scope)?;
            }
            Action::ApproachRoad {
                from,
                to,
                progress,
                duration,
            } => self.apply_motion_scoped(
                self.prepare_transport_approach(
                    actor,
                    RoadPosition {
                        from: *from,
                        to: *to,
                        progress: *progress,
                        duration: *duration,
                    },
                )?,
                scope,
            )?,
        }
        self.host.bump_revision(1)?;
        Ok(())
    }
    pub(crate) fn transport_view(&self, actor: PersonId) -> Option<View> {
        self.host.state.transport.as_ref()?.view(
            &self.host.storage,
            &self.transport_body_lookup(),
            actor,
            self.host.clock().time,
        )
    }
    pub(crate) fn review_transport_choices(&mut self) -> Result<(), Error> {
        if self.host.state.transport.is_none() {
            return Ok(());
        }
        game_transport::review_choices(&mut TransportReview(self))
    }
    pub(crate) fn available_transport_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(world) = &self.host.state.transport else {
            return Vec::new();
        };
        world
            .available_actions(
                &self.host.storage,
                actor,
                self.host.clock().time,
                game_transport::AvailableActionFacts {
                    independent: self.independent(actor),
                    combat_restricted: self.combat_restricts(actor),
                    moving: self.transport_is_moving(actor),
                    stopped_on_road: matches!(
                        game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)),
                        Some(Location::StoppedOnRoad { .. })
                    ),
                    position: self.transport_position(actor),
                },
                || {
                    game_transport::local_bodies(
                        &self.host.storage,
                        actor,
                        self.host.clock().time,
                        &TransportReviewFacts(self),
                    )
                },
                |passenger| self.transport_is_moving(passenger),
                || self.transport_can_depart(actor),
            )
            .into_iter()
            .map(|action| Command::Transport { action })
            .collect()
    }
}
struct TransportReviewFacts<'a>(&'a Simulation);
impl game_transport::BodyFacts for TransportReviewFacts<'_> {
    fn body(&self, actor: PersonId) -> Option<Body> {
        game_transport::BodyFacts::body(&self.0.transport_body_lookup(), actor)
    }
}
impl game_transport::SupportWitnessFacts for TransportReviewFacts<'_> {
    fn person(&self, actor: PersonId) -> bool {
        self.0.person_index.contains_key(&actor)
    }
    fn tactical_contact(&self, a: PersonId, b: PersonId) -> bool {
        self.0
            .host
            .state
            .combat
            .as_ref()
            .is_none_or(|combat| combat.physical_contact(&self.0.host.storage, a, b))
    }
}
struct TransportReview<'a>(&'a mut Simulation);
impl game_transport::BodyFacts for TransportReview<'_> {
    fn body(&self, actor: PersonId) -> Option<Body> {
        game_transport::BodyFacts::body(&TransportReviewFacts(self.0), actor)
    }
}
impl game_transport::SupportWitnessFacts for TransportReview<'_> {
    fn person(&self, actor: PersonId) -> bool {
        self.0.person_index.contains_key(&actor)
    }
    fn tactical_contact(&self, a: PersonId, b: PersonId) -> bool {
        self.0
            .host
            .state
            .combat
            .as_ref()
            .is_none_or(|combat| combat.physical_contact(&self.0.host.storage, a, b))
    }
}
impl game_transport::ReviewHost for TransportReview<'_> {
    fn store(&self) -> &game_ecs::Store {
        &self.0.host.storage
    }
    fn runtime(&self) -> &Runtime {
        self.0.host.state.transport.as_ref().unwrap()
    }
    fn now(&self) -> SimTime {
        self.0.host.clock().time
    }
    fn autonomous(&self, actor: PersonId) -> bool {
        actor != self.0.host.state.ruler
    }
    fn contact(&self, a: PersonId, b: PersonId) -> bool {
        self.0.transport_same_place(a, b)
    }
    fn relation(&self, actor: PersonId, other: PersonId) -> Option<game_transport::RelationFacts> {
        game_characters::decisions::mind::<Command>(&self.0.host.storage, actor)?
            .relationships
            .get(&other)
            .map(|relation| game_transport::RelationFacts {
                trust: relation.trust,
                affection: relation.affection,
            })
    }
    fn offer_facts(&self, actor: PersonId) -> Result<game_transport::OfferResponseFacts, Error> {
        let body = game_physiology::body(&self.0.host.storage, actor).ok_or(Error::Unavailable)?;
        let mind = game_characters::decisions::mind::<Command>(&self.0.host.storage, actor)
            .ok_or(Error::Unavailable)?;
        let rules = self.0.body_rules(actor);
        Ok(game_transport::OfferResponseFacts {
            autonomous: game_transport::OfferEligibilityFacts {
                actor,
                ruler: self.0.host.state.ruler,
                independent: self.0.independent(actor),
            }
            .autonomous_response(),
            alive: body.vitals.alive,
            combat_restricted: self.0.combat_restricts(actor),
            idle_or_resting: matches!(
                self.0.host.storage.activity(actor),
                Some(Activity::Idle | Activity::Rest)
            ),
            health: body.vitals.health,
            max_health: rules.max_health,
            hunger: body.vitals.hunger,
            hunger_harm: rules.hunger_harm,
            fatigue: body.vitals.fatigue,
            fatigue_limit: rules.fatigue_limit,
            independence_goal: mind.dream == decisions::Aspiration::Independence,
        })
    }
    fn support_facts(
        &self,
        actor: PersonId,
        request: &game_transport::SupportRequest,
    ) -> Result<game_transport::SupportFacts, Error> {
        let body = game_physiology::body(&self.0.host.storage, actor).ok_or(Error::Unavailable)?;
        let mind = game_characters::decisions::mind::<Command>(&self.0.host.storage, actor)
            .ok_or(Error::Unavailable)?;
        let rules = self.0.body_rules(actor);
        Ok(game_transport::SupportFacts {
            autonomous: actor != self.0.host.state.ruler,
            empathy: mind.personality.empathy,
            relation: self.relation(actor, request.requester).unwrap_or_default(),
            hunger: body.vitals.hunger,
            meal_relief: rules.meal_relief,
            fatigue: body.vitals.fatigue,
            fatigue_limit: rules.fatigue_limit,
            occupied: game_transport::SupportAvailabilityFacts {
                activity: self
                    .0
                    .host
                    .storage
                    .activity(actor)
                    .map(|activity| match activity {
                        Activity::Idle => game_transport::SupportActivity::Idle,
                        Activity::Rest => game_transport::SupportActivity::Rest,
                        _ => game_transport::SupportActivity::Other,
                    }),
                lineage_duty: self.0.lineage_handles(actor),
                health_duty: self.0.health_actor_busy(actor),
                mission_duty: self.0.mission_actor_busy(actor),
            }
            .occupied(),
            co_present: self.0.transport_same_place(actor, request.passenger),
        })
    }
    fn apply_review(&mut self, change: game_transport::PreparedTransport<()>) -> Result<(), Error> {
        self.0.apply_transport_change(change)
    }
    fn apply_action(&mut self, actor: PersonId, action: &Action) -> Result<(), Error> {
        self.0.transport_command_scoped(
            actor,
            action,
            game_transport::MotionScope::StationaryReview,
        )
    }
}
pub(crate) fn location_position(location: &Location, now: SimTime) -> Result<Position, Error> {
    game_transport::position_from_location(location, now)
}
#[cfg(test)]
fn position_location(position: Position) -> Location {
    game_transport::location_from_position(position)
}

struct TransportSaveFacts<'a> {
    state: &'a CanonicalState<'a>,
    actors: BTreeSet<PersonId>,
    cargo: BTreeMap<PersonId, Quantity>,
}
impl game_transport::RouteReferences for TransportSaveFacts<'_> {
    fn location(&self, actor: PersonId) -> Option<&Location> {
        self.actors
            .contains(&actor)
            .then(|| game_spatial::location(self.state.components, game_ecs::StableId(actor.0)))
            .flatten()
    }
    fn link(&self, from: AreaId, to: AreaId) -> bool {
        self.state
            .state
            .areas
            .iter()
            .find(|area| area.id == from)
            .is_some_and(|area| area.links.iter().any(|link| link.destination == to))
    }
}
impl game_transport::TransportReferences for TransportSaveFacts<'_> {
    fn lifting(&self, actor: PersonId) -> Option<(PersonId, SimTime)> {
        match self.state.components.activity(actor)? {
            Activity::Lift {
                passenger,
                completes,
            } => Some((*passenger, *completes)),
            _ => None,
        }
    }
    fn lifters(&self) -> impl Iterator<Item = (PersonId, PersonId, SimTime)> {
        self.state
            .components
            .activities()
            .filter_map(|(actor, activity)| match activity {
                Activity::Lift {
                    passenger,
                    completes,
                } => Some((actor, *passenger, *completes)),
                _ => None,
            })
    }
}
impl game_transport::MotionReferences for TransportSaveFacts<'_> {
    fn actors(&self) -> impl Iterator<Item = PersonId> {
        self.actors.iter().copied()
    }
    fn arrivals(&self) -> impl Iterator<Item = (PersonId, SimTime)> {
        self.state.state.due.iter().flat_map(|(at, due)| {
            due.iter().filter_map(move |due| match due {
                Due::Arrive(actor) => Some((*actor, *at)),
                _ => None,
            })
        })
    }
    fn alive(&self, actor: PersonId) -> Option<bool> {
        game_physiology::body(self.state.components, actor).map(|body| body.vitals.alive)
    }
    fn physical_enabled(&self) -> bool {
        self.state.state.physical.is_some()
    }
    fn area_exists(&self, area: AreaId) -> bool {
        self.state.state.areas.iter().any(|row| row.id == area)
    }
    fn duration(&self, from: AreaId, to: AreaId) -> Option<SimDuration> {
        self.state
            .state
            .areas
            .iter()
            .find(|area| area.id == from)?
            .links
            .iter()
            .find(|link| link.destination == to)
            .map(|link| link.duration)
    }
}

impl game_transport::MeasuredBodyFacts for TransportSaveFacts<'_> {
    fn measurements(
        &self,
        actor: PersonId,
        at: SimTime,
    ) -> Result<game_transport::BodyMeasurements, Error> {
        let state = self.state;
        let fail = || Error::InvalidSave("missing physical transport body".into());
        let physical = state.state.physical.as_ref().ok_or_else(fail)?;
        let body = game_physiology::body(state.components, actor).ok_or_else(fail)?;
        let age = state
            .state
            .development
            .as_ref()
            .map(|runtime| -> Result<_, Error> {
                Ok((
                    SimDuration(
                        game_lifecycle::life(state.components, actor)
                            .ok_or_else(fail)?
                            .age_at(at)?,
                    ),
                    runtime.rules.adulthood,
                ))
            })
            .transpose()?;
        Ok(game_transport::BodyMeasurements {
            age,
            cargo: self.cargo.get(&actor).copied().unwrap_or(Quantity(0)),
            capacity: physical::rules_for(state, actor).carrying_capacity,
            alive: body.vitals.alive,
            health: body.vitals.health,
            fatigue: body.vitals.fatigue,
            fatigue_limit: physical.rules.fatigue_limit,
            independent: true,
            combat_restricted: false,
            work: game_transport::CarryWork::Idle,
            incapacitation: state
                .state
                .combat
                .as_ref()
                .map(|runtime| runtime.rules.incapacitation),
        })
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let cargo = game_cargo::carried_quantities(state.components)?;
    let facts = TransportSaveFacts {
        state,
        actors: state.people().map(|person| person.id).collect(),
        cargo,
    };
    game_transport::validate_save(
        state.state.transport.as_ref(),
        state.components,
        &facts,
        state.time,
    )
}

pub(crate) fn resolved_locations_for(
    state: &CanonicalState,
) -> Result<BTreeMap<PersonId, Location>, Error> {
    let locations = game_spatial::resolve_locations(
        game_spatial::locations(state.components)
            .map(|(id, location)| (game_core::PersonId(id.0), location))
            .map(|(id, location)| (game_ecs::StableId(id.0), location)),
    )?;
    Ok(locations
        .into_iter()
        .map(|(id, location)| (PersonId(id.0), location))
        .collect())
}

#[cfg(test)]
mod connected_tests {
    use super::*;

    #[test]
    fn returning_from_an_exact_road_endpoint_creates_no_zero_length_journey() {
        let mut sim = Simulation::transport_fixture(1, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        sim.debug_actor_command(actor, Command::Cancel).unwrap();
        *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId((actor).0))
            .expect("location exists") = Location::StoppedOnRoad {
            from: AreaId(1),
            to: AreaId(3),
            progress: SimDuration(0),
            duration: SimDuration(180_000),
        };
        let now = sim.host.clock().time;
        sim.debug_actor_command(
            actor,
            Command::Transport {
                action: Action::ReturnToArea {
                    destination: AreaId(1),
                },
            },
        )
        .unwrap();
        assert_eq!(sim.host.clock().time, now);
        assert_eq!(sim.resolved_location(actor), Location::AtArea(AreaId(1)));
        assert!(
            !sim.host
                .state
                .due
                .values()
                .flatten()
                .any(|due| matches!(due, Due::Arrive(id) if *id == actor))
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }

    #[test]
    fn carrying_reachable_roadside_goods_uses_the_same_conserved_loading_primitive() {
        let mut sim = Simulation::transport_fixture(1, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let holder = sim.person_at(3).id;
        for person in [actor, holder] {
            sim.debug_actor_command(person, Command::Cancel).unwrap();
            *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId((person).0))
                .expect("location exists") = Location::StoppedOnRoad {
                from: AreaId(1),
                to: AreaId(3),
                progress: SimDuration(90_000),
                duration: SimDuration(180_000),
            };
        }
        let source = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(holder),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        sim.debug_actor_command(
            actor,
            Command::Carry {
                source,
                quantity: Quantity(1_000),
            },
        )
        .unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        sim.advance(sim.body_rules(actor).loading_time).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert!(sim.host.events().iter().any(|event| matches!(event.event,
            EventKind::Transferred { source: actual, quantity: Quantity(1_000), .. }
                if actual == source)));
        assert!(matches!(
            sim.resolved_location(actor),
            Location::StoppedOnRoad {
                progress: SimDuration(90_000),
                ..
            }
        ));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn local_action_facts_skip_remote_bodies_and_preserve_exact_road_candidates() {
        let mut sim = Simulation::transport_fixture(61, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let nearby = sim.person_at(7).id;
        let reversed = sim.person_at(8).id;
        let farther = sim.person_at(9).id;
        let road = RoadPosition {
            from: AreaId(1),
            to: AreaId(3),
            progress: SimDuration(90_000),
            duration: SimDuration(180_000),
        };
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId(person_id.0))
                .unwrap() = Location::AtArea(AreaId(3));
        }
        for (person, point) in [
            (actor, road),
            (nearby, road),
            (
                reversed,
                RoadPosition {
                    from: road.to,
                    to: road.from,
                    ..road
                },
            ),
            (
                farther,
                RoadPosition {
                    progress: SimDuration(90_001),
                    ..road
                },
            ),
        ] {
            *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId(person.0))
                .unwrap() = position_location(Position::Road(point));
        }
        struct Counted<'a> {
            bodies: &'a BTreeMap<PersonId, Body>,
            reads: std::cell::RefCell<Vec<PersonId>>,
        }
        impl game_transport::BodyFacts for Counted<'_> {
            fn body(&self, actor: PersonId) -> Option<Body> {
                self.reads.borrow_mut().push(actor);
                self.bodies.get(&actor).cloned()
            }
        }
        let full = sim.transport_bodies().unwrap();
        let reader = Counted {
            bodies: &full,
            reads: std::cell::RefCell::new(Vec::new()),
        };
        impl game_transport::SupportWitnessFacts for Counted<'_> {
            fn person(&self, actor: PersonId) -> bool {
                self.bodies.contains_key(&actor)
            }
            fn tactical_contact(&self, _: PersonId, _: PersonId) -> bool {
                true
            }
        }
        let local =
            game_transport::local_bodies(&sim.host.storage, actor, sim.host.clock().time, &reader)
                .unwrap();
        assert_eq!(reader.reads.borrow().len(), 2);
        assert_eq!(
            reader
                .reads
                .borrow()
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([actor, nearby])
        );
        assert_eq!(
            local.keys().copied().collect::<Vec<_>>(),
            vec![actor, nearby]
        );
        assert!(sim.transport_same_place(actor, reversed));
        let facts = game_transport::AvailableActionFacts {
            independent: true,
            combat_restricted: false,
            moving: false,
            stopped_on_road: false,
            position: sim.transport_position(actor),
        };
        let runtime = sim.host.state.transport.as_ref().unwrap();
        let before = runtime.available_actions(
            &sim.host.storage,
            actor,
            sim.host.clock().time,
            facts,
            || Some(full),
            |_| false,
            || false,
        );
        let after = runtime.available_actions(
            &sim.host.storage,
            actor,
            sim.host.clock().time,
            facts,
            || Some(local),
            |_| false,
            || false,
        );
        assert_eq!(before, after);
        assert_eq!(after, vec![Action::OfferCarry { passenger: nearby }]);
    }
    fn quiet(sim: &mut Simulation) {
        let citizens: Vec<_> = sim
            .people()
            .filter(|p| p.role != Role::Guardian)
            .map(|p| p.id)
            .collect();
        for actor in citizens {
            let mut traits = decisions::Personality::restful();
            traits.empathy = 0;
            sim.debug_set_personality(actor, traits, decisions::Aspiration::Comfort)
                .unwrap();
        }
    }
    fn traveling_birth(workers: usize) -> (Simulation, PersonId, PersonId) {
        let mut sim = Simulation::transport_fixture(
            1,
            Execution {
                workers,
                reverse_jobs: workers > 1,
            },
        )
        .unwrap();
        sim.host
            .state
            .physical
            .as_mut()
            .unwrap()
            .rules
            .fatigue_limit = 2_000_000;
        sim.advance(SimDuration(120_000)).unwrap();
        let gestator = game_lifecycle::all_pregnancies(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|p| matches!(p.state, development::PregnancyState::Gestating { .. }))
            .unwrap()
            .gestator;
        quiet(&mut sim);
        sim.advance(SimDuration(120_000)).unwrap();
        let predecessor = sim.host.state.ruler;
        sim.debug_injure(predecessor, sim.body_rules(predecessor).max_health)
            .unwrap();
        let routes = sim
            .host
            .state
            .areas
            .iter()
            .map(|area| {
                (
                    area.id,
                    area.links
                        .iter()
                        .map(|l| (l.destination, l.duration))
                        .collect(),
                )
            })
            .collect();
        game_characters::decisions::mind_mut::<game_api::Command>(
            &mut sim.host.storage,
            gestator,
        )
        .unwrap()
        .routes = routes;
        sim.debug_move_actor(gestator, AreaId(3)).unwrap();
        sim.debug_move_actor(gestator, AreaId(2)).unwrap();
        sim.advance(SimDuration(60_000)).unwrap();
        let child = sim.host.state.ruler;
        assert_ne!(child, predecessor);
        (sim, gestator, child)
    }
    #[test]
    fn component_capacity_checks_match_the_full_body_snapshot() {
        let (sim, _, _) = traveling_birth(1);
        let bodies = sim.transport_bodies().unwrap();
        let full = game_transport::capacity_plan(&sim.host.storage, &bodies).unwrap();
        for actor in bodies.keys().copied() {
            for amount in [0, 500, 2_000, 20_000] {
                assert_eq!(
                    sim.transport_transfer_capacity(actor, amount),
                    game_transport::transfer_capacity(
                        &sim.host.storage,
                        &bodies,
                        actor,
                        Quantity(amount)
                    )
                    .unwrap()
                );
            }
            let load = full
                .shares
                .iter()
                .filter(|((_, bearer), _)| *bearer == actor)
                .map(|(_, q)| q.0)
                .sum();
            assert_eq!(sim.transport_body_load(actor), Quantity(load));
        }
        assert!(!sim.transport_transfer_capacity(PersonId(u64::MAX), 1));
    }
    #[test]
    fn actual_newborn_follows_the_carrier_on_later_legs_without_own_arrival() {
        let (mut sim, mother, child) = traveling_birth(1);
        assert_eq!(
            *game_spatial::location(&sim.host.storage, game_ecs::StableId((child).0))
                .expect("location exists"),
            Location::Carried { carrier: mother }
        );
        assert!(sim.observe().available_commands.iter().any(|c| matches!(
            c,
            Command::Signal {
                preference: ChildhoodInterest::Food
            }
        )));
        assert!(
            sim.host
                .state
                .due
                .values()
                .flatten()
                .all(|d| !matches!(d,Due::Arrive(id) if *id==child))
        );
        assert_eq!(
            game_physiology::body(&sim.host.storage, child)
                .unwrap()
                .vitals
                .fatigue,
            0
        );
        sim.advance(SimDuration(150_000)).unwrap();
        assert!(matches!(
            sim.resolved_location(child),
            Location::InTransit {
                from: AreaId(3),
                to: AreaId(1),
                departed: SimTime(420_000),
                ..
            }
        ));
        assert_eq!(
            game_physiology::body(&sim.host.storage, child)
                .unwrap()
                .vitals
                .fatigue,
            0
        );
        let loaded = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(loaded.canonical_state(), sim.canonical_state());
    }
    #[test]
    fn a_carrier_dies_at_an_exact_road_position_with_body_passenger_and_cargo_preserved() {
        let (mut sim, mother, child) = traveling_birth(1);
        let cargo = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Carried(mother),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([mother]),
            )
            .unwrap();
        let hunger_harm = sim.body_rules(mother).hunger_harm;
        let body = game_physiology::body_mut(&mut sim.host.storage, mother).unwrap();
        body.vitals.hunger = hunger_harm;
        body.vitals.health = 10_000;
        sim.advance(SimDuration(10_000)).unwrap();
        assert!(
            !game_physiology::body(&sim.host.storage, mother)
                .unwrap()
                .vitals
                .alive
        );
        let stopped = Location::StoppedOnRoad {
            from: AreaId(1),
            to: AreaId(3),
            progress: SimDuration(70_000),
            duration: SimDuration(180_000),
        };
        assert_eq!(
            *game_spatial::location(&sim.host.storage, game_ecs::StableId((mother).0))
                .expect("location exists"),
            stopped
        );
        assert_eq!(sim.resolved_location(child), stopped);
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, cargo)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        sim.advance(SimDuration(20_003)).unwrap();
        assert_eq!(sim.resolved_location(child), stopped);
        let loaded = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(loaded.canonical_state(), sim.canonical_state());
    }
    #[test]
    fn real_timed_lift_requires_the_adult_passengers_personal_acceptance() {
        let mut sim = Simulation::transport_fixture(7, Execution::default()).unwrap();
        quiet(&mut sim);
        let carrier = sim.host.state.ruler;
        let passenger = sim.person_at(8).id;
        sim.transport_command(carrier, &Action::OfferCarry { passenger })
            .unwrap();
        assert!(game_transport::groups(&sim.host.storage).next().is_none());
        assert!(game_transport::lifts(&sim.host.storage).next().is_none());
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.transport_command(
                passenger,
                &Action::AcceptCarry {
                    bearers: vec![carrier, carrier]
                }
            ),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        sim.transport_command(
            passenger,
            &Action::AcceptCarry {
                bearers: vec![carrier],
            },
        )
        .unwrap();
        sim.advance(SimDuration(5_999)).unwrap();
        assert!(game_transport::groups(&sim.host.storage).next().is_none());
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(
            *game_spatial::location(&sim.host.storage, game_ecs::StableId((passenger).0))
                .expect("location exists"),
            Location::Carried { carrier }
        );
        assert_eq!(
            game_transport::group(&sim.host.storage, passenger)
                .unwrap()
                .authority,
            Authority::Consented
        );
        sim.transport_command(passenger, &Action::PutDown { passenger })
            .unwrap();
        assert_eq!(
            *game_spatial::location(&sim.host.storage, game_ecs::StableId((passenger).0))
                .expect("location exists"),
            Location::AtArea(AreaId(1))
        );
    }
    #[test]
    fn birth_transport_and_stopping_replay_identically_across_workers_and_time_slices() {
        let (mut whole, mother, child) = traveling_birth(1);
        let (mut split, _, _) = traveling_birth(4);
        whole.advance(SimDuration(70_003)).unwrap();
        for dt in [1, 19_999, 7_001, 43_002] {
            split.advance(SimDuration(dt)).unwrap();
            let _ = split.observe();
        }
        assert_eq!(whole.canonical_state(), split.canonical_state());
        for sim in [&mut whole, &mut split] {
            sim.transport_command(mother, &Action::Stop).unwrap();
        }
        assert_eq!(
            whole.resolved_location(child),
            split.resolved_location(child)
        );
        whole.advance(SimDuration(11_007)).unwrap();
        for dt in [7_003, 4_004] {
            split.advance(SimDuration(dt)).unwrap();
        }
        assert_eq!(whole.canonical_state(), split.canonical_state());
        let mut restored = Simulation::load(&split.save().unwrap(), Execution::default()).unwrap();
        split.transport_command(mother, &Action::Resume).unwrap();
        restored.transport_command(mother, &Action::Resume).unwrap();
        split.advance(SimDuration(19_003)).unwrap();
        restored.advance(SimDuration(19_003)).unwrap();
        assert_eq!(restored.canonical_state(), split.canonical_state());
    }
    #[test]
    fn a_stopped_roadside_child_can_signal_and_receive_real_carried_food() {
        let (mut sim, mother, child) = traveling_birth(1);
        sim.advance(SimDuration(30_000)).unwrap();
        sim.transport_command(mother, &Action::Stop).unwrap();
        let source = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Carried(mother),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([mother]),
            )
            .unwrap();
        sim.debug_actor_command(
            child,
            Command::Signal {
                preference: ChildhoodInterest::Food,
            },
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert!(
            matches!(
                *sim.host.storage.activity(child).unwrap(),
                Activity::Idle | Activity::Rest
            ),
            "child primary action {:?}",
            *sim.host.storage.activity(child).unwrap()
        );
        assert!(sim.transport_same_place(mother, child));
        assert!(sim.has_stock_access(
            mother,
            game_cargo::inventory(&sim.host.storage, source).expect("inventory exists")
        ));
        assert!(!sim.care_index.contains_key(&child));
        sim.debug_actor_command(mother, Command::Care { child, source })
            .unwrap();
        sim.advance(SimDuration(60_000)).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        assert_eq!(
            game_physiology::body(&sim.host.storage, child)
                .unwrap()
                .vitals
                .hunger,
            0
        );
        assert!(matches!(
            sim.resolved_location(child),
            Location::StoppedOnRoad { .. }
        ));
    }
    #[test]
    fn simultaneous_incoming_cargo_respects_the_entire_bearer_group() {
        incoming_cargo_prefix(false);
    }
    #[test]
    fn incoming_cargo_prefix_to_one_holder_respects_connected_passenger_load() {
        incoming_cargo_prefix(true);
    }
    fn incoming_cargo_prefix(same_holder: bool) {
        let mut sim = Simulation::transport_fixture(23, Execution::default()).unwrap();
        quiet(&mut sim);
        sim.host.state.transport.as_mut().unwrap().rules.adult_mass = Quantity(16_000);
        let a = sim.host.state.ruler;
        let b = sim.person_at(7).id;
        let passenger = sim.person_at(8).id;
        for bearer in [a, b] {
            sim.transport_command(bearer, &Action::OfferCarry { passenger })
                .unwrap();
        }
        sim.transport_command(
            passenger,
            &Action::AcceptCarry {
                bearers: vec![a, b],
            },
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        let source = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(AreaId(1)),
                Quantity(6_000),
                Quantity(6_000),
                BTreeSet::from([a, b]),
            )
            .unwrap();
        let destinations: Vec<_> = [a, if same_holder { a } else { b }]
            .into_iter()
            .map(|actor| {
                sim.debug_add_inventory(
                    Good::Timber,
                    InventoryLocation::Carried(actor),
                    Quantity(0),
                    Quantity(6_000),
                    BTreeSet::from([actor]),
                )
                .unwrap()
            })
            .collect();
        sim.debug_schedule_transfers(
            SimTime(12_000),
            vec![
                TransferIntent {
                    actor: a,
                    source,
                    destination: destinations[0],
                    quantity: Quantity(3_000),
                },
                TransferIntent {
                    actor: if same_holder { a } else { b },
                    source,
                    destination: destinations[1],
                    quantity: Quantity(3_000),
                },
            ],
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            Quantity(3_000)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destinations[0])
                .expect("inventory exists")
                .quantity,
            Quantity(3_000)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destinations[1])
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity
                .0
                + destinations
                    .iter()
                    .map(|id| game_cargo::inventory(&sim.host.storage, *(id))
                        .expect("inventory exists")
                        .quantity
                        .0)
                    .sum::<u64>(),
            6_000
        );
    }
    #[test]
    fn road_observation_uses_real_supplies_and_local_distress_without_endpoint_knowledge() {
        let (mut sim, mother, child) = traveling_birth(1);
        sim.transport_command(mother, &Action::Stop).unwrap();
        let endpoint = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Area(AreaId(1)),
                Quantity(20_000),
                Quantity(20_000),
                BTreeSet::from([mother]),
            )
            .unwrap();
        let carried = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Carried(mother),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([mother]),
            )
            .unwrap();
        sim.debug_actor_command(
            child,
            Command::Signal {
                preference: ChildhoodInterest::Food,
            },
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        let mut traits = decisions::Personality::restful();
        traits.empathy = 900;
        sim.debug_set_personality(mother, traits, decisions::Aspiration::Comfort)
            .unwrap();
        game_physiology::body_mut(&mut sim.host.storage, mother)
            .unwrap()
            .vitals
            .hunger = 0;
        sim.review_decisions().unwrap();
        let mind =
            &game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, mother)
                .unwrap();
        let stocks = game_characters::decisions::stocks(&sim.host.storage, mother)
            .expect("stock knowledge exists");
        assert!(!stocks.known.contains_key(&endpoint));
        assert_eq!(stocks.known[&carried].quantity, Quantity(2_000));
        assert!(matches!(
            mind.child_signals[&child].location,
            Location::StoppedOnRoad { .. }
        ));
        assert!(
            matches!(*sim.host.storage.activity(mother).unwrap(),Activity::Care {child:target,source,..} if target==child&&source==carried)
        );
        sim.advance(SimDuration(60_000)).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, carried)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, endpoint)
                .expect("inventory exists")
                .quantity,
            Quantity(20_000)
        );
    }
    #[test]
    fn a_carried_child_in_battle_requires_the_anchors_actual_tactical_reach() {
        let (mut sim, mother, child) = traveling_birth(1);
        game_transport::replace_route(&mut sim.host.storage, mother, Vec::new())
            .expect("actor route");
        game_physiology::body_mut(&mut sim.host.storage, mother)
            .unwrap()
            .vitals
            .hunger = 0;
        sim.advance(SimDuration(120_000)).unwrap();
        let enemy = sim.person_at(8).id;
        let outsider = sim.person_at(9).id;
        for actor in [enemy, outsider] {
            *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId((actor).0))
                .expect("location exists") = Location::AtArea(AreaId(3));
        }
        sim.debug_start_encounter(vec![(mother, 0), (enemy, 1)])
            .unwrap();
        assert!(sim.transport_same_place(mother, child));
        assert!(!sim.transport_same_place(outsider, child));
        let source = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Carried(outsider),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([outsider]),
            )
            .unwrap();
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.debug_actor_command(outsider, Command::Care { child, source }),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }
    #[test]
    fn an_actual_birth_beside_a_carried_gestator_uses_the_existing_willing_bearers() {
        let mut sim = Simulation::transport_fixture(1, Execution::default()).unwrap();
        sim.host
            .state
            .physical
            .as_mut()
            .unwrap()
            .rules
            .fatigue_limit = 2_000_000;
        sim.advance(SimDuration(120_000)).unwrap();
        let mother = game_lifecycle::all_pregnancies(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|p| matches!(p.state, development::PregnancyState::Gestating { .. }))
            .unwrap()
            .gestator;
        quiet(&mut sim);
        let bearer = sim.host.state.ruler;
        sim.transport_command(bearer, &Action::OfferCarry { passenger: mother })
            .unwrap();
        sim.transport_command(
            mother,
            &Action::AcceptCarry {
                bearers: vec![bearer],
            },
        )
        .unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        sim.debug_move_actor(bearer, AreaId(3)).unwrap();
        sim.advance(SimDuration(60_000)).unwrap();
        let child = game_lifecycle::all_pregnancies(&sim.host.storage)
            .map(|(_, row)| row)
            .find_map(|p| {
                if let development::PregnancyState::Born(child) = p.state {
                    Some(child)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(
            *game_spatial::location(&sim.host.storage, game_ecs::StableId((child).0))
                .expect("location exists"),
            Location::Carried { carrier: bearer }
        );
        assert_eq!(
            game_transport::group(&sim.host.storage, child)
                .unwrap()
                .bearers,
            BTreeSet::from([bearer])
        );
        assert_eq!(sim.resolved_location(child), sim.resolved_location(mother));
        assert!(
            !game_transport::group(&sim.host.storage, child)
                .unwrap()
                .bearers
                .contains(&mother)
        );
        assert_eq!(
            game_lifecycle::life(&sim.host.storage, child)
                .unwrap()
                .parents[0],
            mother
        );
    }
    #[test]
    fn unknown_public_transport_and_yield_targets_are_inert() {
        let mut sim = Simulation::transport_fixture(44, Execution::default()).unwrap();
        let before = sim.canonical_snapshot().unwrap();
        for command in [
            Command::Yield {
                captor: PersonId(999),
            },
            Command::Transport {
                action: Action::OfferCarry {
                    passenger: PersonId(999),
                },
            },
        ] {
            assert_eq!(
                sim.submit(Submission {
                    caller_sequence: 1,
                    command
                }),
                Err(Error::Unavailable)
            );
            assert_eq!(sim.canonical_snapshot().unwrap(), before);
        }
    }
    #[test]
    fn exhaustion_during_an_approach_preserves_the_known_point_through_rest_and_resume() {
        let mut sim = Simulation::transport_fixture(44, Execution::default()).unwrap();
        quiet(&mut sim);
        let actor = sim.host.state.ruler;
        sim.host
            .state
            .physical
            .as_mut()
            .unwrap()
            .rules
            .fatigue_limit = 100_000;
        let target = RoadPosition {
            from: AreaId(1),
            to: AreaId(3),
            progress: SimDuration(150_000),
            duration: SimDuration(180_000),
        };
        sim.transport_approach_road(actor, target).unwrap();
        sim.advance(SimDuration(100_000)).unwrap();
        assert_eq!(
            sim.transport_position(actor).unwrap(),
            Position::Road(RoadPosition {
                progress: SimDuration(100_000),
                ..target
            })
        );
        assert_eq!(
            game_transport::paused_approach(&sim.host.storage, actor)
                .unwrap()
                .target,
            target
        );
        sim.debug_actor_command(actor, Command::Rest).unwrap();
        sim.advance(SimDuration(50_000)).unwrap();
        sim.transport_command(actor, &Action::Resume).unwrap();
        sim.advance(SimDuration(50_000)).unwrap();
        assert_eq!(sim.transport_position(actor), Some(Position::Road(target)));
        assert!(
            game_transport::approaches(&sim.host.storage)
                .next()
                .is_none()
        );
        assert_eq!(sim.host.clock().time, SimTime(200_000));
    }
    #[test]
    fn public_offer_gets_a_personal_npc_response_and_a_real_timed_lift() {
        let mut sim = Simulation::transport_fixture(51, Execution::default()).unwrap();
        quiet(&mut sim);
        let ruler = sim.host.state.ruler;
        let passenger = sim.person_at(8).id;
        sim.debug_set_relationship(
            passenger,
            ruler,
            decisions::Relationship {
                trust: 500,
                affection: 500,
            },
        )
        .unwrap();
        sim.debug_injure(passenger, 120_000).unwrap();
        sim.submit(Submission {
            caller_sequence: 1,
            command: Command::Transport {
                action: Action::OfferCarry { passenger },
            },
        })
        .unwrap();
        for _ in 0..12 {
            let _ = sim.observe();
        }
        assert!(game_transport::lifts(&sim.host.storage).next().is_none());
        sim.advance(SimDuration(60_000)).unwrap();
        assert!(game_transport::lift(&sim.host.storage, passenger).is_some());
        assert!(
            sim.observe()
                .transport
                .unwrap()
                .responses
                .iter()
                .any(|r| r.passenger == passenger
                    && r.kind == game_transport::ResponseKind::Accepted)
        );
        sim.advance(SimDuration(5_999)).unwrap();
        assert!(game_transport::groups(&sim.host.storage).next().is_none());
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(
            *game_spatial::location(&sim.host.storage, game_ecs::StableId((passenger).0))
                .expect("location exists"),
            Location::Carried { carrier: ruler }
        );
        let declining = sim.person_at(9).id;
        sim.debug_set_personality(
            declining,
            decisions::Personality::restful(),
            decisions::Aspiration::Independence,
        )
        .unwrap();
        sim.debug_set_relationship(
            declining,
            ruler,
            decisions::Relationship {
                trust: 500,
                affection: 500,
            },
        )
        .unwrap();
        sim.submit(Submission {
            caller_sequence: 2,
            command: Command::Transport {
                action: Action::OfferCarry {
                    passenger: declining,
                },
            },
        })
        .unwrap();
        sim.advance(SimDuration(54_000)).unwrap();
        assert!(!game_transport::group(&sim.host.storage, declining).is_some());
        assert!(
            sim.observe().transport.unwrap().responses.iter().any(
                |r| r.passenger == declining && r.kind == game_transport::ResponseKind::Refused
            )
        );
    }
    #[test]
    fn public_heavy_person_offer_receives_bounded_voluntary_support() {
        let mut sim = Simulation::transport_fixture(51, Execution::default()).unwrap();
        quiet(&mut sim);
        sim.host.state.transport.as_mut().unwrap().rules.adult_mass = Quantity(16_000);
        sim.host.state.transport.as_mut().unwrap().rules.max_bearers = 2;
        let ruler = sim.host.state.ruler;
        let passenger = sim.person_at(8).id;
        let helper = sim.person_at(7).id;
        sim.debug_set_relationship(
            passenger,
            ruler,
            decisions::Relationship {
                trust: 500,
                affection: 500,
            },
        )
        .unwrap();
        sim.debug_injure(passenger, 120_000).unwrap();
        let mut personality = decisions::Personality::restful();
        personality.empathy = 900;
        sim.debug_set_personality(helper, personality, decisions::Aspiration::Comfort)
            .unwrap();
        sim.debug_set_relationship(
            helper,
            passenger,
            decisions::Relationship {
                trust: 500,
                affection: 500,
            },
        )
        .unwrap();
        sim.submit(Submission {
            caller_sequence: 1,
            command: Command::Transport {
                action: Action::OfferCarry { passenger },
            },
        })
        .unwrap();
        sim.advance(SimDuration(60_000)).unwrap();
        assert!(game_transport::lifts(&sim.host.storage).next().is_none());
        assert_eq!(
            game_transport::offer_inbox(&sim.host.storage, passenger)
                .unwrap()
                .by_bearer
                .keys()
                .copied()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([ruler, helper])
        );
        sim.advance(SimDuration(60_000)).unwrap();
        assert!(game_transport::lift(&sim.host.storage, passenger).is_some());
        sim.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            game_transport::group(&sim.host.storage, passenger)
                .unwrap()
                .bearers,
            BTreeSet::from([ruler, helper])
        );
        assert!(sim.transport_body_load(ruler).0 > 0 && sim.transport_body_load(helper).0 > 0);
    }
}
