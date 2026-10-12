use super::*;
use game_ecs::{Batch, StableId, Store};

/// Admission receipt from the owner of a regional or enforcement change. Its
/// exact action, institution revision and actor are bound to this attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalChangePermit {
    pub actor: PersonId,
    pub institution: InstitutionId,
    pub institution_revision: u64,
    pub change: InstitutionChange,
}
#[derive(Clone, Copy, Debug)]
pub struct CommandIds {
    pub first: u64,
}
#[derive(Clone, Debug)]
pub struct CommandFacts<'a> {
    pub actor: PersonId,
    pub area: AreaId,
    pub now: SimTime,
    pub references: &'a ReferenceFacts,
    pub authority: &'a AuthorityFacts,
    pub external_permit: Option<&'a ExternalChangePermit>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandEffect {
    Funding {
        institution: InstitutionId,
        agreement: AgreementId,
        obligation: ObligationId,
        term: AgreementTerm,
    },
    Proposed {
        institution: InstitutionId,
        proposer: PersonId,
    },
    Founded {
        institution: InstitutionId,
    },
    Left {
        institution: InstitutionId,
        actor: PersonId,
    },
    OfficeSupportChanged {
        institution: InstitutionId,
        office: u16,
        actor: PersonId,
        support: bool,
    },
    Changed {
        institution: InstitutionId,
        proposal: RuleProposalId,
    },
    ExternalOutcome {
        institution: InstitutionId,
        proposal: RuleProposalId,
        outcome: RuleProposalStatus,
        change: InstitutionChange,
    },
    Publish {
        institution: InstitutionId,
    },
    ObserveRegional,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedCommand {
    reads: Vec<Institution>,
    institutions: Vec<(Option<Institution>, Institution)>,
    proposals: Vec<(Option<RuleProposal>, RuleProposal)>,
    concepts: Vec<(ConceptId, InstitutionalRule)>,
    evidence: Vec<(PersonId, BTreeSet<CaseId>, BTreeSet<CaseId>)>,
    resolution: Option<PreparedResolution>,
    pub next_id: u64,
    pub effects: Vec<CommandEffect>,
}
impl PreparedCommand {
    pub fn created_proposals(&self) -> impl Iterator<Item = RuleProposalId> + '_ {
        self.proposals
            .iter()
            .filter(|(before, _)| before.is_none())
            .map(|(_, after)| after.id)
    }
    /// Selected facts for publication of an owner-prepared institutional row.
    pub fn publication_institution<'a>(
        &'a self,
        store: &'a Store,
        id: InstitutionId,
    ) -> Result<&'a Institution, Error> {
        self.institutions
            .iter()
            .find(|(_, after)| after.id == id)
            .map(|(_, after)| after)
            .or_else(|| {
                self.resolution
                    .as_ref()
                    .and_then(|r| r.institution_after.as_ref())
                    .filter(|i| i.id == id)
            })
            .or_else(|| crate::institution(store, id))
            .ok_or(Error::Unavailable)
    }
    pub fn publication_people(
        &self,
        store: &Store,
        id: InstitutionId,
        local: impl IntoIterator<Item = PersonId>,
    ) -> Result<BTreeSet<PersonId>, Error> {
        let i = self.publication_institution(store, id)?;
        Ok(i.charter
            .founders
            .iter()
            .chain(&i.members)
            .chain(&i.invited)
            .copied()
            .chain(local.into_iter().filter(|_| i.charter.open_membership))
            .collect())
    }
    pub fn publication_authority_people(
        &self,
        store: &Store,
        id: InstitutionId,
    ) -> Result<BTreeSet<PersonId>, Error> {
        let i = self.publication_institution(store, id)?;
        Ok(i.charter
            .offices
            .iter()
            .map(|o| o.holder)
            .chain(
                i.office_support
                    .values()
                    .flat_map(|ids| ids.iter().copied()),
            )
            .collect())
    }
    fn publication_proposals(&self, store: &Store, id: InstitutionId) -> Vec<RuleProposal> {
        let mut rows = crate::projected_publication::current_proposals(store, id)
            .into_iter()
            .map(|p| (p.id, p))
            .collect::<BTreeMap<_, _>>();
        for (_, after) in &self.proposals {
            if after.institution == id {
                rows.insert(after.id, after.clone());
            }
        }
        if let Some(r) = &self.resolution {
            for (_, after) in &r.proposals {
                if after.institution == id {
                    rows.insert(after.id, after.clone());
                }
            }
        }
        rows.into_values().rev().take(32).collect()
    }
    pub fn prepare_publications(
        &self,
        world: &Runtime,
        store: &Store,
        authority: &AuthorityFacts,
        at: SimTime,
        index: &crate::perception::ObservationIndex<'_>,
    ) -> Result<Vec<crate::projected_publication::PreparedCommandPublication>, Error> {
        self.verify(store)?;
        self.effects
            .iter()
            .filter_map(|e| {
                if let CommandEffect::Publish { institution } = e {
                    Some(*institution)
                } else {
                    None
                }
            })
            .map(|id| {
                Ok(world.projected_publication(
                    store,
                    self.publication_institution(store, id)?,
                    self.publication_proposals(store, id),
                    authority,
                    at,
                    index,
                ))
            })
            .collect()
    }
    pub fn stage_with_publications(
        mut self,
        world: &Runtime,
        store: &Store,
        publications: Vec<crate::projected_publication::PreparedCommandPublication>,
        batch: &mut Batch,
    ) -> Result<crate::projected_publication::CommandPublicationGuard, Error> {
        self.verify(store)?;
        let mut after = BTreeMap::<PersonId, crate::Knowledge>::new();
        let receipts = publications.clone();
        if receipts
            .iter()
            .any(|p| batch.touches::<Institution>(StableId(p.institution.id.0)))
        {
            return Err(Error::Unavailable);
        }
        for publication in publications {
            publication.verify(store)?;
            if self.publication_institution(store, publication.institution.id)?
                != &publication.institution
                || self.publication_proposals(store, publication.institution.id)
                    != publication.projected_proposals
            {
                return Err(Error::Unavailable);
            }
            for (actor, _) in publication.before {
                after
                    .entry(actor)
                    .or_insert_with(|| crate::knowledge(store, actor).cloned().unwrap_or_default())
                    .institutions
                    .insert(publication.view.id, publication.view.clone());
            }
        }
        for (actor, _, cases) in std::mem::take(&mut self.evidence) {
            after
                .entry(actor)
                .or_insert_with(|| crate::knowledge(store, actor).cloned().unwrap_or_default())
                .cases = cases;
        }
        let mut institutions = self
            .institutions
            .iter()
            .map(|(_, row)| row.id)
            .collect::<BTreeSet<_>>();
        self.stage_into(world, store, batch)?;
        institutions.extend(
            receipts
                .iter()
                .filter(|p| batch.touches::<Institution>(StableId(p.institution.id.0)))
                .map(|p| p.institution.id),
        );
        let preserved = receipts
            .iter()
            .filter(|p| {
                institutions.contains(&p.institution.id)
                    && p.source.as_ref() == Some(&p.institution)
            })
            .map(|p| p.institution.id)
            .collect();
        let readonly = receipts
            .iter()
            .filter(|p| !institutions.contains(&p.institution.id))
            .map(|p| p.institution.id)
            .collect();
        let people = after.keys().copied().collect();
        for (actor, row) in after {
            if batch.touches::<crate::Knowledge>(StableId(actor.0)) {
                return Err(Error::Unavailable);
            }
            batch.insert(StableId(actor.0), row);
        }
        let guard = crate::projected_publication::CommandPublicationGuard::new(
            batch,
            people,
            &receipts,
            institutions,
            preserved,
            readonly,
        );
        guard.validate(batch)?;
        Ok(guard)
    }
    pub fn verify(&self, store: &Store) -> Result<(), Error> {
        if self
            .reads
            .iter()
            .any(|before| crate::storage::institution(store, before.id) != Some(before))
            || self.institutions.iter().any(|(before, after)| {
                crate::storage::institution(store, after.id) != before.as_ref()
            })
            || self
                .proposals
                .iter()
                .any(|(before, after)| crate::storage::proposal(store, after.id) != before.as_ref())
            || self
                .concepts
                .iter()
                .any(|(id, _)| crate::storage::concept(store, id).is_some())
            || self.evidence.iter().any(|(actor, before, _)| {
                crate::storage::knowledge(store, actor).is_none_or(|memory| memory.cases != *before)
            })
        {
            return Err(Error::Unavailable);
        }
        if let Some(resolution) = &self.resolution {
            resolution.verify(store)?;
        }
        Ok(())
    }
    pub fn stage_into(
        self,
        world: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<(), Error> {
        self.verify(store)?;
        if let Some(resolution) = self.resolution {
            return resolution.stage_into(world, store, batch);
        }
        for (_, row) in self.institutions {
            batch.insert(StableId(row.id.0), row);
        }
        for (_, row) in self.proposals {
            batch.insert(StableId(row.id.0), row);
        }
        for (id, rule) in self.concepts {
            batch.insert(StableId(id.0), Concept { rule });
        }
        for (actor, _, cases) in self.evidence {
            let mut row = crate::storage::knowledge(store, actor)
                .ok_or(Error::Unavailable)?
                .clone();
            row.cases = cases;
            batch.insert(StableId(actor.0), row);
        }
        Ok(())
    }
    pub fn commit(self, world: &Runtime, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage_into(world, store, &mut batch)?;
        store.apply_batch(batch)
    }
    fn allocate(&mut self) -> Result<u64, Error> {
        let id = self.next_id;
        if id == 0 {
            return Err(Error::Unavailable);
        }
        self.next_id = id.checked_add(1).ok_or(Error::Overflow)?;
        Ok(id)
    }
}
fn external(change: &InstitutionChange) -> bool {
    matches!(
        change,
        InstitutionChange::RatifyRegional { .. }
            | InstitutionChange::AuthorizeRegionalPassage { .. }
            | InstitutionChange::EnforcementPolicy { .. }
            | InstitutionChange::Adjudicate { .. }
    )
}
fn regional(change: &InstitutionChange) -> bool {
    matches!(
        change,
        InstitutionChange::RatifyRegional { .. }
            | InstitutionChange::AuthorizeRegionalPassage { .. }
    )
}
fn external_permit(
    institution: &Institution,
    change: &InstitutionChange,
    facts: &CommandFacts<'_>,
) -> Result<(), Error> {
    if external(change)
        && !facts.external_permit.is_some_and(|permit| {
            permit.actor == facts.actor
                && permit.institution == institution.id
                && permit.institution_revision == institution.revision
                && permit.change == *change
        })
    {
        return Err(Error::Unavailable);
    }
    Ok(())
}
impl Runtime {
    pub fn prepare_command(
        &self,
        store: &Store,
        action: &InstitutionAction,
        facts: &CommandFacts<'_>,
        ids: CommandIds,
    ) -> Result<PreparedCommand, Error> {
        let actor = facts.actor;
        let area = facts.area;
        if !facts.references.people.contains(&actor)
            || facts.references.eligible_locations.get(&actor) != Some(&area)
        {
            return Err(Error::Unavailable);
        }
        let mut plan = PreparedCommand {
            reads: Vec::new(),
            institutions: Vec::new(),
            proposals: Vec::new(),
            concepts: Vec::new(),
            evidence: Vec::new(),
            resolution: None,
            next_id: ids.first,
            effects: Vec::new(),
        };
        match action {
            InstitutionAction::Found { charter, origin } => {
                facts
                    .references
                    .validate_charter(store, charter, actor, area, facts.now)?;
                if let Some(origin) = origin {
                    let prior =
                        crate::storage::institution(store, origin).ok_or(Error::Unavailable)?;
                    if !charter.founders.is_subset(&prior.members) {
                        return Err(Error::Unavailable);
                    }
                    plan.reads.push(prior.clone());
                }
                let id = InstitutionId(plan.allocate()?);
                if crate::storage::institution(store, id).is_some() {
                    return Err(Error::Unavailable);
                }
                let mut concepts = Vec::new();
                for rule in &charter.rules {
                    let concept = ConceptId(plan.allocate()?);
                    if crate::storage::concept(store, concept).is_some() {
                        return Err(Error::Unavailable);
                    }
                    plan.concepts.push((concept, rule.clone()));
                    concepts.push(concept);
                }
                plan.institutions.push((
                    None,
                    Institution {
                        id,
                        origin: *origin,
                        charter: charter.clone(),
                        members: BTreeSet::from([actor]),
                        declined: BTreeSet::new(),
                        invited: charter.founders.clone(),
                        status: InstitutionStatus::Proposed,
                        concepts,
                        office_support: BTreeMap::new(),
                        grants: BTreeMap::new(),
                        applied: BTreeMap::new(),
                        funding: Vec::new(),
                        revision: 0,
                    },
                ));
                for recipient in &charter.founders {
                    let before = crate::storage::knowledge(store, recipient)
                        .ok_or(Error::Unavailable)?
                        .cases
                        .clone();
                    let mut after = before.clone();
                    after.extend(&charter.evidence);
                    plan.evidence.push((*recipient, before, after));
                }
                plan.effects.push(CommandEffect::Proposed {
                    institution: id,
                    proposer: actor,
                });
                plan.effects
                    .push(CommandEffect::Publish { institution: id });
            }
            InstitutionAction::Join { institution } => {
                let before =
                    crate::storage::institution(store, institution).ok_or(Error::Unavailable)?;
                if before.charter.jurisdiction != area
                    || before.members.contains(&actor)
                    || before.members.len() >= 64
                    || !matches!(
                        before.status,
                        InstitutionStatus::Proposed | InstitutionStatus::Active
                    )
                    || before.status == InstitutionStatus::Proposed
                        && !before.charter.founders.contains(&actor)
                    || before.status == InstitutionStatus::Active
                        && !before.charter.open_membership
                        && !before.invited.contains(&actor)
                {
                    return Err(Error::Unavailable);
                }
                let founding = before.status == InstitutionStatus::Proposed
                    && before.members.len() + 1 == before.charter.founders.len();
                if founding
                    && (before.charter.founders.iter().any(|person| {
                        facts.references.eligible_locations.get(person) != Some(&area)
                    }) || before.charter.contributions.iter().any(|term| {
                        term.deadline()
                            .is_some_and(|deadline| deadline <= facts.now)
                    }))
                {
                    return Err(Error::Unavailable);
                }
                let mut after = before.clone();
                after.members.insert(actor);
                after.declined.remove(&actor);
                for office in &after.charter.offices {
                    after
                        .office_support
                        .entry(office.slot)
                        .or_default()
                        .insert(actor);
                }
                if founding {
                    for term in &after.charter.contributions {
                        let agreement = AgreementId(plan.allocate()?);
                        let obligation = ObligationId(plan.allocate()?);
                        after.funding.push(agreement);
                        plan.effects.push(CommandEffect::Funding {
                            institution: *institution,
                            agreement,
                            obligation,
                            term: term.clone(),
                        });
                    }
                    after.status = InstitutionStatus::Active;
                    for supporters in after.office_support.values_mut() {
                        supporters.extend(&after.members);
                    }
                    plan.effects.push(CommandEffect::Founded {
                        institution: *institution,
                    });
                }
                plan.institutions.push((Some(before.clone()), after));
                plan.effects.push(CommandEffect::Publish {
                    institution: *institution,
                });
            }
            InstitutionAction::Decline { institution } => {
                let before =
                    crate::storage::institution(store, institution).ok_or(Error::Unavailable)?;
                if before.status != InstitutionStatus::Proposed
                    || !before.charter.founders.contains(&actor)
                    || before.members.contains(&actor)
                    || before.charter.jurisdiction != area
                {
                    return Err(Error::Unavailable);
                }
                let mut after = before.clone();
                after.declined.insert(actor);
                after.status = InstitutionStatus::Rejected;
                plan.institutions.push((Some(before.clone()), after));
                plan.effects.push(CommandEffect::Publish {
                    institution: *institution,
                });
            }
            InstitutionAction::Leave { institution } => {
                let before =
                    crate::storage::institution(store, institution).ok_or(Error::Unavailable)?;
                if before.status != InstitutionStatus::Active
                    || !before.members.contains(&actor)
                    || before.charter.jurisdiction != area
                {
                    return Err(Error::Unavailable);
                }
                let mut after = before.clone();
                after.members.remove(&actor);
                for supporters in after.office_support.values_mut() {
                    supporters.remove(&actor);
                }
                for grantees in after.grants.values_mut() {
                    grantees.remove(&actor);
                }
                plan.institutions.push((Some(before.clone()), after));
                plan.effects.push(CommandEffect::Left {
                    institution: *institution,
                    actor,
                });
                plan.effects.push(CommandEffect::Publish {
                    institution: *institution,
                });
            }
            InstitutionAction::Propose {
                institution,
                change,
            } => {
                let before =
                    crate::storage::institution(store, institution).ok_or(Error::Unavailable)?;
                if before.status != InstitutionStatus::Active
                    || before.charter.jurisdiction != area
                    || !before.members.contains(&actor)
                    || !before
                        .subjects(&before.charter.procedure.proposers, facts.authority)
                        .contains(&actor)
                {
                    return Err(Error::Unavailable);
                }
                external_permit(before, change, facts)?;
                facts.references.validate_change(before, change)?;
                let id = RuleProposalId(plan.allocate()?);
                if crate::storage::proposal(store, id).is_some() {
                    return Err(Error::Unavailable);
                }
                plan.institutions
                    .push((Some(before.clone()), before.clone()));
                plan.proposals.push((
                    None,
                    RuleProposal {
                        id,
                        institution: *institution,
                        proposer: actor,
                        change: change.clone(),
                        votes: BTreeMap::new(),
                        base_revision: before.revision,
                        status: RuleProposalStatus::Proposed,
                    },
                ));
                plan.effects.push(CommandEffect::Publish {
                    institution: *institution,
                });
                plan.effects.push(CommandEffect::ObserveRegional);
            }
            InstitutionAction::Vote { proposal, support } => {
                let before = crate::storage::proposal(store, proposal).ok_or(Error::Unavailable)?;
                let institution = crate::storage::institution(store, before.institution)
                    .ok_or(Error::Unavailable)?;
                if before.status != RuleProposalStatus::Proposed
                    || before.base_revision != institution.revision
                    || institution.status != InstitutionStatus::Active
                    || institution.charter.jurisdiction != area
                    || !institution.members.contains(&actor)
                    || before.votes.contains_key(&actor)
                    || !institution
                        .subjects(&institution.charter.procedure.voters, facts.authority)
                        .contains(&actor)
                {
                    return Err(Error::Unavailable);
                }
                external_permit(institution, &before.change, facts)?;
                facts
                    .references
                    .validate_change(institution, &before.change)?;
                institution.revision.checked_add(1).ok_or(Error::Overflow)?;
                let mut voted = before.clone();
                voted.votes.insert(actor, *support);
                let outcome = self.proposal_outcome_for(store, &voted, facts.authority)?;
                let concept = if outcome == RuleProposalStatus::Adopted
                    && matches!(
                        before.change,
                        InstitutionChange::AddRule(_) | InstitutionChange::ReplaceRule { .. }
                    ) {
                    Some(ConceptId(plan.allocate()?))
                } else {
                    None
                };
                let mut resolution =
                    self.prepare_resolution_for(store, &voted, facts.authority, concept)?;
                // Admission depends on current membership even for a pending or rejected vote.
                if resolution.institution_before.is_none() {
                    resolution.institution_before = Some(institution.clone());
                    resolution.institution_after = Some(institution.clone());
                }
                if regional(&before.change) && outcome != RuleProposalStatus::Proposed {
                    plan.effects.push(CommandEffect::ExternalOutcome {
                        institution: institution.id,
                        proposal: *proposal,
                        outcome,
                        change: before.change.clone(),
                    });
                }
                if outcome == RuleProposalStatus::Adopted {
                    plan.effects.push(CommandEffect::Changed {
                        institution: institution.id,
                        proposal: *proposal,
                    });
                    if external(&before.change) && !regional(&before.change) {
                        plan.effects.push(CommandEffect::ExternalOutcome {
                            institution: institution.id,
                            proposal: *proposal,
                            outcome,
                            change: before.change.clone(),
                        });
                    }
                }
                plan.resolution = Some(resolution);
                plan.effects.push(CommandEffect::Publish {
                    institution: institution.id,
                });
            }
            InstitutionAction::SupportOffice {
                institution,
                office,
                support,
            } => {
                let before =
                    crate::storage::institution(store, institution).ok_or(Error::Unavailable)?;
                if before.status != InstitutionStatus::Active
                    || before.charter.jurisdiction != area
                    || !before.members.contains(&actor)
                    || !before
                        .charter
                        .offices
                        .iter()
                        .any(|mandate| mandate.slot == *office)
                {
                    return Err(Error::Unavailable);
                }
                let mut after = before.clone();
                let supporters = after.office_support.entry(*office).or_default();
                if if *support {
                    !supporters.insert(actor)
                } else {
                    !supporters.remove(&actor)
                } {
                    return Err(Error::Unavailable);
                }
                plan.institutions.push((Some(before.clone()), after));
                plan.effects.push(CommandEffect::OfficeSupportChanged {
                    institution: *institution,
                    office: *office,
                    actor,
                    support: *support,
                });
                plan.effects.push(CommandEffect::Publish {
                    institution: *institution,
                });
            }
            _ => return Err(Error::Unavailable),
        }
        Ok(plan)
    }
}
