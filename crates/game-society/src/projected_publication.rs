//! Native command publications are prepared from the selected projected rows.
use crate::institutions::*;
use crate::perception::ObservationIndex;
use crate::*;
use game_core::*;
use game_ecs::{Batch, BatchKey, Store};
use std::collections::BTreeSet;
#[derive(Clone, Debug)]
pub struct PreparedCommandPublication {
    pub(crate) institution: Institution,
    pub(crate) source: Option<Institution>,
    pub(crate) source_proposals: Vec<RuleProposal>,
    pub(crate) projected_proposals: Vec<RuleProposal>,
    pub(crate) before: Vec<(PersonId, Option<InstitutionView>)>,
    pub(crate) view: InstitutionView,
}
impl PreparedCommandPublication {
    pub fn institution(&self) -> InstitutionId {
        self.institution.id
    }
    pub fn recipients(&self) -> impl Iterator<Item = PersonId> + '_ {
        self.before.iter().map(|(actor, _)| *actor)
    }
    /// These are the receiving Society owner's capped views after publication.
    /// A regional receiver never substitutes a hidden global institutional view.
    pub fn personal_views(&self, store: &Store) -> Vec<(PersonId, Vec<InstitutionView>)> {
        self.before
            .iter()
            .map(|(actor, _)| {
                let ids = knowledge(store, *actor)
                    .into_iter()
                    .flat_map(|k| k.institutions.keys().copied())
                    .chain([self.view.id])
                    .collect::<BTreeSet<_>>();
                let views = ids
                    .into_iter()
                    .rev()
                    .take(64)
                    .filter_map(|id| {
                        if id == self.view.id {
                            Some(self.view.clone())
                        } else {
                            knowledge(store, *actor)
                                .and_then(|k| k.institutions.get(&id))
                                .cloned()
                        }
                    })
                    .collect();
                (*actor, views)
            })
            .collect()
    }
    pub(crate) fn verify(&self, store: &Store) -> Result<(), Error> {
        if institution(store, self.institution.id) != self.source.as_ref()
            || current_proposals(store, self.institution.id) != self.source_proposals
            || self.before.iter().any(|(actor, before)| {
                knowledge(store, *actor).and_then(|k| k.institutions.get(&self.institution.id))
                    != before.as_ref()
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}
pub(crate) fn current_proposals(store: &Store, id: InstitutionId) -> Vec<RuleProposal> {
    proposals(store)
        .map(|(_, p)| p)
        .filter(|p| p.institution == id)
        .rev()
        .take(32)
        .cloned()
        .collect()
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicationStamp {
    row: Institution,
    institution: InstitutionView,
    recipients: Vec<PersonId>,
}
impl PreparedCommandPublication {
    fn stamp(&self) -> PublicationStamp {
        PublicationStamp {
            row: self.institution.clone(),
            institution: self.view.clone(),
            recipients: self.recipients().collect(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct CommandPublicationGuard {
    key: BatchKey,
    people: BTreeSet<PersonId>,
    publications: Vec<PublicationStamp>,
    institutions: BTreeSet<InstitutionId>,
    preserved: BTreeSet<InstitutionId>,
    readonly: BTreeSet<InstitutionId>,
}
impl CommandPublicationGuard {
    pub fn preserved_institutions(&self) -> impl Iterator<Item = InstitutionId> + '_ {
        self.preserved.iter().copied()
    }
    pub fn knowledge_receivers(&self) -> impl Iterator<Item = PersonId> + '_ {
        self.people.iter().copied()
    }
    pub fn authorizes_publications(
        &self,
        batch: &Batch,
        publications: &[PreparedCommandPublication],
    ) -> Result<(), Error> {
        self.validate(batch)?;
        if self.publications
            != publications
                .iter()
                .map(PreparedCommandPublication::stamp)
                .collect::<Vec<_>>()
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn validate(&self, batch: &Batch) -> Result<(), Error> {
        if self.key != batch.key()
            || self
                .readonly
                .iter()
                .any(|id| batch.touches::<Institution>(game_ecs::StableId(id.0)))
            || self.institutions.iter().any(|id| {
                batch
                    .edited_ids::<Institution>()
                    .filter(|edited| edited.0 == id.0)
                    .count()
                    != 1
            })
            || self.people.iter().any(|actor| {
                batch
                    .edited_ids::<Knowledge>()
                    .filter(|id| id.0 == actor.0)
                    .count()
                    != 1
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub(crate) fn new(
        batch: &Batch,
        people: BTreeSet<PersonId>,
        publications: &[PreparedCommandPublication],
        institutions: BTreeSet<InstitutionId>,
        preserved: BTreeSet<InstitutionId>,
        readonly: BTreeSet<InstitutionId>,
    ) -> Self {
        Self {
            key: batch.key(),
            people,
            institutions,
            preserved,
            readonly,
            publications: publications
                .iter()
                .map(PreparedCommandPublication::stamp)
                .collect(),
        }
    }
}
impl Runtime {
    pub(crate) fn projected_publication(
        &self,
        store: &Store,
        institution: &Institution,
        projected_proposals: Vec<RuleProposal>,
        authority: &AuthorityFacts,
        at: SimTime,
        index: &ObservationIndex<'_>,
    ) -> PreparedCommandPublication {
        let before = index
            .publication_witnesses(institution)
            .into_iter()
            .map(|actor| {
                (
                    actor,
                    knowledge(store, actor)
                        .and_then(|k| k.institutions.get(&institution.id))
                        .cloned(),
                )
            })
            .collect();
        let mut view = self.actual_institution_view(store, institution, authority, at);
        view.proposals = projected_proposals
            .iter()
            .map(|p| RuleProposalView {
                id: p.id,
                proposer: p.proposer,
                change: p.change.clone(),
                votes: p.votes.clone(),
                status: p.status,
            })
            .collect();
        PreparedCommandPublication {
            institution: institution.clone(),
            source: crate::institution(store, institution.id).cloned(),
            source_proposals: current_proposals(store, institution.id),
            projected_proposals,
            before,
            view,
        }
    }
}

/// The Society owner caps the receiver's views after these exact publications.
pub fn combined_personal_views(
    store: &Store,
    actor: PersonId,
    publications: &[PreparedCommandPublication],
) -> Vec<InstitutionView> {
    let selected = publications
        .iter()
        .filter(|p| p.recipients().any(|id| id == actor))
        .collect::<Vec<_>>();
    let ids = knowledge(store, actor)
        .into_iter()
        .flat_map(|k| k.institutions.keys().copied())
        .chain(selected.iter().map(|p| p.view.id))
        .collect::<BTreeSet<_>>();
    ids.into_iter()
        .rev()
        .take(64)
        .filter_map(|id| {
            selected
                .iter()
                .rev()
                .find(|p| p.view.id == id)
                .map(|p| &p.view)
                .or_else(|| knowledge(store, actor).and_then(|k| k.institutions.get(&id)))
                .cloned()
        })
        .collect()
}
