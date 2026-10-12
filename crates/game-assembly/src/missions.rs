//! Expedition commitments use real people, cargo, personal maps, and tactical bodies.
use crate::*;

pub use game_missions::{
    Companion, ConfinementObservation, Conversation, Mission, PendingTalk, Recruitment, Runtime,
};

impl Simulation {
    pub(crate) fn preflight_mission_target_report(
        &self,
        proposal: &MissionProposal,
        speaker: PersonId,
        target: PersonId,
        area: AreaId,
        observed_at: SimTime,
    ) -> Result<(), Error> {
        self.host
            .state
            .missions
            .as_ref()
            .ok_or(Error::Unavailable)?
            .validate_target_report(
                proposal,
                target,
                observed_at,
                &game_missions::TargetReportFacts {
                    speaker_exists: self.person_index.contains_key(&speaker),
                    target_exists: self.person_index.contains_key(&target),
                    area_exists: self.host.state.areas.iter().any(|a| a.id == area),
                    now: self.host.clock().time,
                },
            )
    }
    pub(crate) fn seed_mission_target_report(
        &mut self,
        mission: MissionId,
        speaker: PersonId,
        target: PersonId,
        area: AreaId,
        observed_at: SimTime,
    ) -> Result<(), Error> {
        let runtime = self
            .host
            .state
            .missions
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let prepared = runtime.prepare_target_report(
            &self.host.storage,
            mission,
            game_missions::TargetReport {
                speaker,
                target,
                area,
                observed_at,
            },
            &game_missions::TargetReportFacts {
                speaker_exists: self.person_index.contains_key(&speaker),
                target_exists: self.person_index.contains_key(&target),
                area_exists: self.host.state.areas.iter().any(|a| a.id == area),
                now: self.host.clock().time,
            },
        )?;
        prepared.commit(
            self.host.state.missions.as_mut().unwrap(),
            &mut self.host.storage,
        )
    }
    pub fn mission_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::combat_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "mission-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let custodians = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .map(|stock| {
                (
                    stock.id,
                    match stock.location {
                        InventoryLocation::Carried(person) => person,
                        InventoryLocation::Area(_) => sim.host.state.ruler,
                    },
                )
            })
            .collect();
        let __custody: BTreeMap<InventoryId, PersonId> = custodians;
        for (id, owner) in __custody {
            game_property::declare_custody(&mut sim.host.storage, id, owner)?;
        }
        sim.host.state.agreements = Some(game_agreements::Runtime {
            last_event: sim.host.next_event() - 1,
            last_observation_event: sim.host.next_event() - 1,
        });
        sim.host.state.missions = Some(Runtime {
            last_event: sim.host.next_event() - 1,
            last_review: sim.host.clock().time,
        });
        Ok(sim)
    }
    fn mission_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        let person = *self.person_index.get(&actor).ok_or(Error::Unavailable)?;
        self.host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let body = game_physiology::body(&self.host.storage, actor).ok_or(Error::Unavailable)?;
        let availability = game_missions::ActorAvailability {
            alive: body.vitals.alive,
            health: body.vitals.health,
            independent: self.independent(actor),
            captive: self
                .host
                .state
                .combat
                .as_ref()
                .is_some_and(|w| w.captivity.contains_key(&actor)),
            incapacitation: self
                .host
                .state
                .combat
                .as_ref()
                .map(|w| w.rules.incapacitation),
        };
        availability
            .area(
                game_spatial::location(
                    &self.host.storage,
                    game_ecs::StableId(self.host.state.people[person].id.0),
                )
                .ok_or(Error::Unavailable)?,
            )
            .ok_or(Error::Unavailable)
    }
    fn mission_assets(
        &self,
        objective: Option<&MissionObjective>,
        terms: &[AgreementTerm],
        supplies: &[MissionSupply],
    ) -> game_missions::MissionAssets {
        let mut facts = game_missions::MissionAssets {
            terms: self.economic_terms_context(terms),
            ..Default::default()
        };
        let mut areas = BTreeSet::new();
        if let Some(objective) = objective {
            match objective {
                MissionObjective::Visit { destination } => {
                    areas.insert(*destination);
                }
                MissionObjective::Rescue { person, return_to } => {
                    areas.insert(*return_to);
                    if self.person_index.contains_key(person) {
                        facts.people.insert(*person);
                    }
                }
                MissionObjective::Defeat { person } | MissionObjective::Confine { person, .. } => {
                    if self.person_index.contains_key(person) {
                        facts.people.insert(*person);
                    }
                }
                MissionObjective::Patrol { areas: route, .. } => {
                    areas.extend(route.iter().copied());
                }
                MissionObjective::Deliver {
                    source,
                    destination,
                    ..
                } => {
                    for id in [source, destination] {
                        if let Some(stock) = game_cargo::inventory(&self.host.storage, *(id)) {
                            facts.terms.inventories.insert(
                                *id,
                                game_agreements::InventoryTerms {
                                    good: stock.good,
                                    capacity: stock.capacity,
                                },
                            );
                        }
                    }
                }
            }
        }
        facts.areas = areas
            .into_iter()
            .filter(|id| self.host.state.areas.iter().any(|a| a.id == *id))
            .collect();
        for supply in supplies {
            if let Some(stock) = game_cargo::inventory(&self.host.storage, supply.source) {
                facts.terms.inventories.insert(
                    supply.source,
                    game_agreements::InventoryTerms {
                        good: stock.good,
                        capacity: stock.capacity,
                    },
                );
            }
            facts.carrying_capacity.insert(
                supply.carrier,
                self.host
                    .state
                    .physical
                    .as_ref()
                    .unwrap()
                    .rules
                    .carrying_capacity,
            );
        }
        facts
    }
    pub(crate) fn mission_objective_valid(&self, objective: &MissionObjective) -> bool {
        self.mission_assets(Some(objective), &[], &[])
            .objective_valid(objective, self.host.clock().time)
    }
    /// Supplies focused body/contact/asset facts; the service owner decides
    /// authority, competing commitments, consent and compensation transitions.
    fn mission_command_facts(
        &self,
        actor: PersonId,
        area: AreaId,
        action: &MissionAction,
    ) -> Result<game_missions::CommandFacts, Error> {
        self.host
            .state
            .missions
            .as_ref()
            .ok_or(Error::Unavailable)?
            .command_facts(
                &self.host.storage,
                actor,
                area,
                action,
                self.host.clock().time,
                &MissionPresence { simulation: self },
            )
    }
    pub(crate) fn mission_command(
        &mut self,
        actor: PersonId,
        action: &MissionAction,
    ) -> Result<(), Error> {
        let area = self.mission_area(actor)?;
        self.host
            .state
            .missions
            .as_ref()
            .ok_or(Error::Unavailable)?
            .validate_action_presence(actor, &MissionPresence { simulation: self })?;
        let facts = self.mission_command_facts(actor, area, action)?;
        let (ids, next) = self.host.state.missions.as_ref().unwrap().command_ids(
            &self.host.storage,
            action,
            self.host.next_id(),
        )?;
        let prepared = self.host.state.missions.as_ref().unwrap().prepare_command(
            &self.host.storage,
            actor,
            action,
            &facts,
            &ids,
        )?;
        prepared.check(
            self.host.state.missions.as_ref().unwrap(),
            &self.host.storage,
        )?;
        if let Some((listener, conversation)) = prepared.conversation.clone() {
            return self.start_mission_talk(actor, listener, conversation);
        }
        let backing = prepared
            .consent
            .as_ref()
            .map(|consent| self.prepare_service_agreement(consent))
            .transpose()?;
        if let Some((plan, after)) = &backing {
            plan.check(&game_agreements::Ledger::new(&self.host.storage))?;
            after
                .checked_sub(self.host.next_id())
                .ok_or(Error::Overflow)?;
        }
        let agreement_events = backing.as_ref().map_or(0, |(p, _)| p.events.len());
        self.ensure_event_capacity(
            prepared
                .events
                .len()
                .checked_add(agreement_events)
                .ok_or(Error::Overflow)?,
        )?;
        let events = prepared.events.clone();
        let notices = prepared.notices.clone();
        let created = prepared.created_missions().collect::<Vec<_>>();
        let mut batch = game_ecs::Batch::new();
        if let Some((plan, after)) = &backing {
            self.stage_prepared_agreement(plan, *after, &mut batch)?;
        }
        let finish = prepared.stage_into(
            self.host.state.missions.as_ref().unwrap(),
            &self.host.storage,
            &mut batch,
        )?;
        self.host.storage.apply_batch(batch)?;
        finish.finish(self.host.state.missions.as_mut().unwrap());
        if let Some((plan, after)) = backing {
            self.publish_prepared_agreement(actor, plan, after)?;
        } else {
            self.host.allocate_ids(
                next.checked_sub(self.host.next_id())
                    .ok_or(Error::Overflow)?,
            )?;
        }
        self.mission_membership
            .register_created(&self.host.storage, created);
        self.publish_mission_effects(events, notices);
        Ok(())
    }
    fn publish_mission_effects(
        &mut self,
        events: Vec<game_missions::MissionEvent>,
        notices: BTreeSet<MissionId>,
    ) {
        use game_missions::MissionEvent;
        for event in events {
            self.emit(match event {
                MissionEvent::Reported {
                    mission,
                    speaker,
                    recipient,
                } => EventKind::MissionReported {
                    mission,
                    speaker,
                    recipient,
                },
                MissionEvent::Proposed { mission, sponsor } => {
                    EventKind::MissionProposed { mission, sponsor }
                }
                MissionEvent::RecruitmentChanged {
                    mission,
                    person,
                    status,
                } => EventKind::MissionRecruitmentChanged {
                    mission,
                    person,
                    status,
                },
                MissionEvent::Departed { mission } => EventKind::MissionDeparted { mission },
                MissionEvent::ObjectiveChanged { mission, leader } => {
                    EventKind::MissionObjectiveChanged { mission, leader }
                }
                MissionEvent::Finished { mission, status } => {
                    EventKind::MissionFinished { mission, status }
                }
            });
        }
        for mission in notices {
            self.publish_mission(mission);
        }
    }

    fn commit_mission_plan(
        &mut self,
        prepared: game_missions::PreparedMission,
    ) -> Result<(), Error> {
        self.ensure_event_capacity(prepared.events.len())?;
        let events = prepared.events.clone();
        let notices = prepared.notices.clone();
        let created = prepared.created_missions().collect::<Vec<_>>();
        let trust = game_characters::decisions::PreparedTrustReactions::prepare::<Command>(
            &self.host.storage,
            prepared
                .trust
                .iter()
                .map(|effect| game_characters::decisions::TrustDelta {
                    observer: effect.observer,
                    subject: effect.leader,
                    delta: effect.delta,
                }),
        )?;
        let mut batch = game_ecs::Batch::new();
        let finish = prepared.stage_into(
            self.host
                .state
                .missions
                .as_ref()
                .ok_or(Error::Unavailable)?,
            &self.host.storage,
            &mut batch,
        )?;
        self.host.storage.validate_batch(&batch)?;
        trust.check_batch::<Command>(&self.host.storage, &batch)?;
        self.host.storage.apply_batch(batch)?;
        finish.finish(self.host.state.missions.as_mut().unwrap());
        trust
            .commit::<Command>(&mut self.host.storage)
            .expect("validated relationship footprint");
        self.mission_membership
            .register_created(&self.host.storage, created);
        self.publish_mission_effects(events, notices);
        Ok(())
    }
    fn mission_membership(&self, actor: PersonId) -> Option<MissionId> {
        self.host.state.missions.as_ref()?;
        self.mission_membership
            .membership(&self.host.storage, actor)
    }
    pub(crate) fn mission_actor_busy(&self, actor: PersonId) -> bool {
        let Some(id) = self.mission_membership(actor) else {
            return false;
        };
        let body = game_physiology::body(&self.host.storage, actor).expect("body exists");
        let rules = &self.host.state.physical.as_ref().unwrap().rules;
        let mind = game_characters::decisions::mind::<Command>(&self.host.storage, actor).unwrap();
        self.host.state.missions.as_ref().unwrap().member_busy(
            &self.host.storage,
            actor,
            game_missions::ServiceAvailability {
                guardian_due: self.guardian_mission_duty(actor, id),
                alive: body.vitals.alive,
                health: body.vitals.health,
                max_health: rules.max_health,
                hunger: body.vitals.hunger,
                meal_relief: rules.meal_relief,
                fatigue: body.vitals.fatigue,
                fatigue_limit: rules.fatigue_limit,
                restricted: self.combat_restricts(actor),
                care_urgency: mind
                    .child_signals
                    .values()
                    .map(|s| s.urgency)
                    .max()
                    .unwrap_or(0),
            },
        )
    }
    fn mission_talk_scene(
        &self,
        speaker: PersonId,
        listener: PersonId,
    ) -> game_missions::TalkScene {
        game_missions::TalkScene {
            speaker_area: self.mission_area(speaker).ok(),
            listener_area: self.mission_area(listener).ok(),
            speaker_combat: self.in_combat(speaker),
            listener_combat: self.in_combat(listener),
            active_deadline: match self.host.storage.activity(speaker) {
                Some(Activity::MissionTalk { completes }) => Some(*completes),
                _ => None,
            },
        }
    }
    fn start_mission_talk(
        &mut self,
        speaker: PersonId,
        listener: PersonId,
        conversation: Conversation,
    ) -> Result<(), Error> {
        let runtime = self
            .host
            .state
            .missions
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let prepared = runtime.prepare_talk_start(
            &self.host.storage,
            speaker,
            listener,
            conversation,
            self.host.clock().time,
            self.mission_talk_scene(speaker, listener),
        )?;
        let completes = prepared.talk.completes;
        let interruptions =
            self.prepare_interruptions(&[speaker], crate::commands::ActivityOwner::Missions)?;
        self.ensure_event_capacity(
            interruptions
                .event_count()?
                .try_into()
                .map_err(|_| Error::Overflow)?,
        )?;
        let mut batch = game_ecs::Batch::new();
        let replacement = prepared.stage_into_with_replacement(&self.host.storage, &mut batch)?;
        let mut finalization = interruptions.stage_into_with_mission_replacement(
            self,
            &mut batch,
            &BTreeMap::from([(speaker, Activity::MissionTalk { completes })]),
            &replacement,
        )?;
        self.host.storage.apply_batch(batch)?;
        finalization.finish_actor(self, speaker);
        Ok(())
    }
    pub(crate) fn cancel_mission_talk(&mut self, actor: PersonId) {
        let Some(runtime) = &self.host.state.missions else {
            return;
        };
        let prepared = runtime.prepare_cancel_talks(&self.host.storage, &[actor]);
        let mut batch = game_ecs::Batch::new();
        let groups = prepared
            .effects()
            .map(|(_, effects)| effects.to_vec())
            .collect::<Vec<_>>();
        for effect in groups.iter().flatten() {
            if matches!(self.host.storage.activity(effect.speaker),Some(Activity::MissionTalk{completes})if *completes==effect.completes)
            {
                batch.insert(game_ecs::StableId(effect.speaker.0), Activity::Idle);
            }
        }
        prepared
            .stage_into(&self.host.storage, &mut batch)
            .expect("immediately prepared speech cancellation");
        self.host
            .storage
            .apply_batch(batch)
            .expect("valid owner cancellation and idle activity");
    }
    pub(crate) fn mission_boundary(&self, limit: SimTime) -> SimTime {
        self.host.state.missions.as_ref().map_or(limit, |runtime| {
            runtime.boundary(&self.host.storage, self.host.clock().time, limit)
        })
    }
    pub(crate) fn mission_view(&self, actor: PersonId) -> Vec<MissionView> {
        self.host
            .state
            .missions
            .as_ref()
            .map_or_else(Vec::new, |w| w.views(&self.host.storage, actor))
    }
    fn mission_can_observe(&self, observer: PersonId, person: PersonId) -> bool {
        self.host.state.missions.as_ref().unwrap().can_observe(
            observer,
            person,
            &MissionPresence { simulation: self },
        )
    }
    fn mission_observation_facts(
        &self,
        observer: PersonId,
        id: MissionId,
    ) -> game_missions::ObservationFacts {
        self.host
            .state
            .missions
            .as_ref()
            .unwrap()
            .observation_facts(
                &self.host.storage,
                observer,
                id,
                self.host.clock().time,
                &MissionPresence { simulation: self },
            )
            .expect("existing mission")
    }
    fn publish_mission(&mut self, id: MissionId) {
        let people = self.host.state.missions.as_ref().unwrap().receivers(
            &self.host.storage,
            id,
            &MissionPresence { simulation: self },
        );
        for person in people {
            self.observe_own_mission(person, id);
        }
    }
    fn observe_own_mission(&mut self, actor: PersonId, id: MissionId) {
        let facts = self.mission_observation_facts(actor, id);
        if let Ok(prepared) = self
            .host
            .state
            .missions
            .as_ref()
            .unwrap()
            .prepare_observation(&self.host.storage, actor, id, &facts)
        {
            prepared
                .commit(&mut self.host.storage)
                .expect("same personal observation snapshot");
        }
    }
    fn recruitment_input(&self, actor: PersonId, m: &Mission) -> game_missions::RecruitmentInput {
        let mind =
            &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                .unwrap();
        let body = &game_physiology::body(&self.host.storage, actor).expect("body exists");
        let social_disruption = game_missions::recruitment_disruption(
            self.mission_area(actor).ok(),
            self.mission_area(m.sponsor).ok(),
            self.invocation_social_penalty(m.sponsor),
        );
        game_missions::RecruitmentInput {
            actor,
            guardian_duty: self.guardian_mission_duty(actor, m.id),
            violent: m.proposal.objective.requires_force(),
            risk_tolerance: mind.personality.risk_tolerance,
            ambition: mind.personality.ambition,
            sociability: mind.personality.sociability,
            empathy: mind.personality.empathy,
            sponsor_trust: mind.relationships.get(&m.sponsor).map_or(0, |r| r.trust),
            social_disruption,
            urgent_care: mind
                .child_signals
                .values()
                .any(|signal| game_missions::urgent_service_care([signal.urgency])),
            hungry: game_missions::service_hungry(
                body.vitals.hunger,
                self.host.state.physical.as_ref().unwrap().rules.meal_relief,
            ),
        }
    }

    fn mission_control_member(&mut self, id: MissionId, actor: PersonId) {
        let Some(world) = &self.host.state.missions else {
            return;
        };
        let Some(body) = game_physiology::body(&self.host.storage, actor) else {
            return;
        };
        let Some(mind) = game_characters::decisions::mind::<Command>(&self.host.storage, actor)
        else {
            return;
        };
        let Some(rules) = self.host.state.physical.as_ref().map(|w| &w.rules) else {
            return;
        };
        let motives = self.mission_service_motives(actor, id);
        let input = game_missions::MemberInput {
            actor,
            now: self.host.clock().time,
            player: actor == self.host.state.ruler,
            in_combat: self.in_combat(actor),
            area: self.mission_area(actor).ok(),
            communication_busy: self.army_actor_talking(actor) || self.realm_actor_busy(actor),
            activity: member_activity(self.host.storage.activity(actor).expect("activity exists")),
            care_urgency: mind
                .child_signals
                .values()
                .map(|signal| signal.urgency)
                .max()
                .unwrap_or(0),
            hunger: body.vitals.hunger,
            meal_relief: rules.meal_relief,
            meal_size: rules.meal_size,
            fatigue: body.vitals.fatigue,
            fatigue_limit: rules.fatigue_limit,
            health: body.vitals.health,
            max_health: rules.max_health,
            motives: &motives,
        };
        let decision = world.member_decision(
            &self.host.storage,
            id,
            input,
            &MissionControlFacts {
                simulation: self,
                actor,
            },
        );
        for attempt in decision.attempts {
            let success = self.execute_mission_member_action(id, actor, &attempt.action);
            if let Some(followup) = attempt.after_result(success) {
                match followup {
                    game_missions::Failure::Action(action) => {
                        self.execute_mission_member_action(id, actor, &action);
                    }
                    game_missions::Failure::Explain(explanation) => {
                        self.mission_explain(id, actor, explanation.text())
                    }
                }
            }
        }
        if let Some(explanation) = decision.explanation {
            self.mission_explain(id, actor, explanation.text());
        }
    }
    fn execute_mission_member_action(
        &mut self,
        id: MissionId,
        actor: PersonId,
        action: &game_missions::MemberAction<Command>,
    ) -> bool {
        use game_missions::MemberAction;
        match action {
            MemberAction::Opaque(command) => {
                self.debug_actor_command(actor, command.clone()).is_ok()
            }
            MemberAction::Mission(action) => self.mission_command(actor, action).is_ok(),
            MemberAction::Eat(source) => self
                .debug_actor_command(actor, Command::Eat { source: *source })
                .is_ok(),
            MemberAction::Rest => self.debug_actor_command(actor, Command::Rest).is_ok(),
            MemberAction::Carry { source, quantity } => self
                .debug_actor_command(
                    actor,
                    Command::Carry {
                        source: *source,
                        quantity: *quantity,
                    },
                )
                .is_ok(),
            MemberAction::Transfer {
                source,
                destination,
                quantity,
            } => self
                .debug_actor_command(
                    actor,
                    Command::Load {
                        source: *source,
                        destination: *destination,
                        quantity: *quantity,
                    },
                )
                .is_ok(),
            MemberAction::Travel(intent) => match intent {
                game_transport::RouteIntent::MoveArea { destination } => self
                    .debug_actor_command(
                        actor,
                        Command::Move {
                            destination: *destination,
                        },
                    )
                    .is_ok(),
                game_transport::RouteIntent::Transport(action) => self
                    .debug_actor_command(
                        actor,
                        Command::Transport {
                            action: action.clone(),
                        },
                    )
                    .is_ok(),
            },
            MemberAction::Move { actor, destination } => {
                self.start_movement(*actor, *destination).is_ok()
            }
            MemberAction::Free(person) => self
                .debug_actor_command(actor, Command::FreeCaptive { target: *person })
                .is_ok(),
            MemberAction::Restrain(person) => self.start_restraint(actor, *person).is_ok(),
            MemberAction::Engage(person) => {
                self.start_mission_encounter(id, actor, *person);
                true
            }
        }
    }
    pub(crate) fn mission_react_to_encounter_end(&mut self, encounter: EncounterId) {
        let Some(encounter) = self
            .host
            .state
            .combat
            .as_ref()
            .and_then(|world| world.encounters.get(&encounter))
        else {
            return;
        };
        let Some(world) = &self.host.state.missions else {
            return;
        };
        let responses = world.encounter_end_responses(&self.host.storage, encounter);
        for (mission, actor) in responses {
            self.mission_control_member(mission, actor);
        }
    }
    fn mission_explain(&mut self, id: MissionId, actor: PersonId, message: &str) {
        let prepared = self
            .host
            .state
            .missions
            .as_ref()
            .unwrap()
            .prepare_explanation(&self.host.storage, id, actor, message)
            .expect("existing member explanation");
        prepared
            .commit(
                self.host.state.missions.as_mut().unwrap(),
                &mut self.host.storage,
            )
            .expect("immediately prepared explanation");
    }
    fn mission_target_observation(
        &self,
        observer: PersonId,
        target: PersonId,
    ) -> Option<game_missions::TargetObservation> {
        if !self.mission_can_observe(observer, target) {
            return None;
        }
        let Location::AtArea(area) = self.resolved_location(target) else {
            return None;
        };
        let body = &game_physiology::body(&self.host.storage, target).expect("body exists");
        let captivity = self
            .host
            .state
            .combat
            .as_ref()
            .and_then(|w| w.captivity.get(&target));
        Some(game_missions::TargetObservation {
            person: target,
            area,
            alive: body.vitals.alive,
            health: body.vitals.health,
            incapacitation: self
                .host
                .state
                .combat
                .as_ref()
                .map_or(0, |w| w.rules.incapacitation),
            captor: captivity.map(|c| c.captor),
            restricted: self.combat_restricts(target),
            observed_at: self.host.clock().time,
        })
    }
    fn mission_objective_knowledge(
        &self,
        actor: PersonId,
        objective: &MissionObjective,
    ) -> game_missions::ObjectiveKnowledge {
        let mind =
            &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                .unwrap();
        let stocks = if let MissionObjective::Deliver {
            source,
            destination,
            ..
        } = objective
        {
            [source, destination]
                .into_iter()
                .filter_map(|id| mind.knowledge.get(id).cloned().map(|k| (*id, k)))
                .collect()
        } else {
            BTreeMap::new()
        };
        let target = objective
            .target_person()
            .and_then(|p| self.mission_target_observation(actor, p));
        game_missions::ObjectiveKnowledge { stocks, target }
    }
    fn start_mission_encounter(&mut self, id: MissionId, actor: PersonId, target: PersonId) {
        let area = match game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)) {
            Some(Location::AtArea(area)) => Some(*area),
            _ => None,
        };
        let linked = self.army_allied_missions(id);
        let plan = self.host.state.missions.as_ref().unwrap().engagement_plan(
            &self.host.storage,
            id,
            target,
            area,
            &linked,
            &MissionEngagementFacts { simulation: self },
        );
        match plan {
            game_missions::EngagementPlan::Keep => {}
            game_missions::EngagementPlan::Approach(encounter) => {
                let _ = self.start_army_approach(actor, id, encounter);
            }
            game_missions::EngagementPlan::Start(participants) => {
                let _ = self.debug_start_encounter(participants);
            }
        }
    }
    pub(crate) fn mission_force_willing(
        &self,
        actor: PersonId,
        id: MissionId,
        target: PersonId,
    ) -> bool {
        let Some(world) = &self.host.state.missions else {
            return false;
        };
        let Some(mission) = game_missions::mission(&self.host.storage, id) else {
            return false;
        };
        world.force_willing(
            &self.host.storage,
            actor,
            id,
            target,
            self.recruitment_input(actor, mission),
            actor == self.host.state.ruler,
        )
    }
    pub(crate) fn mission_combat_order(
        &mut self,
        actor: PersonId,
        id: MissionId,
        objective: MissionObjective,
    ) -> Result<(), Error> {
        self.validate_mission_combat_order(actor, id, &objective)?;
        let leader = game_missions::mission(&self.host.storage, id)
            .expect("record exists")
            .proposal
            .leader;
        self.apply_mission_order(id, leader, objective);
        Ok(())
    }
    /// A leader considers the new objective using their own original service
    /// terms, remembered trust, bodily needs and guardian commitments.
    pub(crate) fn mission_leader_judgment(
        &self,
        leader: PersonId,
        id: MissionId,
        objective: &MissionObjective,
    ) -> Result<i64, Error> {
        self.validate_mission_combat_order(leader, id, objective)?;
        let m = &game_missions::mission(&self.host.storage, id).expect("record exists");
        self.host.state.missions.as_ref().unwrap().leader_judgment(
            &self.host.storage,
            leader,
            id,
            objective,
            self.recruitment_input(leader, m),
        )
    }
    /// The caller has already completed actual communication and obtained this
    /// named leader's choice. Local companions still choose independently.
    pub(crate) fn apply_accepted_leader_objective(
        &mut self,
        leader: PersonId,
        id: MissionId,
        objective: MissionObjective,
    ) -> Result<(), Error> {
        self.validate_mission_combat_order(leader, id, &objective)?;
        self.host
            .state
            .missions
            .as_ref()
            .unwrap()
            .validate_accepted_leader(&self.host.storage, id, leader, self.in_combat(leader))?;
        self.apply_mission_order(id, leader, objective);
        Ok(())
    }
    fn prepare_mission_order(
        &self,
        id: MissionId,
        leader: PersonId,
        objective: &MissionObjective,
        retreat: bool,
    ) -> Result<game_missions::PreparedMission, Error> {
        let heard = self.host.state.missions.as_ref().unwrap().order_heard(
            &self.host.storage,
            id,
            leader,
            &MissionPresence { simulation: self },
        )?;
        self.host.state.missions.as_ref().unwrap().prepare_order(
            &self.host.storage,
            id,
            leader,
            objective,
            &game_missions::OrderFacts {
                now: self.host.clock().time,
                heard,
                assets: self.mission_assets(Some(objective), &[], &[]),
            },
            retreat,
        )
    }
    fn apply_mission_order(
        &mut self,
        id: MissionId,
        leader: PersonId,
        objective: MissionObjective,
    ) {
        let Ok(prepared) = self.prepare_mission_order(id, leader, &objective, false) else {
            return;
        };
        let events = prepared.events.clone();
        let notices = prepared.notices.clone();
        prepared
            .commit(
                self.host.state.missions.as_mut().unwrap(),
                &mut self.host.storage,
            )
            .expect("preflighted communicated objective");
        self.publish_mission_effects(events, notices);
    }
}

