//! Institutional publication and social-case witnesses preserve personal locality.
use crate::institutions::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy)]
pub struct ObserverFacts {
    pub actor: PersonId,
    pub area: Option<AreaId>,
    pub alive: bool,
    pub independent: bool,
    pub conversation_allowed: bool,
    pub empathy: u16,
}
/// One borrowed roster index can serve every publication/receipt in a preparation.
pub struct ObservationIndex<'a> {
    actors: BTreeMap<PersonId, &'a ObserverFacts>,
    areas: BTreeMap<AreaId, Vec<&'a ObserverFacts>>,
}
impl<'a> ObservationIndex<'a> {
    pub fn new(actors: &'a [ObserverFacts]) -> Result<Self, Error> {
        let mut by_actor = BTreeMap::new();
        let mut areas = BTreeMap::<_, Vec<_>>::new();
        for actor in actors {
            if by_actor.insert(actor.actor, actor).is_some() {
                return Err(Error::Unavailable);
            }
            if let Some(area) = actor.area {
                areas.entry(area).or_default().push(actor);
            }
        }
        Ok(Self {
            actors: by_actor,
            areas,
        })
    }
    fn local(&self, area: AreaId) -> impl Iterator<Item = &ObserverFacts> {
        self.areas.get(&area).into_iter().flatten().copied()
    }
    pub fn case_observers(
        &self,
        case: &SocialCaseView,
        public: bool,
    ) -> Result<BTreeMap<PersonId, u16>, Error> {
        let mut observers = self
            .local(case.area)
            .filter(|actor| {
                actor.alive
                    && actor.independent
                    && (public || actor.actor == case.subject || actor.actor == case.counterpart)
            })
            .take(64)
            .map(|actor| (actor.actor, actor.empathy))
            .collect::<BTreeMap<_, _>>();
        let subject = self.actors.get(&case.subject).ok_or(Error::Unavailable)?;
        observers.insert(case.subject, subject.empathy);
        Ok(observers)
    }
    fn publication_witnesses(&self, institution: &Institution) -> Vec<PersonId> {
        let candidates = institution
            .charter
            .founders
            .iter()
            .chain(&institution.members)
            .chain(&institution.invited)
            .copied()
            .chain(
                self.local(institution.charter.jurisdiction)
                    .filter(|_| institution.charter.open_membership)
                    .map(|actor| actor.actor),
            )
            .collect::<BTreeSet<_>>();
        candidates
            .into_iter()
            .filter(|id| {
                self.actors.get(id).is_some_and(|actor| {
                    actor.area == Some(institution.charter.jurisdiction)
                        && actor.alive
                        && actor.independent
                        && actor.conversation_allowed
                })
            })
            .take(64)
            .collect()
    }
}
pub struct PreparedPublication {
    institution: Institution,
    proposals: Vec<RuleProposal>,
    before: Vec<(PersonId, Option<InstitutionView>)>,
    view: InstitutionView,
}
impl Runtime {
    pub fn actual_institution_view(
        &self,
        store: &Store,
        institution: &Institution,
        authority: &AuthorityFacts,
        at: SimTime,
    ) -> InstitutionView {
        InstitutionView {
            id: institution.id,
            origin: institution.origin,
            charter: institution.charter.clone(),
            members: institution.members.clone(),
            declined: institution.declined.clone(),
            status: institution.status,
            offices: institution
                .charter
                .offices
                .iter()
                .map(|office| OfficeView {
                    mandate: office.clone(),
                    supporters: institution
                        .office_support
                        .get(&office.slot)
                        .cloned()
                        .unwrap_or_default(),
                    effective: institution.office_effective(office, authority),
                })
                .collect(),
            proposals: crate::storage::proposals(store)
                .map(|(_, row)| row)
                .filter(|proposal| proposal.institution == institution.id)
                .rev()
                .take(32)
                .map(|proposal| RuleProposalView {
                    id: proposal.id,
                    proposer: proposal.proposer,
                    change: proposal.change.clone(),
                    votes: proposal.votes.clone(),
                    status: proposal.status,
                })
                .collect(),
            funding: institution.funding.clone(),
            duties: institution
                .applied
                .values()
                .flat_map(|people| people.values().copied())
                .collect(),
            observed_at: at,
        }
    }
    pub fn institution_views(&self, store: &Store, actor: PersonId) -> Vec<InstitutionView> {
        crate::storage::knowledge(store, actor).map_or_else(Vec::new, |memory| {
            memory
                .institutions
                .values()
                .rev()
                .take(64)
                .cloned()
                .collect()
        })
    }
    pub fn social_case_views(&self, store: &Store, actor: PersonId) -> Vec<SocialCaseView> {
        crate::storage::knowledge(store, actor).map_or_else(Vec::new, |memory| {
            memory
                .cases
                .iter()
                .rev()
                .take(64)
                .filter_map(|id| crate::storage::case(store, id).cloned())
                .collect()
        })
    }
    pub fn prepare_publication(
        &self,
        store: &Store,
        id: InstitutionId,
        authority: &AuthorityFacts,
        at: SimTime,
        index: &ObservationIndex<'_>,
    ) -> Result<PreparedPublication, Error> {
        let institution = crate::storage::institution(store, id).ok_or(Error::Unavailable)?;
        let before = index
            .publication_witnesses(institution)
            .into_iter()
            .map(|actor| {
                (
                    actor,
                    crate::storage::knowledge(store, actor)
                        .and_then(|memory| memory.institutions.get(&id))
                        .cloned(),
                )
            })
            .collect();
        Ok(PreparedPublication {
            institution: institution.clone(),
            proposals: crate::storage::proposals(store)
                .map(|(_, row)| row)
                .filter(|proposal| proposal.institution == id)
                .rev()
                .take(32)
                .cloned()
                .collect(),
            before,
            view: self.actual_institution_view(store, institution, authority, at),
        })
    }
}
impl PreparedPublication {
    pub fn verify(&self, store: &Store) -> Result<(), Error> {
        if crate::storage::institution(store, self.institution.id) != Some(&self.institution)
            || crate::storage::proposals(store)
                .map(|(_, row)| row)
                .filter(|proposal| proposal.institution == self.institution.id)
                .rev()
                .take(32)
                .ne(self.proposals.iter())
            || self.before.iter().any(|(actor, before)| {
                crate::storage::knowledge(store, actor)
                    .and_then(|memory| memory.institutions.get(&self.institution.id))
                    != before.as_ref()
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into(
        self,
        _world: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<(), Error> {
        self.verify(store)?;
        for (actor, _) in self.before {
            let mut row = crate::storage::knowledge(store, actor)
                .cloned()
                .unwrap_or_default();
            row.institutions
                .insert(self.institution.id, self.view.clone());
            batch.insert(StableId(actor.0), row);
        }
        Ok(())
    }
    pub fn commit(self, world: &Runtime, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage_into(world, store, &mut batch)?;
        store.apply_batch(batch)
    }
}
#[cfg(test)]
mod tests;

impl Runtime {
    pub fn institution_opportunities(
        &self,
        store: &Store,
        actor: PersonId,
        area: AreaId,
        authority: &AuthorityFacts,
    ) -> Vec<InstitutionAction> {
        let Some(memory) = crate::storage::knowledge(store, actor) else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        for id in memory.institutions.keys().rev().take(64) {
            let institution =
                crate::storage::institution(store, *id).expect("validated Society reference");
            if institution.charter.jurisdiction != area {
                continue;
            }
            if institution.status == InstitutionStatus::Proposed
                && institution.charter.founders.contains(&actor)
                && !institution.members.contains(&actor)
            {
                actions.push(InstitutionAction::Join { institution: *id });
                actions.push(InstitutionAction::Decline { institution: *id });
            }
            if institution.status == InstitutionStatus::Active {
                if institution.members.contains(&actor) {
                    actions.push(InstitutionAction::Leave { institution: *id });
                    for office in &institution.charter.offices {
                        actions.push(InstitutionAction::SupportOffice {
                            institution: *id,
                            office: office.slot,
                            support: !institution
                                .office_support
                                .get(&office.slot)
                                .is_some_and(|people| people.contains(&actor)),
                        });
                    }
                } else if institution.charter.open_membership
                    || institution.invited.contains(&actor)
                {
                    actions.push(InstitutionAction::Join { institution: *id });
                }
            }
        }
        for proposal in crate::storage::proposals(store)
            .map(|(_, row)| row)
            .rev()
            .filter(|proposal| proposal.status == RuleProposalStatus::Proposed)
            .take(32)
        {
            let institution = crate::storage::institution(store, proposal.institution)
                .expect("validated Society reference");
            if institution.charter.jurisdiction == area
                && institution.members.contains(&actor)
                && institution
                    .subjects(&institution.charter.procedure.voters, authority)
                    .contains(&actor)
                && !proposal.votes.contains_key(&actor)
            {
                actions.push(InstitutionAction::Vote {
                    proposal: proposal.id,
                    support: true,
                });
                actions.push(InstitutionAction::Vote {
                    proposal: proposal.id,
                    support: false,
                });
            }
        }
        actions
    }
}
