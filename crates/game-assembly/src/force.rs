//! Public bindings for Conflict-owned force components and prepared effects.
use crate::*;
use game_conflict::force as owner;
pub use owner::{Escape, Restraint, Rules, Yielded};
pub type World = owner::Runtime;

pub(crate) struct ForceBindings<'a>(pub(crate) &'a Simulation);
impl owner::Facts for ForceBindings<'_> {
    fn person(&self, actor: PersonId) -> Option<owner::PersonFacts> {
        let sim = self.0;
        let body = game_physiology::body(&sim.host.storage, actor)?;
        let activity = sim.host.storage.activity(actor)?;
        Some(owner::PersonFacts {
            alive: body.vitals.alive,
            health: body.vitals.health,
            independent: sim.independent(actor),
            in_combat: sim.in_combat(actor),
            restricted: sim.combat_restricts(actor),
            idle_or_resting: matches!(activity, Activity::Idle | Activity::Rest),
            raw_location: game_spatial::location(&sim.host.storage, game_ecs::StableId(actor.0))?
                .clone(),
            location: sim.resolved_location(actor),
        })
    }
    fn protect_person(
        &self,
        store: &game_ecs::Store,
        actor: PersonId,
        reads: &mut game_ecs::ReadSet,
    ) {
        reads.read::<game_physiology::Body>(store, game_ecs::StableId(actor.0));
        reads.read::<Location>(store, game_ecs::StableId(actor.0));
    }
    fn protect_willingness(
        &self,
        store: &game_ecs::Store,
        actor: PersonId,
        reads: &mut game_ecs::ReadSet,
    ) {
        reads.read::<game_characters::decisions::Mind<game_api::Command>>(
            store,
            game_ecs::StableId(actor.0),
        );
    }
    fn same_place(&self, actor: PersonId, target: PersonId) -> bool {
        self.0.transport_same_place(actor, target)
    }
    fn combat_available(&self) -> bool {
        self.0.host.state.combat.is_some()
    }
    fn incapacitation(&self) -> u64 {
        self.0
            .host
            .state
            .combat
            .as_ref()
            .map_or(0, |world| world.rules.incapacitation)
    }
    fn ruler(&self) -> PersonId {
        self.0.host.state.ruler
    }
    fn captor(&self, target: PersonId) -> Option<PersonId> {
        game_conflict::captivity(&self.0.host.storage, target).map(|row| row.captor)
    }
    fn same_encounter(&self, target: PersonId, actor: PersonId) -> bool {
        game_conflict::encounters(&self.0.host.storage).any(|(_, battle)| {
            battle.active
                && battle.fighters.contains_key(&target)
                && battle.fighters.contains_key(&actor)
        })
    }
    fn carried(&self, actor: PersonId) -> Vec<InventoryId> {
        self.0
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .cloned()
            .unwrap_or_default()
    }
    fn people(&self) -> impl Iterator<Item = PersonId> {
        self.0.people().map(|person| person.id)
    }
    fn willingness(&self, actor: PersonId, captor: PersonId) -> Option<owner::Willingness> {
        let mind =
            game_characters::decisions::mind::<game_api::Command>(&self.0.host.storage, actor)?;
        Some(owner::Willingness {
            aggression: mind.personality.aggression,
            risk_tolerance: mind.personality.risk_tolerance,
            trust: mind.relationships.get(&captor).map_or(0, |row| row.trust),
        })
    }
    fn positions(&self, target: PersonId, area: AreaId) -> Vec<Point> {
        let encounters = game_conflict::encounters(&self.0.host.storage).collect::<Vec<_>>();
        encounters
            .into_iter()
            .rev()
            .filter(|(_, battle)| battle.area == area)
            .filter_map(|(_, battle)| battle.fighters.get(&target).map(|fighter| fighter.position))
            .collect()
    }
}
fn command(choice: owner::Choice) -> Command {
    match choice {
        owner::Choice::Escape => Command::Escape,
        owner::Choice::WithdrawYield => Command::WithdrawYield,
        owner::Choice::Yield { captor } => Command::Yield { captor },
        owner::Choice::Restrain { target } => Command::Restrain { target },
        owner::Choice::FreeCaptive { target } => Command::FreeCaptive { target },
        owner::Choice::CarryTool { source, quantity } => Command::Carry { source, quantity },
    }
}
fn combat_choice(action: &CombatAction) -> owner::CombatChoice {
    match action {
        CombatAction::Defend => owner::CombatChoice::Defend,
        CombatAction::Wait => owner::CombatChoice::Wait,
        CombatAction::Release { target } => owner::CombatChoice::Release { target: *target },
        _ => owner::CombatChoice::Other,
    }
}
impl Simulation {
    pub fn force_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::lineage_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "force-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.host.state.force = Some(World {
            rules: Rules::default(),
            last_event: sim.host.next_event() - 1,
        });
        Ok(sim)
    }
    fn ensure_force_world(&mut self) {
        if self.host.state.force.is_none() {
            self.host.state.force = Some(World {
                rules: Rules::default(),
                last_event: self.host.next_event() - 1,
            });
        }
    }
    pub(crate) fn force_yield(&mut self, actor: PersonId, captor: PersonId) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        let runtime = self
            .host
            .state
            .force
            .clone()
            .unwrap_or_else(|| World::new(self.host.next_event() - 1));
        let plan = runtime.prepare_yield(
            &self.host.storage,
            &ForceBindings(self),
            actor,
            captor,
            self.host.clock().time,
        )?;
        self.ensure_event_capacity(1)?;
        self.ensure_force_world();
        self.apply_force_change(plan)?;
        self.host.bump_revision(1)
    }
    pub(crate) fn force_withdraw_yield(&mut self, actor: PersonId) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        let runtime = self.host.state.force.as_ref().ok_or(Error::Unavailable)?;
        let plan = runtime.prepare_withdraw(&self.host.storage, &ForceBindings(self), actor)?;
        self.ensure_event_capacity(1)?;
        self.apply_force_change(plan)?;
        self.host.bump_revision(1)
    }
    pub(crate) fn force_area(&self, actor: PersonId) -> Option<AreaId> {
        owner::actor_area(&ForceBindings(self), actor)
    }
    #[cfg(test)]
    fn force_binding(&self, actor: PersonId) -> Option<InventoryId> {
        owner::held_good(
            &self.host.storage,
            &ForceBindings(self),
            actor,
            Good::Binding,
        )
    }
    fn apply_force_change(&mut self, plan: owner::PreparedChange) -> Result<(), Error> {
        let effects = plan.commit(&mut self.host.storage)?;
        self.publish_force_effects(effects);
        Ok(())
    }
    fn publish_force_effects(&mut self, effects: Vec<owner::Effect>) {
        for event in force_events(&effects) {
            self.emit(event);
        }
    }
    fn commit_force_start(&mut self, prepared: PreparedForceStart) -> Result<(), Error> {
        let opening = self.host.transaction_opening();
        self.host
            .preflight_records(prepared.allocation_count(), prepared.event_count(), 1)?;
        let mut batch = game_ecs::Batch::new();
        let finish = prepared.stage_into(self, &mut batch)?;
        finish.validate_batch(self, &batch)?;
        let events = finish.events();
        if let Some(encounter) = finish.encounter_id() {
            let followups = self.prepare_combat_opening_followups_with_force(
                encounter,
                &batch,
                finish.allocation_count(),
                &events,
                1,
                finish.runtime(),
            )?;
            let (transaction, followups) = followups.into_transaction(self)?;
            self.host.commit_transaction(transaction)?;
            finish.finish_metadata(self);
            followups.finish(self);
        } else {
            let mut transaction = game_world::Transaction::new(opening, batch);
            transaction.ids = finish.allocation_count();
            transaction.revisions = 1;
            transaction.events = events;
            self.host.commit_transaction(transaction)?;
            finish.finish_metadata(self);
        }
        Ok(())
    }
    pub(crate) fn start_restraint(
        &mut self,
        actor: PersonId,
        target: PersonId,
    ) -> Result<(), Error> {
        let runtime = self
            .host
            .state
            .force
            .clone()
            .unwrap_or_else(|| World::new(self.host.next_event() - 1));
        let plan = runtime.prepare_restraint_start(
            &self.host.storage,
            &ForceBindings(self),
            actor,
            target,
            self.host.clock().time,
        )?;
        let prepared = self.prepare_force_start(plan, None, Some(actor), runtime)?;
        self.commit_force_start(prepared)
    }
    pub(crate) fn start_escape(&mut self, actor: PersonId) -> Result<(), Error> {
        let runtime = self
            .host
            .state
            .force
            .clone()
            .unwrap_or_else(|| World::new(self.host.next_event() - 1));
        let (plan, intervention) = runtime.prepare_escape_start(
            &self.host.storage,
            &ForceBindings(self),
            actor,
            self.host.clock().time,
        )?;
        let prepared = self.prepare_force_start(plan, intervention, Some(actor), runtime)?;
        self.commit_force_start(prepared)
    }
    pub(crate) fn preflight_force_intervention(&self) -> Result<(), Error> {
        let (ids, events) = owner::intervention_reserve();
        self.host
            .next_id()
            .checked_add(ids)
            .ok_or(Error::Overflow)?;
        if let Some(combat) = &self.host.state.combat {
            self.host
                .clock()
                .time
                .0
                .checked_add(combat.rules.round.0)
                .ok_or(Error::Overflow)?;
        }
        self.ensure_event_capacity(events)
    }
    /// Called after a validated FreeCaptive begins, before advancing its clock.
    pub(crate) fn prepare_force_release_start(
        &self,
        actor: PersonId,
        target: PersonId,
        completes: SimTime,
    ) -> Result<PreparedForceStart, Error> {
        let runtime = self
            .host
            .state
            .force
            .clone()
            .unwrap_or_else(|| World::new(self.host.next_event() - 1));
        let (plan, intervention) = runtime.prepare_release_start(
            &self.host.storage,
            &ForceBindings(self),
            actor,
            target,
            self.host.clock().time,
            completes,
        )?;
        let mut prepared = self.prepare_force_start(plan, intervention, Some(actor), runtime)?;
        if self.host.state.force.is_none() {
            prepared.initialize_after = Some(actor);
        }
        Ok(prepared)
    }
    pub(crate) fn force_passive_participant(&self, actor: PersonId) -> bool {
        owner::passive_participant(&ForceBindings(self), actor)
    }
    pub(crate) fn force_can_combat_act(&self, actor: PersonId) -> bool {
        owner::can_combat_act(&self.host.storage, &ForceBindings(self), actor)
    }
    pub(crate) fn force_action_allowed(
        &self,
        actor: PersonId,
        movement: Option<Point>,
        action: &CombatAction,
    ) -> bool {
        owner::action_allowed(
            &self.host.storage,
            &ForceBindings(self),
            actor,
            movement.is_some(),
            combat_choice(action),
        )
    }
    pub(crate) fn confinement_facts(
        &self,
        captor: PersonId,
        target: PersonId,
        surrendered_to_captor: bool,
        contact: bool,
        admitted: Option<&Location>,
    ) -> Result<game_conflict::ConfinementFacts, Error> {
        self.host.state.combat.as_ref().ok_or(Error::Unavailable)?;
        self.host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let actor = game_physiology::body(&self.host.storage, captor).ok_or(Error::Unavailable)?;
        let victim = game_physiology::body(&self.host.storage, target).ok_or(Error::Unavailable)?;
        Ok(game_conflict::ConfinementFacts {
            captor,
            target,
            captor_alive: actor.vitals.alive,
            captor_health: actor.vitals.health,
            independent: self.independent(captor),
            captor_confined: game_conflict::captivity(&self.host.storage, captor).is_some(),
            target_alive: victim.vitals.alive,
            target_health: victim.vitals.health,
            target_confined: game_conflict::captivity(&self.host.storage, target).is_some(),
            surrendered_to_captor,
            contact,
            admitted_site_contact: admitted.is_none_or(|location| {
                let key = transport::location_position(location, self.host.clock().time)
                    .map(|point| point.key());
                key.is_ok_and(|key| {
                    self.transport_position(captor)
                        .is_some_and(|point| point.key() == key)
                        && self
                            .transport_position(target)
                            .is_some_and(|point| point.key() == key)
                })
            }),
            captor_location: self.resolved_location(captor),
            target_location: self.resolved_location(target),
        })
    }
    fn restraint_context(
        &self,
        location: &Location,
        target_position: Point,
    ) -> Result<(game_conflict::ConfinementContext, combat::PreparedCombatMap), Error> {
        let plan = self.prepare_combat_location_map(location)?;
        let map = plan.map();
        let context = owner::restraint_context(map, location, target_position)?;
        Ok((context, plan))
    }
    fn apply_confinement_with_force(
        &mut self,
        plan: game_conflict::ValidatedConfinement,
        new_map: Option<combat::PreparedCombatMap>,
        ending: Option<(PersonId, owner::PreparedChange)>,
    ) -> Result<(), Error> {
        let opening = self.host.transaction_opening();
        let receipt = plan.receipt().clone();
        self.preflight_confinement_receipt(&receipt, new_map.as_ref())?;
        let map_count = u64::from(new_map.as_ref().is_some_and(|scene| scene.is_new()));
        let interrupted_target: Vec<_> = receipt
            .interrupt_target
            .then_some(receipt.target)
            .into_iter()
            .collect();
        let motion = receipt
            .transport_burden()
            .and_then(|burden| self.prepare_transport_confinement_reconciliation(burden))
            .map(|plan| {
                self.prepare_motion_admission_for_actors(
                    plan,
                    game_transport::MotionScope::Ordinary,
                    commands::ActivityOwner::Force,
                    &interrupted_target,
                )
            })
            .transpose()?;
        let interruption = if receipt.interrupt_target && motion.is_none() {
            Some(self.prepare_interruptions(&[receipt.target], commands::ActivityOwner::Combat)?)
        } else {
            None
        };
        let interruption_count = interruption
            .as_ref()
            .map_or(Ok(0), |plan| plan.event_count())?;
        self.ensure_event_capacity(
            usize::try_from(interruption_count.checked_add(2).ok_or(Error::Overflow)?)
                .map_err(|_| Error::Overflow)?,
        )?;
        let mut batch = game_ecs::Batch::new();
        let map_guard = new_map
            .map(|scene| scene.stage_into(&self.host.storage, &mut batch))
            .transpose()?;
        let confinement = plan.stage_into(&self.host.storage, &mut batch)?;
        let mut final_activities = BTreeMap::new();
        if receipt.interrupt_target {
            final_activities.insert(receipt.target, Activity::Idle);
        }
        let mut force_ending = if let Some((actor, ending)) = ending {
            final_activities.insert(actor, Activity::Idle);
            Some(ending)
        } else {
            None
        };
        if let Some(motion) = &motion {
            let actors = motion.interrupted_actors();
            force_ending = Some(match force_ending {
                Some(ending) => ending.include_interruptions(&self.host.storage, &actors),
                None => owner::prepare_interrupt_many(&self.host.storage, &actors),
            });
        }
        let motion = motion
            .map(|plan| plan.stage_into(self, &mut batch, &final_activities))
            .transpose()?;
        let ending_guard = force_ending
            .map(|ending| match &motion {
                Some(motion) => ending.stage_after_motion(
                    &self.host.storage,
                    &mut batch,
                    motion.motion_finalization(),
                ),
                None => ending.stage_into(&self.host.storage, &mut batch),
            })
            .transpose()?;
        let mut interruption = interruption
            .map(|plan| plan.stage_into(self, &mut batch, &final_activities))
            .transpose()?;
        if interruption.is_none() && motion.is_none() {
            for (actor, activity) in final_activities {
                batch.insert(game_ecs::StableId(actor.0), activity);
            }
        }
        confinement.validate(&self.host.storage, &batch)?;
        if let Some(ending) = &ending_guard {
            ending.validate_batch(&self.host.storage, &batch)?;
        }
        if let Some(finish) = &interruption {
            finish.validate_batch(self, &batch)?;
        }
        if let Some(motion) = &motion {
            motion.validate_batch(self, &batch)?;
        }
        let mut events = match &motion {
            Some(motion) => motion.interruption_events_for(&interrupted_target),
            None => interruption
                .as_ref()
                .map_or_else(Vec::new, |finish| finish.events_for(&interrupted_target)),
        };
        events.push(EventKind::Transferred {
            actor: receipt.captor,
            source: receipt.source,
            destination: receipt.binding,
            quantity: receipt.quantity,
        });
        if let Some(motion) = &motion {
            events.extend(motion.events_after_interruptions_for(&interrupted_target));
        }
        events.push(match receipt.mode {
            game_conflict::ConfinementMode::Tactical { encounter } => EventKind::Captured {
                encounter,
                actor: receipt.captor,
                target: receipt.target,
                binding: receipt.binding,
            },
            game_conflict::ConfinementMode::Timed { .. } => EventKind::Restrained {
                actor: receipt.captor,
                target: receipt.target,
                binding: receipt.binding,
            },
        });
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.ids = map_count.checked_add(1).ok_or(Error::Overflow)?;
        transaction.events = events;
        transaction.guards.extend(map_guard);
        self.host.commit_transaction(transaction)?;
        if let Some(finish) = &mut interruption {
            finish.finish_actor_metadata(self, receipt.target);
        }
        self.inventory_index
            .entry(InventoryLocation::Carried(receipt.target))
            .or_default()
            .push(receipt.binding);
        if let Some(motion) = motion {
            motion.finish_metadata(self);
        }
        Ok(())
    }
    pub(crate) fn preflight_confinement_receipt(
        &self,
        receipt: &game_conflict::ConfinementReceipt,
        new_map: Option<&combat::PreparedCombatMap>,
    ) -> Result<(), Error> {
        let scene_allocation = u64::from(new_map.is_some_and(|scene| scene.is_new()));
        self.host
            .next_id()
            .checked_add(1 + scene_allocation)
            .ok_or(Error::Overflow)?;
        if receipt.binding
            != InventoryId(
                self.host
                    .next_id()
                    .checked_add(scene_allocation)
                    .ok_or(Error::Overflow)?,
            )
        {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(3)
    }
    pub(crate) fn complete_force(&mut self, actor: PersonId) -> Result<(), Error> {
        let Some(runtime) = &self.host.state.force else {
            return Ok(());
        };
        let Some(completion) = runtime.completion(
            &self.host.storage,
            &ForceBindings(self),
            actor,
            self.host.clock().time,
        ) else {
            return Ok(());
        };
        match completion {
            owner::Completion::Escape {
                successful,
                removal,
            } => {
                if successful {
                    let opening = self.host.transaction_opening();
                    let plan = self.prepare_captive_release_with_force(actor, actor, removal)?;
                    self.host.preflight_records(0, plan.event_count()?, 0)?;
                    let mut batch = game_ecs::Batch::new();
                    let finish = plan.stage_into_with_activities(
                        self,
                        &mut batch,
                        &BTreeMap::from([(actor, Activity::Idle)]),
                    )?;
                    finish.validate_batch(self, &batch)?;
                    let mut transaction = game_world::Transaction::new(opening, batch);
                    transaction.events = finish.events();
                    self.host.commit_transaction(transaction)?;
                    finish.finish_metadata(self);
                    return Ok(());
                } else {
                    self.apply_force_change(removal)?;
                }
            }
            owner::Completion::Restrain {
                row: attempt,
                removal,
            } => {
                let target = attempt.target;
                let prepared = (|| -> Result<_, Error> {
                    let (context, scene) =
                        self.restraint_context(&attempt.location, attempt.position)?;
                    let new_map = scene.is_new();
                    let applied = InventoryId(
                        self.host
                            .next_id()
                            .checked_add(u64::from(new_map))
                            .ok_or(Error::Overflow)?,
                    );
                    let surrendered =
                        owner::surrendered(&self.host.storage, &ForceBindings(self), actor, target);
                    let facts = self.confinement_facts(
                        actor,
                        target,
                        surrendered,
                        self.transport_same_place(actor, target),
                        Some(&attempt.location),
                    )?;
                    let source = game_cargo::inventory(&self.host.storage, attempt.binding)
                        .ok_or(Error::Unavailable)?;
                    let plan = game_conflict::prepare_confinement(
                        &self.host.state.combat.as_ref().unwrap().rules,
                        facts.clone(),
                        source,
                        applied,
                        context,
                        game_conflict::ConfinementMode::Timed {
                            completes: attempt.completes,
                            admitted_location: attempt.location.clone(),
                        },
                        self.host.clock().time,
                    )?;
                    Ok((
                        plan.begin(&self.host.storage, &facts)?,
                        new_map.then_some(scene),
                    ))
                })();
                match prepared {
                    Ok((plan, scene)) => {
                        return self.apply_confinement_with_force(
                            plan,
                            scene,
                            Some((actor, removal)),
                        );
                    }
                    Err(error) => {
                        let recovery = owner::restraint_failure(removal, actor, error)?;
                        self.ensure_event_capacity(recovery.effects.len())?;
                        self.apply_force_change(recovery)?;
                    }
                }
            }
        }
        self.host.storage.set_activity(actor, Activity::Idle)?;
        Ok(())
    }
    pub(crate) fn force_guardian_rescue(
        &self,
        guardian: PersonId,
        target: PersonId,
    ) -> Option<Command> {
        let known = game_characters::decisions::stocks(&self.host.storage, guardian)?
            .known
            .values()
            .cloned();
        owner::guardian_rescue(
            &self.host.storage,
            &ForceBindings(self),
            guardian,
            target,
            known,
        )
        .map(command)
    }
    pub(crate) fn force_candidate(&self, actor: PersonId) -> Option<Command> {
        owner::escape_candidate(&self.host.storage, &ForceBindings(self), actor).map(command)
    }
    pub(crate) fn force_allocation_bound(&self, due: &[Due]) -> u64 {
        self.host.state.force.as_ref().map_or(0, |runtime| {
            runtime.allocation_bound(
                &self.host.storage,
                due.iter().filter_map(|event| {
                    if let Due::Physical(actor) = event {
                        Some(*actor)
                    } else {
                        None
                    }
                }),
            )
        })
    }
    pub(crate) fn available_force_commands(&self, actor: PersonId) -> Vec<Command> {
        owner::available_choices(&self.host.storage, &ForceBindings(self), actor)
            .into_iter()
            .map(command)
            .collect()
    }
    pub(crate) fn preflight_force(&self, due: &[Due]) -> Result<(), Error> {
        self.host
            .next_id()
            .checked_add(self.force_allocation_bound(due))
            .ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn update_force(&mut self) {
        let Some(runtime) = &self.host.state.force else {
            return;
        };
        let cursor = runtime.last_event;
        let invalidated = self
            .host
            .events()
            .iter()
            .filter(|event| event.sequence > cursor)
            .map(|event| match event.event {
                EventKind::Departed { actor, .. } => owner::ObservedEvent::Departed { actor },
                EventKind::Struck { actor, .. } => owner::ObservedEvent::Struck { actor },
                _ => owner::ObservedEvent::Other,
            });
        let plan = runtime.prepare_update(&self.host.storage, &ForceBindings(self), invalidated);
        self.apply_force_change(plan)
            .expect("current force event reactions");
        self.host.state.force.as_mut().unwrap().last_event = self.host.next_event() - 1;
    }
    pub(crate) fn force_rescue_target(&self, actor: PersonId) -> Option<PersonId> {
        owner::rescue_target(&self.host.storage, &ForceBindings(self), actor)
    }
    pub(crate) fn force_combat_action(
        &self,
        actor: PersonId,
        encounter: EncounterId,
    ) -> Option<CombatAction> {
        let world = self.host.state.combat.as_ref()?;
        let battle = game_conflict::encounter(&self.host.storage, encounter)?;
        let reachable = self.force_rescue_target(actor).is_some_and(|target| {
            battle.fighters.get(&actor).is_some_and(|mine| {
                world.reachable(&self.host.storage, encounter, mine.position, target)
            })
        });
        match owner::combat_action(&self.host.storage, &ForceBindings(self), actor, reachable)? {
            owner::CombatChoice::Defend => Some(CombatAction::Defend),
            owner::CombatChoice::Wait => Some(CombatAction::Wait),
            owner::CombatChoice::Release { target } => Some(CombatAction::Release { target }),
            owner::CombatChoice::Other => None,
        }
    }
    pub(crate) fn force_view(&self, actor: PersonId) -> ForceView {
        if self.host.state.combat.is_none() {
            return ForceView::default();
        }
        ForceView {
            confinements: owner::confinement_views(
                &self.host.storage,
                &ForceBindings(self),
                actor,
                self.host.state.physical.is_some(),
                game_conflict::captivities(&self.host.storage).map(|(_, row)| row),
            )
            .into_iter()
            .map(|row| ConfinementView {
                target: row.target,
                captor: row.captor,
                location: row.location,
                captured_position: row.captured_position,
                can_escape: row.can_escape,
                escaping: row.escaping,
            })
            .collect(),
        }
    }
}

struct SaveReferences<'a, 'b> {
    state: &'a CanonicalState<'b>,
    locations: BTreeMap<PersonId, Location>,
}
fn force_activity(activity: &Activity) -> owner::Activity {
    match *activity {
        Activity::Escape { tool, completes } => owner::Activity::Escape { tool, completes },
        Activity::Unbind { completes, .. } => owner::Activity::Unbind { completes },
        Activity::Restrain {
            target,
            binding,
            completes,
        } => owner::Activity::Restrain {
            target,
            binding,
            completes,
        },
        _ => owner::Activity::Other,
    }
}
impl owner::References for SaveReferences<'_, '_> {
    fn person(&self, actor: PersonId) -> Option<owner::ValidationPerson> {
        self.state.person(actor)?;
        Some(owner::ValidationPerson {
            alive: game_physiology::body(self.state.components, actor)?
                .vitals
                .alive,
            location: self.locations.get(&actor)?.clone(),
            activity: force_activity(self.state.components.activity(actor)?),
        })
    }
    fn area_exists(&self, area: AreaId) -> bool {
        self.state.state.areas.iter().any(|row| row.id == area)
    }
    fn confined(&self, target: PersonId) -> bool {
        game_conflict::captivity(self.state.components, target).is_some()
    }
    fn same_encounter(&self, target: PersonId, actor: PersonId) -> bool {
        game_conflict::encounters(self.state.components).any(|(_, battle)| {
            battle.active
                && battle.fighters.contains_key(&target)
                && battle.fighters.contains_key(&actor)
        })
    }
    fn due(&self, actor: PersonId, completes: SimTime) -> bool {
        self.state.state.due.get(&completes).is_some_and(|events| {
            events
                .iter()
                .any(|event| matches!(event, Due::Physical(id) if *id == actor))
        })
    }
    fn activities(&self) -> impl Iterator<Item = (PersonId, owner::Activity)> {
        self.state
            .components
            .activities()
            .map(|(actor, activity)| (actor, force_activity(activity)))
    }
}
pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(runtime) = &state.state.force else {
        owner::validate_absent(state.components)?;
        return owner::validate_absent_activities(
            state
                .components
                .activities()
                .map(|(_, activity)| force_activity(activity)),
        );
    };
    if state.state.physical.is_none() || state.state.combat.is_none() {
        return Err(Error::InvalidSave("missing force dependencies".into()));
    }
    let references = SaveReferences {
        state,
        locations: transport::resolved_locations_for(state)?,
    };
    runtime.validate(state.components, &references, state.time, state.next_event)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Simulation {
        let mut sim = Simulation::force_fixture(88, Execution::default()).unwrap();
        let people = sim.people().map(|person| person.id).collect::<Vec<_>>();
        for person in people {
            sim.debug_actor_command(person, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        sim
    }
    fn tool(sim: &Simulation) -> InventoryId {
        game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|stock| {
                stock.good == Good::Tool && stock.location == InventoryLocation::Area(AreaId(1))
            })
            .unwrap()
            .id
    }
    fn capture_by_yield(sim: &mut Simulation, captor: PersonId, target: PersonId) {
        sim.force_yield(target, captor).unwrap();
        sim.start_restraint(captor, target).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert!(game_conflict::captivity(&sim.host.storage, target).is_some());
    }
    #[test]
    fn direct_binding_requires_actual_yield_or_incapacity_and_consumes_real_binding() {
        let mut sim = fixture();
        let captor = sim.host.state.ruler;
        let target = sim.person_at(3).id;
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(sim.start_restraint(captor, target), Err(Error::Unavailable));
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        let source = sim.force_binding(captor).unwrap();
        let opening = game_cargo::inventory(&sim.host.storage, source)
            .expect("inventory exists")
            .quantity;
        capture_by_yield(&mut sim, captor, target);
        let captive = game_conflict::captivity(&sim.host.storage, target).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity
                .0,
            opening.0 - 1_000
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, captive.binding)
                .expect("inventory exists")
                .quantity
                .0,
            1_000
        );
        assert_eq!(captive.position, Point { x: 6_000, y: 8_000 });
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn yield_can_be_withdrawn_before_binding_and_departure_does_not_leave_a_capture_ticket() {
        let mut sim = fixture();
        let captor = sim.host.state.ruler;
        let target = sim.person_at(3).id;
        sim.force_yield(target, captor).unwrap();
        sim.start_restraint(captor, target).unwrap();
        sim.advance(SimDuration(3_000)).unwrap();
        sim.force_withdraw_yield(target).unwrap();
        sim.advance(SimDuration(3_000)).unwrap();
        assert!(!game_conflict::captivity(&sim.host.storage, target).is_some());
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, sim.force_binding(captor).unwrap())
                .expect("inventory exists")
                .quantity
                .0,
            1_000
        );
        sim.force_yield(target, captor).unwrap();
        sim.debug_move_actor(target, AreaId(2)).unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        assert!(!owner::yielded(&sim.host.storage, target).is_some());
    }
    #[test]
    fn an_owned_reachable_tool_can_free_a_captive_but_area_stock_cannot() {
        let mut sim = fixture();
        let captor = sim.host.state.ruler;
        let target = sim.person_at(1).id;
        sim.carry_good(target, tool(&sim), Quantity(1_000)).unwrap();
        sim.advance(MINUTE).unwrap();
        capture_by_yield(&mut sim, captor, target);
        let binding = game_conflict::captivity(&sim.host.storage, target)
            .unwrap()
            .binding;
        sim.start_escape(target).unwrap();
        sim.advance(SimDuration(5_999)).unwrap();
        assert!(game_conflict::captivity(&sim.host.storage, target).is_some());
        sim.advance(SimDuration(1)).unwrap();
        assert!(!game_conflict::captivity(&sim.host.storage, target).is_some());
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, binding)
                .expect("inventory exists")
                .quantity
                .0,
            1_000
        );
        let mut no_tool = fixture();
        let captor = no_tool.host.state.ruler;
        let target = no_tool.person_at(1).id;
        capture_by_yield(&mut no_tool, captor, target);
        assert_eq!(no_tool.start_escape(target), Err(Error::Unavailable));
    }
    #[test]
    fn guardian_regeneration_in_place_does_not_release_real_bindings() {
        let mut sim = fixture();
        let captor = sim.host.state.ruler;
        let guardian = sim.person_at(1).id;
        let max = sim.host.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(guardian, max).unwrap();
        sim.start_restraint(captor, guardian).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        let confinement = game_conflict::captivity(&sim.host.storage, guardian)
            .unwrap()
            .clone();
        sim.advance(SimDuration(180_000)).unwrap();
        assert_eq!(
            game_physiology::body(&sim.host.storage, guardian)
                .unwrap()
                .vitals
                .health,
            max
        );
        assert_eq!(
            game_conflict::captivity(&sim.host.storage, guardian).unwrap(),
            &confinement
        );
        assert_eq!(sim.start_escape(guardian), Err(Error::Unavailable));
    }
    #[test]
    fn earlier_real_combat_injury_can_prevent_the_captives_escape() {
        let mut sim = fixture();
        let captive = sim.host.state.ruler;
        let captor = sim.person_at(3).id;
        sim.carry_good(captive, tool(&sim), Quantity(1_000))
            .unwrap();
        sim.advance(MINUTE).unwrap();
        capture_by_yield(&mut sim, captor, captive);
        let mut hostile = decisions::Personality::industrious();
        hostile.aggression = 900;
        hostile.risk_tolerance = 900;
        sim.debug_set_personality(captor, hostile, decisions::Aspiration::Comfort)
            .unwrap();
        game_conflict::profile_mut(&mut sim.host.storage, captor)
            .unwrap()
            .initiative = 300;
        sim.host.state.combat.as_mut().unwrap().rules.damage = 170_000;
        sim.start_escape(captive).unwrap();
        assert!(game_conflict::captivity(&sim.host.storage, captive).is_some());
        assert!(
            game_physiology::body(&sim.host.storage, captive)
                .unwrap()
                .vitals
                .health
                <= sim.host.state.combat.as_ref().unwrap().rules.incapacitation
        );
        sim.advance(SimDuration(6_000)).unwrap();
        assert!(game_conflict::captivity(&sim.host.storage, captive).is_some());
    }
    #[test]
    fn escape_intervals_are_equivalent_after_partial_time_save_and_worker_changes() {
        let mut a = fixture();
        let captor = a.host.state.ruler;
        let target = a.person_at(1).id;
        a.carry_good(target, tool(&a), Quantity(1_000)).unwrap();
        a.advance(MINUTE).unwrap();
        capture_by_yield(&mut a, captor, target);
        a.start_escape(target).unwrap();
        let mut b = Simulation::load(
            &a.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        a.advance(SimDuration(6_000)).unwrap();
        b.advance(SimDuration(1_733)).unwrap();
        for _ in 0..30 {
            b.observe();
        }
        b = Simulation::load(
            &b.save().unwrap(),
            Execution {
                workers: 2,
                reverse_jobs: true,
            },
        )
        .unwrap();
        b.advance(SimDuration(4_267)).unwrap();
        assert_eq!(a.canonical_state(), b.canonical_state());
    }
    #[test]
    fn closing_health_can_prevent_binding_or_an_escape_at_its_deadline() {
        let mut sim = fixture();
        let captor = sim.host.state.ruler;
        let guardian = sim.person_at(1).id;
        let max = sim.host.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(guardian, max - 44_000).unwrap();
        sim.start_restraint(captor, guardian).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            game_physiology::body(&sim.host.storage, guardian)
                .unwrap()
                .vitals
                .health,
            50_000
        );
        assert!(!game_conflict::captivity(&sim.host.storage, guardian).is_some());
        let mut weak = fixture();
        let captor = weak.host.state.ruler;
        let target = weak.person_at(3).id;
        weak.carry_good(target, tool(&weak), Quantity(1_000))
            .unwrap();
        weak.advance(MINUTE).unwrap();
        capture_by_yield(&mut weak, captor, target);
        let body = game_physiology::body_mut(&mut weak.host.storage, target).unwrap();
        body.vitals.health = 50_000;
        body.vitals.hunger = 700_000;
        weak.start_escape(target).unwrap();
        weak.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            game_physiology::body(&weak.host.storage, target)
                .unwrap()
                .vitals
                .health,
            44_000
        );
        assert!(game_conflict::captivity(&weak.host.storage, target).is_some());
    }
    #[test]
    fn loyal_guardian_fetches_a_tool_and_rescues_through_real_initiative_resistance() {
        let mut sim = fixture();
        let captive = sim.host.state.ruler;
        let captor = sim.person_at(3).id;
        capture_by_yield(&mut sim, captor, captive);
        let mut hostile = decisions::Personality::industrious();
        hostile.aggression = 900;
        hostile.risk_tolerance = 900;
        sim.debug_set_personality(captor, hostile, decisions::Aspiration::Comfort)
            .unwrap();
        sim.advance(SimDuration(114_000)).unwrap();
        assert!(sim.in_combat(captive));
        let encounter = sim.observe().combat.unwrap().encounter;
        sim.submit(Submission {
            caller_sequence: 10,
            command: Command::CombatTurn {
                encounter,
                movement: None,
                action: CombatAction::Defend,
            },
        })
        .unwrap();
        assert!(!game_conflict::captivity(&sim.host.storage, captive).is_some());
        assert!(
            sim.host
                .events()
                .iter()
                .any(|event| matches!(event.event,EventKind::Struck {actor,..} if actor==captor))
        );
        assert!(sim.host.events().iter().any(|event|matches!(event.event,EventKind::Released {actor,target} if actor!=captive&&target==captive)));
    }
}