impl Simulation {
    /// Read actual committed events. A promised delivery never satisfies an objective.
    pub(crate) fn reconcile_missions(&mut self) -> Result<(), Error> {
        let Some(world) = &self.host.state.missions else {
            return Ok(());
        };
        self.ensure_event_capacity(world.reconciliation_event_bound(&self.host.storage)?)?;
        let events = self
            .host
            .events()
            .iter()
            .skip(world.last_event as usize)
            .map(|e| (EventId(e.sequence), e.at, e.event.clone()))
            .collect::<Vec<_>>();
        let mut local: BTreeMap<AreaId, BTreeSet<PersonId>> = BTreeMap::new();
        for person in &self.host.state.people {
            if let Location::AtArea(area) =
                *game_spatial::location(&self.host.storage, game_ecs::StableId((person.id).0))
                    .expect("location exists")
            {
                local.entry(area).or_default().insert(person.id);
            }
        }
        let observers = world.receipt_observers(&self.host.storage);
        for (sequence, event_time, event) in events {
            use game_missions::MissionReceipt;
            let receipt = match event {
                EventKind::Captured { actor, target, .. }
                | EventKind::Restrained { actor, target, .. } => Some(MissionReceipt::Captured {
                    actor,
                    target,
                    at: event_time,
                }),
                EventKind::Released { actor, target } => Some(MissionReceipt::Released {
                    actor,
                    target,
                    at: event_time,
                }),
                EventKind::Departed { actor, from, .. } => {
                    Some(MissionReceipt::Departed { actor, from })
                }
                EventKind::Arrived { actor, area } => {
                    let contacts = local.get(&area).cloned().unwrap_or_default();
                    Some(observers.arrived(
                        self.host.state.missions.as_ref().unwrap(),
                        actor,
                        area,
                        contacts,
                        &MissionPresence { simulation: self },
                    ))
                }
                EventKind::Transferred {
                    actor,
                    source,
                    destination,
                    quantity,
                } => {
                    let destination_carrier =
                        game_cargo::inventory(&self.host.storage, destination).and_then(
                            |i| match i.location {
                                InventoryLocation::Carried(person) => Some(person),
                                _ => None,
                            },
                        );
                    Some(MissionReceipt::Transferred {
                        actor,
                        source,
                        destination,
                        quantity,
                        destination_carrier,
                    })
                }
                EventKind::Ate {
                    source, quantity, ..
                }
                | EventKind::Consumed {
                    source, quantity, ..
                }
                | EventKind::Cared {
                    source, quantity, ..
                } => Some(MissionReceipt::Lost { source, quantity }),
                EventKind::Spoiled {
                    inventory,
                    quantity,
                } => Some(MissionReceipt::Lost {
                    source: inventory,
                    quantity,
                }),
                _ => None,
            };
            if let Some(receipt) = receipt {
                let prepared = self.host.state.missions.as_ref().unwrap().prepare_receipt(
                    &self.host.storage,
                    sequence,
                    &receipt,
                )?;
                prepared.commit(
                    self.host.state.missions.as_mut().unwrap(),
                    &mut self.host.storage,
                )?;
            }
        }
        let speakers = self
            .host
            .state
            .missions
            .as_ref()
            .unwrap()
            .due_talks(&self.host.storage, self.host.clock().time);
        for speaker in speakers {
            let listener = game_missions::talk(&self.host.storage, speaker)
                .unwrap()
                .listener;
            let order = self
                .host
                .state
                .missions
                .as_ref()
                .unwrap()
                .completion_order(&self.host.storage, speaker);
            let authority = order.as_ref().and_then(|order| {
                self.mission_command_facts(
                    speaker,
                    self.mission_area(speaker).ok()?,
                    &MissionAction::Objective {
                        mission: order.mission,
                        objective: order.objective.clone(),
                    },
                )
                .ok()
            });
            let prepared = self
                .host
                .state
                .missions
                .as_ref()
                .unwrap()
                .prepare_talk_completion(
                    &self.host.storage,
                    speaker,
                    self.host.clock().time,
                    self.mission_talk_scene(speaker, listener),
                    authority.as_ref(),
                )?;
            let order = prepared.order.clone();
            let events = prepared.events.clone();
            let clear = prepared.clear.clone();
            let mut batch = game_ecs::Batch::new();
            if matches!(self.host.storage.activity(clear.speaker), Some(Activity::MissionTalk { completes }) if *completes == clear.completes)
            {
                batch.insert(game_ecs::StableId(clear.speaker.0), Activity::Idle);
            }
            prepared.stage_into(&self.host.storage, &mut batch)?;
            self.host.storage.apply_batch(batch)?;
            if let Some(order) = order
                && let Ok(change) = self.prepare_mission_order(
                    order.mission,
                    order.leader,
                    &order.objective,
                    order.retreat,
                )
            {
                self.commit_mission_plan(change)?;
            }
            self.publish_mission_effects(events, BTreeSet::new());
        }
        let ids: Vec<_> = game_missions::missions(&self.host.storage)
            .map(|(id, _)| id)
            .collect();
        for id in ids {
            self.reconcile_mission_outcome(id);
            let actors: Vec<_> = game_missions::mission(&self.host.storage, id)
                .expect("record exists")
                .members
                .keys()
                .copied()
                .collect();
            for actor in actors {
                self.observe_own_mission(actor, id);
            }
            self.publish_mission(id);
        }
        let last_event = self.host.next_event() - 1;
        self.host
            .state
            .missions
            .as_mut()
            .unwrap()
            .advance_receipts(last_event);
        Ok(())
    }
    fn reconcile_mission_outcome(&mut self, id: MissionId) {
        let m = &game_missions::mission(&self.host.storage, id).expect("record exists");
        let bodies = m
            .members
            .keys()
            .map(|p| {
                let body = &game_physiology::body(&self.host.storage, *p).expect("body exists");
                (
                    *p,
                    game_missions::MemberBody {
                        alive: body.vitals.alive,
                        captive: self
                            .host
                            .state
                            .combat
                            .as_ref()
                            .is_some_and(|w| w.captivity.contains_key(p)),
                        area: self.mission_area(*p).ok(),
                    },
                )
            })
            .collect();
        let leader_knowledge =
            self.mission_objective_knowledge(m.proposal.leader, &m.proposal.objective);
        let receivers = self
            .host
            .state
            .missions
            .as_ref()
            .unwrap()
            .receivers(
                &self.host.storage,
                id,
                &MissionPresence { simulation: self },
            )
            .into_iter()
            .collect();
        let facts = game_missions::OutcomeFacts {
            now: self.host.clock().time,
            bodies,
            leader_knowledge,
            receivers,
        };
        let prepared = self
            .host
            .state
            .missions
            .as_ref()
            .unwrap()
            .prepare_outcome(&self.host.storage, id, &facts)
            .expect("actual mission body references");
        self.commit_mission_plan(prepared)
            .expect("joint mission event preflight");
    }
    pub(crate) fn review_missions(&mut self) -> Result<(), Error> {
        let Some(w) = &self.host.state.missions else {
            return Ok(());
        };
        if !w.review_due(self.host.clock().time) {
            return Ok(());
        }
        self.preflight_missions()?;
        let now = self.host.clock().time;
        self.host.state.missions.as_mut().unwrap().reviewed(now);
        let offers = self
            .host
            .state
            .missions
            .as_ref()
            .unwrap()
            .recruitment_reviews(
                &self.host.storage,
                self.host.state.ruler,
                &MissionPresence { simulation: self },
            );
        for id in offers {
            let offer = game_missions::recruitment(&self.host.storage, id).unwrap();
            let m = game_missions::mission(&self.host.storage, offer.mission).unwrap();
            let input = self.recruitment_input(offer.person, m);
            let area = self.mission_area(offer.person).unwrap();
            let inventory = self.host.state.people[self.person_index[&offer.person]].inventory;
            let known = game_characters::decisions::mind::<game_api::Command>(
                &self.host.storage,
                offer.person,
            )
            .unwrap()
            .knowledge
            .values()
            .cloned()
            .collect::<Vec<_>>();
            let Some(action) = self
                .host
                .state
                .missions
                .as_ref()
                .unwrap()
                .recruitment_response(&self.host.storage, offer.id, input, area, inventory, &known)
            else {
                continue;
            };
            // An unavailable material promise remains an unanswered offer; never invent stock.
            let _ = self.mission_command(offer.person, &action);
        }
        let missions = game_missions::missions(&self.host.storage)
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        for id in missions {
            if let Some(leader) = self
                .host
                .state
                .missions
                .as_ref()
                .unwrap()
                .autonomous_departure(&self.host.storage, id, self.host.state.ruler)
            {
                let _ = self.mission_command(leader, &MissionAction::Depart { mission: id });
            }
            let actors = self.host.state.missions.as_ref().unwrap().control_members(
                &self.host.storage,
                &self.mission_membership,
                id,
            );
            for actor in actors {
                self.mission_control_member(id, actor);
            }
        }
        Ok(())
    }
    pub(crate) fn mission_allocation_bound(&self) -> u64 {
        self.host
            .state
            .missions
            .as_ref()
            .map_or(0, |w| w.allocation_bound(&self.host.storage))
    }
    pub(crate) fn mission_event_bound(&self) -> usize {
        self.host
            .state
            .missions
            .as_ref()
            .map_or(0, |w| w.event_bound(&self.host.storage))
    }
    pub(crate) fn preflight_missions(&self) -> Result<(), Error> {
        let Some(runtime) = &self.host.state.missions else {
            return Ok(());
        };
        self.ensure_event_capacity(
            runtime.preflight_budget(&self.host.storage, self.host.next_id())?,
        )
    }
}

