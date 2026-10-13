//! Conversations transmit dated information only after local elapsed speech.
use crate::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
/// A representation adapter supplies the pending deadline from a native
/// activity variant; matching and cleanup are service policy.
pub trait TalkActivityFacts<C: game_ecs::Component> {
    fn deadline(&self, activity: &C) -> Option<SimTime>;
}
pub struct TalkActivityPublication {
    runtime: Runtime,
    guard: game_ecs::ReadGuard,
}
impl TalkActivityPublication {
    pub(crate) fn new(runtime: Runtime, guard: game_ecs::ReadGuard) -> Self {
        Self { runtime, guard }
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store, batch: &Batch) -> Result<(), Error> {
        if runtime != &self.runtime {
            return Err(Error::Unavailable);
        }
        self.guard.validate(store, batch)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TalkScene {
    pub speaker_area: Option<AreaId>,
    pub listener_area: Option<AreaId>,
    pub speaker_combat: bool,
    pub listener_combat: bool,
    pub active_deadline: Option<SimTime>,
}
pub struct TalkReplacement {
    speaker: PersonId,
}
impl TalkReplacement {
    pub fn speaker(&self) -> PersonId {
        self.speaker
    }
}
pub struct PreparedTalkStart {
    before: Option<PendingTalk>,
    pub talk: PendingTalk,
}
impl Runtime {
    pub fn prepare_talk_start(
        &self,
        store: &Store,
        speaker: PersonId,
        listener: PersonId,
        conversation: Conversation,
        now: SimTime,
        scene: TalkScene,
    ) -> Result<PreparedTalkStart, Error> {
        let area = scene.speaker_area.ok_or(Error::Unavailable)?;
        if scene.speaker_combat || scene.listener_combat || scene.listener_area != Some(area) {
            return Err(Error::Unavailable);
        }
        match &conversation {
            Conversation::Objective {
                mission: id,
                leader,
                ..
            }
            | Conversation::Retreat {
                mission: id,
                leader,
            } => {
                if mission(store, *id).is_none_or(|m| {
                    m.proposal.leader != *leader
                        || listener != *leader
                        || (speaker != m.sponsor && speaker != *leader)
                }) {
                    return Err(Error::Unavailable);
                }
            }
            Conversation::Report {
                recipient,
                snapshot,
            } => {
                if *recipient != listener
                    || speaker == listener
                    || knowledge(store, speaker).and_then(|k| k.missions.get(&snapshot.id))
                        != Some(snapshot.as_ref())
                {
                    return Err(Error::Unavailable);
                }
            }
        }
        let talk = PendingTalk {
            speaker,
            listener,
            area,
            completes: SimTime(now.0.checked_add(60_000).ok_or(Error::Overflow)?),
            conversation,
        };
        talk.validate()?;
        Ok(PreparedTalkStart {
            before: crate::talk(store, speaker).cloned(),
            talk,
        })
    }
    pub fn actor_talking(&self, store: &Store, actor: PersonId) -> bool {
        talks(store).any(|(_, t)| t.speaker == actor || t.listener == actor)
    }
    pub fn due_talks(&self, store: &Store, now: SimTime) -> Vec<PersonId> {
        talks(store)
            .filter_map(|(id, t)| (t.completes <= now).then_some(id))
            .collect()
    }
}
impl PreparedTalkStart {
    pub fn stage_into_with_replacement(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<TalkReplacement, Error> {
        let replacement = TalkReplacement {
            speaker: self.talk.speaker,
        };
        self.stage_into(store, batch)?;
        Ok(replacement)
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        if talk(store, self.talk.speaker) != self.before.as_ref()
            || batch.touches::<PendingTalk>(StableId(self.talk.speaker.0))
        {
            return Err(Error::Unavailable);
        }
        batch.insert(StableId(self.talk.speaker.0), self.talk);
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct CommunicatedOrder {
    pub mission: MissionId,
    pub leader: PersonId,
    pub objective: MissionObjective,
    pub retreat: bool,
}
pub struct PreparedTalkCompletion {
    runtime: Runtime,
    before: PendingTalk,
    pub clear: ClearTalk,
    pub order: Option<CommunicatedOrder>,
    report: Option<PreparedObservation>,
    pub events: Vec<MissionEvent>,
}
impl Runtime {
    pub fn completion_order(&self, store: &Store, speaker: PersonId) -> Option<CommunicatedOrder> {
        match &talk(store, speaker)?.conversation {
            Conversation::Objective {
                mission,
                leader,
                objective,
            } => Some(CommunicatedOrder {
                mission: *mission,
                leader: *leader,
                objective: objective.clone(),
                retreat: false,
            }),
            Conversation::Retreat {
                mission: id,
                leader,
            } => Some(CommunicatedOrder {
                mission: *id,
                leader: *leader,
                objective: MissionObjective::Visit {
                    destination: mission(store, *id)?.proposal.home,
                },
                retreat: true,
            }),
            Conversation::Report { .. } => None,
        }
    }
    pub fn prepare_talk_completion(
        &self,
        store: &Store,
        speaker: PersonId,
        now: SimTime,
        scene: TalkScene,
        authority: Option<&CommandFacts>,
    ) -> Result<PreparedTalkCompletion, Error> {
        let before = talk(store, speaker).ok_or(Error::Unavailable)?.clone();
        if before.completes > now {
            return Err(Error::Unavailable);
        }
        let valid = scene.speaker_area == Some(before.area)
            && scene.listener_area == Some(before.area)
            && scene.active_deadline == Some(before.completes);
        let mut result = PreparedTalkCompletion {
            runtime: self.clone(),
            clear: ClearTalk {
                speaker,
                completes: before.completes,
            },
            before,
            order: None,
            report: None,
            events: Vec::new(),
        };
        if !valid {
            return Ok(result);
        }
        match &result.before.conversation {
            Conversation::Objective {
                mission: id,
                objective,
                ..
            } => {
                if authority.is_some_and(|facts| {
                    self.validate_order(store, speaker, *id, objective, facts)
                        .is_ok()
                }) {
                    result.order = self.completion_order(store, speaker);
                }
            }
            Conversation::Retreat { mission: id, .. } => {
                if mission(store, *id).is_some_and(|m| {
                    matches!(
                        m.status,
                        MissionStatus::Traveling | MissionStatus::Operating
                    )
                }) {
                    result.order = self.completion_order(store, speaker);
                }
            }
            Conversation::Report {
                recipient,
                snapshot,
            } => {
                result.report = self.prepare_report(store, speaker, *recipient, snapshot, now)?;
                result.events.push(MissionEvent::Reported {
                    mission: snapshot.id,
                    speaker,
                    recipient: *recipient,
                });
            }
        }
        Ok(result)
    }
}
impl PreparedTalkCompletion {
    pub fn stage_into_with_activities<C: game_ecs::Component>(
        self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl TalkActivityFacts<C>,
        idle: C,
        batch: &mut Batch,
    ) -> Result<TalkActivityPublication, Error> {
        if runtime != &self.runtime {
            return Err(Error::Unavailable);
        }
        let id = StableId(self.before.speaker.0);
        let mut reads = game_ecs::ReadSet::new();
        reads.expect_row(id, Some(self.before.clone()));
        let activity = reads.read::<C>(store, id);
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<PendingTalk>(id);
        if activity.is_some_and(|activity| facts.deadline(activity) == Some(self.before.completes))
        {
            if batch.touches::<C>(id) {
                return Err(Error::Unavailable);
            }
            batch.insert(id, idle);
            admitted.protect::<C>(id);
        }
        if let Some(report) = &self.report {
            report.add_reads(store, &mut reads);
            admitted.protect::<Knowledge>(StableId(report.observer().0));
        }
        let expected_runtime = self.runtime.clone();
        self.stage_into(store, batch)?;
        Ok(TalkActivityPublication::new(
            expected_runtime,
            reads.bind(store, batch, &admitted)?,
        ))
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        if talk(store, self.before.speaker) != Some(&self.before)
            || batch.touches::<PendingTalk>(StableId(self.before.speaker.0))
        {
            return Err(Error::Unavailable);
        }
        if let Some(report) = self.report {
            report.stage_into(store, batch)?;
        }
        batch.remove::<PendingTalk>(StableId(self.before.speaker.0));
        Ok(())
    }
}
