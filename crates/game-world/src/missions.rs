//! Expedition commitments use real people, cargo, personal maps, and tactical bodies.
use crate::*;

pub use game_missions::{
    Companion, ConfinementObservation, Conversation, Mission, PendingTalk, Recruitment, World,
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
        if !self.person_index.contains_key(&speaker)
            || !self.person_index.contains_key(&target)
            || proposal.objective.target_person() != Some(target)
            || !self.state.areas.iter().any(|place| place.id == area)
            || observed_at > self.state.time
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub(crate) fn seed_mission_target_report(
        &mut self,
        mission: MissionId,
        speaker: PersonId,
        target: PersonId,
        area: AreaId,
        observed_at: SimTime,
    ) -> Result<(), Error> {
        let m = self
            .state
            .missions
            .as_ref()
            .and_then(|world| world.missions.get(&mission))
            .ok_or(Error::Unavailable)?;
        if m.sponsor != speaker {
            return Err(Error::Unavailable);
        }
        self.preflight_mission_target_report(&m.proposal, speaker, target, area, observed_at)?;
        // This is the sponsor's supplied report; it does not query the target's
        // current location. Recruits are present for the proposal conversation.
        for member in self
            .state
            .missions
            .as_mut()
            .unwrap()
            .missions
            .get_mut(&mission)
            .unwrap()
            .members
            .values_mut()
        {
            member.contacts.insert(target, area);
        }
        Ok(())
    }
    pub fn mission_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::combat_fixture(seed, execution)?;
        sim.state.scenario = ScenarioInfo {
            id: "mission-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let custodians = sim
            .state
            .inventories
            .values()
            .map(|stock| {
                (
                    stock.id,
                    match stock.location {
                        InventoryLocation::Carried(person) => person,
                        InventoryLocation::Area(_) => sim.state.ruler,
                    },
                )
            })
            .collect();
        sim.state.agreements = Some(agreements::World {
            ledger: game_agreements::State {
                knowledge: BTreeMap::new(),
                last_observation_event: sim.state.next_event - 1,
                agreements: BTreeMap::new(),
                obligations: BTreeMap::new(),
                amendments: BTreeMap::new(),
                last_event: sim.state.next_event - 1,
                next_search: BTreeMap::new(),
                pending_work: BTreeMap::new(),
            },
            custodians,
        });
        sim.state.missions = Some(World {
            missions: BTreeMap::new(),
            recruitments: BTreeMap::new(),
            known: BTreeMap::new(),
            talks: BTreeMap::new(),
            last_event: sim.state.next_event - 1,
            last_review: sim.state.time,
        });
        Ok(sim)
    }
    fn mission_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        let index = self.person_index.get(&actor).ok_or(Error::Unavailable)?;
        let body = &self
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?
            .actors[&actor];
        if !body.vitals.alive
            || body.vitals.health == 0
            || !self.independent(actor)
            || self.state.combat.as_ref().is_some_and(|w| {
                w.captivity.contains_key(&actor) || body.vitals.health <= w.rules.incapacitation
            })
        {
            return Err(Error::Unavailable);
        }
        match self.state.people[*index].location {
            Location::AtArea(area) => Ok(area),
            _ => Err(Error::Unavailable),
        }
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
                        if let Some(stock) = self.state.inventories.get(id) {
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
            .filter(|id| self.state.areas.iter().any(|a| a.id == *id))
            .collect();
        for supply in supplies {
            if let Some(stock) = self.state.inventories.get(&supply.source) {
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
                self.state
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
            .objective_valid(objective, self.state.time)
    }
    /// Supplies focused body/contact/asset facts; the service owner decides
    /// authority, competing commitments, consent and compensation transitions.
    fn mission_command_facts(
        &self,
        actor: PersonId,
        area: AreaId,
        action: &MissionAction,
    ) -> Result<game_missions::CommandFacts, Error> {
        let world = self.state.missions.as_ref().ok_or(Error::Unavailable)?;
        let mut relevant = BTreeSet::from([actor]);
        let mut terms = Vec::new();
        let mut objective = None;
        let mut supplies = Vec::new();
        let mut duties = BTreeSet::new();
        match action {
            MissionAction::Propose { proposal } => {
                relevant.extend(proposal.recruits.iter().copied());
                relevant.insert(proposal.leader);
                terms.extend(proposal.compensation.values().flatten().cloned());
                objective = Some(&proposal.objective);
                supplies = proposal.supplies.clone();
            }
            MissionAction::Accept { recruitment }
            | MissionAction::Refuse { recruitment }
            | MissionAction::AcceptCounteroffer { recruitment }
            | MissionAction::Counteroffer { recruitment, .. } => {
                let offer = world
                    .recruitments
                    .get(recruitment)
                    .ok_or(Error::Unavailable)?;
                let mission = world
                    .missions
                    .get(&offer.mission)
                    .ok_or(Error::Unavailable)?;
                relevant.extend([offer.person, mission.sponsor]);
                terms = if let MissionAction::Counteroffer { compensation, .. } = action {
                    compensation.clone()
                } else {
                    offer.compensation.clone()
                };
                if self.guardian_mission_duty(actor, mission.id)
                    || self.state.decisions.as_ref().unwrap().minds[&actor]
                        .child_signals
                        .values()
                        .any(|signal| signal.urgency >= 20_000)
                {
                    duties.insert(actor);
                }
            }
            MissionAction::Depart { mission }
            | MissionAction::Retreat { mission }
            | MissionAction::Leave { mission }
            | MissionAction::Objective { mission, .. } => {
                let m = world.missions.get(mission).ok_or(Error::Unavailable)?;
                relevant.insert(m.proposal.leader);
                if let MissionAction::Objective {
                    objective: asked, ..
                } = action
                {
                    objective = Some(asked);
                }
            }
            MissionAction::Report { recipient, .. } => {
                relevant.insert(*recipient);
            }
        }
        let contacts = if matches!(action, MissionAction::Propose { .. }) {
            self.state
                .people
                .iter()
                .filter(|p| p.location == Location::AtArea(area))
                .map(|p| (p.id, area))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .take(64)
                .collect()
        } else {
            BTreeMap::new()
        };
        Ok(game_missions::CommandFacts {
            now: self.state.time,
            area,
            available: relevant
                .into_iter()
                .filter(|id| {
                    self.mission_area(*id).ok() == Some(area)
                        && (!matches!(action, MissionAction::Propose { .. })
                            || !self.in_combat(*id))
                })
                .collect(),
            duties,
            contacts,
            assets: self.mission_assets(objective, &terms, &supplies),
        })
    }
    pub(crate) fn mission_command(
        &mut self,
        actor: PersonId,
        action: &MissionAction,
    ) -> Result<(), Error> {
        let area = self.mission_area(actor)?;
        if self.in_combat(actor) {
            return Err(Error::Unavailable);
        }
        let facts = self.mission_command_facts(actor, area, action)?;
        let mut ids = game_missions::MissionIds::default();
        let mut next = self.state.next_id;
        if let MissionAction::Propose { proposal } = action {
            let people: BTreeSet<_> = proposal
                .recruits
                .iter()
                .copied()
                .chain([proposal.leader])
                .collect();
            next.checked_add(1 + people.len() as u64)
                .ok_or(Error::Overflow)?;
            ids.mission = Some(MissionId(next));
            next += 1;
            for person in people {
                ids.recruitments.insert(person, RecruitmentId(next));
                next += 1;
            }
        } else if let MissionAction::Accept { recruitment } = action {
            let offer = self
                .state
                .missions
                .as_ref()
                .unwrap()
                .recruitments
                .get(recruitment)
                .ok_or(Error::Unavailable)?;
            if !offer.compensation.is_empty() {
                ids.backing = Some(AgreementId(next));
            }
        }
        let prepared = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .prepare_command(actor, action, &facts, &ids)?;
        prepared.check(self.state.missions.as_ref().unwrap())?;
        if let Some((listener, conversation)) = prepared.conversation.clone() {
            return self.start_mission_talk(actor, listener, conversation);
        }
        let backing = prepared
            .consent
            .as_ref()
            .map(|consent| self.prepare_service_agreement(consent))
            .transpose()?;
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
        // Both owner plans have passed all guards and effect preflights before
        // either authoritative record set changes. No whole-world rollback.
        prepared.commit(self.state.missions.as_mut().unwrap())?;
        if let Some((plan, after)) = backing {
            self.apply_prepared_agreement(actor, plan, after)?;
        } else {
            self.state.next_id = next;
        }
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
        let trust = prepared.trust.clone();
        prepared.commit(self.state.missions.as_mut().ok_or(Error::Unavailable)?)?;
        self.publish_mission_effects(events, notices);
        for effect in trust {
            let relationship = self
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&effect.observer)
                .unwrap()
                .relationships
                .entry(effect.leader)
                .or_insert(decisions::Relationship {
                    trust: 0,
                    affection: 0,
                });
            relationship.trust = (relationship.trust + effect.delta).clamp(-1000, 1000);
        }
        Ok(())
    }
    fn mission_membership(&self, actor: PersonId) -> Option<MissionId> {
        self.state.missions.as_ref()?.membership(actor)
    }
    pub(crate) fn mission_actor_busy(&self, actor: PersonId) -> bool {
        let Some(id) = self.mission_membership(actor) else {
            return false;
        };
        if self.guardian_mission_duty(actor, id) {
            return false;
        }
        let body = &self.state.physical.as_ref().unwrap().actors[&actor];
        body.vitals.alive
            && body.vitals.health > self.state.physical.as_ref().unwrap().rules.max_health / 2
            && body.vitals.hunger < self.state.physical.as_ref().unwrap().rules.meal_relief
            && body.vitals.fatigue < self.state.physical.as_ref().unwrap().rules.fatigue_limit
            && !self.combat_restricts(actor)
            && !self.state.decisions.as_ref().unwrap().minds[&actor]
                .child_signals
                .values()
                .any(|signal| signal.urgency >= 20_000)
    }
    fn start_mission_talk(
        &mut self,
        speaker: PersonId,
        listener: PersonId,
        conversation: Conversation,
    ) -> Result<(), Error> {
        let area = self.mission_area(speaker)?;
        if self.in_combat(speaker)
            || self.in_combat(listener)
            || self.mission_area(listener).ok() != Some(area)
        {
            return Err(Error::Unavailable);
        }
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(60_000)
                .ok_or(Error::Overflow)?,
        );
        self.interrupt_actor_activity(speaker);
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&speaker)
            .unwrap()
            .activity = Activity::MissionTalk { completes };
        self.state.missions.as_mut().unwrap().talks.insert(
            speaker,
            PendingTalk {
                speaker,
                listener,
                area,
                completes,
                conversation,
            },
        );
        Ok(())
    }
    pub(crate) fn cancel_mission_talk(&mut self, actor: PersonId) {
        let Some(world) = &self.state.missions else {
            return;
        };
        let affected: Vec<_> = world
            .talks
            .values()
            .filter(|talk| talk.speaker == actor || talk.listener == actor)
            .cloned()
            .collect();
        for talk in affected {
            self.state
                .missions
                .as_mut()
                .unwrap()
                .talks
                .remove(&talk.speaker);
            for actor in [talk.speaker] {
                if matches!(self.state.physical.as_ref().unwrap().actors[&actor].activity,Activity::MissionTalk {completes}if completes==talk.completes)
                {
                    self.state
                        .physical
                        .as_mut()
                        .unwrap()
                        .actors
                        .get_mut(&actor)
                        .unwrap()
                        .activity = Activity::Idle;
                }
            }
        }
    }
    pub(crate) fn mission_boundary(&self, limit: SimTime) -> SimTime {
        let Some(w) = &self.state.missions else {
            return limit;
        };
        w.talks
            .values()
            .map(|talk| talk.completes)
            .chain(
                w.missions
                    .values()
                    .filter(|m| {
                        !matches!(
                            m.status,
                            MissionStatus::Completed
                                | MissionStatus::Failed
                                | MissionStatus::Cancelled
                        )
                    })
                    .map(|m| m.proposal.deadline),
            )
            .chain(
                w.missions
                    .values()
                    .filter_map(|m| match m.proposal.objective {
                        MissionObjective::Patrol { until, .. }
                        | MissionObjective::Confine { until, .. }
                            if !matches!(
                                m.status,
                                MissionStatus::Completed
                                    | MissionStatus::Failed
                                    | MissionStatus::Cancelled
                            ) =>
                        {
                            Some(until)
                        }
                        _ => None,
                    }),
            )
            .filter(|at| *at > self.state.time)
            .min()
            .map_or(limit, |next| next.min(limit))
    }

    pub(crate) fn mission_view(&self, actor: PersonId) -> Vec<MissionView> {
        self.state
            .missions
            .as_ref()
            .and_then(|w| w.known.get(&actor))
            .map_or_else(Vec::new, |known| {
                known.values().rev().take(64).cloned().collect()
            })
    }
    fn mission_can_observe(&self, observer: PersonId, person: PersonId) -> bool {
        if observer == person {
            return true;
        }
        let observer_location = self.resolved_location(observer);
        let location = self.resolved_location(person);
        if observer_location != location || !matches!(location, Location::AtArea(_)) {
            return false;
        }
        let Some(w) = &self.state.combat else {
            return true;
        };
        let encounter = w
            .encounters
            .values()
            .find(|e| e.active && e.fighters.contains_key(&observer));
        if let Some(e) = encounter {
            let Some(other) = e.fighters.get(&person) else {
                return false;
            };
            return w
                .map(e.map)
                .is_some_and(|map| map.clear(e.fighters[&observer].position, other.position));
        }
        !self.in_combat(person)
    }
    fn mission_observation_facts(
        &self,
        observer: PersonId,
        id: MissionId,
    ) -> game_missions::ObservationFacts {
        let m = &self.state.missions.as_ref().unwrap().missions[&id];
        let relevant = m
            .members
            .keys()
            .copied()
            .chain([m.sponsor])
            .collect::<BTreeSet<_>>();
        let visible_locations = relevant
            .iter()
            .filter(|p| self.mission_can_observe(observer, **p))
            .map(|p| (*p, self.state.people[self.person_index[p]].location.clone()))
            .collect();
        let heard = relevant
            .into_iter()
            .filter(|p| {
                self.mission_area(*p).is_ok()
                    && self.mission_area(observer).is_ok()
                    && self.transport_same_place(observer, *p)
            })
            .collect();
        game_missions::ObservationFacts {
            now: self.state.time,
            visible_locations,
            heard,
        }
    }
    fn publish_mission(&mut self, id: MissionId) {
        let m = &self.state.missions.as_ref().unwrap().missions[&id];
        let leader = m.proposal.leader;
        let people = m
            .members
            .keys()
            .copied()
            .chain([m.sponsor])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|p| {
                self.mission_area(leader).is_ok()
                    && self.mission_area(*p).is_ok()
                    && self.transport_same_place(*p, leader)
            })
            .collect::<Vec<_>>();
        for person in people {
            self.observe_own_mission(person, id);
        }
    }
    fn observe_own_mission(&mut self, actor: PersonId, id: MissionId) {
        let facts = self.mission_observation_facts(actor, id);
        if let Ok(prepared) = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .prepare_observation(actor, id, &facts)
        {
            prepared
                .commit(self.state.missions.as_mut().unwrap())
                .expect("same personal observation snapshot");
        }
    }
    fn recruitment_input(&self, actor: PersonId, m: &Mission) -> game_missions::RecruitmentInput {
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        let body = &self.state.physical.as_ref().unwrap().actors[&actor];
        let social_disruption = if self
            .mission_area(actor)
            .ok()
            .is_some_and(|area| self.mission_area(m.sponsor).ok() == Some(area))
        {
            self.invocation_social_penalty(m.sponsor) / 3
        } else {
            0
        };
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
                .any(|signal| signal.urgency >= 20_000),
            hungry: body.vitals.hunger >= self.state.physical.as_ref().unwrap().rules.meal_relief,
        }
    }

    fn recruitment_score(&self, actor: PersonId, m: &Mission, offer: &Recruitment) -> i64 {
        game_missions::recruitment_score(self.recruitment_input(actor, m), &offer.compensation)
    }

    fn mission_control_member(&mut self, id: MissionId, actor: PersonId) {
        if actor == self.state.ruler || self.in_combat(actor) {
            return;
        }
        let mut m = self.state.missions.as_ref().unwrap().missions[&id].clone();
        if let Some(view) = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .known
            .get(&actor)
            .and_then(|known| known.get(&id))
        {
            m.status = view.status;
        }
        let member = &m.members[&actor];
        if member.status != CompanionStatus::Accepted {
            return;
        }
        let Ok(area) = self.mission_area(actor) else {
            return;
        };
        if self.guardian_mission_duty(actor, id) {
            if let Some(action) = self.state.missions.as_ref().unwrap().service_withdrawal(
                actor,
                id,
                &self.mission_service_motives(actor, id),
            ) {
                let _ = self.mission_command(actor, &action);
            }
            self.mission_explain(
                id,
                actor,
                "Withdrew to search for and care for the new or unready incarnation",
            );
            return;
        }
        let body = &self.state.physical.as_ref().unwrap().actors[&actor];
        let rules = &self.state.physical.as_ref().unwrap().rules;
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        if self.army_actor_talking(actor) || self.realm_actor_busy(actor) {
            return;
        }
        if matches!(
            body.activity,
            Activity::MissionTalk { .. }
                | Activity::ArmyTalk { .. }
                | Activity::ArmyApproach { .. }
                | Activity::Eat { .. }
                | Activity::Load { .. }
                | Activity::Tell { .. }
                | Activity::Care { .. }
                | Activity::Lesson { .. }
                | Activity::WritingMemory { .. }
                | Activity::ReadingMemory { .. }
                | Activity::TeachingMemory { .. }
                | Activity::MovingRecord { .. }
                | Activity::Escape { .. }
                | Activity::Restrain { .. }
                | Activity::Unbind { .. }
        ) {
            return;
        }
        if mind
            .child_signals
            .values()
            .any(|signal| signal.urgency >= 20_000)
        {
            return;
        }
        if body.vitals.hunger >= rules.meal_relief {
            if let Some(food)=mind.knowledge.values().filter(|k|k.good==Good::Food&&k.accessible&&k.quantity>=rules.meal_size&&k.area==area).min_by_key(|k|(!matches!(self.state.inventories[&k.inventory].location,InventoryLocation::Carried(carrier)if carrier==actor),k.inventory)){let source=food.inventory;let _=self.debug_actor_command(actor,Command::Eat {source});return;}
            return;
        }
        if body.vitals.fatigue >= rules.fatigue_limit / 2
            || body.vitals.health < rules.max_health / 2
        {
            let _ = self.debug_actor_command(actor, Command::Rest);
            return;
        }
        let motives = self.mission_service_motives(actor, id);
        if let Some(action) = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .service_withdrawal(actor, id, &motives)
        {
            let _ = self.mission_command(actor, &action);
            return;
        }
        if matches!(
            m.status,
            MissionStatus::Preparing | MissionStatus::Traveling
        ) {
            for supply in m.proposal.supplies.iter().filter(|s| s.carrier == actor) {
                let Some(known) = mind.knowledge.get(&supply.source) else {
                    self.mission_explain(
                        id,
                        actor,
                        "Waiting for personally known supply permission",
                    );
                    return;
                };
                let delivery = game_logistics::Delivery {
                    requirement: game_logistics::Requirement {
                        good: known.good,
                        quantity: supply.quantity,
                        source: Some(supply.source),
                    },
                    destination: game_logistics::Destination::Contact(actor),
                    remaining: supply.quantity,
                    last_receipt: 0,
                };
                let goal = game_logistics::KnownDestination {
                    destination: delivery.destination,
                    location: self.delivery_observed_location(actor),
                    observed_at: self.state.time,
                    accessible: true,
                    room: None,
                    direct_transfer: false,
                };
                let tags = member
                    .supply_cargo
                    .get(&supply.source)
                    .into_iter()
                    .flat_map(|tags| tags.iter())
                    .map(|(id, q)| (*id, Quantity(*q)))
                    .collect();
                let step = self.plan_delivery_with_source_cargo(
                    actor,
                    &delivery,
                    &goal,
                    supply.quantity,
                    &tags,
                );
                if matches!(step, game_logistics::Step::Arrived { .. }) {
                    continue;
                }
                if let Some(command) = self.delivery_command(actor, step) {
                    let _ = self.debug_actor_command(actor, command);
                } else {
                    self.mission_explain(
                        id,
                        actor,
                        "Waiting for personally known qualified provisions and a usable route",
                    );
                }
                return;
            }
            if m.status == MissionStatus::Preparing {
                let _ = self.debug_actor_command(actor, Command::Rest);
                return;
            }
        }
        if m.status != MissionStatus::Returning
            && let MissionObjective::Deliver {
                source,
                destination,
                quantity,
            } = member.directive
        {
            let Some(known) = mind.knowledge.get(&source) else {
                self.mission_explain(id, actor, "The delivery source is not personally known");
                return;
            };
            let Some(goal) = self.delivery_known_inventory_destination(actor, destination) else {
                self.mission_explain(
                    id,
                    actor,
                    "The delivery destination is not personally known",
                );
                return;
            };
            let delivery = game_logistics::Delivery {
                requirement: game_logistics::Requirement {
                    good: known.good,
                    quantity,
                    source: Some(source),
                },
                destination: game_logistics::Destination::Inventory(destination),
                remaining: Quantity(quantity.0.saturating_sub(member.delivered)),
                last_receipt: 0,
            };
            let tags = member
                .cargo
                .iter()
                .map(|(id, q)| (*id, Quantity(*q)))
                .collect();
            let step =
                self.plan_delivery_with_source_cargo(actor, &delivery, &goal, Quantity(1), &tags);
            if let Some(command) = self.delivery_command(actor, step) {
                let _ = self.debug_actor_command(actor, command);
            } else {
                self.mission_explain(id, actor, "Waiting for personally known qualified cargo, receiving room or a usable route");
            }
            return;
        }
        let destination = if m.status == MissionStatus::Returning {
            Some(m.proposal.home)
        } else {
            self.mission_destination(actor, member, &member.directive)
        };
        if let Some(destination) = destination {
            if destination != area {
                if let MissionObjective::Rescue { person, return_to } = member.directive
                    && destination == return_to
                    && self.state.people[self.person_index[&person]].location
                        == Location::AtArea(area)
                    && self.mission_area(person).is_ok()
                    && person != self.state.ruler
                {
                    let released = &self.state.decisions.as_ref().unwrap().minds[&person];
                    let v = &self.state.physical.as_ref().unwrap().actors[&person].vitals;
                    let trusts = released.relationships.get(&actor).map_or(0, |r| r.trust) >= 0;
                    if trusts
                        && released.personality.empathy >= 300
                        && v.hunger < self.state.physical.as_ref().unwrap().rules.meal_relief
                    {
                        let _ = self.start_movement(person, return_to);
                    }
                }
                if self.start_movement(actor, destination).is_err() {
                    self.mission_explain(
                        id,
                        actor,
                        "No personally known usable route to the objective",
                    );
                }
                return;
            }
            match member.directive {
                MissionObjective::Defeat { person } => {
                    if self.state.people[self.person_index[&person]].location
                        == Location::AtArea(area)
                        && self.state.physical.as_ref().unwrap().actors[&person]
                            .vitals
                            .alive
                        && self.can_fight(person)
                    {
                        self.start_mission_encounter(id, actor, person);
                    }
                }
                MissionObjective::Confine { person, until } => {
                    if self.state.people[self.person_index[&person]].location
                        != Location::AtArea(area)
                    {
                        return;
                    }
                    let captive = self
                        .state
                        .combat
                        .as_ref()
                        .and_then(|world| world.captivity.get(&person));
                    if let Some(captive) = captive {
                        if m.members.contains_key(&captive.captor) {
                            if self.state.time >= until {
                                let _ = self.debug_actor_command(
                                    actor,
                                    Command::FreeCaptive { target: person },
                                );
                            } else {
                                let _ = self.debug_actor_command(actor, Command::Rest);
                            }
                        }
                    } else if self.state.time < until
                        && self.state.physical.as_ref().unwrap().actors[&person]
                            .vitals
                            .alive
                        && self.start_restraint(actor, person).is_err()
                        && self.can_fight(person)
                    {
                        self.start_mission_encounter(id, actor, person);
                    }
                }
                MissionObjective::Rescue { person, .. } => {
                    if self.state.people[self.person_index[&person]].location
                        == Location::AtArea(area)
                        && self
                            .state
                            .combat
                            .as_ref()
                            .is_some_and(|w| w.captivity.contains_key(&person))
                    {
                        let _ = self
                            .debug_actor_command(actor, Command::FreeCaptive { target: person });
                    }
                }
                _ => {
                    let _ = self.debug_actor_command(actor, Command::Rest);
                }
            }
        } else {
            self.mission_explain(
                id,
                actor,
                "The party lacks a credible observed location for its objective",
            );
        }
    }
    pub(crate) fn mission_react_to_encounter_end(&mut self, encounter: EncounterId) {
        let Some(encounter) = self
            .state
            .combat
            .as_ref()
            .and_then(|world| world.encounters.get(&encounter))
        else {
            return;
        };
        if encounter.active {
            return;
        }
        let Some(world) = &self.state.missions else {
            return;
        };
        let responses: Vec<_> = world
            .missions
            .values()
            .filter(|mission| {
                matches!(
                    mission.status,
                    MissionStatus::Traveling | MissionStatus::Operating
                )
            })
            .flat_map(|mission| {
                mission
                    .members
                    .values()
                    .filter(|member| {
                        member.status == CompanionStatus::Accepted
                            && encounter.fighters.contains_key(&member.person)
                            && matches!(member.directive, MissionObjective::Confine { .. })
                    })
                    .map(|member| (mission.id, member.person))
            })
            .collect();
        for (mission, actor) in responses {
            self.mission_control_member(mission, actor);
        }
    }
    fn mission_explain(&mut self, id: MissionId, actor: PersonId, message: &str) {
        self.state
            .missions
            .as_mut()
            .unwrap()
            .missions
            .get_mut(&id)
            .unwrap()
            .members
            .get_mut(&actor)
            .unwrap()
            .explanation = message.into();
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
        let body = &self.state.physical.as_ref()?.actors[&target];
        let captivity = self
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
                .state
                .combat
                .as_ref()
                .map_or(0, |w| w.rules.incapacitation),
            captor: captivity.map(|c| c.captor),
            restricted: self.combat_restricts(target),
            observed_at: self.state.time,
        })
    }
    fn mission_objective_knowledge(
        &self,
        actor: PersonId,
        objective: &MissionObjective,
    ) -> game_missions::ObjectiveKnowledge {
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
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
    fn mission_destination(
        &self,
        actor: PersonId,
        member: &Companion,
        objective: &MissionObjective,
    ) -> Option<AreaId> {
        game_missions::objective_destination(
            member,
            objective,
            &self.mission_objective_knowledge(actor, objective),
            self.mission_area(actor).ok(),
        )
    }
    fn start_mission_encounter(&mut self, id: MissionId, actor: PersonId, target: PersonId) {
        let area = match self.state.people[self.person_index[&actor]].location {
            Location::AtArea(area) => area,
            _ => return,
        };
        if let Some(encounter) = self
            .state
            .combat
            .as_ref()
            .unwrap()
            .encounters
            .values()
            .find(|e| e.active && e.fighters.contains_key(&target))
            .map(|e| e.id)
        {
            let _ = self.start_army_approach(actor, id, encounter);
            return;
        }
        let linked = self.army_allied_missions(id);
        let allies: BTreeSet<_> = linked
            .iter()
            .flat_map(|mission| {
                self.state.missions.as_ref().unwrap().missions[mission]
                    .members
                    .values()
                    .map(|member| (*mission, member))
            })
            .filter(|(mission, member)| {
                member.person != self.state.ruler
                    && member.status == CompanionStatus::Accepted
                    && self.state.people[self.person_index[&member.person]].location
                        == Location::AtArea(area)
                    && self.can_fight(member.person)
                    && !self.in_combat(member.person)
                    && member.directive.requires_force()
                    && member.directive.target_person() == Some(target)
                    && (*mission == id
                        || self.mission_force_willing(member.person, *mission, target))
            })
            .map(|(_, member)| member.person)
            .collect();
        let participants = allies
            .into_iter()
            .map(|p| (p, 0))
            .chain([(target, 1)])
            .collect();
        let _ = self.debug_start_encounter(participants);
    }
    pub(crate) fn mission_force_willing(
        &self,
        actor: PersonId,
        id: MissionId,
        target: PersonId,
    ) -> bool {
        let Some(m) = self
            .state
            .missions
            .as_ref()
            .and_then(|w| w.missions.get(&id))
        else {
            return false;
        };
        let Some(member) = m.members.get(&actor) else {
            return false;
        };
        if member.status != CompanionStatus::Accepted
            || !member.directive.requires_force()
            || member.directive.target_person() != Some(target)
            || self.guardian_mission_duty(actor, id)
        {
            return false;
        }
        let mut proposed = m.clone();
        proposed.proposal.objective = member.directive.clone();
        let offer = &self.state.missions.as_ref().unwrap().recruitments[&member.recruitment];
        actor == self.state.ruler || self.recruitment_score(actor, &proposed, offer) >= 0
    }
    pub(crate) fn mission_combat_order(
        &mut self,
        actor: PersonId,
        id: MissionId,
        objective: MissionObjective,
    ) -> Result<(), Error> {
        self.validate_mission_combat_order(actor, id, &objective)?;
        let leader = self.state.missions.as_ref().unwrap().missions[&id]
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
        let m = &self.state.missions.as_ref().unwrap().missions[&id];
        if m.proposal.leader != leader {
            return Err(Error::Unavailable);
        }
        let mut offered = m.clone();
        offered.proposal.objective = objective.clone();
        let offer =
            &self.state.missions.as_ref().unwrap().recruitments[&m.members[&leader].recruitment];
        Ok(self.recruitment_score(leader, &offered, offer))
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
        if self.state.missions.as_ref().unwrap().missions[&id]
            .proposal
            .leader
            != leader
            || self.in_combat(leader)
        {
            return Err(Error::Unavailable);
        }
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
        let area = self.mission_area(leader)?;
        let m = &self.state.missions.as_ref().unwrap().missions[&id];
        let heard = m
            .members
            .values()
            .filter(|p| {
                p.status == CompanionStatus::Accepted
                    && self.mission_area(p.person).ok() == Some(area)
                    && self.transport_same_place(p.person, leader)
            })
            .map(|p| {
                let choice = if p.person == self.state.ruler {
                    game_missions::ServiceChoice::DeclaredAcceptance
                } else {
                    game_missions::ServiceChoice::Personal(self.recruitment_input(p.person, m))
                };
                (p.person, choice)
            })
            .collect();
        self.state.missions.as_ref().unwrap().prepare_order(
            id,
            leader,
            objective,
            &game_missions::OrderFacts {
                now: self.state.time,
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
            .commit(self.state.missions.as_mut().unwrap())
            .expect("preflighted communicated objective");
        self.publish_mission_effects(events, notices);
    }
}

impl Simulation {
    /// Read actual committed events. A promised delivery never satisfies an objective.
    pub(crate) fn reconcile_missions(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.missions else {
            return Ok(());
        };
        self.ensure_event_capacity(
            world
                .talks
                .len()
                .checked_add(
                    world
                        .missions
                        .len()
                        .checked_mul(36)
                        .ok_or(Error::Overflow)?,
                )
                .ok_or(Error::Overflow)?,
        )?;
        let events = self
            .state
            .events
            .iter()
            .skip(world.last_event as usize)
            .map(|e| (e.id, e.time, e.kind.clone()))
            .collect::<Vec<_>>();
        let mut local: BTreeMap<AreaId, BTreeSet<PersonId>> = BTreeMap::new();
        for person in &self.state.people {
            if let Location::AtArea(area) = person.location {
                local.entry(area).or_default().insert(person.id);
            }
        }
        let interested = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .missions
            .values()
            .flat_map(|m| m.members.keys().copied())
            .collect::<BTreeSet<_>>();
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
                    let observers = contacts
                        .intersection(&interested)
                        .copied()
                        .filter(|p| self.mission_can_observe(*p, actor))
                        .collect();
                    Some(MissionReceipt::Arrived {
                        actor,
                        area,
                        observers,
                        contacts,
                    })
                }
                EventKind::Transferred {
                    actor,
                    source,
                    destination,
                    quantity,
                } => {
                    let destination_carrier =
                        self.state
                            .inventories
                            .get(&destination)
                            .and_then(|i| match i.location {
                                InventoryLocation::Carried(person) => Some(person),
                                _ => None,
                            });
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
                let prepared = self
                    .state
                    .missions
                    .as_ref()
                    .unwrap()
                    .prepare_receipt(sequence, &receipt)?;
                prepared.commit(self.state.missions.as_mut().unwrap())?;
            }
        }
        let talks: Vec<_> = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .talks
            .values()
            .filter(|talk| talk.completes <= self.state.time)
            .cloned()
            .collect();
        for talk in talks {
            let valid = self.mission_area(talk.speaker).ok() == Some(talk.area)
                && self.mission_area(talk.listener).ok() == Some(talk.area)
                && self.state.physical.as_ref().unwrap().actors[&talk.speaker].activity
                    == Activity::MissionTalk {
                        completes: talk.completes,
                    };
            self.state
                .missions
                .as_mut()
                .unwrap()
                .talks
                .remove(&talk.speaker);
            if matches!(self.state.physical.as_ref().unwrap().actors[&talk.speaker].activity,Activity::MissionTalk { completes } if completes == talk.completes)
            {
                self.state
                    .physical
                    .as_mut()
                    .unwrap()
                    .actors
                    .get_mut(&talk.speaker)
                    .unwrap()
                    .activity = Activity::Idle;
            }
            if !valid {
                continue;
            }
            match talk.conversation {
                Conversation::Objective {
                    mission,
                    leader,
                    objective,
                } => {
                    if self
                        .validate_mission_combat_order(talk.speaker, mission, &objective)
                        .is_ok()
                    {
                        self.apply_mission_order(mission, leader, objective);
                    }
                }
                Conversation::Retreat { mission, leader } => {
                    if !matches!(
                        self.state.missions.as_ref().unwrap().missions[&mission].status,
                        MissionStatus::Traveling | MissionStatus::Operating
                    ) {
                        continue;
                    }
                    let home = self.state.missions.as_ref().unwrap().missions[&mission]
                        .proposal
                        .home;
                    if let Ok(prepared) = self.prepare_mission_order(
                        mission,
                        leader,
                        &MissionObjective::Visit { destination: home },
                        true,
                    ) {
                        let events = prepared.events.clone();
                        let notices = prepared.notices.clone();
                        prepared.commit(self.state.missions.as_mut().unwrap())?;
                        self.publish_mission_effects(events, notices);
                    }
                }
                Conversation::Report {
                    recipient,
                    snapshot,
                } => {
                    let mission = snapshot.id;
                    if let Some(prepared) = self.state.missions.as_ref().unwrap().prepare_report(
                        talk.speaker,
                        recipient,
                        &snapshot,
                        self.state.time,
                    )? {
                        prepared.commit(self.state.missions.as_mut().unwrap())?;
                    }
                    self.emit(EventKind::MissionReported {
                        mission,
                        speaker: talk.speaker,
                        recipient,
                    });
                }
            }
        }
        let ids: Vec<_> = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .missions
            .keys()
            .copied()
            .collect();
        for id in ids {
            self.reconcile_mission_outcome(id);
            let actors: Vec<_> = self.state.missions.as_ref().unwrap().missions[&id]
                .members
                .keys()
                .copied()
                .collect();
            for actor in actors {
                self.observe_own_mission(actor, id);
            }
            self.publish_mission(id);
        }
        self.state.missions.as_mut().unwrap().last_event = self.state.next_event - 1;
        Ok(())
    }
    fn reconcile_mission_outcome(&mut self, id: MissionId) {
        let m = &self.state.missions.as_ref().unwrap().missions[&id];
        let bodies = m
            .members
            .keys()
            .map(|p| {
                let body = &self.state.physical.as_ref().unwrap().actors[p];
                (
                    *p,
                    game_missions::MemberBody {
                        alive: body.vitals.alive,
                        captive: self
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
        let receivers = m
            .members
            .keys()
            .copied()
            .chain([m.sponsor])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|p| {
                self.mission_area(m.proposal.leader).is_ok()
                    && self.mission_area(*p).is_ok()
                    && self.transport_same_place(*p, m.proposal.leader)
            })
            .collect();
        let facts = game_missions::OutcomeFacts {
            now: self.state.time,
            bodies,
            leader_knowledge,
            receivers,
        };
        let prepared = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .prepare_outcome(id, &facts)
            .expect("actual mission body references");
        self.commit_mission_plan(prepared)
            .expect("joint mission event preflight");
    }
    pub(crate) fn review_missions(&mut self) -> Result<(), Error> {
        let Some(w) = &self.state.missions else {
            return Ok(());
        };
        if self.state.time == w.last_review || !self.state.time.0.is_multiple_of(60_000) {
            return Ok(());
        }
        self.preflight_missions()?;
        self.state.missions.as_mut().unwrap().last_review = self.state.time;
        let offers: Vec<_> = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .recruitments
            .values()
            .cloned()
            .collect();
        for offer in offers {
            if offer.person == self.state.ruler {
                continue;
            }
            let m = self.state.missions.as_ref().unwrap().missions[&offer.mission].clone();
            if m.status != MissionStatus::Preparing
                || m.members[&offer.person].status != CompanionStatus::Offered
                || offer.countered
                || self
                    .mission_area(offer.person)
                    .ok()
                    .is_none_or(|area| self.mission_area(m.sponsor).ok() != Some(area))
            {
                continue;
            }
            let input = self.recruitment_input(offer.person, &m);
            let area = self.mission_area(offer.person).unwrap();
            let inventory = self.state.people[self.person_index[&offer.person]].inventory;
            let known = self.state.decisions.as_ref().unwrap().minds[&offer.person]
                .knowledge
                .values()
                .cloned()
                .collect::<Vec<_>>();
            let Some(action) = self
                .state
                .missions
                .as_ref()
                .unwrap()
                .recruitment_response(offer.id, input, area, inventory, &known)
            else {
                continue;
            };
            // An unavailable material promise remains an unanswered offer; never invent stock.
            let _ = self.mission_command(offer.person, &action);
        }
        let missions: Vec<_> = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .missions
            .values()
            .cloned()
            .collect();
        for m in missions {
            if m.status == MissionStatus::Preparing
                && m.proposal.leader != self.state.ruler
                && m.members[&m.proposal.leader].status == CompanionStatus::Accepted
                && m.members
                    .values()
                    .all(|person| person.status != CompanionStatus::Offered)
            {
                let _ = self
                    .mission_command(m.proposal.leader, &MissionAction::Depart { mission: m.id });
            }
            for person in m.members.keys() {
                if self.mission_membership(*person) == Some(m.id) {
                    self.mission_control_member(m.id, *person);
                }
            }
        }
        Ok(())
    }
    pub(crate) fn mission_allocation_bound(&self) -> u64 {
        self.state.missions.as_ref().map_or(0, |w| {
            w.missions
                .values()
                .flat_map(|m| m.members.values())
                .filter(|member| {
                    matches!(
                        member.status,
                        CompanionStatus::Accepted | CompanionStatus::Offered
                    )
                })
                .count() as u64
                * 12
        })
    }
    pub(crate) fn mission_event_bound(&self) -> usize {
        self.state.missions.as_ref().map_or(0, |w| {
            w.missions
                .values()
                .map(|m| m.members.len() * 32 + 4)
                .sum::<usize>()
                + w.talks.len() * 33
        })
    }
    pub(crate) fn preflight_missions(&self) -> Result<(), Error> {
        let Some(w) = &self.state.missions else {
            return Ok(());
        };
        let people = w
            .missions
            .values()
            .filter(|m| {
                !matches!(
                    m.status,
                    MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
                )
            })
            .map(|m| m.members.len() as u64)
            .sum::<u64>();
        self.state
            .next_id
            .checked_add(people.checked_mul(12).ok_or(Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(
            people
                .checked_mul(32)
                .and_then(|n| n.checked_add(w.talks.len() as u64 * 2 + w.missions.len() as u64 * 4))
                .ok_or(Error::Overflow)?
                .try_into()
                .map_err(|_| Error::Overflow)?,
        )
    }
}

impl Simulation {
    pub(crate) fn available_mission_commands(&self, actor: PersonId) -> Vec<Command> {
        let Ok(area) = self.mission_area(actor) else {
            return Vec::new();
        };
        if self.in_combat(actor) {
            return Vec::new();
        }
        let Some(w) = &self.state.missions else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        for view in self.mission_view(actor) {
            let Some(m) = w.missions.get(&view.id) else {
                continue;
            };
            for offer in &view.recruitments {
                if offer.status != CompanionStatus::Offered {
                    continue;
                }
                if offer.person == actor && self.mission_area(m.sponsor).ok() == Some(area) {
                    if !offer.countered
                        && view.status == MissionStatus::Preparing
                        && !self.guardian_mission_duty(actor, m.id)
                        && !self.state.decisions.as_ref().unwrap().minds[&actor]
                            .child_signals
                            .values()
                            .any(|signal| signal.urgency >= 20_000)
                    {
                        actions.push(MissionAction::Accept {
                            recruitment: offer.id,
                        });
                    }
                    actions.push(MissionAction::Refuse {
                        recruitment: offer.id,
                    });
                } else if actor == m.sponsor
                    && offer.countered
                    && self.mission_area(offer.person).ok() == Some(area)
                {
                    actions.push(MissionAction::AcceptCounteroffer {
                        recruitment: offer.id,
                    });
                }
            }
            if m.members
                .get(&actor)
                .is_some_and(|member| member.status == CompanionStatus::Accepted)
            {
                actions.push(MissionAction::Leave { mission: m.id });
            }
            if self.mission_area(m.proposal.leader).ok() == Some(area)
                && (actor == m.sponsor || actor == m.proposal.leader)
            {
                if view.status == MissionStatus::Preparing
                    && m.members[&m.proposal.leader].status == CompanionStatus::Accepted
                {
                    actions.push(MissionAction::Depart { mission: m.id });
                }
                if matches!(
                    view.status,
                    MissionStatus::Traveling | MissionStatus::Operating
                ) {
                    actions.push(MissionAction::Retreat { mission: m.id });
                }
            }
            for recipient in self
                .state
                .people
                .iter()
                .filter(|p| p.id != actor && p.location == Location::AtArea(area))
                .map(|p| p.id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(64)
            {
                if self.mission_area(recipient).is_ok() {
                    actions.push(MissionAction::Report {
                        mission: m.id,
                        recipient,
                    });
                }
            }
        }
        actions
            .into_iter()
            .map(|action| Command::Mission { action })
            .collect()
    }
    fn mission_service_motives(
        &self,
        actor: PersonId,
        id: MissionId,
    ) -> game_missions::ServiceMotives {
        let m = &self.state.missions.as_ref().unwrap().missions[&id];
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        let agreement = self.state.missions.as_ref().unwrap().recruitments
            [&m.members[&actor].recruitment]
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
        if actor == self.state.ruler {
            return;
        }
        let Some(id) = self.mission_membership(actor) else {
            return;
        };
        let Some(w) = &self.state.combat else {
            return;
        };
        let Some(e) = w.encounters.get(&encounter) else {
            return;
        };
        let Some(f) = e.fighters.get(&actor) else {
            return;
        };
        let map = w.map(e.map).unwrap();
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        let opponents = f
            .known
            .keys()
            .filter_map(|person| {
                e.fighters
                    .get(person)
                    .filter(|other| {
                        other.team != f.team
                            && !other.withdrawn
                            && !other.surrendered
                            && map.clear(f.position, other.position)
                    })
                    .map(|other| {
                        (
                            *person,
                            game_missions::ObservedOpponent {
                                team: other.team,
                                trust: mind.relationships.get(person).map_or(-1000, |r| r.trust),
                            },
                        )
                    })
            })
            .collect();
        let facts = game_missions::DefectionFacts {
            motives: self.mission_service_motives(actor, id),
            own_team: f.team,
            opponents,
        };
        let Ok(Some(prepared)) = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .prepare_defection(actor, id, &facts)
        else {
            return;
        };
        let effect = prepared.allegiance.expect("prepared defection allegiance");
        if self.ensure_event_capacity(prepared.events.len()).is_err()
            || prepared
                .check(self.state.missions.as_ref().unwrap())
                .is_err()
        {
            return;
        }
        // No action or observation occurs between the guarded service commit
        // and the actual named fighter's team change in this exclusive turn.
        let current = &self.state.combat.as_ref().unwrap().encounters[&encounter];
        if current.fighters[&actor].team != facts.own_team
            || current.fighters[&effect.opponent].team != effect.team
        {
            return;
        }
        self.state
            .combat
            .as_mut()
            .unwrap()
            .encounters
            .get_mut(&encounter)
            .unwrap()
            .fighters
            .get_mut(&actor)
            .unwrap()
            .team = effect.team;
        self.commit_mission_plan(prepared)
            .expect("preflighted fighter allegiance");
        self.observe_own_mission(actor, id);
    }
    pub(crate) fn mission_combat_action(
        &mut self,
        encounter: EncounterId,
        actor: PersonId,
    ) -> Option<(Option<Point>, CombatAction)> {
        // The ruler takes personal turns; an invoked life retains its own controller.
        if actor == self.state.ruler {
            return None;
        }
        let id = self.mission_membership(actor)?;
        let m = &self.state.missions.as_ref()?.missions[&id];
        let member = &m.members[&actor];
        let w = self.state.combat.as_ref()?;
        let e = w.encounters.get(&encounter)?;
        let f = e.fighters.get(&actor)?;
        let map = w.map(e.map)?;
        let mind = &self.state.decisions.as_ref()?.minds[&actor];
        let body = &self.state.physical.as_ref()?.actors[&actor];
        let known_status = self
            .state
            .missions
            .as_ref()?
            .known
            .get(&actor)
            .and_then(|known| known.get(&id))
            .map(|view| view.status);
        let retreat = game_missions::combat_retreat_wanted(
            body.vitals.health,
            self.state.physical.as_ref()?.rules.max_health,
            mind.personality.risk_tolerance,
            known_status,
        );
        if retreat {
            let exit = map
                .exits
                .iter()
                .filter(|(area, _)| decisions::known_path(&mind.routes, e.area, *area).is_ok())
                .filter(|(_, point)| map.route(f.position, *point).is_some())
                .min_by_key(|(area, point)| (point_distance(f.position, *point), *area));
            if let Some((destination, exit)) = exit {
                let movement = mission_combat_step(f.position, *exit, w.rules.movement, map);
                let position = movement.unwrap_or(f.position);
                let action = if point_distance(position, *exit) <= u128::from(w.rules.reach).pow(2)
                    && map.clear(position, *exit)
                {
                    CombatAction::Retreat {
                        destination: *destination,
                    }
                } else {
                    CombatAction::Defend
                };
                return Some((movement, action));
            }
            return Some((None, CombatAction::Surrender));
        }
        let target = match member.directive {
            MissionObjective::Defeat { person }
            | MissionObjective::Rescue { person, .. }
            | MissionObjective::Confine { person, .. }
                if f.known.contains_key(&person) =>
            {
                Some(person)
            }
            _ => f
                .known
                .keys()
                .filter(|person| {
                    e.fighters.get(person).is_some_and(|other| {
                        other.team != f.team && map.clear(f.position, other.position)
                    })
                })
                .min_by_key(|person| (point_distance(f.position, f.known[person]), **person))
                .copied(),
        };
        let Some(target) = target else {
            return Some((
                self.combat_search_step(encounter, actor),
                CombatAction::Defend,
            ));
        };
        let point = f.known[&target];
        let movement = map.navigation_step(
            f.position,
            point,
            w.rules.movement,
            w.rules.reach.saturating_sub(2),
        );
        let visible = map.clear(f.position, e.fighters[&target].position);
        if !visible
            && (movement.is_none()
                || map.clear(f.position, point)
                    && point_distance(f.position, point) <= u128::from(w.rules.reach).pow(2))
        {
            return Some((
                self.combat_search_step(encounter, actor),
                CombatAction::Defend,
            ));
        }
        let from = movement.unwrap_or(f.position);
        let reachable = visible
            && point_distance(from, point) <= u128::from(w.rules.reach).pow(2)
            && map.clear(from, point);
        let action = if reachable
            && matches!(member.directive, MissionObjective::Rescue { person, .. } if person == target)
            && self.can_release(actor, target)
        {
            CombatAction::Release { target }
        } else if reachable
            && matches!(member.directive, MissionObjective::Confine { person, .. } if person == target)
            && (e.fighters[&target].surrendered || !self.can_fight(target))
            && self.binding_stock(actor).is_some()
            && !w.captivity.contains_key(&target)
        {
            CombatAction::Capture { target }
        } else if reachable
            && e.fighters[&target].position == point
            && map.clear(from, e.fighters[&target].position)
            && e.fighters[&target].team != f.team
            && !e.fighters[&target].surrendered
        {
            CombatAction::Attack { target }
        } else {
            CombatAction::Defend
        };
        Some((movement, action))
    }
    pub(crate) fn mission_reinforcements(&self) -> Vec<(EncounterId, PersonId, u32, AreaId)> {
        let Some(w) = &self.state.missions else {
            return Vec::new();
        };
        let mut candidates = Vec::new();
        for m in w.missions.values() {
            for member in m.members.values().filter(|p| {
                p.status == CompanionStatus::Accepted
                    && self.mission_membership(p.person) == Some(m.id)
                    && p.person != self.state.ruler
                    && self.can_fight(p.person)
                    && !self.in_combat(p.person)
            }) {
                let Some(from) = member.last_leg_from else {
                    continue;
                };
                let Ok(area) = self.mission_area(member.person) else {
                    continue;
                };
                if !self.state.events.iter().rev().take_while(|event|event.time==self.state.time).any(|event|event.time==self.state.time && matches!(event.kind,EventKind::Arrived {actor,area:arrived} if actor==member.person && arrived==area)) {continue;}
                for encounter in self
                    .state
                    .combat
                    .as_ref()
                    .into_iter()
                    .flat_map(|w| w.encounters.values())
                    .filter(|e| e.active && e.area == area)
                {
                    let linked = self.army_allied_missions(m.id);
                    if let Some(ally) = encounter.fighters.values().find(|f| {
                        linked.iter().any(|id| {
                            w.missions[id].members.get(&f.actor).is_some_and(|p| {
                                p.status == CompanionStatus::Accepted
                                    && (*id == m.id
                                        || member.directive.target_person().is_some_and(|target| {
                                            self.mission_force_willing(member.person, m.id, target)
                                                && encounter
                                                    .fighters
                                                    .get(&target)
                                                    .is_some_and(|enemy| enemy.team != f.team)
                                        }))
                            })
                        }) && !f.withdrawn
                            && !f.surrendered
                    }) {
                        candidates.push((encounter.id, member.person, ally.team, from));
                        break;
                    }
                }
            }
        }
        candidates
    }
}
fn point_distance(a: Point, b: Point) -> u128 {
    u128::from(a.x.abs_diff(b.x)).pow(2) + u128::from(a.y.abs_diff(b.y)).pow(2)
}
fn mission_combat_step(from: Point, to: Point, limit: u64, map: &combat::Map) -> Option<Point> {
    map.navigation_step(from, to, limit, 0)
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(w) = &s.missions else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent expedition commitments".into());
    let people: BTreeSet<_> = s.people.iter().map(|p| p.id).collect();
    let areas: BTreeSet<_> = s.areas.iter().map(|a| a.id).collect();
    let physical = s.physical.as_ref().ok_or_else(fail)?;
    let agreements = s.agreements.as_ref().ok_or_else(fail)?;
    if s.decisions.is_none()
        || s.combat.is_none()
        || w.last_event >= s.next_event
        || w.last_review > s.time
    {
        return Err(fail());
    }
    let objective_valid = |objective: &MissionObjective| match objective {
        MissionObjective::Visit { destination } => areas.contains(destination),
        MissionObjective::Deliver {
            source,
            destination,
            quantity,
        } => {
            quantity.0 > 0
                && s.inventories
                    .get(source)
                    .zip(s.inventories.get(destination))
                    .is_some_and(|(a, b)| {
                        a.id != b.id && a.good == b.good && quantity.0 <= b.capacity.0
                    })
        }
        MissionObjective::Defeat { person } | MissionObjective::Confine { person, .. } => {
            people.contains(person)
        }
        MissionObjective::Rescue { person, return_to } => {
            people.contains(person) && areas.contains(return_to)
        }
        MissionObjective::Patrol {
            areas: selected, ..
        } => {
            !selected.is_empty()
                && selected.len() <= 16
                && selected.iter().all(|a| areas.contains(a))
        }
    };
    for (id, m) in &w.missions {
        let roster: BTreeSet<_> = m
            .proposal
            .recruits
            .iter()
            .copied()
            .chain([m.proposal.leader])
            .collect();
        if *id != m.id
            || id.0 == 0
            || id.0 >= s.next_id
            || !people.contains(&m.sponsor)
            || !areas.contains(&m.proposal.home)
            || !objective_valid(&m.proposal.objective)
            || !(1..=32).contains(&m.members.len())
            || roster != m.members.keys().copied().collect()
            || m.created_at > s.time
            || m.proposal.deadline <= m.created_at
            || m.departed_at
                .is_some_and(|at| at < m.created_at || at > s.time)
            || m.proposal.supplies.len() > 32
        {
            return Err(fail());
        }
        for (person, terms) in &m.proposal.compensation {
            if !m.members.contains_key(person)
                || terms.len() > 8
                || terms.iter().any(|term| {
                    term.amount() == 0
                        || term.parties()[0] == term.parties()[1]
                        || term
                            .parties()
                            .iter()
                            .any(|p| *p != m.sponsor && *p != *person)
                        || match term {
                            AgreementTerm::Delivery {
                                source,
                                destination,
                                quantity,
                                ..
                            } => !s
                                .inventories
                                .get(source)
                                .zip(s.inventories.get(destination))
                                .is_some_and(|(a, b)| {
                                    a.id != b.id && a.good == b.good && quantity.0 <= b.capacity.0
                                }),
                            AgreementTerm::Access { inventory, .. } => {
                                !s.inventories.contains_key(inventory)
                            }
                            _ => true,
                        }
                })
            {
                return Err(fail());
            }
        }
        for supply in &m.proposal.supplies {
            if !m.members.contains_key(&supply.carrier)
                || !s.inventories.contains_key(&supply.source)
                || supply.quantity.0 == 0
                || supply.quantity > physical.rules.carrying_capacity
            {
                return Err(fail());
            }
        }
        for (person, member) in &m.members {
            let offer = w.recruitments.get(&member.recruitment).ok_or_else(fail)?;
            if *person != member.person
                || !people.contains(person)
                || offer.mission != *id
                || offer.person != *person
                || member.received_at > s.time
                || !objective_valid(&member.directive)
                || member.visited.iter().any(|a| !areas.contains(a))
                || member
                    .contacts
                    .iter()
                    .any(|(p, a)| !people.contains(p) || !areas.contains(a))
                || member
                    .last_leg_from
                    .is_some_and(|area| !areas.contains(&area))
                || member.explanation.len() > 4096
                || member.declared_explanation.len() > 4096
            {
                return Err(fail());
            }
            if let Some(record) = &member.confinement
                && (member.directive.target_person() != Some(record.person)
                    || !matches!(member.directive, MissionObjective::Confine { .. })
                    || record.captured_at > s.time
                    || record
                        .released_at
                        .is_some_and(|at| at > s.time || at < record.captured_at))
            {
                return Err(fail());
            }
            if let MissionObjective::Deliver { quantity, .. } = member.directive
                && member.delivered > quantity.0
            {
                return Err(fail());
            }
            let mut attributed = BTreeMap::<InventoryId, u128>::new();
            for (source, tags) in &member.supply_cargo {
                let supply = m
                    .proposal
                    .supplies
                    .iter()
                    .filter(|supply| supply.carrier == *person && supply.source == *source)
                    .max_by_key(|supply| supply.quantity)
                    .ok_or_else(fail)?;
                let good = s.inventories.get(source).ok_or_else(fail)?.good;
                let mut total = 0u128;
                if tags.is_empty() {
                    return Err(fail());
                }
                for (inventory, quantity) in tags {
                    if *quantity == 0
                        || !s.inventories.get(inventory).is_some_and(|stock| {
                            stock.good == good
                                && stock.location == InventoryLocation::Carried(*person)
                        })
                    {
                        return Err(fail());
                    }
                    total += u128::from(*quantity);
                    *attributed.entry(*inventory).or_default() += u128::from(*quantity);
                }
                if total > u128::from(supply.quantity.0) {
                    return Err(fail());
                }
            }
            if attributed
                .iter()
                .any(|(inventory, q)| *q > u128::from(s.inventories[inventory].quantity.0))
            {
                return Err(fail());
            }
            for (inventory, quantity) in &member.cargo {
                if *quantity == 0
                    || !s.inventories.get(inventory).is_some_and(|i| {
                        i.location == InventoryLocation::Carried(*person)
                            && i.quantity.0 >= *quantity
                    })
                {
                    return Err(fail());
                }
            }
        }
    }
    for (id, offer) in &w.recruitments {
        let m = w.missions.get(&offer.mission).ok_or_else(fail)?;
        if *id != offer.id
            || id.0 == 0
            || id.0 >= s.next_id
            || !m
                .members
                .get(&offer.person)
                .is_some_and(|person| person.recruitment == *id)
            || offer.compensation.len() > 8
            || offer.agreement.is_some_and(|id| {
                !agreements
                    .agreements
                    .get(&id)
                    .is_some_and(|a| a.parties == BTreeSet::from([m.sponsor, offer.person]))
            })
        {
            return Err(fail());
        }
        for term in &offer.compensation {
            if term.amount() == 0
                || term.parties()[0] == term.parties()[1]
                || term
                    .parties()
                    .iter()
                    .any(|p| *p != m.sponsor && *p != offer.person)
                || !matches!(
                    term,
                    AgreementTerm::Delivery { .. } | AgreementTerm::Access { .. }
                )
            {
                return Err(fail());
            }
            match term {
                AgreementTerm::Delivery {
                    source,
                    destination,
                    ..
                } if !s.inventories.contains_key(source)
                    || !s.inventories.contains_key(destination) =>
                {
                    return Err(fail());
                }
                AgreementTerm::Access { inventory, .. }
                    if !s.inventories.contains_key(inventory) =>
                {
                    return Err(fail());
                }
                _ => {}
            }
        }
    }
    for (observer, views) in &w.known {
        if !people.contains(observer) {
            return Err(fail());
        }
        for (id, view) in views {
            if *id != view.id
                || !w.missions.contains_key(id)
                || view.observed_at > s.time
                || !people.contains(&view.sponsor)
                || !people.contains(&view.leader)
                || !areas.contains(&view.home)
                || !objective_valid(&view.objective)
                || view.companions.len() > 32
                || view.recruitments.len() > 32
                || view.reported_by.is_some_and(|p| !people.contains(&p))
            {
                return Err(fail());
            }
            for companion in &view.companions {
                if !people.contains(&companion.person) || companion.observed_at > s.time {
                    return Err(fail());
                }
            }
            for recruitment in &view.recruitments {
                if !w
                    .recruitments
                    .get(&recruitment.id)
                    .is_some_and(|offer| offer.person == recruitment.person && offer.mission == *id)
                {
                    return Err(fail());
                }
            }
        }
    }
    let mut occupied = BTreeSet::new();
    for (speaker, talk) in &w.talks {
        if *speaker != talk.speaker
            || !people.contains(speaker)
            || !people.contains(&talk.listener)
            || !areas.contains(&talk.area)
            || talk.completes <= s.time
        {
            return Err(fail());
        }
        if !occupied.insert(talk.speaker)
            || physical.actors[&talk.speaker].activity
                != (Activity::MissionTalk {
                    completes: talk.completes,
                })
            || ![talk.speaker, talk.listener].iter().all(|actor| {
                s.people
                    .iter()
                    .any(|p| p.id == *actor && p.location == Location::AtArea(talk.area))
            })
        {
            return Err(fail());
        }
        match &talk.conversation {
            Conversation::Objective {
                mission,
                leader,
                objective,
            } => {
                if !w.missions.get(mission).is_some_and(|m| {
                    m.proposal.leader == *leader && (m.sponsor == *speaker || *leader == *speaker)
                }) || !objective_valid(objective)
                {
                    return Err(fail());
                }
            }
            Conversation::Retreat { mission, leader } => {
                if !w.missions.get(mission).is_some_and(|m| {
                    m.proposal.leader == *leader && (m.sponsor == *speaker || *leader == *speaker)
                }) {
                    return Err(fail());
                }
            }
            Conversation::Report {
                recipient,
                snapshot,
            } => {
                if *recipient != talk.listener
                    || !w.missions.contains_key(&snapshot.id)
                    || snapshot.observed_at > s.time
                {
                    return Err(fail());
                }
            }
        }
    }
    for (actor, body) in &physical.actors {
        if matches!(body.activity, Activity::MissionTalk { .. }) != occupied.contains(actor) {
            return Err(fail());
        }
    }
    Ok(())
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
        let world = self.state.missions.as_ref().ok_or(Error::Unavailable)?;
        world.validate_order(actor, id, objective, &facts)?;
        self.ensure_event_capacity(world.missions[&id].members.len() + 1)
    }

    pub(crate) fn guardian_mission_duty(&self, actor: PersonId, id: MissionId) -> bool {
        if self.state.people[self.person_index[&actor]].role != Role::Guardian {
            return false;
        }
        let Some(lineage) = &self.state.lineage else {
            return false;
        };
        let m = &self.state.missions.as_ref().unwrap().missions[&id];
        lineage.current() != Some(m.sponsor)
            || self.state.decisions.as_ref().unwrap().minds[&actor]
                .child_signals
                .get(&m.sponsor)
                .is_some_and(|signal| signal.urgency > 0 || signal.hydration_urgency > 0)
            || !lineage
                .guardians
                .get(&actor)
                .and_then(|g| g.assessments.get(&m.sponsor))
                .is_some_and(|a| a.ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Simulation {
        let mut sim = Simulation::mission_fixture(29, Execution::default()).unwrap();
        for person in sim.state.people.clone() {
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
        let home = sim.state.areas[0].id;
        sim.mission_command(
            sim.state.ruler,
            &MissionAction::Propose {
                proposal: MissionProposal {
                    leader,
                    recruits: BTreeSet::new(),
                    objective,
                    home,
                    supplies,
                    compensation: BTreeMap::new(),
                    deadline: SimTime(sim.state.time.0 + 720_000),
                },
            },
        )
        .unwrap();
        *sim.state
            .missions
            .as_ref()
            .unwrap()
            .missions
            .keys()
            .next_back()
            .unwrap()
    }
    fn accept(sim: &mut Simulation, id: MissionId, person: PersonId) {
        let recruitment =
            sim.state.missions.as_ref().unwrap().missions[&id].members[&person].recruitment;
        sim.mission_command(person, &MissionAction::Accept { recruitment })
            .unwrap();
    }
    fn public_food(sim: &Simulation) -> InventoryId {
        sim.state
            .inventories
            .values()
            .find(|i| i.good == Good::Food && matches!(i.location, InventoryLocation::Area(_)))
            .unwrap()
            .id
    }
    #[test]
    fn supplies_and_route_legs_are_real_and_the_realm_keeps_working() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let worker = sim.state.people[6].id;
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
        let forest = sim.state.areas[2].id;
        let food = public_food(&sim);
        let carried = sim.state.people[sim.person_index[&leader]].inventory;
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
        assert_eq!(sim.state.inventories[&carried].quantity, Quantity(0));
        sim.advance(MINUTE).unwrap();
        assert!(matches!(
            sim.state.physical.as_ref().unwrap().actors[&leader].activity,
            Activity::Load { .. }
        ));
        assert_eq!(sim.state.inventories[&carried].quantity, Quantity(0));
        sim.advance(MINUTE).unwrap();
        assert_eq!(sim.state.inventories[&carried].quantity, Quantity(2_000));
        assert!(matches!(
            sim.state.people[sim.person_index[&leader]].location,
            Location::InTransit { .. }
        ));
        sim.advance(SimDuration(600_000)).unwrap();
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].status,
            MissionStatus::Completed
        );
        assert!(
            sim.state
                .events
                .iter()
                .any(|e| matches!(e.kind,EventKind::Produced {actor,..}if actor==worker))
        );
        assert!(
            sim.state
                .events
                .iter()
                .any(|e| matches!(e.kind,EventKind::Departed {actor,..}if actor==leader))
        );
        assert!(sim.state.events.iter().any(
            |e| matches!(e.kind,EventKind::Arrived {actor,area}if actor==leader&&area==forest)
        ));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn promised_provisions_do_not_count_unmarked_same_good_cargo() {
        let mut sim = fixture();
        let actor = sim.state.people[5].id;
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
        let opening = sim.state.inventories[&source].quantity;
        let forest = sim.state.areas[2].id;
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
            matches!(sim.state.physical.as_ref().unwrap().actors[&actor].activity, Activity::Load { source: from, .. } if from == source)
        );
        assert_eq!(sim.state.inventories[&source].quantity, opening);
        assert!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor]
                .supply_cargo
                .is_empty()
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state.inventories[&source].quantity,
            Quantity(opening.0 - 2000)
        );
        assert_eq!(sim.state.inventories[&unqualified].quantity, Quantity(2000));
        let tags = &sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].supply_cargo
            [&source];
        assert_eq!(tags.values().sum::<u64>(), 2000);
        assert!(matches!(
            sim.state.people[sim.person_index[&actor]].location,
            Location::InTransit { .. }
        ));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn autonomous_delivery_uses_promised_origin_and_preserves_unqualified_cargo() {
        let mut sim = fixture();
        let actor = sim.state.people[5].id;
        sim.debug_set_personality(
            actor,
            decisions::Personality::industrious(),
            decisions::Aspiration::Independence,
        )
        .unwrap();
        let area = sim.state.areas[0].id;
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
        let opening = sim.state.inventories[&source].quantity;
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
        assert_eq!(sim.state.inventories[&destination].quantity, Quantity(0));
        assert_eq!(sim.state.inventories[&source].quantity, opening);
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
        assert_eq!(sim.state, sliced.state);
        assert_eq!(
            sim.state.inventories[&source].quantity,
            Quantity(opening.0 - 1000)
        );
        assert_eq!(sim.state.inventories[&destination].quantity, Quantity(1000));
        assert_eq!(sim.state.inventories[&carried].quantity, Quantity(2000));
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].delivered,
            1000
        );
    }
    #[test]
    fn npc_refuses_danger_and_counteroffers_for_a_marginal_job() {
        let mut sim = fixture();
        let recruit = sim.state.people[5].id;
        let enemy = sim.state.people[6].id;
        {
            let traits = &mut sim
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&recruit)
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
            sim.state.missions.as_ref().unwrap().missions[&id].members[&recruit].status,
            CompanionStatus::Refused
        );
        let farm = sim.state.areas[1].id;
        let id = propose(
            &mut sim,
            recruit,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        sim.advance(MINUTE).unwrap();
        let offer = sim.state.missions.as_ref().unwrap().recruitments
            [&sim.state.missions.as_ref().unwrap().missions[&id].members[&recruit].recruitment]
            .clone();
        assert!(offer.countered);
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&recruit].status,
            CompanionStatus::Offered
        );
        sim.mission_command(
            sim.state.ruler,
            &MissionAction::AcceptCounteroffer {
                recruitment: offer.id,
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        let offer = &sim.state.missions.as_ref().unwrap().recruitments[&offer.id];
        assert!(offer.agreement.is_some());
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&recruit].status,
            CompanionStatus::Accepted
        );
        let obligations = &sim.state.agreements.as_ref().unwrap().obligations;
        assert!(
            obligations
                .values()
                .any(|obligation| obligation.performed == 0)
        );
    }
    #[test]
    fn absent_sponsor_receives_no_telepathic_progress_and_reports_take_time() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        sim.debug_set_personality(
            leader,
            decisions::Personality::industrious(),
            decisions::Aspiration::Independence,
        )
        .unwrap();
        let forest = sim.state.areas[2].id;
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
        let ruler = sim.state.ruler;
        assert_eq!(
            sim.state.missions.as_ref().unwrap().known[&ruler][&id].status,
            MissionStatus::Traveling
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].status,
            MissionStatus::Returning
        );
        let snapshot = sim.mission_view(leader)[0].clone();
        assert!(
            snapshot.observed_at
                > sim.state.missions.as_ref().unwrap().known[&ruler][&id].observed_at
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
            sim.state.people[sim.person_index[&leader]].location,
            Location::AtArea(sim.state.areas[0].id)
        );
        // A cached account can be reported to a bystander who never joined the party.
        let recipient = sim.state.people[8].id;
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
        let leader = sim.state.people[5].id;
        let forest = sim
            .debug_add_disconnected_area("Unseen far clearing")
            .unwrap();
        let intermediary = sim.state.areas[2].id;
        sim.state.areas[2].links.push(Link {
            destination: forest,
            duration: MINUTE,
        });
        sim.state.areas.last_mut().unwrap().links.push(Link {
            destination: intermediary,
            duration: MINUTE,
        });
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&leader)
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
            sim.state.people[sim.person_index[&leader]].location,
            Location::AtArea(sim.state.areas[0].id)
        );
        assert!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&leader]
                .explanation
                .contains("personally known")
        );
        assert!(
            !sim.state
                .events
                .iter()
                .any(|e| matches!(e.kind,EventKind::Departed {actor,..}if actor==leader))
        );
    }
    #[test]
    fn delivery_credit_requires_the_conserved_origin_cargo() {
        let mut sim = fixture();
        let actor = sim.state.ruler;
        let area = sim.state.areas[0].id;
        let source = sim
            .state
            .inventories
            .values()
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
        let carried = sim
            .state
            .inventories
            .values()
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
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].delivered,
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
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].delivered,
            0
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].cargo[&carried],
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
            sim.state.inventories[&destination].quantity,
            Quantity(1_000)
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].delivered,
            1_000
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].status,
            MissionStatus::Completed
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn changed_objectives_take_local_time_and_may_be_refused() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let enemy = sim.state.people[6].id;
        let farm = sim.state.areas[1].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        accept(&mut sim, id, leader);
        let old = sim.state.missions.as_ref().unwrap().missions[&id].members[&leader]
            .directive
            .clone();
        sim.mission_command(
            sim.state.ruler,
            &MissionAction::Objective {
                mission: id,
                objective: MissionObjective::Defeat { person: enemy },
            },
        )
        .unwrap();
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&leader].directive,
            old
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&leader].status,
            CompanionStatus::Withdrawn
        );
        assert!(
            !sim.state
                .events
                .iter()
                .any(|e| matches!(e.kind, EventKind::EncounterStarted { .. }))
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn known_enemy_relationship_can_motivate_an_actual_defection() {
        let mut sim = fixture();
        let actor = sim.state.people[5].id;
        let enemy = sim.state.people[6].id;
        let leader = sim.state.people[7].id;
        let home = sim.state.areas[0].id;
        sim.mission_command(
            sim.state.ruler,
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
        let id = *sim
            .state
            .missions
            .as_ref()
            .unwrap()
            .missions
            .keys()
            .next_back()
            .unwrap();
        accept(&mut sim, id, leader);
        accept(&mut sim, id, actor);
        let mind = sim
            .state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
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
            .debug_start_encounter(vec![(sim.state.ruler, 0), (actor, 0), (enemy, 1)])
            .unwrap();
        // The root combat loop observes fighters before making each decision.
        let point =
            sim.state.combat.as_ref().unwrap().encounters[&battle].fighters[&enemy].position;
        sim.state
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
            sim.state.combat.as_ref().unwrap().encounters[&battle].fighters[&actor].team,
            1
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].status,
            CompanionStatus::Defected
        );
        assert_eq!(
            sim.state.people[sim.person_index[&actor]].location,
            Location::AtArea(sim.state.areas[0].id)
        );
    }
    #[test]
    fn worker_order_saves_slices_and_passive_queries_preserve_mission_world() {
        let mut a = fixture();
        let leader = a.state.people[5].id;
        let farm = a.state.areas[1].id;
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
        assert_eq!(a.state, b.state);
    }
    #[test]
    fn ruler_order_spends_own_action_and_companion_takes_its_own_turn() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let enemy = sim.state.people[6].id;
        let ruler = sim.state.ruler;
        let farm = sim.state.areas[1].id;
        {
            let traits = &mut sim
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&leader)
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
        let rules = sim.state.combat.as_ref().unwrap().rules.clone();
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
        let before = sim.state.clone();
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
        assert_eq!(sim.state, before);
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
            !sim.state
                .events
                .iter()
                .any(|event| matches!(event.kind,EventKind::Struck {actor,..} if actor==ruler))
        );
        assert!(sim.state.events.iter().any(|event|matches!(event.kind,EventKind::Struck {actor,target,..} if actor==leader && target==enemy)));
        assert!(sim.state.combat.as_ref().unwrap().encounters[&battle].cursor > 1);
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn surrender_and_capture_stop_real_companion_travel() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let ruler = sim.state.ruler;
        let forest = sim.state.areas[2].id;
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
        let body = sim
            .state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&leader)
            .unwrap();
        body.vitals.health = 70_000;
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&leader)
            .unwrap()
            .personality
            .risk_tolerance = 10;
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&leader)
            .unwrap()
            .routes
            .clear();
        sim.state
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
            sim.state.combat.as_ref().unwrap().encounters[&battle].fighters[&leader].surrendered
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
            sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&leader)
        );
        assert_eq!(
            sim.state.people[sim.person_index[&leader]].location,
            Location::AtArea(sim.state.areas[0].id)
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&leader].status,
            CompanionStatus::Captured
        );
        assert!(
            !sim.state
                .events
                .iter()
                .any(|event| matches!(event.kind,EventKind::Departed {actor,..} if actor==leader))
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn unready_guardian_cannot_accept_service_by_bypassing_its_assessment() {
        let mut sim = Simulation::lineage_fixture(29, Execution::default()).unwrap();
        let prototype = fixture();
        sim.state.missions = prototype.state.missions;
        sim.state.agreements = Some(agreements::World {
            ledger: game_agreements::State {
                agreements: BTreeMap::new(),
                obligations: BTreeMap::new(),
                amendments: BTreeMap::new(),
                last_event: sim.state.next_event - 1,
                next_search: BTreeMap::new(),
                pending_work: BTreeMap::new(),
                knowledge: BTreeMap::new(),
                last_observation_event: sim.state.next_event - 1,
            },
            custodians: sim
                .state
                .inventories
                .values()
                .map(|i| {
                    (
                        i.id,
                        match i.location {
                            InventoryLocation::Carried(p) => p,
                            InventoryLocation::Area(_) => sim.state.ruler,
                        },
                    )
                })
                .collect(),
        });
        let guardian = sim.state.people[1].id;
        let forest = sim.state.areas[2].id;
        let id = propose(
            &mut sim,
            guardian,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![],
        );
        let recruitment =
            sim.state.missions.as_ref().unwrap().missions[&id].members[&guardian].recruitment;
        let before = sim.state.clone();
        assert!(
            sim.mission_command(guardian, &MissionAction::Accept { recruitment })
                .is_err()
        );
        assert_eq!(sim.state, before);
        assert!(!sim.mission_actor_busy(guardian));
    }
    #[test]
    fn local_views_show_declared_choices_without_private_reasoning() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let farm = sim.state.areas[1].id;
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
            !sim.mission_view(sim.state.ruler)[0].companions[0]
                .explanation
                .contains("private trust")
        );
        assert!(
            sim.mission_view(sim.state.ruler)[0].companions[0]
                .explanation
                .contains("Accepted")
        );
    }
    #[test]
    fn invalid_remote_tactical_order_preserves_turn_and_body() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let enemy = sim.state.people[6].id;
        let ruler = sim.state.ruler;
        let farm = sim.state.areas[1].id;
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
        let before = sim.state.clone();
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
        assert_eq!(sim.state, before);
        assert!(
            sim.mission_command(ruler, &MissionAction::Leave { mission: id })
                .is_err()
        );
        assert_eq!(sim.state, before);
    }
    #[test]
    fn confinement_requires_actual_binding_and_does_not_release_at_the_named_time() {
        let mut sim = fixture();
        let actor = sim.state.ruler;
        let target = sim.state.people[3].id;
        let home = sim.state.areas[0].id;
        sim.interrupt_actor_activity(target);
        sim.state.people[sim.person_index[&target]].location = Location::AtArea(home);
        for stock in sim
            .state
            .inventories
            .values_mut()
            .filter(|stock| stock.good == Good::Tool)
        {
            stock.access.remove(&target);
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
            sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
        assert!(!sim.state.missions.as_ref().unwrap().missions[&id].succeeded);
        sim.debug_actor_command(actor, Command::FreeCaptive { target })
            .unwrap();
        sim.advance(MINUTE).unwrap();
        assert!(
            !sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&target)
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].status,
            MissionStatus::Completed
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn a_received_target_report_remains_dated_information_even_when_the_target_is_elsewhere() {
        let mut sim = fixture();
        let actor = sim.state.ruler;
        let target = sim.state.people[3].id;
        let reported_area = sim.state.areas[2].id;
        let id = propose(
            &mut sim,
            actor,
            MissionObjective::Defeat { person: target },
            vec![],
        );
        let actual = sim.state.people[sim.person_index[&target]].location.clone();
        sim.seed_mission_target_report(id, actor, target, reported_area, SimTime(0))
            .unwrap();
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].contacts[&target],
            reported_area
        );
        assert_eq!(sim.state.people[sim.person_index[&target]].location, actual);
        let before = sim.state.clone();
        assert_eq!(
            sim.seed_mission_target_report(id, target, target, reported_area, SimTime(0)),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.state, before);
    }
    #[test]
    fn an_accepted_confinement_leader_can_capture_on_its_next_turn_without_waiting_a_minute() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let target = sim.state.people[3].id;
        let home = sim.state.areas[0].id;
        sim.interrupt_actor_activity(leader);
        sim.interrupt_actor_activity(target);
        sim.state.people[sim.person_index[&leader]].location = Location::AtArea(home);
        sim.state.people[sim.person_index[&target]].location = Location::AtArea(home);
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
        sim.state.combat.as_mut().unwrap().rules.damage = 1;
        sim.debug_start_encounter(vec![(leader, 0), (target, 1)])
            .unwrap();
        sim.state
            .combat
            .as_mut()
            .unwrap()
            .maps
            .get_mut(&home)
            .unwrap()
            .obstacles
            .clear();
        let current = sim.state.physical.as_ref().unwrap().actors[&target]
            .vitals
            .health;
        let incap = sim.state.combat.as_ref().unwrap().rules.incapacitation;
        sim.debug_injure(target, current - (incap - 12_000))
            .unwrap();
        sim.advance(SimDuration(24_000)).unwrap();
        assert_eq!(
            sim.state.combat.as_ref().unwrap().captivity[&target].captor,
            leader
        );
        let record = sim.state.missions.as_ref().unwrap().missions[&id].members[&leader]
            .confinement
            .as_ref()
            .unwrap();
        assert!(record.captured_at.0 < MINUTE.0);
    }
}