impl Simulation {
    pub(crate) fn available_mission_commands(&self, actor: PersonId) -> Vec<Command> {
        self.host
            .state
            .missions
            .as_ref()
            .map_or_else(Vec::new, |runtime| {
                runtime
                    .available_commands(
                        &self.host.storage,
                        actor,
                        &MissionPresence { simulation: self },
                    )
                    .into_iter()
                    .map(|action| Command::Mission { action })
                    .collect()
            })
    }
    fn mission_service_motives(
        &self,
        actor: PersonId,
        id: MissionId,
    ) -> game_missions::ServiceMotives {
        let m = &game_missions::mission(&self.host.storage, id).expect("record exists");
        let mind =
            &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                .unwrap();
        let agreement =
            game_missions::recruitment(&self.host.storage, m.members[&actor].recruitment)
                .expect("record exists")
                .agreement;
        let payment = agreement.and_then(|id| {
            self.agreement_view(actor)
                .into_iter()
                .find(|view| view.id == id)
        });
        game_missions::ServiceMotives {
            honesty: mind.personality.honesty,
            leader_trust: mind
                .relationships
                .get(&m.proposal.leader)
                .map_or(0, |r| r.trust),
            guardian_duty: self.guardian_mission_duty(actor, id),
            payment,
        }
    }
    /// Tactical observation supplies real opponents; service owns deliberation.
    /// The combat owner effect applies only to this actual existing fighter.
    pub(crate) fn review_mission_combat(&mut self, encounter: EncounterId, actor: PersonId) {
        let Some(id) = self.mission_membership(actor) else {
            return;
        };
        let Some(conflict) = &self.host.state.combat else {
            return;
        };
        let Some(fighter) = conflict
            .encounters
            .get(&encounter)
            .and_then(|e| e.fighters.get(&actor))
        else {
            return;
        };
        let own_team = fighter.team;
        let mind = game_characters::decisions::mind::<Command>(&self.host.storage, actor).unwrap();
        let Ok(Some(prepared)) = self
            .host
            .state
            .missions
            .as_ref()
            .unwrap()
            .prepare_combat_defection(
                &self.host.storage,
                actor,
                id,
                game_missions::CombatDefectionFacts {
                    player: self.host.state.ruler,
                    own_team,
                    motives: self.mission_service_motives(actor, id),
                    relationships: &mind.relationships,
                },
                conflict.visible_opponents(encounter, actor),
            )
        else {
            return;
        };
        let effect = prepared.allegiance.expect("prepared defection allegiance");
        if self.ensure_event_capacity(prepared.events.len()).is_err()
            || prepared
                .check(
                    self.host.state.missions.as_ref().unwrap(),
                    &self.host.storage,
                )
                .is_err()
        {
            return;
        }
        let Ok(allegiance) = self.host.state.combat.as_ref().unwrap().prepare_allegiance(
            encounter,
            actor,
            effect.opponent,
            own_team,
            effect.team,
        ) else {
            return;
        };
        let events = prepared.events.clone();
        let notices = prepared.notices.clone();
        let mut batch = game_ecs::Batch::new();
        let Ok(finish) = prepared.stage_into(
            self.host.state.missions.as_ref().unwrap(),
            &self.host.storage,
            &mut batch,
        ) else {
            return;
        };
        if self.host.storage.validate_batch(&batch).is_err()
            || allegiance
                .check(self.host.state.combat.as_ref().unwrap())
                .is_err()
        {
            return;
        }
        self.host
            .storage
            .apply_batch(batch)
            .expect("validated service footprint");
        finish.finish(self.host.state.missions.as_mut().unwrap());
        allegiance
            .commit(self.host.state.combat.as_mut().unwrap())
            .expect("validated fighter allegiance");
        self.publish_mission_effects(events, notices);
        self.observe_own_mission(actor, id);
    }
    pub(crate) fn mission_combat_action(
        &mut self,
        encounter: EncounterId,
        actor: PersonId,
    ) -> Option<(Option<Point>, CombatAction)> {
        let id = self.mission_membership(actor)?;
        let world = self.host.state.missions.as_ref()?;
        let conflict = self.host.state.combat.as_ref()?;
        let battle = conflict.encounters.get(&encounter)?;
        let fighter = battle.fighters.get(&actor)?;
        let map = conflict.map(battle.map)?;
        let mind = game_characters::decisions::mind::<Command>(&self.host.storage, actor)?;
        let body = game_physiology::body(&self.host.storage, actor)?;
        let decision = world.tactical_decision(
            &self.host.storage,
            id,
            game_missions::TacticalInput {
                actor,
                player: actor == self.host.state.ruler,
                area: battle.area,
                map,
                fighter,
                roster: &battle.fighters,
                routes: &mind.routes,
                health: body.vitals.health,
                max_health: self.host.state.physical.as_ref()?.rules.max_health,
                risk_tolerance: mind.personality.risk_tolerance,
                movement: conflict.rules.movement,
                reach: conflict.rules.reach,
                binding_available: self.binding_stock(actor).is_some(),
            },
            &MissionTacticalFacts {
                simulation: self,
                actor,
            },
        )?;
        let search = decision
            .search
            .map(|after| game_conflict::PreparedSearch::new(encounter, fighter, after));
        if let Some(search) = search {
            search
                .commit(self.host.state.combat.as_mut()?)
                .expect("immediately prepared search state");
        }
        let action = match decision.action {
            game_missions::TacticAction::Defend => CombatAction::Defend,
            game_missions::TacticAction::Surrender => CombatAction::Surrender,
            game_missions::TacticAction::Retreat(destination) => {
                CombatAction::Retreat { destination }
            }
            game_missions::TacticAction::Release(target) => CombatAction::Release { target },
            game_missions::TacticAction::Capture(target) => CombatAction::Capture { target },
            game_missions::TacticAction::Attack(target) => CombatAction::Attack { target },
        };
        Some((decision.movement, action))
    }
    pub(crate) fn mission_reinforcements(&self) -> Vec<(EncounterId, PersonId, u32, AreaId)> {
        self.host
            .state
            .missions
            .as_ref()
            .map_or_else(Vec::new, |world| {
                world
                    .reinforcements(
                        &self.host.storage,
                        self.host.clock().time,
                        &MissionReinforcementFacts { simulation: self },
                    )
                    .into_iter()
                    .map(|r| (r.encounter, r.actor, r.team, r.from))
                    .collect()
            })
    }
}