#[cfg(test)]
mod transport_force_tests {
    use super::*;

    fn carried_captive(npc_captor: bool) -> (Simulation, PersonId, PersonId) {
        let mut sim = Simulation::transport_fixture(91, Execution::default()).unwrap();
        let people: Vec<_> = sim.people().map(|person| person.id).collect();
        for person in people {
            sim.debug_actor_command(person, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let ruler = sim.host.state.ruler;
        let companion = sim.person_at(3).id;
        let (captor, captive) = if npc_captor {
            (companion, ruler)
        } else {
            (ruler, companion)
        };
        sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Carried(captive),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([captive]),
        )
        .unwrap();
        sim.force_yield(captive, captor).unwrap();
        sim.start_restraint(captor, captive).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        sim.transport_command(captor, &TransportAction::OfferCarry { passenger: captive })
            .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            game_transport::group(&sim.host.storage, captive)
                .unwrap()
                .authority,
            game_transport::Authority::Captive
        );
        sim.debug_move_actor(captor, AreaId(2)).unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        (sim, captor, captive)
    }

    #[test]
    fn a_carried_captive_keeps_actual_bindings_and_can_escape_with_a_reachable_tool() {
        let (mut sim, captor, captive) = carried_captive(false);
        let binding = game_conflict::captivity(&sim.host.storage, captive)
            .unwrap()
            .binding;
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, binding)
                .expect("inventory exists")
                .location,
            InventoryLocation::Carried(captive)
        );
        assert!(matches!(
            sim.force_view(captor).confinements[0].location,
            Location::InTransit { .. }
        ));
        let mut restored = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        for world in [&mut sim, &mut restored] {
            world.start_escape(captive).unwrap();
            world.advance(SimDuration(6_000)).unwrap();
            assert!(!game_conflict::captivity(&world.host.storage, captive).is_some());
            assert!(game_transport::group(&world.host.storage, captive).is_none());
            assert!(matches!(
                world.resolved_location(captive),
                Location::StoppedOnRoad {
                    progress: SimDuration(36_000),
                    ..
                }
            ));
            assert_eq!(
                game_cargo::inventory(&world.host.storage, binding)
                    .expect("inventory exists")
                    .quantity,
                Quantity(1_000)
            );
            Simulation::load(&world.save().unwrap(), Execution::default()).unwrap();
        }
        assert_eq!(sim.canonical_state(), restored.canonical_state());
    }

    #[test]
    fn a_willing_bearer_intervenes_in_a_real_roadside_initiative_encounter() {
        let (mut sim, captor, captive) = carried_captive(true);
        {
            let personality = &mut game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                captor,
            )
            .unwrap()
            .personality;
            personality.risk_tolerance = 1_000;
            personality.aggression = 1_000;
        }
        sim.start_escape(captive).unwrap();
        assert!(owner::escape(&sim.host.storage, captive).unwrap().contested);
        let battle = game_conflict::encounters(&sim.host.storage)
            .map(|(_, battle)| battle)
            .find(|battle| battle.active)
            .unwrap();
        assert!(battle.fighters.contains_key(&captor));
        assert!(battle.fighters.contains_key(&captive));
        assert_eq!(
            battle.location,
            Location::StoppedOnRoad {
                from: AreaId(1),
                to: AreaId(2),
                progress: SimDuration(30_000),
                duration: SimDuration(120_000)
            }
        );
        assert!(game_conflict::captivity(&sim.host.storage, captive).is_some());
        assert!(matches!(
            sim.resolved_location(captor),
            Location::StoppedOnRoad { .. }
        ));
        let restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
    }
}

