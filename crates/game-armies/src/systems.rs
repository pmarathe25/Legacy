//! Army-owned communication, autonomy, coordinated approaches and validation.
use crate::*;
use game_core::*;
use game_missions::{CompanionStatus, MissionObjective, MissionStatus, MissionView};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
const MINUTE: SimDuration = SimDuration(60_000);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
        statement: Dated<Detachment>,
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
    pub(crate) fn army(&self) -> ArmyId {
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
#[serde(deny_unknown_fields)]
pub struct QueuedMessage {
    pub listener: PersonId,
    pub queued_at: SimTime,
    pub message: Message,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Talk {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub area: AreaId,
    pub started: SimTime,
    pub completes: SimTime,
    pub message: Message,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    pub rules: Rules,
    pub last_review: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Queue {
    pub messages: VecDeque<QueuedMessage>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// Borrowed facts and calls into the native owners. Army policy stays in `Systems`.
pub trait Ports {
    fn store(&self) -> &game_ecs::Store;
    fn store_mut(&mut self) -> &mut game_ecs::Store;
    fn army_runtime(&self) -> Option<&World>;
    fn army_runtime_mut(&mut self) -> Option<&mut World>;
    fn now(&self) -> SimTime;
    fn next_id(&self) -> u64;
    fn allocate_id(&mut self) -> Result<u64, Error>;
    fn ruler(&self) -> PersonId;
    fn people(&self) -> Vec<PersonId>;
    fn areas(&self) -> Vec<AreaId>;
    fn person_exists(&self, actor: PersonId) -> bool;
    fn independent(&self, actor: PersonId) -> bool;
    fn combat_restricts(&self, actor: PersonId) -> bool;
    fn actor_body(&self, actor: PersonId) -> Option<game_conflict::TargetBody>;
    fn resolved_location(&self, actor: PersonId) -> Location;
    fn location(&self, actor: PersonId) -> Location;
    fn fatigue(&self, actor: PersonId) -> Option<(u64, u64)>;
    fn encounter(&self, id: EncounterId) -> Option<&game_conflict::Encounter>;
    fn encounters(&self) -> Vec<&game_conflict::Encounter>;
    fn combat_map(&self, id: CombatMapId) -> Option<&game_conflict::Map>;
    fn combat_round(&self) -> SimDuration;
    fn known_routes(
        &self,
        actor: PersonId,
    ) -> Option<&BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>>;
    fn missions_enabled(&self) -> Option<()>;
    fn agreements_enabled(&self) -> Option<()>;
    fn activity(&self, actor: PersonId) -> Option<ActorActivity>;
    fn activities(&self) -> Vec<(PersonId, ActorActivity)>;
    fn set_activity(&mut self, actor: PersonId, activity: ActorActivity);
    fn conversation_actor_available(&self, actor: PersonId) -> bool;
    fn interrupt_actor_activity(&mut self, actor: PersonId);
    fn ensure_event_capacity(&self, count: usize) -> Result<(), Error>;
    fn mission_view(&self, actor: PersonId) -> Vec<MissionView>;
    fn mission_objective_valid(&self, objective: &MissionObjective) -> bool;
    /// Stage Army and native owner plans together before any rows or events publish.
    fn commit_army_change(&mut self, change: PreparedChange) -> Result<(), Error>;
    fn mission_leader_judgment(
        &self,
        actor: PersonId,
        mission: MissionId,
        objective: &MissionObjective,
    ) -> Result<i64, Error>;
    fn can_fight(&self, actor: PersonId) -> bool;
    fn mission_force_willing(&self, actor: PersonId, mission: MissionId, target: PersonId) -> bool;
    fn admit_local_army_approach(&mut self, approach: &Approach) -> Result<(), Error>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorActivity {
    Idle,
    Other,
    ArmyTalk {
        army: ArmyId,
        listener: PersonId,
        completes: SimTime,
    },
    ArmyApproach {
        mission: MissionId,
        encounter: EncounterId,
        completes: SimTime,
    },
}

pub trait Systems: Ports {
    fn army_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        if !self.person_exists(actor) || !self.independent(actor) || self.combat_restricts(actor) {
            return Err(Error::Unavailable);
        }
        let body = self.actor_body(actor).ok_or(Error::Unavailable)?;
        if !body.alive || body.health == 0 {
            return Err(Error::Unavailable);
        }
        match self.resolved_location(actor) {
            Location::AtArea(area) => Ok(area),
            _ => Err(Error::Unavailable),
        }
    }
    fn army_facts(&self, mission: MissionId) -> Result<DetachmentFacts, Error> {
        let m = self
            .missions_enabled()
            .and_then(|_| game_missions::mission(self.store(), mission))
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
        let w = self.army_runtime().ok_or(Error::Unavailable)?;
        if queue(self.store(), speaker)
            .map(|q| &q.messages)
            .map_or(0, VecDeque::len)
            .checked_add(additional)
            .is_none_or(|n| n > 128)
        {
            return Err(Error::Unavailable);
        }
        self.now()
            .0
            .checked_add(w.rules.talk_time.0)
            .ok_or(Error::Overflow)?;
        Ok(())
    }
    fn queue_army_message(&mut self, speaker: PersonId, listener: PersonId, message: Message) {
        let queued_at = self.now();
        enqueue(
            self.store_mut(),
            speaker,
            QueuedMessage {
                listener,
                queued_at,
                message,
            },
        )
        .expect("preflighted army queue");
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
            statement: Kernel::new(self.store())
                .view(speaker, army)
                .unwrap()
                .detachments[&mission]
                .clone(),
        }
    }
    fn commit_army_patch(&mut self, patch: PreparedChange) -> Result<(), Error> {
        self.commit_army_change(patch)
    }
    fn army_command(&mut self, actor: PersonId, action: &ArmyAction) -> Result<(), Error> {
        let area = self.army_area(actor)?;
        self.preflight_armies()?;
        self.army_runtime().ok_or(Error::Unavailable)?;
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
                self.next_id().checked_add(1).ok_or(Error::Overflow)?;
                let id = ArmyId(self.next_id());
                let patch = Kernel::new(self.store()).prepare_form(
                    id,
                    actor,
                    missions,
                    &facts,
                    self.now(),
                )?;
                self.commit_army_patch(patch)?;
                self.allocate_id()?;
                for mission in missions {
                    let leader = facts[mission].leader;
                    if leader == actor {
                        let p = Kernel::new(self.store()).prepare_consider_own_invitation(
                            id,
                            *mission,
                            actor,
                            self.now(),
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
                let commander = crate::army(self.store(), *army)
                    .ok_or(Error::Unavailable)?
                    .commander;
                self.army_queue_capacity(actor, usize::from(actor != commander))?;
                let patch = Kernel::new(self.store()).prepare_respond(
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
                    self.now(),
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
                let a = crate::army(self.store(), *army).ok_or(Error::Unavailable)?;
                for mission in missions {
                    let leader = a.detachments.get(mission).ok_or(Error::Unavailable)?.leader;
                    if self.army_area(leader).ok() != Some(area) {
                        return Err(Error::Unavailable);
                    }
                }
                self.army_queue_capacity(actor, missions.len())?;
                self.next_id().checked_add(1).ok_or(Error::Overflow)?;
                let id = ArmyOrderId(self.next_id());
                let p = Kernel::new(self.store()).prepare_order(
                    OrderProposal {
                        army: *army,
                        id,
                        commander: actor,
                        missions: missions.clone(),
                        objective: objective.clone(),
                        expires: *expires,
                    },
                    self.now(),
                )?;
                self.commit_army_patch(p)?;
                self.allocate_id()?;
                for mission in missions {
                    let leader = crate::army(self.store(), *army)
                        .expect("army exists")
                        .detachments[mission]
                        .leader;
                    if leader == actor {
                        let f = self.army_facts(*mission)?;
                        let p = Kernel::new(self.store()).prepare_consider_own_order(
                            *army,
                            id,
                            *mission,
                            actor,
                            &f,
                            self.now(),
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
                let commander = crate::army(self.store(), *army)
                    .ok_or(Error::Unavailable)?
                    .commander;
                self.army_queue_capacity(actor, usize::from(actor != commander))?;
                let p=Kernel::new(self.store()).prepare_respond_order(OrderDecision {army:*army,order:*order,mission:*mission,actor,response:*response,
                    explanation:format!("Personally chose {response:?} after considering the received army objective")},&facts,self.now())?;
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
                self.next_id().checked_add(1).ok_or(Error::Overflow)?;
                let report = ArmyReportId(self.next_id());
                let p = Kernel::new(self.store()).prepare_report(
                    *army,
                    ArmyReport {
                        id: report,
                        army: *army,
                        mission: *mission,
                        speaker: actor,
                        sent_at: self.now(),
                        snapshot,
                    },
                    self.now(),
                )?;
                self.commit_army_patch(p)?;
                self.allocate_id()?;
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
                let commander = crate::army(self.store(), *army)
                    .ok_or(Error::Unavailable)?
                    .commander;
                self.army_queue_capacity(actor, usize::from(actor != commander))?;
                let p = Kernel::new(self.store()).prepare_leave(
                    *army,
                    *mission,
                    actor,
                    *reason,
                    "Personally withdrew support for this command relationship".into(),
                    self.now(),
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
                let p = Kernel::new(self.store()).prepare_disband(*army, actor, self.now())?;
                self.commit_army_patch(p)?;
            }
        }
        self.start_queued_army_talk(actor)?;
        self.reconcile_armies()?;
        Ok(())
    }
    fn start_queued_army_talk(&mut self, speaker: PersonId) -> Result<(), Error> {
        let owner_now = self.now();
        let w = self.army_runtime().ok_or(Error::Unavailable)?;
        if talks(self.store())
            .map(|(_, row)| row)
            .any(|t| t.speaker == speaker || t.listener == speaker)
        {
            return Ok(());
        }
        let Some(message) = queue(self.store(), speaker)
            .map(|q| &q.messages)
            .and_then(|q| q.front())
            .cloned()
        else {
            return Ok(());
        };
        let Ok(area) = self.army_area(speaker) else {
            return Ok(());
        };
        if self.army_area(message.listener).ok() != Some(area)
            || talks(self.store())
                .map(|(_, row)| row)
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
        pop_queue(self.store_mut(), speaker);
        self.set_activity(
            speaker,
            ActorActivity::ArmyTalk {
                army: message.message.army(),
                listener: message.listener,
                completes,
            },
        );
        insert_talk(
            self.store_mut(),
            speaker,
            Talk {
                speaker,
                listener: message.listener,
                area,
                started: owner_now,
                completes,
                message: message.message,
            },
        )?;
        Ok(())
    }
    fn interrupt_army_talk(&mut self, actor: PersonId) {
        let Some(_runtime) = self.army_runtime() else {
            return;
        };
        let talks: Vec<_> = talks(self.store())
            .map(|(_, row)| row)
            .filter(|t| t.speaker == actor || t.listener == actor)
            .cloned()
            .collect();
        for talk in talks {
            remove_talk(self.store_mut(), talk.speaker).expect("registered army talk");
            if matches!(self.activity(talk.speaker).expect("body exists"),ActorActivity::ArmyTalk {completes,..} if completes==talk.completes)
            {
                self.set_activity(talk.speaker, ActorActivity::Idle);
            }
        }
        if approach(self.store(), actor).is_some()
            && remove_approach(self.store_mut(), actor)
                .expect("registered approach")
                .is_some()
            && matches!(
                self.activity(actor).expect("body exists"),
                ActorActivity::ArmyApproach { .. }
            )
        {
            self.set_activity(actor, ActorActivity::Idle);
        }
        // Interrupted speech is not received or silently retried.
    }
    fn army_boundary(&self, limit: SimTime) -> SimTime {
        let Some(_runtime) = self.army_runtime() else {
            return limit;
        };
        talks(self.store())
            .map(|(_, row)| row)
            .map(|t| t.completes)
            .chain(
                approaches(self.store())
                    .map(|(_, row)| row)
                    .map(|a| a.completes),
            )
            .chain(Kernel::new(self.store()).next_boundary(self.now()))
            .filter(|at| *at > self.now())
            .min()
            .map_or(limit, |at| at.min(limit))
    }
    fn army_actor_talking(&self, actor: PersonId) -> bool {
        self.army_runtime().is_some_and(|_| {
            talks(self.store())
                .map(|(_, row)| row)
                .any(|t| t.speaker == actor || t.listener == actor)
        })
    }
    fn army_allied_missions(&self, mission: MissionId) -> BTreeSet<MissionId> {
        let Some(_runtime) = self.army_runtime() else {
            return BTreeSet::from([mission]);
        };
        let Some(id) = Kernel::new(self.store()).commanding_army(mission) else {
            return BTreeSet::from([mission]);
        };
        crate::army(self.store(), id)
            .expect("army exists")
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
        self.army_runtime()?;
        let army = Kernel::new(self.store()).commanding_army(mission)?;
        let a = crate::army(self.store(), army).expect("army exists");
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
        self.army_runtime()?;
        let a = crate::army(self.store(), army)?;
        let m = game_missions::mission(self.store(), mission)?;
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
            if actor != self.ruler() {
                return Some(target);
            }
            let known = Kernel::new(self.store()).view(actor, army)?;
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
        Kernel::new(self.store())
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
        let e = self.encounter(encounter)?;
        let map = self.combat_map(e.map)?;
        let routes = self.known_routes(actor)?;
        map.exits
            .iter()
            .filter(|(to, p)| {
                routes.get(&e.area).is_some_and(|r| r.contains_key(to)) && map.clear(*p, *p)
            })
            .min_by_key(|(to, _)| *to)
            .map(|(_, p)| *p)
    }
    fn start_army_approach(
        &mut self,
        actor: PersonId,
        mission: MissionId,
        encounter: EncounterId,
    ) -> Result<(), Error> {
        let area = self.army_area(actor)?;
        if self.location(actor) != Location::AtArea(area)
            || self.army_actor_talking(actor)
            || self
                .army_runtime()
                .is_none_or(|_| approach(self.store(), actor).is_some())
        {
            return Err(Error::Unavailable);
        }
        let army = Kernel::new(self.store())
            .commanding_army(mission)
            .ok_or(Error::Unavailable)?;
        let target = self
            .army_personal_force_target(actor, army, mission, area)
            .ok_or(Error::Unavailable)?;
        let (army, order) = self
            .army_order_for_approach(mission, target)
            .ok_or(Error::Unavailable)?;
        let completes = SimTime(
            self.now()
                .0
                .checked_add(self.combat_round().0)
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
            started: self.now(),
            completes,
        };
        self.validate_army_approach(&approach, false)?;
        self.interrupt_actor_activity(actor);
        self.set_activity(
            actor,
            ActorActivity::ArmyApproach {
                mission,
                encounter,
                completes,
            },
        );
        insert_approach(self.store_mut(), actor, approach)?;
        Ok(())
    }
    fn validate_army_approach(&self, a: &Approach, completed: bool) -> Result<(u32, Point), Error> {
        self.army_runtime().ok_or(Error::Unavailable)?;
        let army = crate::army(self.store(), a.army).ok_or(Error::Unavailable)?;
        let order = army.orders.get(&a.order).ok_or(Error::Unavailable)?;
        let e = self.encounter(a.encounter).ok_or(Error::Unavailable)?;
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
            || !self
                .fatigue(a.actor)
                .is_some_and(|(current, limit)| current < limit)
            || self.location(a.actor) != Location::AtArea(a.area)
            || (army.commander != a.actor
                && !self.mission_force_willing(a.actor, a.mission, a.target))
            || self.army_personal_force_target(a.actor, a.army, a.mission, a.area) != Some(a.target)
            || !matches!(
                game_missions::mission(self.store(), a.mission)
                    .expect("record exists")
                    .status,
                MissionStatus::Traveling | MissionStatus::Operating
            )
            || e.fighters
                .get(&a.target)
                .is_none_or(|f| f.withdrawn || f.surrendered)
            || !self.actor_body(a.target).is_some_and(|b| b.alive)
            || (completed
                && (a.completes != self.now()
                    || approach(self.store(), a.actor) != Some(a)
                    || self.activity(a.actor)
                        != Some(ActorActivity::ArmyApproach {
                            mission: a.mission,
                            encounter: a.encounter,
                            completes: a.completes,
                        })))
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
                        game_missions::mission(self.store(), *id)
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
    fn reconcile_armies(&mut self) -> Result<(), Error> {
        let Some(_runtime) = self.army_runtime() else {
            return Ok(());
        };
        let invalid: Vec<_> = talks(self.store())
            .map(|(_, row)| row)
            .filter(|t| {
                self.army_area(t.speaker).ok() != Some(t.area)
                    || self.army_area(t.listener).ok() != Some(t.area)
            })
            .map(|t| t.speaker)
            .collect();
        for speaker in invalid {
            self.interrupt_army_talk(speaker);
        }
        let approaches: Vec<_> = approaches(self.store())
            .map(|(_, row)| row)
            .cloned()
            .collect();
        for approach in approaches {
            if self.validate_army_approach(&approach, false).is_err() {
                self.interrupt_army_talk(approach.actor);
                continue;
            }
            if approach.completes == self.now()
                && self.admit_local_army_approach(&approach).is_err()
            {
                self.interrupt_army_talk(approach.actor);
            }
        }
        let talks: Vec<_> = talks(self.store())
            .map(|(_, row)| row)
            .filter(|t| t.completes <= self.now())
            .cloned()
            .collect();
        for talk in talks {
            let valid = self.army_area(talk.speaker).ok() == Some(talk.area)
                && self.army_area(talk.listener).ok() == Some(talk.area)
                && self.activity(talk.speaker).expect("body exists")
                    == ActorActivity::ArmyTalk {
                        army: talk.message.army(),
                        listener: talk.listener,
                        completes: talk.completes,
                    };
            remove_talk(self.store_mut(), talk.speaker).expect("registered army talk");
            if matches!(self.activity(talk.speaker).expect("body exists"),ActorActivity::ArmyTalk {completes,..} if completes==talk.completes)
            {
                self.set_activity(talk.speaker, ActorActivity::Idle);
            }
            if !valid {
                continue;
            }
            let receipt = Communication {
                speaker: talk.speaker,
                listener: talk.listener,
                started_at: talk.started,
                completed_at: self.now(),
            };
            let k = &Kernel::new(self.store());
            let patch = match talk.message {
                Message::Invitation { army, mission } => {
                    k.prepare_hear_invitation(army, mission, receipt, self.now())
                }
                Message::Participation {
                    army,
                    mission,
                    statement,
                } => {
                    k.prepare_receive_participation(army, mission, &statement, receipt, self.now())
                }
                Message::Order {
                    army,
                    order,
                    mission,
                } => self.army_facts(mission).and_then(|f| {
                    k.prepare_hear_order(army, order, mission, receipt, &f, self.now())
                }),
                Message::Reply {
                    army,
                    order,
                    mission,
                } => k.prepare_hear_order_response(army, order, mission, receipt, self.now()),
                Message::Report { army, report } => {
                    k.prepare_hear_report(army, report, receipt, self.now())
                }
            };
            if let Ok(p) = patch {
                self.commit_army_patch(p)?;
            }
            self.start_queued_army_talk(talk.speaker)?;
        }
        let ids: Vec<_> = armies(self.store()).map(|(id, _)| id).collect();
        for id in ids {
            let a = crate::army(self.store(), id).expect("army exists");
            if a.orders.values().any(|o| {
                o.expires <= self.now()
                    && o.recipients.values().any(|r| {
                        matches!(
                            r.status,
                            OrderStatus::AwaitingDelivery | OrderStatus::Received
                        )
                    })
            }) {
                let expired = Kernel::new(self.store()).prepare_expire(id, self.now())?;
                self.commit_army_patch(expired)?;
            }
            let missions: Vec<_> = crate::army(self.store(), id)
                .expect("army exists")
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
                    let p =
                        Kernel::new(self.store()).prepare_native_termination(id, &f, self.now())?;
                    self.commit_army_patch(p)?;
                }
            }
        }
        Ok(())
    }
    fn review_armies(&mut self) -> Result<(), Error> {
        let Some(w) = self.army_runtime() else {
            return Ok(());
        };
        if w.last_review == self.now() || !self.now().0.is_multiple_of(MINUTE.0) {
            return Ok(());
        }
        self.preflight_armies()?;
        let now = self.now();
        self.army_runtime_mut().unwrap().last_review = now;
        let actors: Vec<_> = knowledges(self.store()).map(|(id, _)| id).collect();
        for actor in actors {
            if actor == self.ruler()
                || self.army_area(actor).is_err()
                || talks(self.store())
                    .map(|(_, row)| row)
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
                    let objective = &game_missions::mission(self.store(), mission)
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
                            || self.now() >= o.value.expires
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
        let speakers: Vec<_> = queues(self.store()).map(|(id, _)| id).collect();
        for speaker in speakers {
            self.start_queued_army_talk(speaker)?;
        }
        Ok(())
    }
    fn army_view(&self, actor: PersonId) -> Vec<ArmyView> {
        let mut views = self
            .army_runtime()
            .map_or_else(Vec::new, |_| Kernel::new(self.store()).views(actor));
        let Ok(area) = self.army_area(actor) else {
            return views;
        };
        for view in &mut views {
            for mission in view.knowledge.detachments.keys() {
                let Some(target) = self.army_personal_force_target(actor, view.id, *mission, area)
                else {
                    continue;
                };
                for e in self.encounters().into_iter().filter(|e| {
                    e.active
                        && e.location == Location::AtArea(area)
                        && e.fighters.contains_key(&target)
                }) {
                    let Some(entry) = self.army_entrance(actor, e.id) else {
                        continue;
                    };
                    let map = self.combat_map(e.map).unwrap();
                    let linked = self.army_allied_missions(*mission);
                    let visible_ally = e.fighters.values().any(|f| {
                        !f.withdrawn
                            && !f.surrendered
                            && map.clear(entry, f.position)
                            && linked.iter().any(|id| {
                                game_missions::mission(self.store(), *id)
                                    .expect("record exists")
                                    .members
                                    .get(&f.actor)
                                    .is_some_and(|p| p.status == CompanionStatus::Accepted)
                            })
                    });
                    if visible_ally {
                        view.local_engagements.push(LocalEngagement {
                            mission: *mission,
                            encounter: e.id,
                            target,
                            observed_at: self.now(),
                        });
                    }
                }
            }
        }
        views
    }
    fn available_army_commands(&self, actor: PersonId) -> Vec<ArmyAction> {
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
                if self.now() >= o.value.expires {
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
    }
    fn army_allocation_bound(&self) -> u64 {
        0
    }
    fn army_event_bound(&self) -> usize {
        self.army_runtime().map_or(0, |_| {
            armies(self.store())
                .map(|(_, a)| a)
                .map(|a| a.detachments.len() * 64)
                .sum::<usize>()
                + talks(self.store()).count() * 64
        })
    }
    fn preflight_armies(&self) -> Result<(), Error> {
        self.preflight_armies_at(self.now())
    }
    fn preflight_armies_at(&self, at: SimTime) -> Result<(), Error> {
        if let Some(w) = self.army_runtime() {
            at.0.checked_add(w.rules.talk_time.0)
                .ok_or(Error::Overflow)?;
            at.0.checked_add(self.combat_round().0)
                .ok_or(Error::Overflow)?;
            self.ensure_event_capacity(self.army_event_bound())?;
        }
        Ok(())
    }
    fn validate_armies(&self) -> Result<(), Error> {
        let Some(w) = self.army_runtime() else {
            if armies(self.store()).next().is_some()
                || knowledges(self.store()).next().is_some()
                || talks(self.store()).next().is_some()
                || queues(self.store()).next().is_some()
                || approaches(self.store()).next().is_some()
            {
                return Err(Error::InvalidSave(
                    "Army components without their runtime".into(),
                ));
            }
            return Ok(());
        };
        w.rules.validate()?;
        let bad = || Error::InvalidSave("Invalid army communication or native references".into());
        self.missions_enabled().ok_or_else(bad)?;
        let people = self.people().into_iter().collect();
        let areas = self.areas().into_iter().collect();
        let mission_ids = game_missions::missions(self.store())
            .map(|(id, _)| id)
            .collect();
        Kernel::new(self.store()).validate(
            &ValidationCatalog {
                people: &people,
                missions: &mission_ids,
                now: self.now(),
            },
            |o| {
                if game_missions::saved_objective_valid(self.store(), o, &people, &areas) {
                    Ok(())
                } else {
                    Err(bad())
                }
            },
            |v| {
                game_missions::validate_saved_view(
                    self.store(),
                    v,
                    self.now(),
                    &people,
                    &areas,
                    self.agreements_enabled().is_some(),
                )
            },
        )?;
        if w.last_review > self.now() {
            return Err(bad());
        }
        for a in armies(self.store()).map(|(_, a)| a) {
            if a.id.0 >= self.next_id()
                || a.orders.keys().any(|id| id.0 >= self.next_id())
                || a.reports.keys().any(|id| id.0 >= self.next_id())
            {
                return Err(bad());
            }
        }
        let mut occupied = BTreeSet::new();
        for (speaker, t) in talks(self.store()) {
            if speaker != t.speaker
                || !occupied.insert(speaker)
                || !occupied.insert(t.listener)
                || t.started > self.now()
                || t.completes <= self.now()
                || t.started.0.checked_add(w.rules.talk_time.0) != Some(t.completes.0)
                || self.army_area(speaker).ok() != Some(t.area)
                || self.army_area(t.listener).ok() != Some(t.area)
                || self.activity(speaker).expect("body exists")
                    != (ActorActivity::ArmyTalk {
                        army: t.message.army(),
                        listener: t.listener,
                        completes: t.completes,
                    })
                || !army_message_valid(self.store(), speaker, t.listener, &t.message)
            {
                return Err(bad());
            }
        }
        for (speaker, row) in queues(self.store()) {
            let q = &row.messages;
            if !self.person_exists(speaker) || q.len() > 128 {
                return Err(bad());
            }
            for message in q {
                if message.queued_at > self.now()
                    || !self.person_exists(message.listener)
                    || speaker == message.listener
                    || !army_message_valid(
                        self.store(),
                        speaker,
                        message.listener,
                        &message.message,
                    )
                {
                    return Err(bad());
                }
            }
        }
        for (actor, a) in approaches(self.store()) {
            if actor != a.actor
                || !self.person_exists(actor)
                || a.started > self.now()
                || a.completes <= self.now()
                || a.started.0.checked_add(self.combat_round().0) != Some(a.completes.0)
                || self.activity(actor).expect("body exists")
                    != (ActorActivity::ArmyApproach {
                        mission: a.mission,
                        encounter: a.encounter,
                        completes: a.completes,
                    })
                || self.validate_army_approach(a, false).is_err()
            {
                return Err(bad());
            }
        }
        for (actor, activity) in self.activities() {
            if matches!(activity, ActorActivity::ArmyTalk { .. })
                != talk(self.store(), actor).is_some()
            {
                return Err(bad());
            }
            if matches!(activity, ActorActivity::ArmyApproach { .. })
                != approach(self.store(), actor).is_some()
            {
                return Err(bad());
            }
        }
        Ok(())
    }
}
impl<T: Ports + ?Sized> Systems for T {}
fn army_message_valid(
    store: &game_ecs::Store,
    speaker: PersonId,
    listener: PersonId,
    message: &Message,
) -> bool {
    let Some(a) = crate::army(store, message.army()) else {
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