struct MissionValidationFacts<'a> {
    state: &'a CanonicalState<'a>,
}
impl game_missions::ValidationFacts for MissionValidationFacts<'_> {
    fn inventory(&self, id: InventoryId) -> Option<&game_cargo::Inventory> {
        game_cargo::inventory(self.state.components, id)
    }
    fn agreement_parties(&self, id: AgreementId) -> Option<&BTreeSet<PersonId>> {
        game_agreements::agreement(self.state.components, id).map(|row| &row.parties)
    }
    fn talk_deadline(&self, actor: PersonId) -> Option<SimTime> {
        match self.state.components.activity(actor) {
            Some(Activity::MissionTalk { completes }) => Some(*completes),
            _ => None,
        }
    }
    fn at_area(&self, actor: PersonId) -> Option<AreaId> {
        match game_spatial::location(self.state.components, game_ecs::StableId(actor.0)) {
            Some(Location::AtArea(area)) => Some(*area),
            _ => None,
        }
    }
    fn talking_people(&self) -> impl Iterator<Item = PersonId> {
        self.state
            .components
            .activities()
            .filter_map(|(id, a)| matches!(a, Activity::MissionTalk { .. }).then_some(id))
    }
}
pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(runtime) = &s.state.missions else {
        if game_missions::missions(s.components).next().is_some()
            || game_missions::recruitments(s.components).next().is_some()
            || game_missions::knowledges(s.components).next().is_some()
            || game_missions::talks(s.components).next().is_some()
        {
            return Err(Error::InvalidSave(
                "mission components without runtime".into(),
            ));
        }
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent expedition commitments".into());
    let physical = s.state.physical.as_ref().ok_or_else(fail)?;
    if s.state.decisions.is_none() || s.state.combat.is_none() || s.state.agreements.is_none() {
        return Err(fail());
    }
    let people = s.state.people.iter().map(|p| p.id).collect();
    let areas = s.state.areas.iter().map(|a| a.id).collect();
    game_missions::validate_context(
        runtime,
        s.components,
        game_missions::ValidationCatalog {
            now: s.time,
            next_id: s.next_id,
            next_event: s.next_event,
            people: &people,
            areas: &areas,
            carrying_capacity: physical.rules.carrying_capacity,
        },
        &MissionValidationFacts { state: s },
    )
}

