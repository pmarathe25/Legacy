//! Army relationships coordinate existing named mission leaders. This adapter
//! owns communication timing; missions retain recruitment, cargo and decisions.
use crate::*;
use game_armies::{
    ArmyAction, ArmyView, Communication, Departure, DetachmentFacts, Effect, OrderDecision,
    OrderProposal, OrderResponse, OrderStatus, Participation, ParticipationDecision,
    PreparedChange,
};
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub talk_time: SimDuration,
}
impl Default for Rules {
    fn default() -> Self {
        Self { talk_time: MINUTE }
    }
}
impl Rules {
    pub fn validate(&self) -> Result<(), Error> {
        if self.talk_time.0 == 0 {
            Err(Error::InvalidConfiguration(
                "Army conversation time must be positive".into(),
            ))
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    Invitation {
        army: ArmyId,
        mission: MissionId,
    },
    Participation {
        army: ArmyId,
        mission: MissionId,
        statement: game_armies::Dated<game_armies::Detachment>,
    },
    Order {
        army: ArmyId,
        order: ArmyOrderId,
        mission: MissionId,
    },
    Reply {
        army: ArmyId,
        order: ArmyOrderId,
        mission: MissionId,
    },
    Report {
        army: ArmyId,
        report: ArmyReportId,
    },
}
impl Message {
    fn army(&self) -> ArmyId {
        match *self {
            Self::Invitation { army, .. }
            | Self::Participation { army, .. }
            | Self::Order { army, .. }
            | Self::Reply { army, .. }
            | Self::Report { army, .. } => army,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedMessage {
    pub listener: PersonId,
    pub queued_at: SimTime,
    pub message: Message,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Talk {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub area: AreaId,
    pub started: SimTime,
    pub completes: SimTime,
    pub message: Message,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub kernel: game_armies::World,
    pub rules: Rules,
    pub talks: BTreeMap<PersonId, Talk>,
    pub queues: BTreeMap<PersonId, VecDeque<QueuedMessage>>,
    pub approaches: BTreeMap<PersonId, Approach>,
    pub last_review: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approach {
    pub actor: PersonId,
    pub mission: MissionId,
    pub army: ArmyId,
    pub order: ArmyOrderId,
    pub encounter: EncounterId,
    pub area: AreaId,
    pub target: PersonId,
    pub started: SimTime,
    pub completes: SimTime,
}
impl Simulation {
    /// Explicit army fixture: 65 citizens, one opponent, two guardians and the
    /// commander are distinct ordinary people before recruitment begins.
    pub fn army_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::kernel_fixture(seed, 69, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "army-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.initialize_physical_people(physical::Rules {
            hunger_harm: 6_000_000,
            fatigue_limit: 18_000_000,
            max_health: 180_000,
            meal_relief: 3_000_000,
            meal_size: Quantity(1_000),
            eating_time: MINUTE,
            loading_time: MINUTE,
            carrying_capacity: Quantity(10_000),
        })?;
        let food = game_cargo::inventories(&sim.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap();
        game_cargo::inventory_mut(&mut sim.host.storage, food)
            .unwrap()
            .quantity = Quantity(200_000);
        let routes = decisions::fixture_routes(&sim.host.state.areas);
        let minds = sim
            .host
            .state
            .people
            .iter()
            .map(|p| {
                let mut personality = decisions::Personality::industrious();
                personality.ambition = 1_000;
                personality.sociability = 1_000;
                personality.risk_tolerance = 1_000;
                personality.empathy = 0;
                personality.aggression = 0;
                (
                    p.id,
                    decisions::Mind {
                        interests: BTreeMap::new(),
                        routes: routes.clone(),
                        last_failure: None,
                        child_signals: BTreeMap::new(),
                        personality,
                        dream: decisions::Aspiration::Independence,
                        knowledge: BTreeMap::from([(
                            food,
                            StockKnowledge {
                                inventory: food,
                                area: sim.host.state.areas[0].id,
                                good: Good::Food,
                                quantity: Quantity(200_000),
                                accessible: true,
                                observed_at: SimTime(0),
                                learned_at: SimTime(0),
                                source: KnowledgeSource::Observed(p.id),
                            },
                        )]),
                        sites: BTreeMap::new(),
                        relationships: BTreeMap::new(),
                        values: BTreeMap::new(),
                        goal: decisions::Goal::Rest,
                        plan: None,
                        explanation: decisions::Explanation {
                            goal: decisions::Goal::Rest,
                            need: 0,
                            benefit: 0,
                            value: 0,
                            total: 0,
                            blocker: None,
                        },
                        retry_after: SimTime(0),
                    },
                )
            })
            .collect();
        sim.initialize_decision_people(minds)?;
        sim.host.state.combat = Some(combat::World {
            rules: combat::Rules {
                encounter_limit: 128,
                round: SimDuration(6_000),
                movement: 5_000,
                reach: 2_000,
                damage: 45_000,
                defence: 45_000,
                incapacitation: 45_000,
            },
            profiles: sim
                .host
                .state
                .people
                .iter()
                .map(|p| {
                    (
                        p.id,
                        combat::Profile {
                            initiative: if p.role == Role::Ruler { 200 } else { 100 },
                        },
                    )
                })
                .collect(),
            maps: BTreeMap::new(),
            road_maps: BTreeMap::new(),
            encounters: BTreeMap::new(),
            captivity: BTreeMap::new(),
        });
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
            game_property::declare_custody(&mut sim.host.storage, id, owner)?;
        }
        sim.host.state.agreements = Some(game_agreements::Runtime {
            last_event: sim.host.next_event() - 1,
            last_observation_event: sim.host.next_event() - 1,
        });
        sim.host.state.missions = Some(missions::Runtime {
            last_event: sim.host.next_event() - 1,
            last_review: sim.host.clock().time,
        });
        sim.initialize_armies(Rules::default())?;
        let village = sim.host.state.areas[0].id;
        sim.debug_add_inventory(
            Good::Binding,
            InventoryLocation::Area(village),
            Quantity(3_000),
            Quantity(3_000),
            sim.host.state.people.iter().map(|p| p.id).collect(),
        )?;
        Ok(sim)
    }
    pub fn initialize_armies(&mut self, rules: Rules) -> Result<(), Error> {
        rules.validate()?;
        if self.host.state.armies.is_some()
            || self.host.state.missions.is_none()
            || self.host.state.physical.is_none()
            || self.host.state.decisions.is_none()
            || self.host.state.combat.is_none()
        {
            return Err(Error::Unavailable);
        }
        self.host.state.armies = Some(World {
            kernel: game_armies::World::default(),
            rules,
            talks: BTreeMap::new(),
            queues: BTreeMap::new(),
            approaches: BTreeMap::new(),
            last_review: self.host.clock().time,
        });
        Ok(())
    }
    fn army_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        if !self.person_index.contains_key(&actor)
            || !self.independent(actor)
            || self.combat_restricts(actor)
        {
            return Err(Error::Unavailable);
        }
        let body = self
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, actor))
            .ok_or(Error::Unavailable)?;
        if !body.vitals.alive || body.vitals.health == 0 {
            return Err(Error::Unavailable);
        }
        match self.resolved_location(actor) {
            Location::AtArea(area) => Ok(area),
            _ => Err(Error::Unavailable),
        }
    }
    fn army_facts(&self, mission: MissionId) -> Result<DetachmentFacts, Error> {
        let m = self
            .host
            .state
            .missions
            .as_ref()
            .and_then(|_| game_missions::mission(&self.host.storage, mission))
            .ok_or(Error::Unavailable)?;
        Ok(DetachmentFacts {
            mission,
            leader: m.proposal.leader,
            status: m.status,
            members: m
                .members
                .values()
                .filter(|p| p.status == CompanionStatus::Accepted)
                .map(|p| p.person)
                .collect(),
            leader_capable: self.army_area(m.proposal.leader).is_ok(),
        })
    }
    fn army_queue_capacity(&self, speaker: PersonId, additional: usize) -> Result<(), Error> {
        let w = self.host.state.armies.as_ref().ok_or(Error::Unavailable)?;
        if w.queues
            .get(&speaker)
            .map_or(0, VecDeque::len)
            .checked_add(additional)
            .is_none_or(|n| n > 128)
        {
            return Err(Error::Unavailable);
        }
        self.host
            .clock()
            .time
            .0
            .checked_add(w.rules.talk_time.0)
            .ok_or(Error::Overflow)?;
        Ok(())
    }
    fn queue_army_message(&mut self, speaker: PersonId, listener: PersonId, message: Message) {
        let queued_at = self.host.clock().time;
        self.host
            .state
            .armies
            .as_mut()
            .unwrap()
            .queues
            .entry(speaker)
            .or_default()
            .push_back(QueuedMessage {
                listener,
                queued_at,
                message,
            });
    }
    fn participation_message(
        &self,
        army: ArmyId,
        mission: MissionId,
        speaker: PersonId,
    ) -> Message {
        Message::Participation {
            army,
            mission,
            statement: self
                .host
                .state
                .armies
                .as_ref()
                .unwrap()
                .kernel
                .view(speaker, army)
                .unwrap()
                .detachments[&mission]
                .clone(),
        }
    }
    fn commit_army_patch(&mut self, patch: PreparedChange) -> Result<(), Error> {
        patch.check(&self.host.state.armies.as_ref().unwrap().kernel)?;
        // Every native request is preflighted before either domain publishes.
        for effect in &patch.effects {
            match effect {
                Effect::OfferObjective {
                    mission,
                    leader,
                    objective,
                } => self.validate_mission_combat_order(*leader, *mission, objective)?,
                Effect::OfferRetreat { mission, leader } => {
                    let home = game_missions::mission(&self.host.storage, *mission)
                        .expect("record exists")
                        .proposal
                        .home;
                    self.validate_mission_combat_order(
                        *leader,
                        *mission,
                        &MissionObjective::Visit { destination: home },
                    )?;
                }
                Effect::ReleaseCommand {
                    mission,
                    leader,
                    reason: Departure::Defect,
                } => {
                    if game_missions::mission(&self.host.storage, *mission)
                        .expect("record exists")
                        .members
                        .get(leader)
                        .is_none_or(|m| m.status != CompanionStatus::Accepted)
                    {
                        return Err(Error::Unavailable);
                    }
                }
                Effect::ReleaseCommand { .. } => {}
            }
        }
        self.ensure_event_capacity(self.army_event_bound())?;
        let effects = patch.commit(&mut self.host.state.armies.as_mut().unwrap().kernel)?;
        for effect in effects {
            match effect {
                Effect::OfferObjective {
                    mission,
                    leader,
                    objective,
                } => self.apply_accepted_leader_objective(leader, mission, objective)?,
                Effect::OfferRetreat { mission, leader } => {
                    let home = game_missions::mission(&self.host.storage, mission)
                        .expect("record exists")
                        .proposal
                        .home;
                    self.apply_accepted_leader_objective(
                        leader,
                        mission,
                        MissionObjective::Visit { destination: home },
                    )?;
                    let m = game_missions::mission_mut(&mut self.host.storage, mission).unwrap();
                    m.status = MissionStatus::Returning;
                    m.succeeded = false;
                    m.explanation="The leader chose to retreat after an army order; companions retain their choices".into();
                }
                Effect::ReleaseCommand {
                    mission,
                    leader,
                    reason: Departure::Defect,
                } => self.mission_command(leader, &MissionAction::Leave { mission })?,
                Effect::ReleaseCommand { .. } => {}
            }
        }
        Ok(())
    }
    pub(crate) fn army_command(
        &mut self,
        actor: PersonId,
        action: &ArmyAction,
    ) -> Result<(), Error> {
        let area = self.army_area(actor)?;
        self.preflight_armies()?;
        let w = self.host.state.armies.as_ref().ok_or(Error::Unavailable)?;
        match action {
            ArmyAction::Approach {
                army,
                mission,
                encounter,
            } => {
                let known = self
                    .army_view(actor)
                    .into_iter()
                    .find(|v| v.id == *army)
                    .is_some_and(|v| {
                        v.local_engagements
                            .iter()
                            .any(|e| e.mission == *mission && e.encounter == *encounter)
                    });
                if !known {
                    return Err(Error::Unavailable);
                }
                self.start_army_approach(actor, *mission, *encounter)?;
            }
            ArmyAction::Form { missions } => {
                let mut facts = BTreeMap::new();
                let known = self.mission_view(actor);
                for id in missions {
                    let view = known
                        .iter()
                        .find(|v| v.id == *id)
                        .ok_or(Error::Unavailable)?;
                    if self.army_area(view.leader).ok() != Some(area) {
                        return Err(Error::Unavailable);
                    }
                    facts.insert(*id, self.army_facts(*id)?);
                }
                self.army_queue_capacity(actor, missions.len())?;
                self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
                let id = ArmyId(self.host.next_id());
                let patch =
                    w.kernel
                        .prepare_form(id, actor, missions, &facts, self.host.clock().time)?;
                self.commit_army_patch(patch)?;
                self.host.allocate_id()?;
                for mission in missions {
                    let leader = facts[mission].leader;
                    if leader == actor {
                        let p = self
                            .host
                            .state
                            .armies
                            .as_ref()
                            .unwrap()
                            .kernel
                            .prepare_consider_own_invitation(
                                id,
                                *mission,
                                actor,
                                self.host.clock().time,
                            )?;
                        self.commit_army_patch(p)?;
                    } else {
                        self.queue_army_message(
                            actor,
                            leader,
                            Message::Invitation {
                                army: id,
                                mission: *mission,
                            },
                        );
                    }
                }
            }
            ArmyAction::Respond {
                army,
                mission,
                accept,
            } => {
                let facts = self.army_facts(*mission)?;
                let commander = w
                    .kernel
                    .armies
                    .get(army)
                    .ok_or(Error::Unavailable)?
                    .commander;
                self.army_queue_capacity(actor, usize::from(actor != commander))?;
                let patch = w.kernel.prepare_respond(
                    ParticipationDecision {
                        army: *army,
                        mission: *mission,
                        actor,
                        accept: *accept,
                        explanation: if *accept {
                            "Personally accepted coordination with this commander".into()
                        } else {
                            "Personally declined this command relationship".into()
                        },
                    },
                    &facts,
                    self.host.clock().time,
                )?;
                self.commit_army_patch(patch)?;
                if commander != actor {
                    self.queue_army_message(
                        actor,
                        commander,
                        self.participation_message(*army, *mission, actor),
                    );
                }
            }
            ArmyAction::Order {
                army,
                missions,
                objective,
                expires,
            } => {
                if !self.mission_objective_valid(objective) {
                    return Err(Error::Unavailable);
                }
                let a = w.kernel.armies.get(army).ok_or(Error::Unavailable)?;
                for mission in missions {
                    let leader = a.detachments.get(mission).ok_or(Error::Unavailable)?.leader;
                    if self.army_area(leader).ok() != Some(area) {
                        return Err(Error::Unavailable);
                    }
                }
                self.army_queue_capacity(actor, missions.len())?;
                self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
                let id = ArmyOrderId(self.host.next_id());
                let p = w.kernel.prepare_order(
                    OrderProposal {
                        army: *army,
                        id,
                        commander: actor,
                        missions: missions.clone(),
                        objective: objective.clone(),
                        expires: *expires,
                    },
                    self.host.clock().time,
                )?;
                self.commit_army_patch(p)?;
                self.host.allocate_id()?;
                for mission in missions {
                    let leader = self.host.state.armies.as_ref().unwrap().kernel.armies[army]
                        .detachments[mission]
                        .leader;
                    if leader == actor {
                        let f = self.army_facts(*mission)?;
                        let p = self
                            .host
                            .state
                            .armies
                            .as_ref()
                            .unwrap()
                            .kernel
                            .prepare_consider_own_order(
                                *army,
                                id,
                                *mission,
                                actor,
                                &f,
                                self.host.clock().time,
                            )?;
                        self.commit_army_patch(p)?;
                    } else {
                        self.queue_army_message(
                            actor,
                            leader,
                            Message::Order {
                                army: *army,
                                order: id,
                                mission: *mission,
                            },
                        );
                    }
                }
            }
            ArmyAction::RespondOrder {
                army,
                order,
                mission,
                response,
            } => {
                let facts = self.army_facts(*mission)?;
                let commander = w
                    .kernel
                    .armies
                    .get(army)
                    .ok_or(Error::Unavailable)?
                    .commander;
                self.army_queue_capacity(actor, usize::from(actor != commander))?;
                let p=w.kernel.prepare_respond_order(OrderDecision {army:*army,order:*order,mission:*mission,actor,response:*response,
                    explanation:format!("Personally chose {response:?} after considering the received army objective")},&facts,self.host.clock().time)?;
                self.commit_army_patch(p)?;
                if commander != actor {
                    self.queue_army_message(
                        actor,
                        commander,
                        Message::Reply {
                            army: *army,
                            order: *order,
                            mission: *mission,
                        },
                    );
                }
            }
            ArmyAction::Report {
                army,
                mission,
                recipient,
            } => {
                if self.army_area(*recipient).ok() != Some(area) || *recipient == actor {
                    return Err(Error::Unavailable);
                }
                self.army_queue_capacity(actor, 1)?;
                let snapshot = self
                    .mission_view(actor)
                    .into_iter()
                    .find(|m| m.id == *mission)
                    .ok_or(Error::Unavailable)?;
                self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
                let report = ArmyReportId(self.host.next_id());
                let p = w.kernel.prepare_report(
                    *army,
                    game_armies::ArmyReport {
                        id: report,
                        army: *army,
                        mission: *mission,
                        speaker: actor,
                        sent_at: self.host.clock().time,
                        snapshot,
                    },
                    self.host.clock().time,
                )?;
                self.commit_army_patch(p)?;
                self.host.allocate_id()?;
                self.queue_army_message(
                    actor,
                    *recipient,
                    Message::Report {
                        army: *army,
                        report,
                    },
                );
            }
            ArmyAction::Leave {
                army,
                mission,
                reason,
            } => {
                let commander = w
                    .kernel
                    .armies
                    .get(army)
                    .ok_or(Error::Unavailable)?
                    .commander;
                self.army_queue_capacity(actor, usize::from(actor != commander))?;
                let p = w.kernel.prepare_leave(
                    *army,
                    *mission,
                    actor,
                    *reason,
                    "Personally withdrew support for this command relationship".into(),
                    self.host.clock().time,
                )?;
                self.commit_army_patch(p)?;
                if commander != actor {
                    self.queue_army_message(
                        actor,
                        commander,
                        self.participation_message(*army, *mission, actor),
                    );
                }
            }
            ArmyAction::Disband { army } => {
                let p = w
                    .kernel
                    .prepare_disband(*army, actor, self.host.clock().time)?;
                self.commit_army_patch(p)?;
            }
        }
        self.start_queued_army_talk(actor)?;
        self.reconcile_armies()?;
        Ok(())
    }
    fn start_queued_army_talk(&mut self, speaker: PersonId) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        let w = self.host.state.armies.as_ref().ok_or(Error::Unavailable)?;
        if w.talks
            .values()
            .any(|t| t.speaker == speaker || t.listener == speaker)
        {
            return Ok(());
        }
        let Some(message) = w.queues.get(&speaker).and_then(|q| q.front()).cloned() else {
            return Ok(());
        };
        let Ok(area) = self.army_area(speaker) else {
            return Ok(());
        };
        if self.army_area(message.listener).ok() != Some(area)
            || w.talks
                .values()
                .any(|t| t.speaker == message.listener || t.listener == message.listener)
        {
            return Ok(());
        }
        if !self.conversation_actor_available(speaker)
            || !self.conversation_actor_available(message.listener)
        {
            return Ok(());
        }
        let completes = SimTime(
            owner_now
                .0
                .checked_add(w.rules.talk_time.0)
                .ok_or(Error::Overflow)?,
        );
        self.interrupt_actor_activity(speaker);
        self.host
            .state
            .armies
            .as_mut()
            .unwrap()
            .queues
            .get_mut(&speaker)
            .unwrap()
            .pop_front();
        *self.host.storage.activity_mut(speaker).unwrap() = Activity::ArmyTalk {
            army: message.message.army(),
            listener: message.listener,
            completes,
        };
        self.host.state.armies.as_mut().unwrap().talks.insert(
            speaker,
            Talk {
                speaker,
                listener: message.listener,
                area,
                started: owner_now,
                completes,
                message: message.message,
            },
        );
        Ok(())
    }
    pub(crate) fn interrupt_army_talk(&mut self, actor: PersonId) {
        let Some(w) = &self.host.state.armies else {
            return;
        };
        let talks: Vec<_> = w
            .talks
            .values()
            .filter(|t| t.speaker == actor || t.listener == actor)
            .cloned()
            .collect();
        for talk in talks {
            self.host
                .state
                .armies
                .as_mut()
                .unwrap()
                .talks
                .remove(&talk.speaker);
            if matches!(*self.host.storage.activity(talk.speaker).expect("body exists"),Activity::ArmyTalk {completes,..} if completes==talk.completes)
            {
                *self.host.storage.activity_mut(talk.speaker).unwrap() = Activity::Idle;
            }
        }
        if self
            .host
            .state
            .armies
            .as_mut()
            .unwrap()
            .approaches
            .remove(&actor)
            .is_some()
            && matches!(
                *self.host.storage.activity(actor).expect("body exists"),
                Activity::ArmyApproach { .. }
            )
        {
            *self.host.storage.activity_mut(actor).unwrap() = Activity::Idle;
        }
        // Interrupted speech is not received or silently retried.
    }
    pub(crate) fn army_boundary(&self, limit: SimTime) -> SimTime {
        let Some(w) = &self.host.state.armies else {
            return limit;
        };
        w.talks
            .values()
            .map(|t| t.completes)
            .chain(w.approaches.values().map(|a| a.completes))
            .chain(w.kernel.next_boundary(self.host.clock().time))
            .filter(|at| *at > self.host.clock().time)
            .min()
            .map_or(limit, |at| at.min(limit))
    }
    pub(crate) fn army_actor_talking(&self, actor: PersonId) -> bool {
        self.host.state.armies.as_ref().is_some_and(|w| {
            w.talks
                .values()
                .any(|t| t.speaker == actor || t.listener == actor)
        })
    }
    pub(crate) fn army_allied_missions(&self, mission: MissionId) -> BTreeSet<MissionId> {
        let Some(w) = &self.host.state.armies else {
            return BTreeSet::from([mission]);
        };
        let Some(id) = w.kernel.commanding_army(mission) else {
            return BTreeSet::from([mission]);
        };
        w.kernel.armies[&id]
            .detachments
            .values()
            .filter(|d| d.participation.active())
            .map(|d| d.mission)
            .collect()
    }
    fn army_order_for_approach(
        &self,
        mission: MissionId,
        target: PersonId,
    ) -> Option<(ArmyId, ArmyOrderId)> {
        let w = self.host.state.armies.as_ref()?;
        let army = w.kernel.commanding_army(mission)?;
        let a = &w.kernel.armies[&army];
        a.orders
            .values()
            .rev()
            .find(|o| {
                o.objective.requires_force()
                    && o.objective.target_person() == Some(target)
                    && o.recipients
                        .get(&mission)
                        .is_some_and(|r| r.status == OrderStatus::Accepted)
            })
            .map(|o| (army, o.id))
    }
    fn army_personal_force_target(
        &self,
        actor: PersonId,
        army: ArmyId,
        mission: MissionId,
        area: AreaId,
    ) -> Option<PersonId> {
        let w = self.host.state.armies.as_ref()?;
        let a = w.kernel.armies.get(&army)?;
        let m = game_missions::mission(&self.host.storage, mission)?;
        if let Some(member) = m.members.get(&actor) {
            let target = member
                .directive
                .target_person()
                .filter(|_| member.directive.requires_force())?;
            if member.status != CompanionStatus::Accepted
                || member.contacts.get(&target) != Some(&area)
            {
                return None;
            }
            if actor != self.host.state.ruler {
                return Some(target);
            }
            let known = w.kernel.view(actor, army)?;
            if known.commander.as_ref().is_some_and(|c| c.value == actor)
                || known.responses.values().any(|rs| {
                    rs.get(&mission).is_some_and(|r| {
                        r.value.leader == actor && r.value.status == OrderStatus::Accepted
                    })
                })
            {
                return Some(target);
            }
            return None;
        }
        if a.commander != actor {
            return None;
        }
        w.kernel
            .view(actor, army)?
            .orders
            .values()
            .rev()
            .find(|o| {
                o.value.commander == actor
                    && o.value.recipients.contains_key(&mission)
                    && o.value.objective.requires_force()
            })
            .and_then(|o| o.value.objective.target_person())
    }
    fn army_entrance(&self, actor: PersonId, encounter: EncounterId) -> Option<Point> {
        let w = self.host.state.combat.as_ref()?;
        let e = w.encounters.get(&encounter)?;
        let map = w.map(e.map)?;
        let routes =
            &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)?
                .routes;
        map.exits
            .iter()
            .filter(|(to, p)| {
                routes.get(&e.area).is_some_and(|r| r.contains_key(to)) && map.clear(*p, *p)
            })
            .min_by_key(|(to, _)| *to)
            .map(|(_, p)| *p)
    }
    pub(crate) fn start_army_approach(
        &mut self,
        actor: PersonId,
        mission: MissionId,
        encounter: EncounterId,
    ) -> Result<(), Error> {
        let area = self.army_area(actor)?;
        if *game_spatial::location(
            &self.host.storage,
            game_ecs::StableId((self.host.state.people[self.person_index[&actor]].id).0),
        )
        .expect("location exists")
            != Location::AtArea(area)
            || self.army_actor_talking(actor)
            || self
                .host
                .state
                .armies
                .as_ref()
                .is_none_or(|w| w.approaches.contains_key(&actor))
        {
            return Err(Error::Unavailable);
        }
        let army = self
            .host
            .state
            .armies
            .as_ref()
            .unwrap()
            .kernel
            .commanding_army(mission)
            .ok_or(Error::Unavailable)?;
        let target = self
            .army_personal_force_target(actor, army, mission, area)
            .ok_or(Error::Unavailable)?;
        let (army, order) = self
            .army_order_for_approach(mission, target)
            .ok_or(Error::Unavailable)?;
        let completes = SimTime(
            self.host
                .clock()
                .time
                .0
                .checked_add(self.host.state.combat.as_ref().unwrap().rules.round.0)
                .ok_or(Error::Overflow)?,
        );
        let approach = Approach {
            actor,
            mission,
            army,
            order,
            encounter,
            area,
            target,
            started: self.host.clock().time,
            completes,
        };
        self.validate_army_approach(&approach, false)?;
        self.interrupt_actor_activity(actor);
        *self.host.storage.activity_mut(actor).unwrap() = Activity::ArmyApproach {
            mission,
            encounter,
            completes,
        };
        self.host
            .state
            .armies
            .as_mut()
            .unwrap()
            .approaches
            .insert(actor, approach);
        Ok(())
    }
    pub(crate) fn validate_army_approach(
        &self,
        a: &Approach,
        completed: bool,
    ) -> Result<(u32, Point), Error> {
        let w = self.host.state.armies.as_ref().ok_or(Error::Unavailable)?;
        let army = w.kernel.armies.get(&a.army).ok_or(Error::Unavailable)?;
        let order = army.orders.get(&a.order).ok_or(Error::Unavailable)?;
        let e = self
            .host
            .state
            .combat
            .as_ref()
            .unwrap()
            .encounters
            .get(&a.encounter)
            .ok_or(Error::Unavailable)?;
        if army.disbanded_at.is_some()
            || army
                .detachments
                .get(&a.mission)
                .is_none_or(|d| !d.participation.active())
            || order
                .recipients
                .get(&a.mission)
                .is_none_or(|r| r.status != OrderStatus::Accepted)
            || order.objective.target_person() != Some(a.target)
            || !order.objective.requires_force()
            || !e.active
            || e.location != Location::AtArea(a.area)
            || e.fighters.contains_key(&a.actor)
            || self.army_area(a.actor).ok() != Some(a.area)
            || !self.can_fight(a.actor)
            || game_physiology::body(&self.host.storage, a.actor)
                .expect("body exists")
                .vitals
                .fatigue
                >= self.body_rules(a.actor).fatigue_limit
            || *game_spatial::location(
                &self.host.storage,
                game_ecs::StableId((self.host.state.people[self.person_index[&a.actor]].id).0),
            )
            .expect("location exists")
                != Location::AtArea(a.area)
            || (army.commander != a.actor
                && !self.mission_force_willing(a.actor, a.mission, a.target))
            || self.army_personal_force_target(a.actor, a.army, a.mission, a.area) != Some(a.target)
            || !matches!(
                game_missions::mission(&self.host.storage, a.mission)
                    .expect("record exists")
                    .status,
                MissionStatus::Traveling | MissionStatus::Operating
            )
            || e.fighters
                .get(&a.target)
                .is_none_or(|f| f.withdrawn || f.surrendered)
            || !game_physiology::body(&self.host.storage, a.target)
                .expect("body exists")
                .vitals
                .alive
            || (completed && a.completes != self.host.clock().time)
        {
            return Err(Error::Unavailable);
        }
        let linked = self.army_allied_missions(a.mission);
        let team = e
            .fighters
            .values()
            .filter(|f| !f.withdrawn && !f.surrendered)
            .find_map(|f| {
                linked
                    .iter()
                    .any(|id| {
                        game_missions::mission(&self.host.storage, *id)
                            .expect("record exists")
                            .members
                            .get(&f.actor)
                            .is_some_and(|m| m.status == CompanionStatus::Accepted)
                    })
                    .then_some(f.team)
            })
            .ok_or(Error::Unavailable)?;
        let entry = self
            .army_entrance(a.actor, a.encounter)
            .ok_or(Error::NoRoute)?;
        if e.fighters[&a.target].team == team {
            return Err(Error::Unavailable);
        }
        Ok((team, entry))
    }
    pub(crate) fn reconcile_armies(&mut self) -> Result<(), Error> {
        let Some(w) = &self.host.state.armies else {
            return Ok(());
        };
        let invalid: Vec<_> = w
            .talks
            .values()
            .filter(|t| {
                self.army_area(t.speaker).ok() != Some(t.area)
                    || self.army_area(t.listener).ok() != Some(t.area)
            })
            .map(|t| t.speaker)
            .collect();
        for speaker in invalid {
            self.interrupt_army_talk(speaker);
        }
        let approaches: Vec<_> = self
            .host
            .state
            .armies
            .as_ref()
            .unwrap()
            .approaches
            .values()
            .cloned()
            .collect();
        for approach in approaches {
            if self.validate_army_approach(&approach, false).is_err() {
                self.interrupt_army_talk(approach.actor);
                continue;
            }
            if approach.completes == self.host.clock().time
                && self.admit_local_army_approach(&approach).is_err()
            {
                self.interrupt_army_talk(approach.actor);
            }
        }
        let talks: Vec<_> = self
            .host
            .state
            .armies
            .as_ref()
            .unwrap()
            .talks
            .values()
            .filter(|t| t.completes <= self.host.clock().time)
            .cloned()
            .collect();
        for talk in talks {
            let valid = self.army_area(talk.speaker).ok() == Some(talk.area)
                && self.army_area(talk.listener).ok() == Some(talk.area)
                && *self
                    .host
                    .storage
                    .activity(talk.speaker)
                    .expect("body exists")
                    == Activity::ArmyTalk {
                        army: talk.message.army(),
                        listener: talk.listener,
                        completes: talk.completes,
                    };
            self.host
                .state
                .armies
                .as_mut()
                .unwrap()
                .talks
                .remove(&talk.speaker);
            if matches!(*self.host.storage.activity(talk.speaker).expect("body exists"),Activity::ArmyTalk {completes,..} if completes==talk.completes)
            {
                *self.host.storage.activity_mut(talk.speaker).unwrap() = Activity::Idle;
            }
            if !valid {
                continue;
            }
            let receipt = Communication {
                speaker: talk.speaker,
                listener: talk.listener,
                started_at: talk.started,
                completed_at: self.host.clock().time,
            };
            let k = &self.host.state.armies.as_ref().unwrap().kernel;
            let patch = match talk.message {
                Message::Invitation { army, mission } => {
                    k.prepare_hear_invitation(army, mission, receipt, self.host.clock().time)
                }
                Message::Participation {
                    army,
                    mission,
                    statement,
                } => k.prepare_receive_participation(
                    army,
                    mission,
                    &statement,
                    receipt,
                    self.host.clock().time,
                ),
                Message::Order {
                    army,
                    order,
                    mission,
                } => self.army_facts(mission).and_then(|f| {
                    k.prepare_hear_order(army, order, mission, receipt, &f, self.host.clock().time)
                }),
                Message::Reply {
                    army,
                    order,
                    mission,
                } => k.prepare_hear_order_response(
                    army,
                    order,
                    mission,
                    receipt,
                    self.host.clock().time,
                ),
                Message::Report { army, report } => {
                    k.prepare_hear_report(army, report, receipt, self.host.clock().time)
                }
            };
            if let Ok(p) = patch {
                self.commit_army_patch(p)?;
            }
            self.start_queued_army_talk(talk.speaker)?;
        }
        let ids: Vec<_> = self
            .host
            .state
            .armies
            .as_ref()
            .unwrap()
            .kernel
            .armies
            .keys()
            .copied()
            .collect();
        for id in ids {
            let a = &self.host.state.armies.as_ref().unwrap().kernel.armies[&id];
            if a.orders.values().any(|o| {
                o.expires <= self.host.clock().time
                    && o.recipients.values().any(|r| {
                        matches!(
                            r.status,
                            OrderStatus::AwaitingDelivery | OrderStatus::Received
                        )
                    })
            }) {
                let expired = self
                    .host
                    .state
                    .armies
                    .as_ref()
                    .unwrap()
                    .kernel
                    .prepare_expire(id, self.host.clock().time)?;
                self.commit_army_patch(expired)?;
            }
            let missions: Vec<_> = self.host.state.armies.as_ref().unwrap().kernel.armies[&id]
                .detachments
                .values()
                .filter(|d| d.participation.active())
                .map(|d| d.mission)
                .collect();
            for mission in missions {
                let f = self.army_facts(mission)?;
                if matches!(
                    f.status,
                    MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
                ) {
                    let p = self
                        .host
                        .state
                        .armies
                        .as_ref()
                        .unwrap()
                        .kernel
                        .prepare_native_termination(id, &f, self.host.clock().time)?;
                    self.commit_army_patch(p)?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn review_armies(&mut self) -> Result<(), Error> {
        let Some(w) = &self.host.state.armies else {
            return Ok(());
        };
        if w.last_review == self.host.clock().time
            || !self.host.clock().time.0.is_multiple_of(MINUTE.0)
        {
            return Ok(());
        }
        self.preflight_armies()?;
        self.host.state.armies.as_mut().unwrap().last_review = self.host.clock().time;
        let actors: Vec<_> = self
            .host
            .state
            .armies
            .as_ref()
            .unwrap()
            .kernel
            .known
            .keys()
            .copied()
            .collect();
        for actor in actors {
            if actor == self.host.state.ruler
                || self.army_area(actor).is_err()
                || self
                    .host
                    .state
                    .armies
                    .as_ref()
                    .unwrap()
                    .talks
                    .values()
                    .any(|t| t.speaker == actor || t.listener == actor)
            {
                continue;
            }
            for view in self.army_view(actor) {
                for d in view.knowledge.detachments.values() {
                    if d.value.leader != actor
                        || d.value.participation != Participation::Offered
                        || d.value.heard_at.is_none()
                    {
                        continue;
                    }
                    let mission = d.value.mission;
                    let objective = &game_missions::mission(&self.host.storage, mission)
                        .expect("record exists")
                        .members[&actor]
                        .directive;
                    let accept = self
                        .mission_leader_judgment(actor, mission, objective)
                        .is_ok_and(|score| score >= 0);
                    let _ = self.army_command(
                        actor,
                        &ArmyAction::Respond {
                            army: view.id,
                            mission,
                            accept,
                        },
                    );
                }
                for (id, o) in view.knowledge.orders {
                    for (mission, r) in o.value.recipients {
                        if r.leader != actor
                            || r.status != OrderStatus::Received
                            || self.host.clock().time >= o.value.expires
                        {
                            continue;
                        }
                        let willing = self
                            .mission_leader_judgment(actor, mission, &o.value.objective)
                            .is_ok_and(|score| score >= 0);
                        let response = if willing {
                            OrderResponse::Accept
                        } else {
                            OrderResponse::Refuse
                        };
                        let _ = self.army_command(
                            actor,
                            &ArmyAction::RespondOrder {
                                army: view.id,
                                order: id,
                                mission,
                                response,
                            },
                        );
                    }
                }
            }
        }
        let speakers: Vec<_> = self
            .host
            .state
            .armies
            .as_ref()
            .unwrap()
            .queues
            .keys()
            .copied()
            .collect();
        for speaker in speakers {
            self.start_queued_army_talk(speaker)?;
        }
        Ok(())
    }
    pub(crate) fn army_view(&self, actor: PersonId) -> Vec<ArmyView> {
        let mut views = self
            .host
            .state
            .armies
            .as_ref()
            .map_or_else(Vec::new, |w| w.kernel.views(actor));
        let Ok(area) = self.army_area(actor) else {
            return views;
        };
        for view in &mut views {
            for mission in view.knowledge.detachments.keys() {
                let Some(target) = self.army_personal_force_target(actor, view.id, *mission, area)
                else {
                    continue;
                };
                for e in self
                    .host
                    .state
                    .combat
                    .as_ref()
                    .unwrap()
                    .encounters
                    .values()
                    .filter(|e| {
                        e.active
                            && e.location == Location::AtArea(area)
                            && e.fighters.contains_key(&target)
                    })
                {
                    let Some(entry) = self.army_entrance(actor, e.id) else {
                        continue;
                    };
                    let map = self.host.state.combat.as_ref().unwrap().map(e.map).unwrap();
                    let linked = self.army_allied_missions(*mission);
                    let visible_ally = e.fighters.values().any(|f| {
                        !f.withdrawn
                            && !f.surrendered
                            && map.clear(entry, f.position)
                            && linked.iter().any(|id| {
                                game_missions::mission(&self.host.storage, *id)
                                    .expect("record exists")
                                    .members
                                    .get(&f.actor)
                                    .is_some_and(|p| p.status == CompanionStatus::Accepted)
                            })
                    });
                    if visible_ally {
                        view.local_engagements.push(game_armies::LocalEngagement {
                            mission: *mission,
                            encounter: e.id,
                            target,
                            observed_at: self.host.clock().time,
                        });
                    }
                }
            }
        }
        views
    }
    pub(crate) fn available_army_commands(&self, actor: PersonId) -> Vec<Command> {
        if self.army_area(actor).is_err() {
            return Vec::new();
        }
        let mut result = Vec::new();
        for view in self.army_view(actor) {
            for e in &view.local_engagements {
                result.push(ArmyAction::Approach {
                    army: view.id,
                    mission: e.mission,
                    encounter: e.encounter,
                });
            }
            for d in view
                .knowledge
                .detachments
                .values()
                .filter(|d| d.value.leader == actor)
            {
                let mission = d.value.mission;
                if d.value.participation == Participation::Offered && d.value.heard_at.is_some() {
                    for accept in [true, false] {
                        result.push(ArmyAction::Respond {
                            army: view.id,
                            mission,
                            accept,
                        });
                    }
                }
                if d.value.participation == Participation::Accepted {
                    result.push(ArmyAction::Leave {
                        army: view.id,
                        mission,
                        reason: Departure::Withdraw,
                    });
                }
            }
            for (order, o) in view.knowledge.orders {
                if self.host.clock().time >= o.value.expires {
                    continue;
                }
                for (mission, r) in o.value.recipients {
                    if r.leader == actor && r.status == OrderStatus::Received {
                        for response in [
                            OrderResponse::Accept,
                            OrderResponse::Refuse,
                            OrderResponse::Retreat,
                        ] {
                            result.push(ArmyAction::RespondOrder {
                                army: view.id,
                                order,
                                mission,
                                response,
                            });
                        }
                    }
                }
            }
            if view.knowledge.commander.is_some_and(|d| d.value == actor)
                && view.knowledge.disbanded_at.is_none()
            {
                result.push(ArmyAction::Disband { army: view.id });
            }
        }
        result
            .into_iter()
            .map(|action| Command::Army { action })
            .collect()
    }
    pub(crate) fn army_allocation_bound(&self) -> u64 {
        0
    }
    pub(crate) fn army_event_bound(&self) -> usize {
        self.host.state.armies.as_ref().map_or(0, |w| {
            w.kernel
                .armies
                .values()
                .map(|a| a.detachments.len() * 64)
                .sum::<usize>()
                + w.talks.len() * 64
        })
    }
    pub(crate) fn preflight_armies(&self) -> Result<(), Error> {
        self.preflight_armies_at(self.host.clock().time)
    }
    pub(crate) fn preflight_armies_at(&self, at: SimTime) -> Result<(), Error> {
        if let Some(w) = &self.host.state.armies {
            at.0.checked_add(w.rules.talk_time.0)
                .ok_or(Error::Overflow)?;
            at.0.checked_add(self.host.state.combat.as_ref().unwrap().rules.round.0)
                .ok_or(Error::Overflow)?;
            self.ensure_event_capacity(self.army_event_bound())?;
        }
        Ok(())
    }
    pub(crate) fn validate_armies(&self) -> Result<(), Error> {
        let Some(w) = &self.host.state.armies else {
            return Ok(());
        };
        w.rules.validate()?;
        let bad = || Error::InvalidSave("Invalid army communication or native references".into());
        self.host.state.missions.as_ref().ok_or_else(bad)?;
        let people = self.person_index.keys().copied().collect();
        let mission_ids = game_missions::missions(&self.host.storage)
            .map(|(id, _)| id)
            .collect();
        w.kernel.validate(
            &game_armies::ValidationCatalog {
                people: &people,
                missions: &mission_ids,
                now: self.host.clock().time,
            },
            |o| {
                if army_saved_objective_valid(&self.canonical_state(), o) {
                    Ok(())
                } else {
                    Err(bad())
                }
            },
            |v| validate_army_mission_snapshot(&self.canonical_state(), v),
        )?;
        if w.last_review > self.host.clock().time {
            return Err(bad());
        }
        for a in w.kernel.armies.values() {
            if a.id.0 >= self.host.next_id()
                || a.orders.keys().any(|id| id.0 >= self.host.next_id())
                || a.reports.keys().any(|id| id.0 >= self.host.next_id())
            {
                return Err(bad());
            }
        }
        let mut occupied = BTreeSet::new();
        for (speaker, t) in &w.talks {
            if *speaker != t.speaker
                || !occupied.insert(*speaker)
                || !occupied.insert(t.listener)
                || t.started > self.host.clock().time
                || t.completes <= self.host.clock().time
                || t.started.0.checked_add(w.rules.talk_time.0) != Some(t.completes.0)
                || self.army_area(*speaker).ok() != Some(t.area)
                || self.army_area(t.listener).ok() != Some(t.area)
                || *self.host.storage.activity(*speaker).expect("body exists")
                    != (Activity::ArmyTalk {
                        army: t.message.army(),
                        listener: t.listener,
                        completes: t.completes,
                    })
                || !army_message_valid(w, *speaker, t.listener, &t.message)
            {
                return Err(bad());
            }
        }
        for (speaker, q) in &w.queues {
            if !self.person_index.contains_key(speaker) || q.len() > 128 {
                return Err(bad());
            }
            for message in q {
                if message.queued_at > self.host.clock().time
                    || !self.person_index.contains_key(&message.listener)
                    || *speaker == message.listener
                    || !army_message_valid(w, *speaker, message.listener, &message.message)
                {
                    return Err(bad());
                }
            }
        }
        for (actor, a) in &w.approaches {
            if *actor != a.actor
                || !self.person_index.contains_key(actor)
                || a.started > self.host.clock().time
                || a.completes <= self.host.clock().time
                || a.started
                    .0
                    .checked_add(self.host.state.combat.as_ref().unwrap().rules.round.0)
                    != Some(a.completes.0)
                || *self.host.storage.activity(*actor).expect("body exists")
                    != (Activity::ArmyApproach {
                        mission: a.mission,
                        encounter: a.encounter,
                        completes: a.completes,
                    })
                || self.validate_army_approach(a, false).is_err()
            {
                return Err(bad());
            }
        }
        for (actor, activity) in self.host.storage.activities() {
            if matches!(activity, Activity::ArmyTalk { .. }) != w.talks.contains_key(&actor) {
                return Err(bad());
            }
            if matches!(activity, Activity::ArmyApproach { .. })
                != w.approaches.contains_key(&actor)
            {
                return Err(bad());
            }
        }
        Ok(())
    }
}

fn army_message_valid(w: &World, speaker: PersonId, listener: PersonId, message: &Message) -> bool {
    let Some(a) = w.kernel.armies.get(&message.army()) else {
        return false;
    };
    match *message {
        Message::Invitation { mission, .. } => {
            a.commander == speaker
                && a.detachments
                    .get(&mission)
                    .is_some_and(|d| d.leader == listener)
        }
        Message::Participation {
            mission,
            ref statement,
            ..
        } => {
            statement.value.mission == mission
                && statement.value.leader == speaker
                && statement.source == KnowledgeSource::Observed(speaker)
                && statement.value.changed_at <= statement.observed_at
                && statement.observed_at <= statement.learned_at
                && statement.value.explanation.len() <= 512
                && a.commander == listener
                && a.detachments
                    .get(&mission)
                    .is_some_and(|d| d.leader == speaker)
        }
        Message::Order { order, mission, .. } => {
            a.commander == speaker
                && a.orders
                    .get(&order)
                    .and_then(|o| o.recipients.get(&mission))
                    .is_some_and(|r| r.leader == listener)
        }
        Message::Reply { order, mission, .. } => {
            a.commander == listener
                && a.orders
                    .get(&order)
                    .and_then(|o| o.recipients.get(&mission))
                    .is_some_and(|r| r.leader == speaker)
        }
        Message::Report { report, .. } => {
            a.reports.get(&report).is_some_and(|r| r.speaker == speaker)
        }
    }
}
fn army_saved_objective_valid(s: &CanonicalState, objective: &MissionObjective) -> bool {
    let area = |a: &AreaId| s.state.areas.iter().any(|p| p.id == *a);
    let person = |id: &PersonId| s.state.people.iter().any(|p| p.id == *id);
    match objective {
        MissionObjective::Visit { destination } => area(destination),
        MissionObjective::Deliver {
            source,
            destination,
            quantity,
        } => {
            quantity.0 > 0
                && source != destination
                && game_cargo::inventory(s.components, *(source))
                    .zip(game_cargo::inventory(s.components, *(destination)))
                    .is_some_and(|(a, b)| a.good == b.good)
        }
        MissionObjective::Defeat { person: p } | MissionObjective::Confine { person: p, .. } => {
            person(p)
        }
        MissionObjective::Rescue {
            person: p,
            return_to,
        } => person(p) && area(return_to),
        MissionObjective::Patrol { areas, .. } => {
            !areas.is_empty() && areas.len() <= 16 && areas.iter().all(area)
        }
    }
}
fn validate_army_mission_snapshot(s: &CanonicalState, v: &MissionView) -> Result<(), Error> {
    let bad = || Error::InvalidSave("Invalid dated army mission report".into());
    let people: BTreeSet<_> = s.state.people.iter().map(|p| p.id).collect();
    let areas: BTreeSet<_> = s.state.areas.iter().map(|a| a.id).collect();
    s.state.missions.as_ref().ok_or_else(bad)?;
    if !game_missions::mission(s.components, v.id).is_some()
        || !people.contains(&v.sponsor)
        || !people.contains(&v.leader)
        || !areas.contains(&v.home)
        || !army_saved_objective_valid(s, &v.objective)
        || v.observed_at > s.time
        || v.companions.len() > 32
        || v.recruitments.len() > 32
        || v.supplies.len() > 32
        || v.reported_by.is_some_and(|p| !people.contains(&p))
    {
        return Err(bad());
    }
    for c in &v.companions {
        if !people.contains(&c.person)
            || c.observed_at > s.time
            || c.location.as_ref().is_some_and(|l| match l {
                Location::AtArea(a) => !areas.contains(a),
                Location::Carried { carrier } => !people.contains(carrier),
                Location::StoppedOnRoad {
                    from,
                    to,
                    progress,
                    duration,
                } => {
                    !areas.contains(from)
                        || !areas.contains(to)
                        || from == to
                        || duration.0 == 0
                        || progress > duration
                }
                Location::InTransit {
                    from,
                    to,
                    departed,
                    arrives,
                    road_start,
                } => {
                    !areas.contains(from)
                        || !areas.contains(to)
                        || from == to
                        || departed >= arrives
                        || road_start.is_some_and(|p| !p.valid())
                }
            })
        {
            return Err(bad());
        }
    }
    for r in &v.recruitments {
        if !game_missions::recruitment(s.components, r.id)
            .is_some_and(|a| a.person == r.person && a.mission == v.id)
            || r.payment_agreement.is_some_and(|id| {
                s.state
                    .agreements
                    .as_ref()
                    .is_none_or(|_| game_agreements::agreement(s.components, id).is_none())
            })
            || r.compensation.iter().any(|t| {
                t.parties().iter().any(|p| !people.contains(p))
                    || match t {
                        AgreementTerm::Delivery {
                            source,
                            destination,
                            ..
                        } => {
                            !game_cargo::inventory(s.components, *(source)).is_some()
                                || !game_cargo::inventory(s.components, *(destination)).is_some()
                        }
                        AgreementTerm::Access { inventory, .. } => {
                            !game_cargo::inventory(s.components, *(inventory)).is_some()
                        }
                        _ => true,
                    }
            })
        {
            return Err(bad());
        }
    }
    if v.supplies.iter().any(|supply| {
        !people.contains(&supply.carrier)
            || !game_cargo::inventory(s.components, supply.source).is_some()
            || supply.quantity.0 == 0
    }) {
        return Err(bad());
    }
    Ok(())
}