pub(crate) struct PreparedCaptiveRelease {
    actor: PersonId,
    target: PersonId,
    force: owner::PreparedChange,
    motion: Option<crate::transport_motion::PreparedMotionAdmission>,
}
pub(crate) struct CaptiveReleaseFinalization {
    actor: PersonId,
    target: PersonId,
    force: owner::Receipt,
    motion: Option<crate::transport_motion::StagedMotionAdmission>,
    activities: Option<game_ecs::ReadGuard>,
}
impl PreparedCaptiveRelease {
    pub(crate) fn event_count(&self) -> Result<u64, Error> {
        self.motion
            .as_ref()
            .map_or(0, |plan| plan.event_count())
            .checked_add(1)
            .ok_or(Error::Overflow)?
            .checked_add(u64::try_from(self.force.effects.len()).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)
    }
    pub(crate) fn stage_into(
        self,
        sim: &Simulation,
        batch: &mut game_ecs::Batch,
    ) -> Result<CaptiveReleaseFinalization, Error> {
        self.stage_into_with_activities(sim, batch, &BTreeMap::new())
    }
    pub(crate) fn stage_into_with_activities(
        self,
        sim: &Simulation,
        batch: &mut game_ecs::Batch,
        activities: &BTreeMap<PersonId, Activity>,
    ) -> Result<CaptiveReleaseFinalization, Error> {
        let motion = self
            .motion
            .map(|plan| plan.stage_into(sim, batch, activities))
            .transpose()?;
        let activity_guard = if motion.is_none() && !activities.is_empty() {
            let mut reads = game_ecs::ReadSet::new();
            let mut admitted = game_ecs::ReadFootprint::new();
            for (actor, activity) in activities {
                reads.read::<Activity>(&sim.host.storage, game_ecs::StableId(actor.0));
                batch.insert(game_ecs::StableId(actor.0), activity.clone());
                admitted.protect::<Activity>(game_ecs::StableId(actor.0));
            }
            Some(reads.bind(&sim.host.storage, batch, &admitted)?)
        } else {
            None
        };
        let force = if let Some(motion) = &motion {
            self.force
                .stage_after_motion(&sim.host.storage, batch, motion.motion_finalization())?
        } else {
            self.force.stage_into(&sim.host.storage, batch)?
        };
        Ok(CaptiveReleaseFinalization {
            actor: self.actor,
            target: self.target,
            force,
            motion,
            activities: activity_guard,
        })
    }
}
impl CaptiveReleaseFinalization {
    pub(crate) fn force_receipt(&self) -> &owner::Receipt {
        &self.force
    }
    pub(crate) fn motion_finalization(&self) -> Option<&game_transport::MotionFinalization> {
        self.motion
            .as_ref()
            .map(|motion| motion.motion_finalization())
    }
    pub(crate) fn validate_batch(
        &self,
        sim: &Simulation,
        batch: &game_ecs::Batch,
    ) -> Result<(), Error> {
        self.force.validate_batch(&sim.host.storage, batch)?;
        if let Some(guard) = &self.activities {
            guard.validate(&sim.host.storage, batch)?;
        }
        if let Some(motion) = &self.motion {
            motion.validate_batch(sim, batch)?;
        }
        Ok(())
    }
    pub(crate) fn events(&self) -> Vec<EventKind> {
        let mut events = self
            .motion
            .as_ref()
            .map_or_else(Vec::new, |motion| motion.events());
        events.extend(force_events(&self.force.effects));
        events.push(EventKind::Released {
            actor: self.actor,
            target: self.target,
        });
        events
    }
    pub(crate) fn finish_metadata(self, sim: &mut Simulation) {
        if let Some(motion) = self.motion {
            motion.finish_metadata(sim);
        }
    }
}
impl Simulation {
    pub(crate) fn prepare_captive_release(
        &self,
        actor: PersonId,
        target: PersonId,
    ) -> Result<PreparedCaptiveRelease, Error> {
        let force = owner::prepare_release_cleanup(&self.host.storage, target)?;
        self.prepare_captive_release_with_force(actor, target, force)
    }
    fn prepare_captive_release_with_force(
        &self,
        actor: PersonId,
        target: PersonId,
        force: owner::PreparedChange,
    ) -> Result<PreparedCaptiveRelease, Error> {
        let force = force.include_release_cleanup(&self.host.storage, target)?;
        let motion = if let Some(runtime) = &self.host.state.transport {
            // Transport owns whether this confinement has a carrying edge and
            // validates the receiving release operation before any cleanup.
            if game_transport::group(&self.host.storage, target).is_some() {
                Some(self.prepare_motion_admission_preserving(
                    crate::transport_motion::MotionPlan::Direct(
                        runtime.prepare_confinement_release(
                            &self.host.storage,
                            target,
                            self.host.clock().time,
                        )?,
                    ),
                    game_transport::MotionScope::Ordinary,
                    commands::ActivityOwner::Force,
                )?)
            } else {
                None
            }
        } else {
            None
        };
        Ok(PreparedCaptiveRelease {
            actor,
            target,
            force,
            motion,
        })
    }
}