impl Simulation {
    pub(crate) fn validate_mission_combat_order(
        &self,
        actor: PersonId,
        id: MissionId,
        objective: &MissionObjective,
    ) -> Result<(), Error> {
        let area = self.mission_area(actor)?;
        let facts = self.mission_command_facts(
            actor,
            area,
            &MissionAction::Objective {
                mission: id,
                objective: objective.clone(),
            },
        )?;
        let world = self
            .host
            .state
            .missions
            .as_ref()
            .ok_or(Error::Unavailable)?;
        world.validate_order(&self.host.storage, actor, id, objective, &facts)?;
        self.ensure_event_capacity(world.order_event_bound(&self.host.storage, id)?)
    }

    pub(crate) fn guardian_mission_duty(&self, actor: PersonId, id: MissionId) -> bool {
        let Some(lineage) = &self.host.state.lineage else {
            return false;
        };
        let sponsor = game_missions::mission(&self.host.storage, id)
            .expect("record exists")
            .sponsor;
        let signal = game_characters::decisions::mind::<Command>(&self.host.storage, actor)
            .and_then(|mind| mind.child_signals.get(&sponsor));
        lineage.guardian_service_due(game_lineage::GuardianServiceFacts {
            actor,
            sponsor,
            guardian: self.host.state.people[self.person_index[&actor]].role == Role::Guardian,
            care_urgency: signal.map_or(0, |s| s.urgency),
            hydration_urgency: signal.map_or(0, |s| s.hydration_urgency),
        })
    }
}

struct MissionTacticalFacts<'a> {
    simulation: &'a Simulation,
    actor: PersonId,
}
impl game_missions::TacticalFacts for MissionTacticalFacts<'_> {
    fn capabilities(&self, target: PersonId) -> game_missions::TacticalCapabilities {
        game_missions::TacticalCapabilities {
            release_available: self.simulation.can_release(self.actor, target),
            target_can_fight: self.simulation.can_fight(target),
            target_captive: self
                .simulation
                .host
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target),
        }
    }
}
struct MissionReinforcementFacts<'a> {
    simulation: &'a Simulation,
}
impl game_missions::ReinforcementFacts for MissionReinforcementFacts<'_> {
    fn arrivals(&self) -> impl Iterator<Item = game_missions::ArrivalFacts> {
        self.simulation
            .host
            .events()
            .iter()
            .rev()
            .take_while(|e| e.at == self.simulation.host.clock().time)
            .filter_map(|e| {
                if let EventKind::Arrived { actor, area } = e.event {
                    Some(game_missions::ArrivalFacts {
                        actor,
                        area,
                        at: e.at,
                    })
                } else {
                    None
                }
            })
    }
    fn body(&self, mission: MissionId, person: PersonId) -> game_missions::ReinforcementBody {
        let m = game_missions::mission(&self.simulation.host.storage, mission).unwrap();
        game_missions::ReinforcementBody {
            player: person == self.simulation.host.state.ruler,
            can_fight: self.simulation.can_fight(person),
            in_combat: self.simulation.in_combat(person),
            area: self.simulation.mission_area(person).ok(),
            recruitment: self.simulation.recruitment_input(person, m),
        }
    }
    fn encounters(&self) -> impl Iterator<Item = &game_conflict::Encounter> {
        self.simulation
            .host
            .state
            .combat
            .as_ref()
            .into_iter()
            .flat_map(|world| world.encounters.values())
    }
    fn linked_missions(&self, mission: MissionId) -> BTreeSet<MissionId> {
        self.simulation.army_allied_missions(mission)
    }
}

struct MissionEngagementFacts<'a> {
    simulation: &'a Simulation,
}
impl game_missions::EngagementFacts for MissionEngagementFacts<'_> {
    fn encounters(&self, target: PersonId) -> impl Iterator<Item = game_missions::EncounterFacts> {
        self.simulation
            .host
            .state
            .combat
            .as_ref()
            .into_iter()
            .flat_map(|world| world.encounters.values())
            .map(move |e| game_missions::EncounterFacts {
                id: e.id,
                active: e.active,
                target_present: e.fighters.contains_key(&target),
            })
    }
    fn fighter(&self, mission: MissionId, person: PersonId) -> game_missions::FighterFacts {
        let sim = self.simulation;
        let m = &game_missions::mission(&sim.host.storage, mission).expect("record exists");
        game_missions::FighterFacts {
            player: person == sim.host.state.ruler,
            at_area: match game_spatial::location(&sim.host.storage, game_ecs::StableId(person.0)) {
                Some(Location::AtArea(area)) => Some(*area),
                _ => None,
            },
            can_fight: sim.can_fight(person),
            in_combat: sim.in_combat(person),
            recruitment: sim.recruitment_input(person, m),
        }
    }
}

fn member_activity(activity: &Activity) -> game_missions::MemberActivity {
    use game_missions::MemberActivity as M;
    match activity {
        Activity::MissionTalk { .. } => M::MissionTalk,
        Activity::ArmyTalk { .. } => M::ArmyTalk,
        Activity::ArmyApproach { .. } => M::ArmyApproach,
        Activity::Eat { .. } => M::Eat,
        Activity::Load { .. } => M::Load,
        Activity::Tell { .. } => M::Tell,
        Activity::Care { .. } => M::Care,
        Activity::Lesson { .. } => M::Lesson,
        Activity::WritingMemory { .. } => M::WritingMemory,
        Activity::ReadingMemory { .. } => M::ReadingMemory,
        Activity::TeachingMemory { .. } => M::TeachingMemory,
        Activity::MovingRecord { .. } => M::MovingRecord,
        Activity::Escape { .. } => M::Escape,
        Activity::Restrain { .. } => M::Restrain,
        Activity::Unbind { .. } => M::Unbind,
        _ => M::Other,
    }
}
struct MissionControlFacts<'a> {
    simulation: &'a Simulation,
    actor: PersonId,
}
impl game_logistics::AcquisitionSource<Command> for MissionControlFacts<'_> {
    fn offers(
        &self,
        good: Good,
        quantity: Quantity,
        room: Quantity,
    ) -> Vec<game_logistics::Candidate<Command>> {
        let personality = self
            .simulation
            .invocation_personality(self.actor)
            .unwrap_or_else(|| {
                game_characters::decisions::mind::<Command>(
                    &self.simulation.host.storage,
                    self.actor,
                )
                .unwrap()
                .personality
                .clone()
            });
        game_logistics::AcquisitionSource::offers(
            &crate::logistics::AcquisitionFacts {
                simulation: self.simulation,
                actor: self.actor,
                resources: None,
                personality,
            },
            good,
            quantity,
            room,
        )
    }
}
impl game_missions::MemberFacts<Command> for MissionControlFacts<'_> {
    fn food(&self) -> impl Iterator<Item = game_missions::FoodObservation> {
        game_characters::decisions::mind::<Command>(&self.simulation.host.storage,self.actor).into_iter().flat_map(|mind|mind.knowledge.values()).map(|k|game_missions::FoodObservation{
            inventory:k.inventory,good:k.good,accessible:k.accessible,quantity:k.quantity,area:k.area,
            carried_by_actor:matches!(game_cargo::inventory(&self.simulation.host.storage,k.inventory).expect("inventory exists").location,InventoryLocation::Carried(carrier)if carrier==self.actor),
        })
    }
    fn source_good(&self, inventory: InventoryId) -> Option<Good> {
        game_characters::decisions::mind::<Command>(&self.simulation.host.storage, self.actor)?
            .knowledge
            .get(&inventory)
            .map(|k| k.good)
    }
    fn inventory_destination(
        &self,
        inventory: InventoryId,
    ) -> Option<game_logistics::KnownDestination> {
        self.simulation
            .delivery_known_inventory_destination(self.actor, inventory)
    }
    fn actor_location(&self) -> Location {
        self.simulation.delivery_observed_location(self.actor)
    }
    fn objective_knowledge(
        &self,
        objective: &MissionObjective,
    ) -> game_missions::ObjectiveKnowledge {
        self.simulation
            .mission_objective_knowledge(self.actor, objective)
    }
    fn target(&self, person: PersonId) -> game_missions::TargetFacts {
        let sim = self.simulation;
        let location = game_spatial::location(&sim.host.storage, game_ecs::StableId(person.0))
            .expect("location exists");
        let released =
            game_characters::decisions::mind::<Command>(&sim.host.storage, person).unwrap();
        let body = game_physiology::body(&sim.host.storage, person).expect("body exists");
        game_missions::TargetFacts {
            at_area: match location {
                Location::AtArea(area) => Some(*area),
                _ => None,
            },
            available_area: sim.mission_area(person).is_ok(),
            player: person == sim.host.state.ruler,
            alive: body.vitals.alive,
            can_fight: sim.can_fight(person),
            captor: sim
                .host
                .state
                .combat
                .as_ref()
                .and_then(|w| w.captivity.get(&person))
                .map(|c| c.captor),
            trust: released
                .relationships
                .get(&self.actor)
                .map_or(0, |r| r.trust),
            empathy: released.personality.empathy,
            hunger: body.vitals.hunger,
        }
    }
    fn delivery_facts(
        &self,
        request: &game_missions::MemberDelivery<'_>,
    ) -> game_logistics::DeliveryFacts<'_, Command> {
        let tags = request
            .origin_cargo
            .into_iter()
            .flat_map(|tags| tags.iter())
            .map(|(id, q)| (*id, Quantity(*q)))
            .collect();
        self.simulation.delivery_facts_with_source_cargo(
            self.actor,
            &request.delivery,
            &request.destination,
            crate::logistics::DeliveryProviders {
                source_cargo: &tags,
                offers: &[],
                resources: None,
            },
        )
    }
}

