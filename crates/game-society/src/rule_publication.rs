//! Rule effects, recognized duties, and their public knowledge share one publication.
use crate::{
    deliberation::RuleApplications, institutions::*, projected_publication::*, storage::*,
};
use game_core::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleApplicationEffect {
    Access {
        institution: InstitutionId,
        index: u16,
        actor: PersonId,
    },
    Duty {
        institution: InstitutionId,
        index: u16,
        actor: PersonId,
        agreement: AgreementId,
    },
}
pub struct PreparedRuleApplication {
    runtime: Runtime,
    at: SimTime,
    before: Institution,
    after: Institution,
    agreements: Option<game_agreements::PreparedTransition>,
    duties: Vec<game_agreements::InstitutionDuty>,
    reads: ReadSet,
    pub effects: Vec<RuleApplicationEffect>,
    pub next_id: u64,
}
pub struct RuleApplicationPublication {
    runtime: Runtime,
    at: SimTime,
    rows: ReadGuard,
    agreements: Option<ReadGuard>,
    publications: CommandPublicationGuard,
}
fn beneficiary(institution: &Institution, who: &Beneficiary) -> Option<PersonId> {
    match who {
        Beneficiary::Person(id) => Some(*id),
        Beneficiary::Office(slot) => institution
            .charter
            .offices
            .iter()
            .find(|office| office.slot == *slot)
            .map(|office| office.holder),
    }
}
fn duty_matches(
    institution: &Institution,
    rule: &InstitutionalRule,
    actor: PersonId,
    term: &game_agreements::AgreementTerm,
    at: SimTime,
) -> bool {
    match (&rule.effect, term) {
        (
            SocialEffect::Labor {
                site,
                millis,
                beneficiary: who,
                due_after,
            },
            game_agreements::AgreementTerm::Labor {
                debtor,
                creditor,
                site: actual,
                millis: amount,
                deadline,
            },
        ) => {
            *debtor == actor
                && *actual == *site
                && *amount == *millis
                && beneficiary(institution, who) == Some(*creditor)
                && institution.members.contains(creditor)
                && Some(deadline.0) == at.0.checked_add(due_after.0)
        }
        (
            SocialEffect::Delivery {
                source,
                destination,
                quantity,
                beneficiary: who,
                due_after,
            },
            game_agreements::AgreementTerm::Delivery {
                debtor,
                creditor,
                source: actual_source,
                destination: actual_destination,
                quantity: amount,
                deadline,
            },
        ) => {
            *debtor == actor
                && *actual_source == *source
                && *actual_destination == *destination
                && *amount == *quantity
                && beneficiary(institution, who) == Some(*creditor)
                && institution.members.contains(creditor)
                && Some(deadline.0) == at.0.checked_add(due_after.0)
        }
        _ => false,
    }
}
impl Runtime {
    pub fn prepare_rule_application(
        &self,
        store: &Store,
        application: &RuleApplications,
        first_id: u64,
        at: SimTime,
    ) -> Result<PreparedRuleApplication, Error> {
        if first_id == 0 {
            return Err(Error::Unavailable);
        }
        let mut reads = ReadSet::new();
        let before = reads
            .read::<Institution>(store, StableId(application.institution.0))
            .cloned()
            .ok_or(Error::Unavailable)?;
        if before.status != InstitutionStatus::Active {
            return Err(Error::Unavailable);
        }
        let mut after = before.clone();
        let mut effects = Vec::new();
        let mut duties = Vec::new();
        let mut next_id = first_id;
        for intent in &application.intents {
            let (institution, concept, index, actor) = match intent {
                RuleIntent::Access {
                    institution,
                    concept,
                    index,
                    actor,
                    ..
                }
                | RuleIntent::Duty {
                    institution,
                    concept,
                    index,
                    actor,
                    ..
                } => (*institution, *concept, *index, *actor),
            };
            if institution != before.id || before.concepts.get(usize::from(index)) != Some(&concept)
            {
                return Err(Error::Unavailable);
            }
            let rule = reads
                .read::<Concept>(store, StableId(concept.0))
                .ok_or(Error::Unavailable)?;
            if before.charter.rules.get(usize::from(index)) != Some(&rule.rule) {
                return Err(Error::Unavailable);
            }
            match intent {
                RuleIntent::Access {
                    inventory, allowed, ..
                } => {
                    if !matches!(rule.rule.effect,SocialEffect::Access {inventory:target,allowed:value} if target==*inventory && value==*allowed)
                    {
                        return Err(Error::Unavailable);
                    }
                    let changed = if *allowed {
                        after.grants.entry(concept).or_default().insert(actor)
                    } else {
                        let mut changed = false;
                        for id in &before.concepts {
                            let rule = reads
                                .read::<Concept>(store, StableId(id.0))
                                .ok_or(Error::Unavailable)?;
                            if matches!(rule.rule.effect,SocialEffect::Access {inventory:target,..} if target==*inventory)
                                && let Some(people) = after.grants.get_mut(id)
                            {
                                changed |= people.remove(&actor);
                            }
                        }
                        changed
                    };
                    if changed {
                        effects.push(RuleApplicationEffect::Access {
                            institution,
                            index,
                            actor,
                        });
                    }
                }
                RuleIntent::Duty { term, .. } => {
                    if after
                        .applied
                        .get(&concept)
                        .is_some_and(|people| people.contains_key(&actor))
                        || !duty_matches(&after, &rule.rule, actor, term, at)
                    {
                        return Err(Error::Unavailable);
                    }
                    let agreement = AgreementId(next_id);
                    let obligation = ObligationId(next_id.checked_add(1).ok_or(Error::Overflow)?);
                    next_id = next_id.checked_add(2).ok_or(Error::Overflow)?;
                    reads.exclusive_entity(store, StableId(agreement.0))?;
                    reads.exclusive_entity(store, StableId(obligation.0))?;
                    duties.push(game_agreements::InstitutionDuty {
                        institution,
                        agreement,
                        obligation,
                        term: term.clone(),
                    });
                    after
                        .applied
                        .entry(concept)
                        .or_default()
                        .insert(actor, agreement);
                    effects.push(RuleApplicationEffect::Duty {
                        institution,
                        index,
                        actor,
                        agreement,
                    });
                }
            }
        }
        let agreements = if duties.is_empty() {
            None
        } else {
            Some(game_agreements::Ledger::new(store).prepare_institutional_duties(&duties, at)?)
        };
        Ok(PreparedRuleApplication {
            runtime: self.clone(),
            at,
            before,
            after,
            agreements,
            duties,
            reads,
            effects,
            next_id,
        })
    }
}
impl PreparedRuleApplication {
    /// Native receiving owners attach these already allocated physical duties.
    pub fn duties(&self) -> &[game_agreements::InstitutionDuty] {
        &self.duties
    }
    pub fn agreement_transition(&self) -> Option<&game_agreements::PreparedTransition> {
        self.agreements.as_ref()
    }
    pub fn institution(&self) -> &Institution {
        &self.after
    }
    pub fn accepted_people(&self) -> BTreeSet<PersonId> {
        self.duties
            .iter()
            .flat_map(|duty| duty.term.parties())
            .collect()
    }
    pub fn publication_people(
        &self,
        local: impl IntoIterator<Item = PersonId>,
    ) -> BTreeSet<PersonId> {
        self.after
            .charter
            .founders
            .iter()
            .chain(&self.after.members)
            .chain(&self.after.invited)
            .copied()
            .chain(
                local
                    .into_iter()
                    .filter(|_| self.after.charter.open_membership),
            )
            .chain(crate::review::authority_people(&self.after))
            .collect()
    }
    pub fn prepare_publications(
        &self,
        runtime: &Runtime,
        store: &Store,
        authority: &AuthorityFacts,
        at: SimTime,
        index: &crate::perception::ObservationIndex<'_>,
    ) -> Result<Vec<PreparedCommandPublication>, Error> {
        if runtime != &self.runtime || at != self.at {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        Ok(vec![runtime.projected_publication(
            store,
            &self.after,
            crate::projected_publication::current_proposals(store, self.after.id),
            authority,
            at,
            index,
        )])
    }
    pub fn stage_with_publications(
        self,
        runtime: &Runtime,
        store: &Store,
        at: SimTime,
        publications: Vec<PreparedCommandPublication>,
        batch: &mut Batch,
    ) -> Result<RuleApplicationPublication, Error> {
        if runtime != &self.runtime
            || at != self.at
            || publications.len() != 1
            || publications[0].institution != self.after
        {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        let mut reads = self.reads;
        let mut writes = ReadFootprint::new();
        let institution = StableId(self.after.id.0);
        if batch.touches::<Institution>(institution) {
            return Err(Error::Unavailable);
        }
        let mut institutions = BTreeSet::new();
        let mut readonly = BTreeSet::new();
        if self.before != self.after {
            institutions.insert(self.after.id);
            writes.protect::<Institution>(institution);
            batch.insert(institution, self.after);
        } else {
            readonly.insert(self.after.id);
        }
        let agreements = self
            .agreements
            .as_ref()
            .map(|plan| plan.stage_into(store, batch))
            .transpose()?;
        for duty in &self.duties {
            writes.protect::<game_agreements::Agreement>(StableId(duty.agreement.0));
            writes.protect::<game_agreements::Obligation>(StableId(duty.obligation.0));
        }
        let mut receivers = BTreeSet::new();
        reads.membership::<RuleProposal>(store, move |_, row| row.institution.0 == institution.0);
        for publication in &publications {
            publication.verify(store)?;
            if publication.source_proposals != publication.projected_proposals {
                return Err(Error::Unavailable);
            }
            for proposal in &publication.source_proposals {
                reads.read::<RuleProposal>(store, StableId(proposal.id.0));
            }
            for (actor, _) in &publication.before {
                let id = StableId(actor.0);
                if !receivers.insert(*actor) || batch.touches::<Knowledge>(id) {
                    return Err(Error::Unavailable);
                }
                let mut row = reads
                    .read::<Knowledge>(store, id)
                    .cloned()
                    .unwrap_or_default();
                row.institutions
                    .insert(publication.view.id, publication.view.clone());
                writes.protect::<Knowledge>(id);
                batch.insert(id, row);
            }
        }
        let publication_guard = CommandPublicationGuard::new(
            batch,
            receivers,
            &publications,
            institutions,
            BTreeSet::new(),
            readonly,
        );
        publication_guard.validate(batch)?;
        Ok(RuleApplicationPublication {
            runtime: self.runtime,
            at: self.at,
            rows: reads.bind(store, batch, &writes)?,
            agreements,
            publications: publication_guard,
        })
    }
}
impl RuleApplicationPublication {
    pub fn publication_guard(&self) -> &CommandPublicationGuard {
        &self.publications
    }
    pub fn validate(
        &self,
        runtime: &Runtime,
        store: &Store,
        at: SimTime,
        batch: &Batch,
    ) -> Result<(), Error> {
        if runtime != &self.runtime || at != self.at {
            return Err(Error::Unavailable);
        }
        self.rows.validate(store, batch)?;
        if let Some(agreements) = &self.agreements {
            agreements.validate(store, batch)?;
        }
        self.publications.validate(batch)
    }
    pub fn row_guards(&self) -> Vec<ReadGuard> {
        let mut guards = vec![self.rows.clone()];
        guards.extend(self.agreements.clone());
        guards
    }
}