struct PreparedForceEncounter {
    id: EncounterId,
    area: AreaId,
    map: combat::PreparedCombatMap,
    start: game_conflict::PreparedStart,
    motion: crate::transport_motion::PreparedMotionAdmission,
}
pub(crate) struct PreparedForceStart {
    force: owner::PreparedChange,
    runtime: World,
    initialize_after: Option<PersonId>,
    scheduled: Option<(PersonId, SimTime)>,
    interrupted: Option<interruptions::PreparedInterruptions>,
    actors: Vec<PersonId>,
    activities: BTreeMap<PersonId, Activity>,
    encounter: Option<PreparedForceEncounter>,
    allocations: u64,
    event_count: u64,
}
pub(crate) struct ForceStartFinalization {
    force: owner::Receipt,
    runtime: World,
    scheduled: Option<(PersonId, SimTime)>,
    interrupted: Option<interruptions::InterruptionFinalization>,
    actors: Vec<PersonId>,
    motion: Option<crate::transport_motion::StagedMotionAdmission>,
    encounter: Option<(EncounterId, AreaId)>,
    guards: Vec<game_ecs::ReadGuard>,
    allocations: u64,
}
impl Simulation {
    fn prepare_force_start(
        &self,
        force: owner::PreparedChange,
        intervention: Option<owner::Intervention>,
        interrupt_actor: Option<PersonId>,
        runtime: World,
    ) -> Result<PreparedForceStart, Error> {
        let (reserve_ids, reserve_events) = force.start_reserve();
        self.host
            .next_id()
            .checked_add(reserve_ids)
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(reserve_events)?;
        let scheduled = force.scheduled_work();
        let mut activities = BTreeMap::new();
        if let Some(work) = force.work() {
            let (actor, activity) = match work {
                owner::Work::Restrain { actor, row } => (
                    *actor,
                    Activity::Restrain {
                        target: row.target,
                        binding: row.binding,
                        completes: row.completes,
                    },
                ),
                owner::Work::Escape { actor, row } => (
                    *actor,
                    Activity::Escape {
                        tool: row.tool.ok_or(Error::Unavailable)?,
                        completes: row.completes,
                    },
                ),
            };
            activities.insert(actor, activity);
        }
        let mut actors = interrupt_actor.into_iter().collect::<Vec<_>>();
        let encounter = if let Some(intervention) = intervention {
            actors = intervention
                .participants
                .iter()
                .map(|(actor, _)| *actor)
                .collect();
            let transport = self
                .host
                .state
                .transport
                .as_ref()
                .cloned()
                .unwrap_or_else(|| self.transport_default_runtime());
            let stops = transport.prepare_force_stops(
                &self.host.storage,
                &actors,
                self.host.clock().time,
            )?;
            let motion = self.prepare_motion_admission_for_actors(
                crate::transport_motion::MotionPlan::Direct(stops),
                game_transport::MotionScope::Ordinary,
                commands::ActivityOwner::Force,
                &actors,
            )?;
            let opening = self.prepare_encounter_opening_after_motion(
                &intervention.participants,
                motion.projection(),
            )?;
            let area = opening.area();
            let map =
                self.prepare_combat_location_map_at(opening.location(), self.host.next_id())?;
            let id = EncounterId(
                self.host
                    .next_id()
                    .checked_add(u64::from(map.is_new()))
                    .ok_or(Error::Overflow)?,
            );
            let fatigue = actors
                .iter()
                .map(|actor| {
                    Ok((
                        *actor,
                        game_physiology::body(&self.host.storage, *actor)
                            .ok_or(Error::Unavailable)?
                            .vitals
                            .fatigue,
                    ))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let start = self
                .host
                .state
                .combat
                .as_ref()
                .ok_or(Error::Unavailable)?
                .prepare_start(
                    &self.host.storage,
                    id,
                    opening,
                    map.map(),
                    &intervention.participants,
                    &fatigue,
                    self.host.clock().time,
                )?;
            activities = actors
                .iter()
                .map(|actor| (*actor, Activity::Combat { encounter: id }))
                .collect();
            Some(PreparedForceEncounter {
                id,
                area,
                map,
                start,
                motion,
            })
        } else {
            None
        };
        let interrupted = if actors.is_empty() || encounter.is_some() {
            None
        } else {
            Some(self.prepare_interruptions(&actors, commands::ActivityOwner::Force)?)
        };
        let allocations = encounter
            .as_ref()
            .map_or(0, |plan| u64::from(plan.map.is_new()) + 1);
        let event_count = (force.effects.len() as u64)
            .checked_add(
                interrupted
                    .as_ref()
                    .map_or(Ok(0), |plan| plan.event_count())?,
            )
            .ok_or(Error::Overflow)?
            .checked_add(
                encounter
                    .as_ref()
                    .map_or(0, |plan| plan.motion.event_count() + 1),
            )
            .ok_or(Error::Overflow)?;
        Ok(PreparedForceStart {
            force,
            runtime,
            initialize_after: None,
            scheduled,
            interrupted,
            actors,
            activities,
            encounter,
            allocations,
            event_count,
        })
    }
}
impl PreparedForceStart {
    pub(crate) fn event_count(&self) -> u64 {
        self.event_count
    }
    pub(crate) fn planned_activities(&self) -> &BTreeMap<PersonId, Activity> {
        &self.activities
    }
    pub(crate) fn allocation_count(&self) -> u64 {
        self.allocations
    }
    pub(crate) fn stage_into(
        self,
        sim: &Simulation,
        batch: &mut game_ecs::Batch,
    ) -> Result<ForceStartFinalization, Error> {
        self.stage_into_with_activities(sim, batch, &BTreeMap::new())
    }
    pub(crate) fn stage_into_with_activities(
        mut self,
        sim: &Simulation,
        batch: &mut game_ecs::Batch,
        fallback: &BTreeMap<PersonId, Activity>,
    ) -> Result<ForceStartFinalization, Error> {
        let mut final_activities = fallback.clone();
        final_activities.extend(self.activities);
        self.activities = final_activities;
        let mut guards = Vec::new();
        let (motion, encounter) = if let Some(plan) = self.encounter {
            let motion = plan.motion.stage_into(sim, batch, &self.activities)?;
            guards.push(plan.map.stage_into(&sim.host.storage, batch)?);
            guards.push(plan.start.stage_after_motion(
                sim.host.state.combat.as_ref().ok_or(Error::Unavailable)?,
                &sim.host.storage,
                batch,
                motion.motion_finalization(),
            )?);
            (Some(motion), Some((plan.id, plan.area)))
        } else {
            (None, None)
        };
        let force = if let Some(motion) = &motion {
            self.force
                .stage_after_motion(&sim.host.storage, batch, motion.motion_finalization())?
        } else {
            self.force.stage_into(&sim.host.storage, batch)?
        };
        let interrupted = self
            .interrupted
            .map(|plan| plan.stage_into(sim, batch, &self.activities))
            .transpose()?;
        if let Some(actor) = self.initialize_after {
            let prefix = if let Some(motion) = &motion {
                motion.interruption_events_for(&[actor])
            } else {
                interrupted
                    .as_ref()
                    .map_or_else(Vec::new, |plan| plan.events_for(&[actor]))
            };
            self.runtime.last_event = self
                .runtime
                .last_event
                .checked_add(u64::try_from(prefix.len()).map_err(|_| Error::Overflow)?)
                .ok_or(Error::Overflow)?;
        }
        Ok(ForceStartFinalization {
            force,
            runtime: self.runtime,
            scheduled: self.scheduled,
            interrupted,
            actors: self.actors,
            motion,
            encounter,
            guards,
            allocations: self.allocations,
        })
    }
}
impl ForceStartFinalization {
    pub(crate) fn validate_batch(
        &self,
        sim: &Simulation,
        batch: &game_ecs::Batch,
    ) -> Result<(), Error> {
        self.force.validate_batch(&sim.host.storage, batch)?;
        for guard in &self.guards {
            guard.validate(&sim.host.storage, batch)?;
        }
        if let Some(motion) = &self.motion {
            motion.validate_batch(sim, batch)?;
        }
        if let Some(interrupted) = &self.interrupted {
            interrupted.validate_batch(sim, batch)?;
        }
        Ok(())
    }
    pub(crate) fn encounter_id(&self) -> Option<EncounterId> {
        self.encounter.map(|(id, _)| id)
    }
    pub(crate) fn allocation_count(&self) -> u64 {
        self.allocations
    }
    pub(crate) fn runtime(&self) -> &World {
        &self.runtime
    }
    pub(crate) fn events(&self) -> Vec<EventKind> {
        let mut events = self
            .motion
            .as_ref()
            .map_or_else(Vec::new, |motion| motion.events());
        if let Some(interrupted) = &self.interrupted {
            events.extend(interrupted.events_for(&self.actors));
        }
        events.extend(force_events(&self.force.effects));
        if let Some((id, area)) = self.encounter {
            events.push(EventKind::EncounterStarted {
                encounter: id,
                area,
            });
        }
        events
    }
    pub(crate) fn finish_metadata(mut self, sim: &mut Simulation) {
        if sim.host.state.force.is_none() {
            sim.host.state.force = Some(self.runtime);
        }
        if let Some(motion) = self.motion {
            motion.finish_metadata(sim);
        }
        if let Some(interrupted) = &mut self.interrupted {
            for actor in self.actors {
                interrupted.finish_actor_metadata(sim, actor);
            }
        }
        if let Some((actor, completes)) = self.scheduled {
            sim.host
                .state
                .due
                .entry(completes)
                .or_default()
                .push(Due::Physical(actor));
        }
    }
}

fn force_events(effects: &[owner::Effect]) -> Vec<EventKind> {
    effects
        .iter()
        .map(|effect| match *effect {
            owner::Effect::Yielded { actor, captor } => EventKind::Yielded { actor, captor },
            owner::Effect::YieldWithdrawn { actor } => EventKind::YieldWithdrawn { actor },
            owner::Effect::EscapeAttempted { actor } => EventKind::EscapeAttempted { actor },
            owner::Effect::EscapeInterrupted { actor } => EventKind::EscapeInterrupted { actor },
            owner::Effect::WorkInterrupted { actor } => EventKind::Interrupted { actor },
        })
        .collect()
}