struct MissionPresence<'a> {
    simulation: &'a Simulation,
}
impl game_missions::PresenceFacts for MissionPresence<'_> {
    fn location(&self, person: PersonId) -> Location {
        self.simulation.resolved_location(person)
    }
    fn raw_location(&self, person: PersonId) -> Location {
        game_spatial::location(&self.simulation.host.storage, game_ecs::StableId(person.0))
            .expect("person location")
            .clone()
    }
    fn area(&self, person: PersonId) -> Option<AreaId> {
        self.simulation.mission_area(person).ok()
    }
    fn same_place(&self, a: PersonId, b: PersonId) -> bool {
        self.simulation.transport_same_place(a, b)
    }
    fn in_combat(&self, person: PersonId) -> bool {
        self.simulation.in_combat(person)
    }
    fn combat_scene(
        &self,
        person: PersonId,
    ) -> Option<(&game_conflict::Encounter, &game_conflict::Map)> {
        let combat = self.simulation.host.state.combat.as_ref()?;
        let encounter = combat
            .encounters
            .values()
            .find(|e| e.active && e.fighters.contains_key(&person))?;
        Some((encounter, combat.map(encounter.map)?))
    }
}
impl game_missions::CommandAvailabilityFacts for MissionPresence<'_> {
    fn area(&self, person: PersonId) -> Option<AreaId> {
        self.simulation.mission_area(person).ok()
    }
    fn in_combat(&self, person: PersonId) -> bool {
        self.simulation.in_combat(person)
    }
    fn urgent_care(&self, person: PersonId) -> bool {
        game_missions::urgent_service_care(
            game_characters::decisions::mind::<Command>(&self.simulation.host.storage, person)
                .into_iter()
                .flat_map(|mind| mind.child_signals.values())
                .map(|signal| signal.urgency),
        )
    }
    fn guardian_duty(&self, person: PersonId, mission: MissionId) -> bool {
        self.simulation.guardian_mission_duty(person, mission)
    }
    fn local_people(&self, area: AreaId) -> impl Iterator<Item = PersonId> {
        self.simulation
            .host
            .state
            .people
            .iter()
            .filter(move |person| {
                game_spatial::location(
                    &self.simulation.host.storage,
                    game_ecs::StableId(person.id.0),
                ) == Some(&Location::AtArea(area))
            })
            .map(|p| p.id)
    }
}

impl game_missions::OrderHearingFacts for MissionPresence<'_> {
    fn player(&self) -> PersonId {
        self.simulation.host.state.ruler
    }
    fn recruitment(
        &self,
        person: PersonId,
        mission: &game_missions::Mission,
    ) -> game_missions::RecruitmentInput {
        self.simulation.recruitment_input(person, mission)
    }
}

