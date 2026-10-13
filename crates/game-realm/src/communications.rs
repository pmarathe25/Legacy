//! Timed reports freeze the sender's knowledge and require continuing contact.
use crate::*;
use game_core::*;
use game_ecs::{Batch, BatchKey, Component, ReadFootprint, StableId, Store};
use std::collections::BTreeSet;
pub trait TalkActivityFacts<C: Component> {
    fn report(&self, activity: &C) -> Option<TalkActivity>;
}
pub struct TalkActivityPublication {
    runtime: Runtime,
    guard: game_ecs::ReadGuard,
    talk: TalkBatchGuard,
}
impl TalkActivityPublication {
    pub fn validate(&self, runtime: &Runtime, store: &Store, batch: &Batch) -> Result<(), Error> {
        if runtime != &self.runtime {
            return Err(Error::Unavailable);
        }
        self.talk.validate(batch)?;
        self.guard.validate(store, batch)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportReplacement {
    key: BatchKey,
    speaker: PersonId,
    before: Option<PendingTalk>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TalkBatchGuard {
    key: BatchKey,
    edited: BTreeSet<PersonId>,
    dependencies: ReadFootprint,
    activity: ReadFootprint,
}
impl TalkBatchGuard {
    pub fn validate(&self, batch: &Batch) -> Result<(), Error> {
        let ids = batch
            .edited_ids::<PendingTalk>()
            .map(|id| PersonId(id.0))
            .collect::<Vec<_>>();
        if self.key != batch.key()
            || !self.dependencies.is_untouched(batch)
            || !self.activity.has_one_edit_per_row(batch)
            || ids.len() != self.edited.len()
            || ids.into_iter().collect::<BTreeSet<_>>() != self.edited
        {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedTalkStart {
    before: Option<PendingTalk>,
    after: PendingTalk,
    speaker: ActorFacts,
    recipient: ActorFacts,
}
impl PreparedTalkStart {
    pub fn pending(&self) -> &PendingTalk {
        &self.after
    }
    pub fn check(
        &self,
        runtime: &Runtime,
        store: &Store,
        now: SimTime,
        presence: &impl Presence,
    ) -> Result<(), Error> {
        if runtime
            .prepare_report_start(
                store,
                now,
                self.after.speaker,
                self.after.recipient,
                self.after.report.commitment,
                presence,
            )?
            .after
            != self.after
            || now != self.after.report.sent_at
            || presence.actor(self.after.speaker) != Some(self.speaker.clone())
            || presence.actor(self.after.recipient) != Some(self.recipient.clone())
            || !presence.same_place(self.after.speaker, self.after.recipient)
            || talk(store, self.after.speaker) != self.before.as_ref()
            || Ledger::new(store).prepare_report(
                self.after.speaker,
                self.after.report.commitment,
                now,
            )? != self.after.report
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into_with_replacement(
        self,
        runtime: &Runtime,
        store: &Store,
        now: SimTime,
        presence: &impl Presence,
        batch: &mut Batch,
    ) -> Result<ReportReplacement, Error> {
        self.check(runtime, store, now, presence)?;
        let id = StableId(self.after.speaker.0);
        if batch.touches::<PendingTalk>(id) {
            return Err(Error::Unavailable);
        }
        let token = ReportReplacement {
            key: batch.key(),
            speaker: self.after.speaker,
            before: self.before,
        };
        batch.insert(id, self.after);
        Ok(token)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedTalkCancellations {
    runtime: Runtime,
    actors: Vec<PersonId>,
    before: Vec<(PersonId, PendingTalk)>,
    pub events: Vec<RealmEvent>,
}
impl PreparedTalkCancellations {
    /// Realm interruptions clear any current Realm speech variant, including a
    /// changed deadline. The binding supplies only the factual variant stamp.
    pub fn activity_resets(&self, mut is_report: impl FnMut(PersonId) -> bool) -> Vec<PersonId> {
        self.before
            .iter()
            .filter_map(|(speaker, _)| is_report(*speaker).then_some(*speaker))
            .collect()
    }
    pub fn stage_into_with_activities<C: Component>(
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
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = ReadFootprint::new();
        let actors = self.actors.clone();
        reads.membership::<PendingTalk>(store, move |speaker, talk| {
            actors.contains(&PersonId(speaker.0)) || actors.contains(&talk.recipient)
        });
        for (id, before) in &self.before {
            let id = StableId(id.0);
            reads.expect_row(id, Some(before.clone()));
            let activity = reads.read::<C>(store, id);
            if activity.is_some_and(|activity| facts.report(activity).is_some()) {
                if batch.touches::<C>(id) {
                    return Err(Error::Unavailable);
                }
                batch.insert(id, idle.clone());
                admitted.protect::<C>(id);
            }
            admitted.protect::<PendingTalk>(id);
        }
        let expected_runtime = self.runtime.clone();
        let talk = self.stage_into(store, batch, None)?;
        Ok(TalkActivityPublication {
            runtime: expected_runtime,
            guard: reads.bind(store, batch, &admitted)?,
            talk,
        })
    }
    pub fn speakers(&self) -> impl Iterator<Item = PersonId> + '_ {
        self.before.iter().map(|(id, _)| *id)
    }
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut Batch,
        replacement: Option<&ReportReplacement>,
    ) -> Result<TalkBatchGuard, Error> {
        if self
            .before
            .iter()
            .any(|(id, before)| talk(store, *id) != Some(before))
            || replacement.is_some_and(|r| {
                r.key != batch.key() || talk(store, r.speaker) != r.before.as_ref()
            })
        {
            return Err(Error::Unavailable);
        }
        let mut edited = BTreeSet::new();
        if let Some(r) = replacement {
            edited.insert(r.speaker);
        }
        for (id, _) in self.before {
            if replacement.is_some_and(|r| r.speaker == id) {
                continue;
            }
            if batch.touches::<PendingTalk>(StableId(id.0)) {
                return Err(Error::Unavailable);
            }
            batch.remove::<PendingTalk>(StableId(id.0));
            edited.insert(id);
        }
        let guard = TalkBatchGuard {
            key: batch.key(),
            edited,
            dependencies: ReadFootprint::new(),
            activity: ReadFootprint::new(),
        };
        guard.validate(batch)?;
        Ok(guard)
    }
}
#[derive(Clone, Debug)]
pub struct ReportReception {
    pub sender: PersonId,
    pub recipient: PersonId,
    pub institutions: Vec<game_society::institutions::InstitutionView>,
    pub stocks: Vec<StockKnowledge>,
}
#[derive(Clone, Debug)]
pub struct PreparedTalkCompletion<C: Component> {
    runtime: Runtime,
    before: PendingTalk,
    at: SimTime,
    speaker: Option<ActorFacts>,
    recipient: Option<ActorFacts>,
    same_place: bool,
    activity_before: Option<C>,
    recipient_activity_before: Option<C>,
    dependencies: ReadFootprint,
    change: Option<PreparedChange>,
    pub reception: Option<ReportReception>,
    pub event: RealmEvent,
}
impl<C: Component> PreparedTalkCompletion<C> {
    pub fn stage_into_guarded(
        self,
        runtime: &Runtime,
        store: &Store,
        now: SimTime,
        presence: &impl Presence<Activity = C>,
        idle_activity: C,
        batch: &mut Batch,
    ) -> Result<(TalkActivityPublication, Option<RealmBatchGuard>), Error> {
        if runtime != &self.runtime {
            return Err(Error::Unavailable);
        }
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = ReadFootprint::new();
        let speaker = StableId(self.before.speaker.0);
        reads.expect_row(speaker, Some(self.before.clone()));
        reads.expect_row(speaker, self.activity_before.clone());
        reads.expect_row(
            StableId(self.before.recipient.0),
            self.recipient_activity_before.clone(),
        );
        admitted.protect::<PendingTalk>(speaker);
        admitted.protect::<C>(speaker);
        if let Some(change) = &self.change {
            if let Some(before) = &change.before {
                reads.expect_row(StableId(change.id.0), Some(before.clone()));
            }
            if change.after.is_some() {
                admitted.protect::<Commitment>(StableId(change.id.0));
            }
            for patch in &change.knowledge {
                reads.read::<PersonalKnowledge>(store, StableId(patch.actor.0));
                admitted.protect::<PersonalKnowledge>(StableId(patch.actor.0));
            }
        }
        let expected_runtime = self.runtime.clone();
        let (talk, realm) = self.stage_into(store, now, presence, idle_activity, batch)?;
        Ok((
            TalkActivityPublication {
                runtime: expected_runtime,
                guard: reads.bind(store, batch, &admitted)?,
                talk,
            },
            realm,
        ))
    }
    pub fn stage_into(
        self,
        store: &Store,
        now: SimTime,
        presence: &impl Presence<Activity = C>,
        idle_activity: C,
        batch: &mut Batch,
    ) -> Result<(TalkBatchGuard, Option<RealmBatchGuard>), Error> {
        if now != self.at
            || presence.actor(self.before.speaker) != self.speaker
            || presence.actor(self.before.recipient) != self.recipient
            || presence.same_place(self.before.speaker, self.before.recipient) != self.same_place
            || store.get::<C>(StableId(self.before.speaker.0)) != self.activity_before.as_ref()
            || store.get::<C>(StableId(self.before.recipient.0))
                != self.recipient_activity_before.as_ref()
            || talk(store, self.before.speaker) != Some(&self.before)
            || batch.touches::<C>(StableId(self.before.speaker.0))
            || batch.touches::<PendingTalk>(StableId(self.before.speaker.0))
        {
            return Err(Error::Unavailable);
        }
        let realm = self
            .change
            .map(|p| p.stage_into(store, batch))
            .transpose()?;
        batch.remove::<PendingTalk>(StableId(self.before.speaker.0));
        let mut dependencies = self.dependencies;
        dependencies.remove::<C>(StableId(self.before.speaker.0));
        let mut activity = ReadFootprint::new();
        activity.protect::<C>(StableId(self.before.speaker.0));
        batch.insert(StableId(self.before.speaker.0), idle_activity);
        let guard = TalkBatchGuard {
            key: batch.key(),
            edited: BTreeSet::from([self.before.speaker]),
            dependencies,
            activity,
        };
        guard.validate(batch)?;
        Ok((guard, realm))
    }
}
impl Runtime {
    pub fn actor_busy(&self, store: &Store, actor: PersonId) -> bool {
        talks(store).any(|(speaker, t)| speaker == actor || t.recipient == actor)
    }
    pub fn prepare_cancel_talks(
        &self,
        store: &Store,
        actors: &[PersonId],
    ) -> PreparedTalkCancellations {
        let before = talks(store)
            .filter(|(speaker, t)| actors.contains(speaker) || actors.contains(&t.recipient))
            .map(|(speaker, t)| (speaker, t.clone()))
            .collect::<Vec<_>>();
        let events = before
            .iter()
            .map(|(speaker, _)| RealmEvent::Interrupted { actor: *speaker })
            .collect();
        PreparedTalkCancellations {
            runtime: self.clone(),
            actors: actors.to_vec(),
            before,
            events,
        }
    }
    pub fn prepare_report_start(
        &self,
        store: &Store,
        now: SimTime,
        speaker: PersonId,
        recipient: PersonId,
        id: RealmCommitmentId,
        presence: &impl Presence,
    ) -> Result<PreparedTalkStart, Error> {
        self.rules.validate()?;
        let sender = presence.actor(speaker).ok_or(Error::Unavailable)?;
        let receiver = presence.actor(recipient).ok_or(Error::Unavailable)?;
        let area = contact_area(sender.clone()).ok_or(Error::Unavailable)?;
        if speaker == recipient
            || contact_area(receiver.clone()) != Some(area)
            || !presence.same_place(speaker, recipient)
            || !receiver.idle_or_rest
            || receiver.foreign_talking
            || self.actor_busy(store, recipient)
        {
            return Err(Error::Unavailable);
        }
        let report = Ledger::new(store).prepare_report(speaker, id, now)?;
        let referenced = report
            .snapshot
            .revisions
            .values()
            .flat_map(|r| r.proposal.value.participants.keys().copied())
            .collect::<BTreeSet<_>>();
        let stocks=game_characters::decisions::stocks(store,speaker).ok_or(Error::Unavailable)?.known.values().filter(|k|report.snapshot.revisions.values().any(|r|r.proposal.value.terms.iter().any(|term| match term {
            Term::Delivery{term:game_agreements::AgreementTerm::Delivery{source,destination,..},..}=>k.inventory==*source || k.inventory==*destination,
            Term::Protection{patrol,..}=>patrol.supplies.iter().any(|s|s.source==k.inventory) || patrol.compensation.values().flatten().any(|t|matches!(t,game_agreements::AgreementTerm::Delivery{source,destination,..} if k.inventory==*source || k.inventory==*destination)),_=>false,
        }))).take(64).cloned().collect();
        let institutions = institution_view(store, speaker)
            .into_iter()
            .filter(|i| referenced.contains(&i.id))
            .collect();
        let completes = SimTime(
            now.0
                .checked_add(self.rules.talk_time.0)
                .ok_or(Error::Overflow)?,
        );
        let after = PendingTalk {
            speaker,
            recipient,
            area,
            completes,
            report,
            institutions,
            stocks,
        };
        after.validate()?;
        Ok(PreparedTalkStart {
            before: talk(store, speaker).cloned(),
            after,
            speaker: sender,
            recipient: receiver,
        })
    }
    pub fn prepare_report_completion<P: Presence>(
        &self,
        store: &Store,
        now: SimTime,
        speaker: PersonId,
        presence: &P,
    ) -> Result<PreparedTalkCompletion<P::Activity>, Error> {
        let before = talk(store, speaker).cloned().ok_or(Error::Unavailable)?;
        if now < before.completes {
            return Err(Error::Unavailable);
        }
        let contact = actor_area(presence, speaker) == Some(before.area)
            && actor_area(presence, before.recipient) == Some(before.area)
            && presence.same_place(speaker, before.recipient)
            && presence
                .actor(before.recipient)
                .is_some_and(|p| p.idle_or_rest);
        let change = contact
            .then(|| {
                Ledger::new(store).prepare_receive_report(before.recipient, &before.report, now)
            })
            .transpose()
            .ok()
            .flatten();
        let reception = change.as_ref().map(|_| ReportReception {
            sender: speaker,
            recipient: before.recipient,
            institutions: before.institutions.clone(),
            stocks: before.stocks.clone(),
        });
        let event = if reception.is_some() {
            RealmEvent::Reported {
                commitment: before.report.commitment,
                sender: speaker,
                recipient: before.recipient,
            }
        } else {
            RealmEvent::Interrupted { actor: speaker }
        };
        let mut dependencies = presence.actor_reads(speaker);
        dependencies.extend(presence.actor_reads(before.recipient));
        dependencies.protect::<P::Activity>(StableId(before.recipient.0));
        let speaker_facts = presence.actor(speaker);
        let recipient = presence.actor(before.recipient);
        let same_place = presence.same_place(speaker, before.recipient);
        let activity_before = store.get::<P::Activity>(StableId(speaker.0)).cloned();
        let recipient_activity_before = store
            .get::<P::Activity>(StableId(before.recipient.0))
            .cloned();
        Ok(PreparedTalkCompletion {
            runtime: self.clone(),
            before,
            at: now,
            speaker: speaker_facts,
            recipient,
            same_place,
            activity_before,
            recipient_activity_before,
            dependencies,
            change,
            reception,
            event,
        })
    }
}
