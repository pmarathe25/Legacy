//! Hearing and telling a known case are guarded Society transitions.
use crate::institutions::*;
use crate::storage::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use std::collections::{BTreeMap, BTreeSet};
pub struct CaseCommandFacts<'a> {
    pub actor: PersonId,
    pub area: AreaId,
    pub now: SimTime,
    pub recipient_area: Option<AreaId>,
    pub authority: &'a AuthorityFacts,
    pub refusal: Option<CaseId>,
    pub refusal_observers: &'a BTreeMap<PersonId, u16>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseCommandEffects {
    pub publish: Option<InstitutionId>,
    pub recorded: Option<(CaseId, PersonId)>,
}
#[derive(Clone)]
enum CaseChange {
    Hear {
        institution: InstitutionId,
        shared: bool,
        grant: Option<(ConceptId, BTreeSet<PersonId>)>,
        refusal: Option<(CaseId, SocialCaseView, BTreeMap<PersonId, u16>)>,
    },
    Tell {
        recipient: PersonId,
    },
}
#[derive(Clone)]
pub struct PreparedCaseCommand {
    runtime: Runtime,
    at: SimTime,
    actor: PersonId,
    case: SocialCaseView,
    before_institution: Option<Institution>,
    before_heard: Option<bool>,
    before_grant: Option<(ConceptId, Option<BTreeSet<PersonId>>)>,
    actor_knows: bool,
    recipient_knows: Option<(PersonId, bool)>,
    change: CaseChange,
}
impl Runtime {
    pub fn case_command_record(
        &self,
        store: &Store,
        action: &InstitutionAction,
        at: SimTime,
    ) -> Option<SocialCaseView> {
        let InstitutionAction::HearCase {
            case, share: false, ..
        } = action
        else {
            return None;
        };
        crate::storage::case(store, case).map(|case| SocialCaseView {
            id: CaseId(0),
            outcome: SocialOutcome::Refused,
            observed_at: at,
            ..case.clone()
        })
    }
    pub fn prepare_case_command(
        &self,
        store: &Store,
        action: &InstitutionAction,
        facts: CaseCommandFacts<'_>,
    ) -> Result<PreparedCaseCommand, Error> {
        let id = match action {
            InstitutionAction::HearCase { case, .. } | InstitutionAction::TellCase { case, .. } => {
                *case
            }
            _ => return Err(Error::Unavailable),
        };
        let case = crate::storage::case(store, id).ok_or(Error::Unavailable)?;
        let actor_knows = crate::storage::knowledge(store, facts.actor)
            .is_some_and(|memory| memory.cases.contains(&id));
        if !actor_knows {
            return Err(Error::Unavailable);
        }
        match *action {
            InstitutionAction::HearCase {
                institution,
                office,
                share,
                ..
            } => {
                let current =
                    crate::storage::institution(store, institution).ok_or(Error::Unavailable)?;
                let mandate = current
                    .charter
                    .offices
                    .iter()
                    .find(|mandate| mandate.slot == office)
                    .ok_or(Error::Unavailable)?;
                let inventory = case.inventory.ok_or(Error::Unavailable)?;
                if current.status != InstitutionStatus::Active
                    || current.charter.jurisdiction != facts.area
                    || !current.office_effective(mandate, facts.authority)
                    || mandate.holder != facts.actor
                    || !mandate.powers.contains(&OfficePower::HearAccessCases)
                    || !mandate.inventories.contains(&inventory)
                    || case.kind != SocialCaseKind::Exclusion
                    || case.area != facts.area
                    || crate::storage::hearing(store, id).is_some()
                    || !current.members.contains(&case.subject)
                {
                    return Err(Error::Unavailable);
                }
                let grant = if share {
                    let concept = current
                        .concepts
                        .iter()
                        .copied()
                        .find(|id| {
                            matches!(crate::storage::concept(store, id).expect("validated Society reference").effect,
                        SocialEffect::Access{inventory:target,allowed:true}if target==inventory)
                                && crate::storage::concept(store, id).expect("validated Society reference").authority == office
                        })
                        .ok_or(Error::Unavailable)?;
                    let mut actors = current.grants.get(&concept).cloned().unwrap_or_default();
                    actors.insert(case.subject);
                    Some((concept, actors))
                } else {
                    None
                };
                let refusal = if !share {
                    let refusal = facts
                        .refusal
                        .filter(|id| id.0 > 0 && !crate::storage::case(store, id).is_some())
                        .ok_or(Error::Unavailable)?;
                    Some((
                        refusal,
                        SocialCaseView {
                            id: refusal,
                            outcome: SocialOutcome::Refused,
                            observed_at: facts.now,
                            ..case.clone()
                        },
                        facts.refusal_observers.clone(),
                    ))
                } else {
                    if facts.refusal.is_some() {
                        return Err(Error::Unavailable);
                    }
                    None
                };
                Ok(PreparedCaseCommand {
                    runtime: self.clone(),
                    at: facts.now,
                    actor: facts.actor,
                    case: case.clone(),
                    before_institution: Some(current.clone()),
                    before_heard: crate::storage::hearing(store, id).map(|row| row.shared),
                    before_grant: grant
                        .as_ref()
                        .map(|(concept, _)| (*concept, current.grants.get(concept).cloned())),
                    actor_knows,
                    recipient_knows: None,
                    change: CaseChange::Hear {
                        institution,
                        shared: share,
                        grant,
                        refusal,
                    },
                })
            }
            InstitutionAction::TellCase { recipient, .. } => {
                if recipient == facts.actor
                    || facts.recipient_area != Some(facts.area)
                    || facts.refusal.is_some()
                {
                    return Err(Error::Unavailable);
                }
                let memory =
                    crate::storage::knowledge(store, recipient).ok_or(Error::Unavailable)?;
                Ok(PreparedCaseCommand {
                    runtime: self.clone(),
                    at: facts.now,
                    actor: facts.actor,
                    case: case.clone(),
                    before_institution: None,
                    before_heard: None,
                    before_grant: None,
                    actor_knows,
                    recipient_knows: Some((recipient, memory.cases.contains(&id))),
                    change: CaseChange::Tell { recipient },
                })
            }
            _ => Err(Error::Unavailable),
        }
    }
}
impl PreparedCaseCommand {
    pub fn verify(&self, store: &Store) -> Result<(), Error> {
        if crate::storage::case(store, self.case.id) != Some(&self.case)
            || crate::storage::knowledge(store, self.actor)
                .is_some_and(|memory| memory.cases.contains(&self.case.id))
                != self.actor_knows
            || self.before_institution.as_ref().is_some_and(|institution| {
                crate::storage::institution(store, institution.id) != Some(institution)
            })
            || self.recipient_knows.is_some_and(|(actor, before)| {
                crate::storage::knowledge(store, actor)
                    .is_none_or(|memory| memory.cases.contains(&self.case.id) != before)
            })
        {
            return Err(Error::Unavailable);
        }
        if let CaseChange::Hear {
            institution,
            refusal,
            ..
        } = &self.change
            && (crate::storage::hearing(store, self.case.id).map(|row| row.shared)
                != self.before_heard
                || self.before_grant.as_ref().is_some_and(|(concept, before)| {
                    crate::storage::institution(store, institution)
                        .and_then(|row| row.grants.get(concept))
                        != before.as_ref()
                })
                || refusal
                    .as_ref()
                    .is_some_and(|(id, _, _)| crate::storage::case(store, id).is_some()))
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into(
        self,
        world: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<CaseCommandEffects, Error> {
        if world != &self.runtime {
            return Err(Error::Unavailable);
        }
        self.verify(store)?;
        match self.change {
            CaseChange::Tell { recipient } => {
                let mut row = crate::storage::knowledge(store, recipient)
                    .ok_or(Error::Unavailable)?
                    .clone();
                row.cases.insert(self.case.id);
                batch.insert(StableId(recipient.0), row);
                Ok(CaseCommandEffects {
                    publish: None,
                    recorded: None,
                })
            }
            CaseChange::Hear {
                institution,
                shared,
                grant,
                refusal,
            } => {
                if let Some((concept, actors)) = grant {
                    let mut row = crate::storage::institution(store, institution)
                        .ok_or(Error::Unavailable)?
                        .clone();
                    row.grants.insert(concept, actors);
                    batch.insert(StableId(institution.0), row);
                }
                batch.insert(StableId(self.case.id.0), Hearing { shared });
                let recorded = if let Some((id, case, observers)) = refusal {
                    let subject = case.subject;
                    let mut learning =
                        world.prepare_case_learning(store, case, None, &observers)?;
                    learning.also_remember(store, self.actor);
                    learning.stage_into(store, batch)?;
                    Some((id, subject))
                } else {
                    None
                };
                Ok(CaseCommandEffects {
                    publish: Some(institution),
                    recorded,
                })
            }
        }
    }
    pub fn commit(self, world: &Runtime, store: &mut Store) -> Result<CaseCommandEffects, Error> {
        let at = self.at;
        let mut batch = Batch::new();
        let publication = self.stage_with_publications(world, store, at, Vec::new(), &mut batch)?;
        publication.validate(world, store, at, &batch)?;
        store.apply_batch(batch)?;
        Ok(publication.effects)
    }
}
impl PreparedCaseCommand {
    fn projected_institution(&self) -> Option<Institution> {
        let mut row = self.before_institution.clone()?;
        if let CaseChange::Hear {
            grant: Some((concept, actors)),
            ..
        } = &self.change
        {
            row.grants.insert(*concept, actors.clone());
        }
        Some(row)
    }
    pub fn prepare_publications(
        &self,
        world: &Runtime,
        store: &Store,
        authority: &AuthorityFacts,
        at: SimTime,
        index: &crate::perception::ObservationIndex<'_>,
    ) -> Result<Vec<crate::projected_publication::PreparedCommandPublication>, Error> {
        if world != &self.runtime || at != self.at {
            return Err(Error::Unavailable);
        }
        self.verify(store)?;
        Ok(self
            .projected_institution()
            .map(|institution| {
                world.projected_publication(
                    store,
                    &institution,
                    crate::projected_publication::current_proposals(store, institution.id),
                    authority,
                    at,
                    index,
                )
            })
            .into_iter()
            .collect())
    }
    pub fn stage_with_publications(
        self,
        world: &Runtime,
        store: &Store,
        at: SimTime,
        publications: Vec<crate::projected_publication::PreparedCommandPublication>,
        batch: &mut Batch,
    ) -> Result<CaseCommandPublication, Error> {
        if world != &self.runtime || at != self.at {
            return Err(Error::Unavailable);
        }
        self.verify(store)?;
        let mut reads = game_ecs::ReadSet::new();
        reads.read::<SocialCaseView>(store, StableId(self.case.id.0));
        reads.read::<Knowledge>(store, StableId(self.actor.0));
        if let Some((recipient, _)) = self.recipient_knows {
            reads.read::<Knowledge>(store, StableId(recipient.0));
        }
        if let Some(before) = &self.before_institution {
            reads.read::<Institution>(store, StableId(before.id.0));
            reads.read::<Hearing>(store, StableId(self.case.id.0));
            for concept in &before.concepts {
                reads.read::<Concept>(store, StableId(concept.0));
            }
        }
        if let CaseChange::Hear {
            refusal: Some((id, _, observers)),
            ..
        } = &self.change
        {
            reads.exclusive_entity(store, StableId(id.0))?;
            for observer in observers.keys() {
                reads.read::<Knowledge>(store, StableId(observer.0));
            }
        }
        let projected = self.projected_institution();
        let runtime = self.runtime.clone();
        let mut own = Batch::new();
        let effects = self.stage_into(world, store, &mut own)?;
        let mut after = own
            .insertions::<Knowledge>()
            .map(|(id, row)| (PersonId(id.0), row.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut instances = own
            .insertions::<Institution>()
            .map(|(id, _)| InstitutionId(id.0))
            .collect::<BTreeSet<_>>();
        let mut preserved = BTreeSet::new();
        let mut readonly = BTreeSet::new();
        for publication in &publications {
            publication.verify(store)?;
            if projected.as_ref() != Some(&publication.institution)
                || publication.source_proposals != publication.projected_proposals
            {
                return Err(Error::Unavailable);
            }
            let institution = publication.institution.id;
            reads.membership::<RuleProposal>(store, move |_, row| row.institution == institution);
            for proposal in &publication.source_proposals {
                reads.read::<RuleProposal>(store, StableId(proposal.id.0));
            }
            if instances.contains(&institution) {
                if publication.source.as_ref() == Some(&publication.institution) {
                    preserved.insert(institution);
                }
            } else {
                readonly.insert(institution);
            }
            for (actor, _) in &publication.before {
                let opening = reads
                    .read::<Knowledge>(store, StableId(actor.0))
                    .cloned()
                    .unwrap_or_default();
                after
                    .entry(*actor)
                    .or_insert(opening)
                    .institutions
                    .insert(institution, publication.view.clone());
            }
        }
        let mut writes = game_ecs::ReadFootprint::new();
        macro_rules! stage {
            ($row:ty) => {
                for (id, row) in own.insertions::<$row>() {
                    if batch.touches::<$row>(id) {
                        return Err(Error::Unavailable);
                    }
                    reads.read::<$row>(store, id);
                    writes.protect::<$row>(id);
                    batch.insert(id, row.clone());
                }
            };
        }
        stage!(Institution);
        stage!(Hearing);
        stage!(SocialCaseView);
        let recipients = after.keys().copied().collect();
        for (actor, row) in after {
            let id = StableId(actor.0);
            if batch.touches::<Knowledge>(id) {
                return Err(Error::Unavailable);
            }
            reads.read::<Knowledge>(store, id);
            writes.protect::<Knowledge>(id);
            batch.insert(id, row);
        }
        instances.extend(
            publications
                .iter()
                .filter(|publication| {
                    batch.touches::<Institution>(StableId(publication.institution.id.0))
                })
                .map(|publication| publication.institution.id),
        );
        let publication_guard = crate::projected_publication::CommandPublicationGuard::new(
            batch,
            recipients,
            &publications,
            instances,
            preserved,
            readonly,
        );
        publication_guard.validate(batch)?;
        Ok(CaseCommandPublication {
            runtime,
            at,
            rows: reads.bind(store, batch, &writes)?,
            publications: publication_guard,
            effects,
        })
    }
}
impl CaseCommandPublication {
    pub fn validate(
        &self,
        world: &Runtime,
        store: &Store,
        at: SimTime,
        batch: &Batch,
    ) -> Result<(), Error> {
        if world != &self.runtime || at != self.at {
            return Err(Error::Unavailable);
        }
        self.rows.validate(store, batch)?;
        self.publications.validate(batch)
    }
    pub fn publication_guard(&self) -> &crate::projected_publication::CommandPublicationGuard {
        &self.publications
    }
    pub fn row_guard(&self) -> game_ecs::ReadGuard {
        self.rows.clone()
    }
}

#[cfg(test)]
mod tests;

pub struct CaseCommandPublication {
    runtime: Runtime,
    at: SimTime,
    rows: game_ecs::ReadGuard,
    publications: crate::projected_publication::CommandPublicationGuard,
    pub effects: CaseCommandEffects,
}
