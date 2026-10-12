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
pub struct PreparedCaseCommand {
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
        let mut batch = Batch::new();
        let effects = self.stage_into(world, store, &mut batch)?;
        store.apply_batch(batch)?;
        Ok(effects)
    }
}
#[cfg(test)]
mod tests;