impl game_missions::CommandFactSource for MissionPresence<'_> {
    fn assets(
        &self,
        objective: Option<&MissionObjective>,
        terms: &[AgreementTerm],
        supplies: &[MissionSupply],
    ) -> game_missions::MissionAssets {
        self.simulation.mission_assets(objective, terms, supplies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Simulation {
        let mut sim = Simulation::mission_fixture(29, Execution::default()).unwrap();
        for person in sim.host.state.people.clone() {
            sim.debug_set_personality(
                person.id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        sim
    }
    fn propose(
        sim: &mut Simulation,
        leader: PersonId,
        objective: MissionObjective,
        supplies: Vec<MissionSupply>,
    ) -> MissionId {
        let home = sim.host.state.areas[0].id;
        sim.mission_command(
            sim.host.state.ruler,
            &MissionAction::Propose {
                proposal: MissionProposal {
                    leader,
                    recruits: BTreeSet::new(),
                    objective,
                    home,
                    supplies,
                    compensation: BTreeMap::new(),
                    deadline: SimTime(sim.host.clock().time.0 + 720_000),
                },
            },
        )
        .unwrap();
        game_missions::missions(&sim.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap()
    }
    fn accept(sim: &mut Simulation, id: MissionId, person: PersonId) {
        let recruitment = game_missions::mission(&sim.host.storage, id)
            .expect("record exists")
            .members[&person]
            .recruitment;
        sim.mission_command(person, &MissionAction::Accept { recruitment })
            .unwrap();
    }
    fn public_food(sim: &Simulation) -> InventoryId {
        game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|i| i.good == Good::Food && matches!(i.location, InventoryLocation::Area(_)))
            .unwrap()
            .id
    }
    #[test]
    fn supplies_and_route_legs_are_real_and_the_realm_keeps_working() {
        let mut sim = fixture();
        let leader = sim.host.state.people[5].id;
        let worker = sim.host.state.people[6].id;
        sim.debug_set_personality(
            leader,
            decisions::Personality::industrious(),
            decisions::Aspiration::Independence,
        )
        .unwrap();
        sim.debug_set_personality(
            worker,
            decisions::Personality::industrious(),
            decisions::Aspiration::Independence,
        )
        .unwrap();
        let forest = sim.host.state.areas[2].id;
        let food = public_food(&sim);
        let carried = sim.host.state.people[sim.person_index[&leader]].inventory;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![MissionSupply {
                carrier: leader,
                source: food,
                quantity: Quantity(2_000),
            }],
        );
        accept(&mut sim, id, leader);
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, carried)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        sim.advance(MINUTE).unwrap();
        assert!(matches!(
            *sim.host.storage.activity(leader).expect("body exists"),
            Activity::Load { .. }
        ));
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, carried)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, carried)
                .expect("inventory exists")
                .quantity,
            Quantity(2_000)
        );
        assert!(matches!(
            *game_spatial::location(
                &sim.host.storage,
                game_ecs::StableId((sim.host.state.people[sim.person_index[&leader]].id).0)
            )
            .expect("location exists"),
            Location::InTransit { .. }
        ));
        sim.advance(SimDuration(600_000)).unwrap();
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .status,
            MissionStatus::Completed
        );
        assert!(
            sim.host
                .events()
                .iter()
                .any(|e| matches!(e.event,EventKind::Produced {actor,..}if actor==worker))
        );
        assert!(
            sim.host
                .events()
                .iter()
                .any(|e| matches!(e.event,EventKind::Departed {actor,..}if actor==leader))
        );
        assert!(sim.host.events().iter().any(
            |e| matches!(e.event,EventKind::Arrived {actor,area}if actor==leader&&area==forest)
        ));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn promised_provisions_do_not_count_unmarked_same_good_cargo() {
        let mut sim = fixture();
        let actor = sim.host.state.people[5].id;
        sim.debug_set_personality(
            actor,
            decisions::Personality::industrious(),
            decisions::Aspiration::Independence,
        )
        .unwrap();
        let source = public_food(&sim);
        let unqualified = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Carried(actor),
                Quantity(2000),
                Quantity(10000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let opening = game_cargo::inventory(&sim.host.storage, source)
            .expect("inventory exists")
            .quantity;
        let forest = sim.host.state.areas[2].id;
        let id = propose(
            &mut sim,
            actor,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![MissionSupply {
                carrier: actor,
                source,
                quantity: Quantity(2000),
            }],
        );
        accept(&mut sim, id, actor);
        sim.advance(MINUTE).unwrap();
        assert!(
            matches!(*sim.host.storage.activity(actor).expect("body exists"), Activity::Load { source: from, .. } if from == source)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            opening
        );
        assert!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&actor]
                .supply_cargo
                .is_empty()
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            Quantity(opening.0 - 2000)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, unqualified)
                .expect("inventory exists")
                .quantity,
            Quantity(2000)
        );
        let tags = &game_missions::mission(&sim.host.storage, id)
            .expect("record exists")
            .members[&actor]
            .supply_cargo[&source];
        assert_eq!(tags.values().sum::<u64>(), 2000);
        assert!(matches!(
            *game_spatial::location(
                &sim.host.storage,
                game_ecs::StableId((sim.host.state.people[sim.person_index[&actor]].id).0)
            )
            .expect("location exists"),
            Location::InTransit { .. }
        ));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn autonomous_delivery_uses_promised_origin_and_preserves_unqualified_cargo() {
        let mut sim = fixture();
        let actor = sim.host.state.people[5].id;
        sim.debug_set_personality(
            actor,
            decisions::Personality::industrious(),
            decisions::Aspiration::Independence,
        )
        .unwrap();
        let area = sim.host.state.areas[0].id;
        let source = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                Quantity(1000),
                Quantity(10000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let carried = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Carried(actor),
                Quantity(2000),
                Quantity(10000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let destination = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                Quantity(0),
                Quantity(10000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        sim.review_decisions().unwrap();
        let opening = game_cargo::inventory(&sim.host.storage, source)
            .expect("inventory exists")
            .quantity;
        let id = propose(
            &mut sim,
            actor,
            MissionObjective::Deliver {
                source,
                destination,
                quantity: Quantity(1000),
            },
            vec![],
        );
        accept(&mut sim, id, actor);
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            opening
        );
        let mut sliced = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(SimDuration(180000)).unwrap();
        for elapsed in [17000, 43000, 111000, 9000] {
            sliced.advance(SimDuration(elapsed)).unwrap();
        }
        assert_eq!(sim.canonical_state(), sliced.canonical_state());
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            Quantity(opening.0 - 1000)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(1000)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, carried)
                .expect("inventory exists")
                .quantity,
            Quantity(2000)
        );
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&actor]
                .delivered,
            1000
        );
    }
    #[test]
    fn npc_refuses_danger_and_counteroffers_for_a_marginal_job() {
        let mut sim = fixture();
        let recruit = sim.host.state.people[5].id;
        let enemy = sim.host.state.people[6].id;
        {
            let traits = &mut game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                recruit,
            )
            .unwrap()
            .personality;
            traits.ambition = 0;
            traits.sociability = 400;
        }
        let id = propose(
            &mut sim,
            recruit,
            MissionObjective::Defeat { person: enemy },
            vec![],
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&recruit]
                .status,
            CompanionStatus::Refused
        );
        let farm = sim.host.state.areas[1].id;
        let id = propose(
            &mut sim,
            recruit,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        sim.advance(MINUTE).unwrap();
        let offer = game_missions::recruitment(
            &sim.host.storage,
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&recruit]
                .recruitment,
        )
        .expect("record exists")
        .clone();
        assert!(offer.countered);
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&recruit]
                .status,
            CompanionStatus::Offered
        );
        sim.mission_command(
            sim.host.state.ruler,
            &MissionAction::AcceptCounteroffer {
                recruitment: offer.id,
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        let offer =
            &game_missions::recruitment(&sim.host.storage, offer.id).expect("record exists");
        assert!(offer.agreement.is_some());
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&recruit]
                .status,
            CompanionStatus::Accepted
        );
        let obligations = game_agreements::obligations(&sim.host.storage)
            .map(|(_, row)| row)
            .collect::<Vec<_>>();
        assert!(
            obligations
                .iter()
                .any(|obligation| obligation.performed == 0)
        );
    }
    #[test]
    fn absent_sponsor_receives_no_telepathic_progress_and_reports_take_time() {
        let mut sim = fixture();
        let leader = sim.host.state.people[5].id;
        sim.debug_set_personality(
            leader,
            decisions::Personality::industrious(),
            decisions::Aspiration::Independence,
        )
        .unwrap();
        let forest = sim.host.state.areas[2].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.advance(SimDuration(240_000)).unwrap();
        let ruler = sim.host.state.ruler;
        assert_eq!(
            game_missions::knowledge(&sim.host.storage, ruler)
                .unwrap()
                .missions[&id]
                .status,
            MissionStatus::Traveling
        );
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .status,
            MissionStatus::Returning
        );
        let snapshot = sim.mission_view(leader)[0].clone();
        assert!(
            snapshot.observed_at
                > game_missions::knowledge(&sim.host.storage, ruler)
                    .unwrap()
                    .missions[&id]
                    .observed_at
        );
        assert!(
            sim.mission_command(
                leader,
                &MissionAction::Report {
                    mission: id,
                    recipient: ruler
                }
            )
            .is_err()
        );
        sim.advance(SimDuration(420_000)).unwrap();
        assert_eq!(
            *game_spatial::location(
                &sim.host.storage,
                game_ecs::StableId((sim.host.state.people[sim.person_index[&leader]].id).0)
            )
            .expect("location exists"),
            Location::AtArea(sim.host.state.areas[0].id)
        );
        // A cached account can be reported to a bystander who never joined the party.
        let recipient = sim.host.state.people[8].id;
        assert!(sim.mission_view(recipient).is_empty());
        sim.mission_command(
            leader,
            &MissionAction::Report {
                mission: id,
                recipient,
            },
        )
        .unwrap();
        assert!(sim.mission_view(recipient).is_empty());
        let mut saved = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        saved.advance(MINUTE).unwrap();
        assert_eq!(saved.mission_view(recipient)[0].reported_by, Some(leader));
        assert_eq!(
            saved.mission_view(recipient)[0].observed_at,
            sim.mission_view(leader)[0].observed_at
        );
    }
    #[test]
    fn missing_personal_route_blocks_leader_without_moving_the_party() {
        let mut sim = fixture();
        let leader = sim.host.state.people[5].id;
        let forest = sim
            .debug_add_disconnected_area("Unseen far clearing")
            .unwrap();
        let intermediary = sim.host.state.areas[2].id;
        sim.host.state.areas[2].links.push(Link {
            destination: forest,
            duration: MINUTE,
        });
        sim.host.state.areas.last_mut().unwrap().links.push(Link {
            destination: intermediary,
            duration: MINUTE,
        });
        game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, leader)
            .unwrap()
            .routes
            .clear();
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.advance(SimDuration(180_000)).unwrap();
        assert_eq!(
            *game_spatial::location(
                &sim.host.storage,
                game_ecs::StableId((sim.host.state.people[sim.person_index[&leader]].id).0)
            )
            .expect("location exists"),
            Location::AtArea(sim.host.state.areas[0].id)
        );
        assert!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&leader]
                .explanation
                .contains("personally known")
        );
        assert!(
            !sim.host
                .events()
                .iter()
                .any(|e| matches!(e.event,EventKind::Departed {actor,..}if actor==leader))
        );
    }
    #[test]
    fn delivery_credit_requires_the_conserved_origin_cargo() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let area = sim.host.state.areas[0].id;
        let source = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|i| i.good == Good::Timber && i.location == InventoryLocation::Area(area))
            .unwrap()
            .id;
        let destination = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                Quantity(0),
                Quantity(10_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let carried = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|i| i.good == Good::Timber && i.location == InventoryLocation::Carried(actor))
            .unwrap()
            .id;
        let id = propose(
            &mut sim,
            actor,
            MissionObjective::Deliver {
                source,
                destination,
                quantity: Quantity(1_000),
            },
            vec![],
        );
        sim.mission_command(actor, &MissionAction::Depart { mission: id })
            .unwrap();
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&actor]
                .delivered,
            0
        );
        sim.debug_actor_command(
            actor,
            Command::Load {
                source,
                destination: carried,
                quantity: Quantity(1_000),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&actor]
                .delivered,
            0
        );
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&actor]
                .cargo[&carried],
            1_000
        );
        sim.debug_actor_command(
            actor,
            Command::Load {
                source: carried,
                destination,
                quantity: Quantity(1_000),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&actor]
                .delivered,
            1_000
        );
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .status,
            MissionStatus::Completed
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn changed_objectives_take_local_time_and_may_be_refused() {
        let mut sim = fixture();
        let leader = sim.host.state.people[5].id;
        let enemy = sim.host.state.people[6].id;
        let farm = sim.host.state.areas[1].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        accept(&mut sim, id, leader);
        let old = game_missions::mission(&sim.host.storage, id)
            .expect("record exists")
            .members[&leader]
            .directive
            .clone();
        sim.mission_command(
            sim.host.state.ruler,
            &MissionAction::Objective {
                mission: id,
                objective: MissionObjective::Defeat { person: enemy },
            },
        )
        .unwrap();
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&leader]
                .directive,
            old
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&leader]
                .status,
            CompanionStatus::Withdrawn
        );
        assert!(
            !sim.host
                .events()
                .iter()
                .any(|e| matches!(e.event, EventKind::EncounterStarted { .. }))
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn known_enemy_relationship_can_motivate_an_actual_defection() {
        let mut sim = fixture();
        let actor = sim.host.state.people[5].id;
        let enemy = sim.host.state.people[6].id;
        let leader = sim.host.state.people[7].id;
        let home = sim.host.state.areas[0].id;
        sim.mission_command(
            sim.host.state.ruler,
            &MissionAction::Propose {
                proposal: MissionProposal {
                    leader,
                    recruits: BTreeSet::from([actor]),
                    objective: MissionObjective::Defeat { person: enemy },
                    home,
                    supplies: vec![],
                    compensation: BTreeMap::new(),
                    deadline: SimTime(720_000),
                },
            },
        )
        .unwrap();
        let id = game_missions::missions(&sim.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap();
        accept(&mut sim, id, leader);
        accept(&mut sim, id, actor);
        let mind =
            game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, actor)
                .unwrap();
        mind.personality.honesty = 100;
        mind.relationships.insert(
            leader,
            decisions::Relationship {
                trust: -900,
                affection: 0,
            },
        );
        mind.relationships.insert(
            enemy,
            decisions::Relationship {
                trust: 900,
                affection: 0,
            },
        );
        let battle = sim
            .debug_start_encounter(vec![(sim.host.state.ruler, 0), (actor, 0), (enemy, 1)])
            .unwrap();
        // The root combat loop observes fighters before making each decision.
        let point =
            sim.host.state.combat.as_ref().unwrap().encounters[&battle].fighters[&enemy].position;
        sim.host
            .state
            .combat
            .as_mut()
            .unwrap()
            .encounters
            .get_mut(&battle)
            .unwrap()
            .fighters
            .get_mut(&actor)
            .unwrap()
            .known
            .insert(enemy, point);
        sim.review_mission_combat(battle, actor);
        assert_eq!(
            sim.host.state.combat.as_ref().unwrap().encounters[&battle].fighters[&actor].team,
            1
        );
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&actor]
                .status,
            CompanionStatus::Defected
        );
        assert_eq!(
            *game_spatial::location(
                &sim.host.storage,
                game_ecs::StableId((sim.host.state.people[sim.person_index[&actor]].id).0)
            )
            .expect("location exists"),
            Location::AtArea(sim.host.state.areas[0].id)
        );
    }
    #[test]
    fn worker_order_saves_slices_and_passive_queries_preserve_mission_world() {
        let mut a = fixture();
        let leader = a.host.state.people[5].id;
        let farm = a.host.state.areas[1].id;
        let id = propose(
            &mut a,
            leader,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        accept(&mut a, id, leader);
        let mut b = Simulation::load(
            &a.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        a.advance(SimDuration(540_000)).unwrap();
        for duration in [17_000, 43_000, 93_001, 11_999, 151_007, 223_993] {
            b.observe();
            b.mission_view(leader);
            b.advance(SimDuration(duration)).unwrap();
        }
        assert_eq!(a.canonical_state(), b.canonical_state());
    }
    #[test]
    fn ruler_order_spends_own_action_and_companion_takes_its_own_turn() {
        let mut sim = fixture();
        let leader = sim.host.state.people[5].id;
        let enemy = sim.host.state.people[6].id;
        let ruler = sim.host.state.ruler;
        let farm = sim.host.state.areas[1].id;
        {
            let traits = &mut game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                leader,
            )
            .unwrap()
            .personality;
            traits.ambition = 900;
            traits.sociability = 900;
            traits.risk_tolerance = 900;
            traits.empathy = 100;
        }
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        accept(&mut sim, id, leader);
        let battle = sim
            .debug_start_encounter(vec![(ruler, 0), (leader, 0), (enemy, 1)])
            .unwrap();
        let rules = sim.host.state.combat.as_ref().unwrap().rules.clone();
        let start = Point { x: 1000, y: 1000 };
        let close = Point {
            x: 1000 + (rules.movement / 2) as i64,
            y: 1000,
        };
        let leader_position = Point {
            x: close.x + rules.reach as i64,
            y: 1000,
        };
        {
            let encounter = sim
                .host
                .state
                .combat
                .as_mut()
                .unwrap()
                .encounters
                .get_mut(&battle)
                .unwrap();
            encounter.fighters.get_mut(&ruler).unwrap().position = start;
            encounter.fighters.get_mut(&leader).unwrap().position = leader_position;
            encounter.fighters.get_mut(&enemy).unwrap().position = Point {
                x: leader_position.x + (rules.reach / 2) as i64,
                y: 1000,
            };
        }
        let before = sim.canonical_snapshot().unwrap();
        assert!(
            sim.submit(Submission {
                caller_sequence: sim.next_caller_sequence().unwrap(),
                command: Command::CombatTurn {
                    encounter: battle,
                    movement: None,
                    action: CombatAction::MissionObjective {
                        mission: id,
                        objective: MissionObjective::Defeat { person: enemy }
                    }
                }
            })
            .is_err()
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::CombatTurn {
                encounter: battle,
                movement: Some(close),
                action: CombatAction::MissionObjective {
                    mission: id,
                    objective: MissionObjective::Defeat { person: enemy },
                },
            },
        })
        .unwrap();
        assert!(
            !sim.host
                .events()
                .iter()
                .any(|event| matches!(event.event,EventKind::Struck {actor,..} if actor==ruler))
        );
        assert!(sim.host.events().iter().any(|event|matches!(event.event,EventKind::Struck {actor,target,..} if actor==leader && target==enemy)));
        assert!(sim.host.state.combat.as_ref().unwrap().encounters[&battle].cursor > 1);
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn surrender_and_capture_stop_real_companion_travel() {
        let mut sim = fixture();
        let leader = sim.host.state.people[5].id;
        let ruler = sim.host.state.ruler;
        let forest = sim.host.state.areas[2].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.mission_command(ruler, &MissionAction::Depart { mission: id })
            .unwrap();
        let body = game_physiology::body_mut(&mut sim.host.storage, leader).unwrap();
        body.vitals.health = 70_000;
        game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, leader)
            .unwrap()
            .personality
            .risk_tolerance = 10;
        game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, leader)
            .unwrap()
            .routes
            .clear();
        sim.host
            .state
            .combat
            .as_mut()
            .unwrap()
            .profiles
            .get_mut(&leader)
            .unwrap()
            .initiative = 300;
        let battle = sim
            .debug_start_encounter(vec![(ruler, 0), (leader, 1)])
            .unwrap();
        assert!(
            sim.host.state.combat.as_ref().unwrap().encounters[&battle].fighters[&leader]
                .surrendered
        );
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::CombatTurn {
                encounter: battle,
                movement: None,
                action: CombatAction::Capture { target: leader },
            },
        })
        .unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        assert!(
            sim.host
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&leader)
        );
        assert_eq!(
            *game_spatial::location(
                &sim.host.storage,
                game_ecs::StableId((sim.host.state.people[sim.person_index[&leader]].id).0)
            )
            .expect("location exists"),
            Location::AtArea(sim.host.state.areas[0].id)
        );
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&leader]
                .status,
            CompanionStatus::Captured
        );
        assert!(
            !sim.host
                .events()
                .iter()
                .any(|event| matches!(event.event,EventKind::Departed {actor,..} if actor==leader))
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn unready_guardian_cannot_accept_service_by_bypassing_its_assessment() {
        let mut sim = Simulation::lineage_fixture(29, Execution::default()).unwrap();
        let prototype = fixture();
        sim.host.state.missions = prototype.host.state.missions;
        let __custody: BTreeMap<InventoryId, PersonId> = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .map(|i| {
                (
                    i.id,
                    match i.location {
                        InventoryLocation::Carried(p) => p,
                        InventoryLocation::Area(_) => sim.host.state.ruler,
                    },
                )
            })
            .collect();
        for (id, owner) in __custody {
            game_property::declare_custody(&mut sim.host.storage, id, owner).unwrap();
        }
        sim.host.state.agreements = Some(game_agreements::Runtime {
            last_event: sim.host.next_event() - 1,
            last_observation_event: sim.host.next_event() - 1,
        });
        let guardian = sim.host.state.people[1].id;
        let forest = sim.host.state.areas[2].id;
        let id = propose(
            &mut sim,
            guardian,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![],
        );
        let recruitment = game_missions::mission(&sim.host.storage, id)
            .expect("record exists")
            .members[&guardian]
            .recruitment;
        let before = sim.canonical_snapshot().unwrap();
        assert!(
            sim.mission_command(guardian, &MissionAction::Accept { recruitment })
                .is_err()
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        assert!(!sim.mission_actor_busy(guardian));
    }
    #[test]
    fn local_views_show_declared_choices_without_private_reasoning() {
        let mut sim = fixture();
        let leader = sim.host.state.people[5].id;
        let farm = sim.host.state.areas[1].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.mission_explain(id, leader, "My private trust, risk, and route judgment");
        sim.observe_own_mission(leader, id);
        sim.publish_mission(id);
        assert!(
            sim.mission_view(leader)[0].companions[0]
                .explanation
                .contains("private trust")
        );
        assert!(
            !sim.mission_view(sim.host.state.ruler)[0].companions[0]
                .explanation
                .contains("private trust")
        );
        assert!(
            sim.mission_view(sim.host.state.ruler)[0].companions[0]
                .explanation
                .contains("Accepted")
        );
    }
    #[test]
    fn invalid_remote_tactical_order_preserves_turn_and_body() {
        let mut sim = fixture();
        let leader = sim.host.state.people[5].id;
        let enemy = sim.host.state.people[6].id;
        let ruler = sim.host.state.ruler;
        let farm = sim.host.state.areas[1].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.debug_move_actor(leader, farm).unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        let battle = sim
            .debug_start_encounter(vec![(ruler, 0), (enemy, 1)])
            .unwrap();
        let before = sim.canonical_snapshot().unwrap();
        assert!(
            sim.submit(Submission {
                caller_sequence: sim.next_caller_sequence().unwrap(),
                command: Command::CombatTurn {
                    encounter: battle,
                    movement: Some(Point { x: 5_000, y: 8_000 }),
                    action: CombatAction::MissionObjective {
                        mission: id,
                        objective: MissionObjective::Defeat { person: enemy }
                    }
                }
            })
            .is_err()
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        assert!(
            sim.mission_command(ruler, &MissionAction::Leave { mission: id })
                .is_err()
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }
    #[test]
    fn confinement_requires_actual_binding_and_does_not_release_at_the_named_time() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let target = sim.host.state.people[3].id;
        let home = sim.host.state.areas[0].id;
        sim.interrupt_actor_activity(target);
        *game_spatial::location_mut(
            &mut sim.host.storage,
            game_ecs::StableId((sim.host.state.people[sim.person_index[&target]].id).0),
        )
        .expect("location exists") = Location::AtArea(home);
        let tools: Vec<_> = game_cargo::inventories(&sim.host.storage)
            .filter(|(_, stock)| stock.good == Good::Tool)
            .map(|(id, _)| id)
            .collect();
        for id in tools {
            game_cargo::inventory_mut(&mut sim.host.storage, id)
                .unwrap()
                .access
                .remove(&target);
        }
        let until = SimTime(30_000);
        let id = propose(
            &mut sim,
            actor,
            MissionObjective::Confine {
                person: target,
                until,
            },
            vec![],
        );
        sim.seed_mission_target_report(id, actor, target, home, SimTime(0))
            .unwrap();
        sim.mission_command(actor, &MissionAction::Depart { mission: id })
            .unwrap();
        sim.force_yield(target, actor).unwrap();
        sim.start_restraint(actor, target).unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        assert!(
            sim.host
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
        assert!(
            !game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .succeeded
        );
        sim.debug_actor_command(actor, Command::FreeCaptive { target })
            .unwrap();
        sim.advance(MINUTE).unwrap();
        assert!(
            !sim.host
                .state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .status,
            MissionStatus::Completed
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn a_received_target_report_remains_dated_information_even_when_the_target_is_elsewhere() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let target = sim.host.state.people[3].id;
        let reported_area = sim.host.state.areas[2].id;
        let id = propose(
            &mut sim,
            actor,
            MissionObjective::Defeat { person: target },
            vec![],
        );
        let actual = (*game_spatial::location(
            &sim.host.storage,
            game_ecs::StableId((sim.host.state.people[sim.person_index[&target]].id).0),
        )
        .expect("location exists"))
        .clone();
        sim.seed_mission_target_report(id, actor, target, reported_area, SimTime(0))
            .unwrap();
        assert_eq!(
            game_missions::mission(&sim.host.storage, id)
                .expect("record exists")
                .members[&actor]
                .contacts[&target],
            reported_area
        );
        assert_eq!(
            *game_spatial::location(
                &sim.host.storage,
                game_ecs::StableId((sim.host.state.people[sim.person_index[&target]].id).0)
            )
            .expect("location exists"),
            actual
        );
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.seed_mission_target_report(id, target, target, reported_area, SimTime(0)),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }
    #[test]
    fn an_accepted_confinement_leader_can_capture_on_its_next_turn_without_waiting_a_minute() {
        let mut sim = fixture();
        let leader = sim.host.state.people[5].id;
        let target = sim.host.state.people[3].id;
        let home = sim.host.state.areas[0].id;
        sim.interrupt_actor_activity(leader);
        sim.interrupt_actor_activity(target);
        *game_spatial::location_mut(
            &mut sim.host.storage,
            game_ecs::StableId((sim.host.state.people[sim.person_index[&leader]].id).0),
        )
        .expect("location exists") = Location::AtArea(home);
        *game_spatial::location_mut(
            &mut sim.host.storage,
            game_ecs::StableId((sim.host.state.people[sim.person_index[&target]].id).0),
        )
        .expect("location exists") = Location::AtArea(home);
        sim.debug_add_inventory(
            Good::Binding,
            InventoryLocation::Carried(leader),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([leader]),
        )
        .unwrap();
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Confine {
                person: target,
                until: SimTime(120_000),
            },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.mission_command(leader, &MissionAction::Depart { mission: id })
            .unwrap();
        sim.host.state.combat.as_mut().unwrap().rules.damage = 1;
        sim.debug_start_encounter(vec![(leader, 0), (target, 1)])
            .unwrap();
        sim.host
            .state
            .combat
            .as_mut()
            .unwrap()
            .maps
            .get_mut(&home)
            .unwrap()
            .obstacles
            .clear();
        let current = game_physiology::body(&sim.host.storage, target)
            .expect("body exists")
            .vitals
            .health;
        let incap = sim.host.state.combat.as_ref().unwrap().rules.incapacitation;
        sim.debug_injure(target, current - (incap - 12_000))
            .unwrap();
        sim.advance(SimDuration(24_000)).unwrap();
        assert_eq!(
            sim.host.state.combat.as_ref().unwrap().captivity[&target].captor,
            leader
        );
        let record = game_missions::mission(&sim.host.storage, id)
            .expect("record exists")
            .members[&leader]
            .confinement
            .as_ref()
            .unwrap();
        assert!(record.captured_at.0 < MINUTE.0);
    }
    #[test]
    fn native_mission_save_contains_four_columns_and_no_runtime_entity_maps() {
        let mut sim = fixture();
        let ruler = sim.host.state.ruler;
        let leader = sim.host.state.people[5].id;
        let home = sim.host.state.areas[0].id;
        let away = sim.host.state.areas[1].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit { destination: away },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.mission_command(
            ruler,
            &MissionAction::Objective {
                mission: id,
                objective: MissionObjective::Visit { destination: home },
            },
        )
        .unwrap();
        let saved = sim.save().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            value["state"]["state"]["missions"]
                .as_object()
                .unwrap()
                .len(),
            2
        );
        for column in [
            "missions/mission",
            "missions/recruitment",
            "missions/knowledge",
            "missions/talk",
        ] {
            assert!(
                !value["state"]["components"]["columns"][column]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
        let mut reversed = value.clone();
        for column in [
            "missions/mission",
            "missions/recruitment",
            "missions/knowledge",
            "missions/talk",
        ] {
            reversed["state"]["components"]["columns"][column]
                .as_array_mut()
                .unwrap()
                .reverse();
        }
        assert!(
            Simulation::load(
                &serde_json::to_vec(&reversed).unwrap(),
                Execution::default()
            )
            .is_err()
        );
        let rows = game_missions::missions(&sim.host.storage)
            .map(|(id, row)| (id, row.clone()))
            .collect::<Vec<_>>();
        for (id, row) in rows.into_iter().rev() {
            game_missions::remove_mission(&mut sim.host.storage, id).unwrap();
            game_missions::insert_mission(&mut sim.host.storage, id, row).unwrap();
        }
        let rows = game_missions::recruitments(&sim.host.storage)
            .map(|(id, row)| (id, row.clone()))
            .collect::<Vec<_>>();
        for (id, row) in rows.into_iter().rev() {
            game_missions::remove_recruitment(&mut sim.host.storage, id).unwrap();
            game_missions::insert_recruitment(&mut sim.host.storage, id, row).unwrap();
        }
        let rows = game_missions::knowledges(&sim.host.storage)
            .map(|(id, row)| (id, row.clone()))
            .collect::<Vec<_>>();
        for (id, row) in rows.into_iter().rev() {
            game_missions::remove_knowledge(&mut sim.host.storage, id).unwrap();
            game_missions::insert_knowledge(&mut sim.host.storage, id, row).unwrap();
        }
        assert_eq!(sim.save().unwrap(), saved);
        let replay = Simulation::load(
            &saved,
            Execution {
                workers: 2,
                reverse_jobs: true,
            },
        )
        .unwrap();
        assert_eq!(replay.canonical_state(), sim.canonical_state());
        assert_eq!(replay.save().unwrap(), saved);
        for attack in 0..8 {
            let mut forged = value.clone();
            match attack {
                0 => forged["state"]["state"]["missions"] = serde_json::Value::Null,
                1 => {
                    forged["state"]["components"]["columns"]["missions/mission"][0]["value"]["id"] =
                        serde_json::json!(home.0)
                }
                2 => {
                    forged["state"]["components"]["columns"]["missions/mission"][0]["entity"] =
                        serde_json::json!(home.0)
                }
                3 => {
                    forged["state"]["components"]["columns"]["missions/recruitment"][0]["value"]["mission"] =
                        serde_json::json!(999999)
                }
                4 => {
                    forged["state"]["components"]["columns"]["missions/knowledge"][0]["entity"] =
                        serde_json::json!(home.0)
                }
                5 => {
                    forged["state"]["components"]["columns"]["missions/talk"][0]["value"]["speaker"] =
                        serde_json::json!(leader.0)
                }
                6 => forged["state"]["next_id"] = serde_json::json!(id.0),
                _ => forged["state"]["state"]["missions"]["missions"] = serde_json::json!({}),
            }
            assert!(
                Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default())
                    .is_err(),
                "native mission restore attack {attack}"
            );
            assert_eq!(sim.save().unwrap(), saved);
        }
        let next_id = sim.host.next_id();
        let next_event = sim.host.next_event();
        assert!(
            sim.mission_command(
                ruler,
                &MissionAction::Objective {
                    mission: id,
                    objective: MissionObjective::Visit {
                        destination: AreaId(999999)
                    }
                }
            )
            .is_err()
        );
        assert_eq!(sim.host.next_id(), next_id);
        assert_eq!(sim.host.next_event(), next_event);
        assert_eq!(sim.save().unwrap(), saved);
    }
}
