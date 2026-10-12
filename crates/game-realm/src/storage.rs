//! Regional records live in the host store; this crate retains no entity world.
use crate::*;
use game_core::*;
use game_ecs::{Batch, BatchKey, Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Index,
};

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
                "realm talk time must be positive".into(),
            ))
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub rules: Rules,
    pub last_event: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalKnowledge {
    pub commitments: BTreeMap<RealmCommitmentId, Knowledge>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requests {
    pub reconsider_after: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PendingOwner {
    Agreement(AgreementId),
    Mission(MissionId),
    Passage(RuleProposalId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingWork {
    pub actor: PersonId,
    pub owner: PendingOwner,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingTalk {
    pub speaker: PersonId,
    pub recipient: PersonId,
    pub area: AreaId,
    pub completes: SimTime,
    pub report: Report,
    pub institutions: Vec<game_society::institutions::InstitutionView>,
    pub stocks: Vec<StockKnowledge>,
}
fn invalid() -> Error {
    Error::InvalidConfiguration("invalid Realm component".into())
}
impl Commitment {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0 || self.revisions.is_empty() {
            return Err(invalid());
        }
        let mut prior = SimTime(0);
        let participants = self.revisions[0]
            .proposal
            .participants
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        for (number, r) in self.revisions.iter().enumerate() {
            if usize::try_from(r.number).ok() != Some(number)
                || r.proposed_at < prior
                || r.proposal.ratify_by <= r.proposed_at
                || !(2..=8).contains(&r.proposal.participants.len())
                || !(1..=16).contains(&r.proposal.terms.len())
                || r.proposal
                    .participants
                    .keys()
                    .copied()
                    .collect::<BTreeSet<_>>()
                    != participants
                || !r
                    .proposal
                    .participants
                    .values()
                    .any(|p| p.representative == r.proposal.proposer)
                || r.proposal.participants.iter().any(|(id, p)| {
                    id.0 == 0
                        || *id != p.institution
                        || p.settlement.0 == 0
                        || p.representative.0 == 0
                })
                || r.requested
                    .iter()
                    .any(|i| usize::from(*i) >= r.proposal.terms.len())
                || r.backed.iter().any(|(i, b)| {
                    !r.requested.contains(i)
                        || r.proposal
                            .terms
                            .get(usize::from(*i))
                            .is_none_or(|t| !crate::persistence::backing_shape(t, b))
                })
                || r.pending.iter().any(|(i, p)| {
                    !r.requested.contains(i)
                        || p.actor.0 == 0
                        || match p.owner {
                            PendingOwner::Agreement(id) => id.0 == 0,
                            PendingOwner::Mission(id) => id.0 == 0,
                            PendingOwner::Passage(id) => id.0 == 0,
                        }
                })
            {
                return Err(invalid());
            }
            prior = r.proposed_at;
        }
        Ok(())
    }
}
impl PersonalKnowledge {
    pub fn validate(&self) -> Result<(), Error> {
        if self.commitments.iter().any(|(id, k)| {
            id.0 == 0
                || k.revisions.values().any(|r| {
                    r.proposal.observed_at > r.proposal.learned_at
                        || r.ratifications
                            .values()
                            .any(|d| d.observed_at > d.learned_at)
                        || r.backed.values().any(|b| b.observed_at > b.learned_at)
                })
        }) {
            Err(invalid())
        } else {
            Ok(())
        }
    }
}
impl PendingTalk {
    pub fn validate(&self) -> Result<(), Error> {
        if self.speaker.0 == 0
            || self.recipient.0 == 0
            || self.speaker == self.recipient
            || self.area.0 == 0
            || self.report.speaker != self.speaker
            || self.report.commitment.0 == 0
            || self.report.sent_at >= self.completes
            || self.stocks.len() > 64
            || self
                .institutions
                .iter()
                .any(|i| i.observed_at > self.report.sent_at)
            || self
                .stocks
                .iter()
                .any(|s| s.observed_at > s.learned_at || s.learned_at > self.report.sent_at)
        {
            Err(invalid())
        } else {
            Ok(())
        }
    }
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Commitment>("realm/commitment", Commitment::validate)?;
    schema.register::<PersonalKnowledge>("realm/knowledge", PersonalKnowledge::validate)?;
    schema.register::<Requests>("realm/requests", |_| Ok(()))?;
    schema.register::<PendingTalk>("realm/talk", PendingTalk::validate)
}
pub fn commitment(store: &Store, id: RealmCommitmentId) -> Option<&Commitment> {
    store.get(StableId(id.0))
}
pub fn commitments(store: &Store) -> impl Iterator<Item = (RealmCommitmentId, &Commitment)> {
    store
        .iter::<Commitment>()
        .map(|(id, row)| (RealmCommitmentId(id.0), row))
}
pub fn insert_commitment(store: &mut Store, row: Commitment) -> Result<(), Error> {
    store.insert_or_spawn(StableId(row.id.0), row)
}
pub fn knowledge(store: &Store, id: PersonId) -> Option<&PersonalKnowledge> {
    store.get(StableId(id.0))
}
pub fn knowledges(store: &Store) -> impl Iterator<Item = (PersonId, &PersonalKnowledge)> {
    store
        .iter::<PersonalKnowledge>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_knowledge(
    store: &mut Store,
    id: PersonId,
    row: PersonalKnowledge,
) -> Result<(), Error> {
    store.insert_or_spawn(StableId(id.0), row)
}
pub fn requests(store: &Store, id: PersonId) -> Option<&Requests> {
    store.get(StableId(id.0))
}
pub fn request_rows(store: &Store) -> impl Iterator<Item = (PersonId, &Requests)> {
    store
        .iter::<Requests>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_requests(store: &mut Store, id: PersonId, row: Requests) -> Result<(), Error> {
    store.insert_or_spawn(StableId(id.0), row)
}
pub fn talk(store: &Store, id: PersonId) -> Option<&PendingTalk> {
    store.get(StableId(id.0))
}
pub fn talks(store: &Store) -> impl Iterator<Item = (PersonId, &PendingTalk)> {
    store
        .iter::<PendingTalk>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_talk(store: &mut Store, row: PendingTalk) -> Result<(), Error> {
    store.insert_or_spawn(StableId(row.speaker.0), row)
}
/// A borrowed query facade, never a private store or canonical entity cache.
pub struct Ledger<'a> {
    pub(crate) store: &'a Store,
    pub(crate) commitments: CommitmentRows<'a>,
}
impl<'a> Ledger<'a> {
    pub(crate) fn projected(store: &'a Store, projected: &'a Commitment) -> Self {
        Self {
            store,
            commitments: CommitmentRows {
                store,
                projected: Some(projected),
            },
        }
    }
    pub fn new(store: &'a Store) -> Self {
        Self {
            store,
            commitments: CommitmentRows {
                store,
                projected: None,
            },
        }
    }
}
pub(crate) struct CommitmentRows<'a> {
    store: &'a Store,
    projected: Option<&'a Commitment>,
}
impl<'a> CommitmentRows<'a> {
    pub(crate) fn projection(&self, id: RealmCommitmentId) -> Option<&'a Commitment> {
        self.projected.filter(|row| row.id == id)
    }
    pub fn get(&self, id: &RealmCommitmentId) -> Option<&'a Commitment> {
        self.projected
            .filter(|row| row.id == *id)
            .or_else(|| commitment(self.store, *id))
    }
    pub fn values(&self) -> impl Iterator<Item = &'a Commitment> {
        self.iter().map(|(_, row)| row)
    }
    pub fn iter(&self) -> impl Iterator<Item = (RealmCommitmentId, &'a Commitment)> {
        let projected = self.projected;
        commitments(self.store)
            .map(move |(id, row)| (id, projected.filter(|p| p.id == id).unwrap_or(row)))
    }
}
impl Index<&RealmCommitmentId> for CommitmentRows<'_> {
    type Output = Commitment;
    fn index(&self, id: &RealmCommitmentId) -> &Self::Output {
        self.get(id).expect("Realm commitment exists")
    }
}
#[derive(Clone, Debug)]
pub struct RealmBatchGuard {
    key: BatchKey,
    commitments: BTreeSet<RealmCommitmentId>,
    people: BTreeSet<PersonId>,
    reads: Vec<crate::reads::WorkRead>,
    fresh: BTreeSet<RealmCommitmentId>,
    dependencies: game_ecs::ReadFootprint,
    projected: Option<game_society::projected_publication::CommandPublicationGuard>,
    agreement: Option<game_agreements::ProposalPublicationFinalization>,
}
impl RealmBatchGuard {
    pub(crate) fn new(
        batch: &Batch,
        commitments: BTreeSet<RealmCommitmentId>,
        people: BTreeSet<PersonId>,
        fresh: BTreeSet<RealmCommitmentId>,
        reads: Vec<crate::reads::WorkRead>,
    ) -> Self {
        Self {
            key: batch.key(),
            commitments,
            people,
            reads,
            fresh,
            dependencies: game_ecs::ReadFootprint::new(),
            projected: None,
            agreement: None,
        }
    }
    pub(crate) fn protect_reads(&mut self, reads: game_ecs::ReadFootprint) {
        self.dependencies.extend(reads);
    }
    pub(crate) fn protect_agreement_reads(
        &mut self,
        mut reads: game_ecs::ReadFootprint,
        source: &game_agreements::ProposalPublicationFinalization,
        batch: &Batch,
        identity: (AgreementId, PersonId),
        parties: &BTreeSet<PersonId>,
        term: &game_agreements::AgreementTerm,
    ) -> Result<(), Error> {
        let (agreement, actor) = identity;
        source.authorizes_creation(batch, agreement, actor, parties, term)?;
        reads.remove::<game_agreements::Knowledge>(StableId(actor.0));
        self.dependencies.extend(reads);
        self.agreement = Some(source.clone());
        Ok(())
    }
    pub(crate) fn protect_projected_reads(
        &mut self,
        mut reads: game_ecs::ReadFootprint,
        source: &game_society::projected_publication::CommandPublicationGuard,
        batch: &Batch,
    ) -> Result<(), Error> {
        source.validate(batch)?;
        for id in source.preserved_institutions() {
            reads.remove::<game_society::institutions::Institution>(StableId(id.0));
        }
        for id in source.knowledge_receivers() {
            reads.remove::<game_society::Knowledge>(StableId(id.0));
        }
        self.dependencies.extend(reads);
        self.projected = Some(source.clone());
        Ok(())
    }
    pub fn validate(&self, batch: &Batch) -> Result<(), Error> {
        if !self.dependencies.is_untouched(batch) {
            return Err(Error::Unavailable);
        }
        if let Some(source) = &self.projected {
            source.validate(batch)?;
        }
        if let Some(source) = &self.agreement {
            source.validate_batch(batch)?;
        }
        if self
            .fresh
            .iter()
            .any(|id| batch.edit_count(StableId(id.0)) != 1)
        {
            return Err(Error::Unavailable);
        }
        for read in &self.reads {
            read.check_batch(batch)?;
        }
        let commitments = batch
            .edited_ids::<Commitment>()
            .map(|id| RealmCommitmentId(id.0))
            .collect::<Vec<_>>();
        let people = batch
            .edited_ids::<PersonalKnowledge>()
            .map(|id| PersonId(id.0))
            .collect::<Vec<_>>();
        if self.key != batch.key()
            || commitments.len() != self.commitments.len()
            || commitments.into_iter().collect::<BTreeSet<_>>() != self.commitments
            || people.len() != self.people.len()
            || people.into_iter().collect::<BTreeSet<_>>() != self.people
        {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
}
