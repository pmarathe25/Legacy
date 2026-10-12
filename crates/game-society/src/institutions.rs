//! Executable, bounded social arrangements people can propose and recognize.
use crate::enforcement::{EnforcementPolicy, EnforcementRemedy};
use crate::storage::*;
use game_agreements::AgreementTerm;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod resources;
pub use resources::*;
mod commands;
mod validation;
pub use commands::*;
pub use validation::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subjects {
    pub members: bool,
    pub named: BTreeSet<PersonId>,
    pub offices: BTreeSet<u16>,
}
impl Subjects {
    pub fn members() -> Self {
        Self {
            members: true,
            named: BTreeSet::new(),
            offices: BTreeSet::new(),
        }
    }
    pub fn named(actor: PersonId) -> Self {
        Self {
            members: false,
            named: BTreeSet::from([actor]),
            offices: BTreeSet::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionProcedure {
    pub proposers: Subjects,
    pub voters: Subjects,
    pub minimum_support: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialCondition {
    Local,
    Member,
    HungerAtLeast(u64),
    KnownStockAtLeast {
        inventory: InventoryId,
        quantity: Quantity,
    },
    Excluded {
        inventory: InventoryId,
    },
    KnownCooperationAtLeast(u16),
    KnownDefaultsAtLeast(u16),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleExpression {
    Fact(SocialCondition),
    All(Vec<RuleExpression>),
    Any(Vec<RuleExpression>),
    Not(Box<RuleExpression>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Beneficiary {
    Person(PersonId),
    Office(u16),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialEffect {
    Access {
        inventory: InventoryId,
        allowed: bool,
    },
    Labor {
        site: SiteId,
        millis: u64,
        beneficiary: Beneficiary,
        due_after: SimDuration,
    },
    Delivery {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        beneficiary: Beneficiary,
        due_after: SimDuration,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstitutionalRule {
    pub subjects: Subjects,
    pub when: RuleExpression,
    pub effect: SocialEffect,
    pub authority: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OfficePower {
    Negotiate,
    Adjudicate,
    Enforce,
    Access,
    Labor,
    Delivery,
    HearAccessCases,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficeMandate {
    pub slot: u16,
    pub holder: PersonId,
    pub inventories: BTreeSet<InventoryId>,
    pub sites: BTreeSet<SiteId>,
    pub powers: BTreeSet<OfficePower>,
    pub minimum_support: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstitutionCharter {
    pub purpose: String,
    pub jurisdiction: AreaId,
    pub founders: BTreeSet<PersonId>,
    pub open_membership: bool,
    pub procedure: DecisionProcedure,
    pub offices: Vec<OfficeMandate>,
    /// Ordered policies: the last applicable access effect wins within this
    /// institution. Other institutions and independent permissions retain theirs.
    pub rules: Vec<InstitutionalRule>,
    pub contributions: Vec<AgreementTerm>,
    #[serde(default)]
    pub evidence: BTreeSet<CaseId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstitutionChange {
    RatifyRegional {
        commitment: RealmCommitmentId,
        revision: u32,
        support: bool,
    },
    AuthorizeRegionalPassage {
        commitment: RealmCommitmentId,
        revision: u32,
        index: u16,
    },
    EnforcementPolicy {
        office: u16,
        policy: Option<EnforcementPolicy>,
    },
    Adjudicate {
        case: EnforcementCaseId,
        remedy: EnforcementRemedy,
    },
    AddRule(InstitutionalRule),
    ReplaceRule {
        index: u16,
        rule: InstitutionalRule,
    },
    RemoveRule {
        index: u16,
    },
    Procedure(DecisionProcedure),
    Appoint {
        office: u16,
        holder: PersonId,
    },
    Invite(PersonId),
    Dissolve,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstitutionAction {
    Found {
        charter: InstitutionCharter,
        origin: Option<InstitutionId>,
    },
    Join {
        institution: InstitutionId,
    },
    Decline {
        institution: InstitutionId,
    },
    Leave {
        institution: InstitutionId,
    },
    Propose {
        institution: InstitutionId,
        change: InstitutionChange,
    },
    Vote {
        proposal: RuleProposalId,
        support: bool,
    },
    SupportOffice {
        institution: InstitutionId,
        office: u16,
        support: bool,
    },
    HearCase {
        institution: InstitutionId,
        office: u16,
        case: CaseId,
        share: bool,
    },
    TellCase {
        recipient: PersonId,
        case: CaseId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstitutionStatus {
    Proposed,
    Active,
    Rejected,
    Dissolved,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleProposalStatus {
    Proposed,
    Adopted,
    Rejected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialCaseKind {
    Exclusion,
    Cooperation,
    Default,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialOutcome {
    Unresolved,
    Shared,
    Contributed,
    Defaulted,
    Refused,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialCaseView {
    pub id: CaseId,
    pub kind: SocialCaseKind,
    pub subject: PersonId,
    pub counterpart: PersonId,
    pub area: AreaId,
    pub inventory: Option<InventoryId>,
    pub good: Good,
    pub need_band: u8,
    pub reciprocal: bool,
    pub outcome: SocialOutcome,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficeView {
    pub mandate: OfficeMandate,
    pub supporters: BTreeSet<PersonId>,
    pub effective: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleProposalView {
    pub id: RuleProposalId,
    pub proposer: PersonId,
    pub change: InstitutionChange,
    pub votes: BTreeMap<PersonId, bool>,
    pub status: RuleProposalStatus,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstitutionView {
    pub id: InstitutionId,
    pub origin: Option<InstitutionId>,
    pub charter: InstitutionCharter,
    pub members: BTreeSet<PersonId>,
    pub declined: BTreeSet<PersonId>,
    pub status: InstitutionStatus,
    pub offices: Vec<OfficeView>,
    pub proposals: Vec<RuleProposalView>,
    pub funding: Vec<AgreementId>,
    pub duties: Vec<AgreementId>,
    pub observed_at: SimTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptValue {
    pub weight: i32,
    pub experiences: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Institution {
    pub id: InstitutionId,
    pub origin: Option<InstitutionId>,
    pub charter: InstitutionCharter,
    pub members: BTreeSet<PersonId>,
    pub declined: BTreeSet<PersonId>,
    pub invited: BTreeSet<PersonId>,
    pub status: InstitutionStatus,
    pub concepts: Vec<ConceptId>,
    pub office_support: BTreeMap<u16, BTreeSet<PersonId>>,
    pub grants: BTreeMap<ConceptId, BTreeSet<PersonId>>,
    pub applied: BTreeMap<ConceptId, BTreeMap<PersonId, AgreementId>>,
    pub funding: Vec<AgreementId>,
    pub revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleProposal {
    pub id: RuleProposalId,
    pub institution: InstitutionId,
    pub proposer: PersonId,
    pub change: InstitutionChange,
    pub votes: BTreeMap<PersonId, bool>,
    pub base_revision: u64,
    pub status: RuleProposalStatus,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub last_event: u64,
}

/// Authoritative liveness supplied by the physical owner. Membership and
/// support remain institutional facts; death does not silently rewrite them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthorityFacts {
    pub alive: BTreeSet<PersonId>,
}
impl Institution {
    pub fn office_effective(&self, office: &OfficeMandate, facts: &AuthorityFacts) -> bool {
        self.status == InstitutionStatus::Active
            && self.members.contains(&office.holder)
            && facts.alive.contains(&office.holder)
            && self
                .office_support
                .get(&office.slot)
                .map_or(0, |supporters| {
                    supporters
                        .intersection(&self.members)
                        .filter(|person| facts.alive.contains(person))
                        .count()
                })
                >= usize::from(office.minimum_support)
    }
    pub fn subjects(&self, subjects: &Subjects, facts: &AuthorityFacts) -> BTreeSet<PersonId> {
        let mut selected = subjects.named.clone();
        if subjects.members {
            selected.extend(self.members.iter().copied());
        }
        for slot in &subjects.offices {
            if let Some(office) = self.charter.offices.iter().find(|o| o.slot == *slot)
                && self.office_effective(office, facts)
            {
                selected.insert(office.holder);
            }
        }
        selected
    }
}
pub struct ConditionInput<'a> {
    pub observer: PersonId,
    pub subject: PersonId,
    pub subject_local: bool,
    pub own_hunger: u64,
    pub known_stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
}
pub(crate) fn apply_case_learning(
    after: &mut Knowledge,
    case: &SocialCaseView,
    concept: Option<ConceptId>,
    observer: PersonId,
    empathy: u16,
) {
    after.cases.insert(case.id);
    if let Some(concept) = concept {
        let value = after.values.entry(concept).or_insert(ConceptValue {
            weight: 0,
            experiences: 0,
        });
        let gain = match case.outcome {
            SocialOutcome::Shared | SocialOutcome::Contributed => {
                if observer == case.subject || observer == case.counterpart {
                    500
                } else {
                    i32::from(empathy) / 2
                }
            }
            SocialOutcome::Defaulted => -300,
            _ => 0,
        };
        value.weight = (value.weight + gain).clamp(-2000, 2000);
        value.experiences = value.experiences.saturating_add(1);
    }
}

impl Runtime {
    pub fn known_case_count(
        &self,
        store: &Store,
        observer: PersonId,
        subject: PersonId,
        kind: SocialCaseKind,
    ) -> usize {
        crate::storage::knowledge(store, observer).map_or(0, |memory| {
            memory
                .cases
                .iter()
                .rev()
                .take(64)
                .filter(|id| {
                    crate::storage::case(store, *id)
                        .is_some_and(|c| c.subject == subject && c.kind == kind)
                })
                .count()
        })
    }
    pub fn condition_holds(
        &self,
        store: &Store,
        institution: &Institution,
        input: &ConditionInput<'_>,
        expression: &RuleExpression,
    ) -> bool {
        match expression {
            RuleExpression::All(items) => items
                .iter()
                .all(|e| self.condition_holds(store, institution, input, e)),
            RuleExpression::Any(items) => items
                .iter()
                .any(|e| self.condition_holds(store, institution, input, e)),
            RuleExpression::Not(item) => !self.condition_holds(store, institution, input, item),
            RuleExpression::Fact(condition) => match condition {
                SocialCondition::Local => input.subject_local,
                SocialCondition::Member => institution.members.contains(&input.subject),
                SocialCondition::HungerAtLeast(amount) => {
                    if input.observer == input.subject {
                        input.own_hunger >= *amount
                    } else {
                        crate::storage::knowledge(store, input.observer).is_some_and(|memory| {
                            memory.cases.iter().rev().take(64).any(|id| {
                                crate::storage::case(store, id).is_some_and(|c| {
                                    c.subject == input.subject
                                        && c.need_band > 0
                                        && u64::from(c.need_band) * 180_000 >= *amount
                                })
                            })
                        })
                    }
                }
                SocialCondition::KnownStockAtLeast {
                    inventory,
                    quantity,
                } => input.known_stocks.get(inventory).is_some_and(|stock| {
                    stock.quantity >= *quantity && stock.area == institution.charter.jurisdiction
                }),
                SocialCondition::Excluded { inventory } => {
                    if input.observer == input.subject
                        && input
                            .known_stocks
                            .get(inventory)
                            .is_some_and(|stock| stock.accessible)
                    {
                        return false;
                    }
                    if institution.concepts.iter().any(|id|crate::storage::concept(store, id).is_some_and(|r|matches!(r.effect,SocialEffect::Access {inventory:target,allowed:true} if target==*inventory)) && institution.grants.get(id).is_some_and(|people|people.contains(&input.subject))) {return false;}
                    crate::storage::knowledge(store, input.observer)
                        .and_then(|memory| {
                            memory.cases.iter().rev().take(64).find_map(|id| {
                                crate::storage::case(store, id)
                                    .filter(|c| {
                                        c.subject == input.subject
                                            && c.kind == SocialCaseKind::Exclusion
                                            && c.inventory == Some(*inventory)
                                    })
                                    .map(|c| c.outcome)
                            })
                        })
                        .is_some_and(|outcome| {
                            matches!(outcome, SocialOutcome::Unresolved | SocialOutcome::Refused)
                        })
                }
                SocialCondition::KnownCooperationAtLeast(amount) => {
                    self.known_case_count(
                        store,
                        input.observer,
                        input.subject,
                        SocialCaseKind::Cooperation,
                    ) >= usize::from(*amount)
                }
                SocialCondition::KnownDefaultsAtLeast(amount) => {
                    self.known_case_count(
                        store,
                        input.observer,
                        input.subject,
                        SocialCaseKind::Default,
                    ) >= usize::from(*amount)
                }
            },
        }
    }
    pub fn precedent_score(&self, store: &Store, observer: PersonId, case: &SocialCaseView) -> i64 {
        let Some(memory) = crate::storage::knowledge(store, observer) else {
            return 0;
        };
        let mut candidates = memory
            .cases
            .iter()
            .rev()
            .take(64)
            .filter_map(|id| {
                let prior = crate::storage::case(store, id)?;
                if prior.id == case.id
                    || prior.kind != case.kind
                    || prior.good != case.good
                    || prior.outcome == SocialOutcome::Unresolved
                {
                    return None;
                }
                let distance = u16::from(prior.need_band.abs_diff(case.need_band)) * 2
                    + u16::from(prior.reciprocal != case.reciprocal) * 3;
                let judgment = match prior.outcome {
                    SocialOutcome::Shared => 600,
                    SocialOutcome::Contributed => 400,
                    SocialOutcome::Defaulted => -600,
                    SocialOutcome::Refused => -300,
                    _ => 0,
                };
                Some((distance, prior.id, judgment))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(distance, id, _)| (*distance, *id));
        let selected = candidates.into_iter().take(8).collect::<Vec<_>>();
        if selected.is_empty() {
            return 0;
        }
        selected
            .iter()
            .map(|(distance, _, judgment)| judgment / (1 + i64::from(*distance)))
            .sum::<i64>()
            / selected.len() as i64
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedResolution {
    institution_before: Option<Institution>,
    institution_after: Option<Institution>,
    proposals: Vec<(RuleProposal, RuleProposal)>,
    concept: Option<(ConceptId, InstitutionalRule)>,
    competing_proposals: Option<BTreeSet<RuleProposalId>>,
    pub outcome: RuleProposalStatus,
    pub external_change: Option<InstitutionChange>,
}
impl Runtime {
    pub fn proposal_outcome(
        &self,
        store: &Store,
        id: RuleProposalId,
        facts: &AuthorityFacts,
    ) -> Result<RuleProposalStatus, Error> {
        let proposal = crate::storage::proposal(store, id).ok_or(Error::Unavailable)?;
        self.proposal_outcome_for(store, proposal, facts)
    }
    fn proposal_outcome_for(
        &self,
        store: &Store,
        proposal: &RuleProposal,
        facts: &AuthorityFacts,
    ) -> Result<RuleProposalStatus, Error> {
        let institution =
            crate::storage::institution(store, proposal.institution).ok_or(Error::Unavailable)?;
        if proposal.status != RuleProposalStatus::Proposed
            || proposal.base_revision != institution.revision
        {
            return Err(Error::Unavailable);
        }
        let voters = institution
            .subjects(&institution.charter.procedure.voters, facts)
            .intersection(&institution.members)
            .copied()
            .collect::<BTreeSet<_>>();
        let yes = voters
            .iter()
            .filter(|id| proposal.votes.get(id) == Some(&true))
            .count();
        let no = voters
            .iter()
            .filter(|id| proposal.votes.get(id) == Some(&false))
            .count();
        let minimum = usize::from(institution.charter.procedure.minimum_support);
        Ok(if yes >= minimum {
            RuleProposalStatus::Adopted
        } else if voters.len().saturating_sub(no) < minimum {
            RuleProposalStatus::Rejected
        } else {
            RuleProposalStatus::Proposed
        })
    }
    /// Global allocation supplies a concept ID only for a successful rule change.
    /// External domain decisions are returned to the coordinator, never executed here.
    pub fn prepare_resolution(
        &self,
        store: &Store,
        id: RuleProposalId,
        facts: &AuthorityFacts,
        concept: Option<ConceptId>,
    ) -> Result<PreparedResolution, Error> {
        let proposal = crate::storage::proposal(store, id).ok_or(Error::Unavailable)?;
        self.prepare_resolution_for(store, proposal, facts, concept)
    }
    fn prepare_resolution_for(
        &self,
        store: &Store,
        proposal: &RuleProposal,
        facts: &AuthorityFacts,
        concept: Option<ConceptId>,
    ) -> Result<PreparedResolution, Error> {
        let id = proposal.id;
        let outcome = self.proposal_outcome_for(store, proposal, facts)?;
        let institution =
            crate::storage::institution(store, proposal.institution).ok_or(Error::Unavailable)?;
        let mut plan = PreparedResolution {
            institution_before: None,
            institution_after: None,
            proposals: Vec::new(),
            concept: None,
            competing_proposals: None,
            outcome,
            external_change: None,
        };
        if outcome == RuleProposalStatus::Proposed {
            if crate::storage::proposal(store, id) != Some(proposal) {
                plan.proposals.push((
                    crate::storage::proposal(store, id)
                        .expect("validated Society reference")
                        .clone(),
                    proposal.clone(),
                ));
            }
            return Ok(plan);
        }
        let mut changed = proposal.clone();
        changed.status = outcome;
        plan.proposals.push((
            crate::storage::proposal(store, id)
                .expect("validated Society reference")
                .clone(),
            changed,
        ));
        if outcome == RuleProposalStatus::Rejected {
            return Ok(plan);
        }
        let mut after = institution.clone();
        match &proposal.change {
            InstitutionChange::AddRule(rule) | InstitutionChange::ReplaceRule { rule, .. } => {
                let id = concept.ok_or(Error::Unavailable)?;
                if crate::storage::concept(store, id).is_some() {
                    return Err(Error::Unavailable);
                }
                plan.concept = Some((id, rule.clone()));
                if let InstitutionChange::ReplaceRule { index, .. } = proposal.change {
                    let old = after
                        .concepts
                        .get_mut(index as usize)
                        .ok_or(Error::Unavailable)?;
                    after.grants.remove(old);
                    *old = id;
                    *after
                        .charter
                        .rules
                        .get_mut(index as usize)
                        .ok_or(Error::Unavailable)? = rule.clone();
                } else {
                    after.charter.rules.push(rule.clone());
                    after.concepts.push(id);
                }
            }
            InstitutionChange::RemoveRule { index } => {
                if *index as usize >= after.concepts.len()
                    || *index as usize >= after.charter.rules.len()
                {
                    return Err(Error::Unavailable);
                }
                let old = after.concepts.remove(*index as usize);
                after.grants.remove(&old);
                after.charter.rules.remove(*index as usize);
            }
            InstitutionChange::Procedure(procedure) => after.charter.procedure = procedure.clone(),
            InstitutionChange::Appoint { office, holder } => {
                if !after.members.contains(holder) {
                    return Err(Error::Unavailable);
                }
                after
                    .charter
                    .offices
                    .iter_mut()
                    .find(|o| o.slot == *office)
                    .ok_or(Error::Unavailable)?
                    .holder = *holder;
                after
                    .office_support
                    .insert(*office, BTreeSet::from([*holder]));
            }
            InstitutionChange::Invite(actor) => {
                after.invited.insert(*actor);
            }
            InstitutionChange::Dissolve => {
                after.status = InstitutionStatus::Dissolved;
                after.grants.clear();
            }
            InstitutionChange::Adjudicate {
                remedy:
                    EnforcementRemedy::Depose {
                        office,
                        replacement,
                    },
                ..
            } => {
                if !after.members.contains(replacement) {
                    return Err(Error::Unavailable);
                }
                after
                    .charter
                    .offices
                    .iter_mut()
                    .find(|mandate| mandate.slot == *office)
                    .ok_or(Error::Unavailable)?
                    .holder = *replacement;
                after.office_support.insert(
                    *office,
                    proposal
                        .votes
                        .iter()
                        .filter(|(actor, support)| **support && after.members.contains(actor))
                        .map(|(actor, _)| *actor)
                        .collect(),
                );
                plan.external_change = Some(proposal.change.clone());
            }
            InstitutionChange::RatifyRegional { .. }
            | InstitutionChange::AuthorizeRegionalPassage { .. }
            | InstitutionChange::EnforcementPolicy { .. }
            | InstitutionChange::Adjudicate { .. } => {
                plan.external_change = Some(proposal.change.clone())
            }
        }
        after.revision = after.revision.checked_add(1).ok_or(Error::Overflow)?;
        plan.competing_proposals = Some(
            crate::storage::proposals(store)
                .map(|(_, row)| row)
                .filter(|proposal| {
                    proposal.institution == institution.id
                        && proposal.status == RuleProposalStatus::Proposed
                })
                .map(|proposal| proposal.id)
                .collect(),
        );
        for other in crate::storage::proposals(store)
            .map(|(_, row)| row)
            .filter(|p| {
                p.institution == institution.id
                    && p.status == RuleProposalStatus::Proposed
                    && p.id != id
            })
        {
            let mut rejected = other.clone();
            rejected.status = RuleProposalStatus::Rejected;
            plan.proposals.push((other.clone(), rejected));
        }
        plan.institution_before = Some(institution.clone());
        plan.institution_after = Some(after);
        Ok(plan)
    }
}
impl PreparedResolution {
    pub fn verify(&self, store: &Store) -> Result<(), Error> {
        if self
            .institution_before
            .as_ref()
            .is_some_and(|old| crate::storage::institution(store, old.id) != Some(old))
            || self
                .proposals
                .iter()
                .any(|(old, _)| crate::storage::proposal(store, old.id) != Some(old))
            || self
                .concept
                .as_ref()
                .is_some_and(|(id, _)| crate::storage::concept(store, id).is_some())
            || self.competing_proposals.as_ref().is_some_and(|expected| {
                let Some(institution) = &self.institution_before else {
                    return true;
                };
                crate::storage::proposals(store)
                    .map(|(_, row)| row)
                    .filter(|proposal| {
                        proposal.institution == institution.id
                            && proposal.status == RuleProposalStatus::Proposed
                    })
                    .map(|proposal| proposal.id)
                    .collect::<BTreeSet<_>>()
                    != *expected
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into(
        self,
        _state: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<(), Error> {
        self.verify(store)?;
        if let Some((id, rule)) = self.concept {
            batch.insert(StableId(id.0), Concept { rule });
        }
        if let Some(row) = self.institution_after {
            batch.insert(StableId(row.id.0), row);
        }
        for (_, row) in self.proposals {
            batch.insert(StableId(row.id.0), row);
        }
        Ok(())
    }
    pub fn commit(self, state: &Runtime, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage_into(state, store, &mut batch)?;
        store.apply_batch(batch)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleIntent {
    Access {
        institution: InstitutionId,
        concept: ConceptId,
        index: u16,
        actor: PersonId,
        inventory: InventoryId,
        allowed: bool,
    },
    Duty {
        institution: InstitutionId,
        concept: ConceptId,
        index: u16,
        actor: PersonId,
        term: AgreementTerm,
    },
}
impl Runtime {
    // Store is explicit; the remaining arguments are the existing bounded rule evaluation facts.
    #[allow(clippy::too_many_arguments)]
    pub fn rule_intent(
        &self,
        store: &Store,
        institution: &Institution,
        index: usize,
        input: &ConditionInput<'_>,
        authority: &AuthorityFacts,
        office_local: bool,
        now: SimTime,
    ) -> Option<RuleIntent> {
        let rule = institution.charter.rules.get(index)?;
        let concept = *institution.concepts.get(index)?;
        let office = institution
            .charter
            .offices
            .iter()
            .find(|o| o.slot == rule.authority)?;
        if !office_local
            || !institution.office_effective(office, authority)
            || !input.subject_local
            || !institution
                .subjects(&rule.subjects, authority)
                .contains(&input.subject)
            || !self.condition_holds(store, institution, input, &rule.when)
        {
            return None;
        }
        if let SocialEffect::Access { inventory, allowed } = rule.effect {
            return Some(RuleIntent::Access {
                institution: institution.id,
                concept,
                index: index as u16,
                actor: input.subject,
                inventory,
                allowed,
            });
        }
        if !institution.members.contains(&input.subject)
            || crate::storage::institution(store, institution.id)?
                .applied
                .get(&concept)
                .is_some_and(|people| people.contains_key(&input.subject))
        {
            return None;
        }
        let (beneficiary, due_after) = match rule.effect {
            SocialEffect::Labor {
                ref beneficiary,
                due_after,
                ..
            }
            | SocialEffect::Delivery {
                ref beneficiary,
                due_after,
                ..
            } => (beneficiary, due_after),
            _ => return None,
        };
        let beneficiary = match beneficiary {
            Beneficiary::Person(id) => *id,
            Beneficiary::Office(slot) => {
                institution
                    .charter
                    .offices
                    .iter()
                    .find(|o| o.slot == *slot)?
                    .holder
            }
        };
        if beneficiary == input.subject || !institution.members.contains(&beneficiary) {
            return None;
        }
        let deadline = SimTime(now.0.checked_add(due_after.0)?);
        let term = match rule.effect {
            SocialEffect::Labor { site, millis, .. } => AgreementTerm::Labor {
                debtor: input.subject,
                creditor: beneficiary,
                site,
                millis,
                deadline,
            },
            SocialEffect::Delivery {
                source,
                destination,
                quantity,
                ..
            } => AgreementTerm::Delivery {
                debtor: input.subject,
                creditor: beneficiary,
                source,
                destination,
                quantity,
                deadline,
            },
            _ => return None,
        };
        Some(RuleIntent::Duty {
            institution: institution.id,
            concept,
            index: index as u16,
            actor: input.subject,
            term,
        })
    }
    /// Records the institution's own grant overlay, not an inventory permission.
    pub fn apply_access_intent(
        &self,
        store: &mut Store,
        intent: &RuleIntent,
    ) -> Result<bool, Error> {
        let RuleIntent::Access {
            institution,
            concept,
            actor,
            inventory,
            allowed,
            ..
        } = intent
        else {
            return Err(Error::Unavailable);
        };
        let current = crate::storage::institution(store, institution).ok_or(Error::Unavailable)?;
        if !current.concepts.contains(concept) {
            return Err(Error::Unavailable);
        }
        let matching=current.concepts.iter().filter(|id|crate::storage::concept(store, *id).is_some_and(|rule|matches!(rule.effect,SocialEffect::Access {inventory:target,..} if target==*inventory))).copied().collect::<Vec<_>>();
        let current = crate::storage::institution_mut(store, institution).unwrap();
        if *allowed {
            return Ok(current.grants.entry(*concept).or_default().insert(*actor));
        }
        let mut changed = false;
        for id in matching {
            if let Some(people) = current.grants.get_mut(&id) {
                changed |= people.remove(actor);
            }
        }
        Ok(changed)
    }
    pub fn register_duty(
        &self,
        store: &mut Store,
        intent: &RuleIntent,
        agreement: AgreementId,
    ) -> Result<(), Error> {
        let RuleIntent::Duty {
            institution,
            concept,
            actor,
            ..
        } = intent
        else {
            return Err(Error::Unavailable);
        };
        let current =
            crate::storage::institution_mut(store, institution).ok_or(Error::Unavailable)?;
        if !current.concepts.contains(concept)
            || current
                .applied
                .get(concept)
                .is_some_and(|people| people.contains_key(actor))
        {
            return Err(Error::Unavailable);
        }
        current
            .applied
            .entry(*concept)
            .or_default()
            .insert(*actor, agreement);
        Ok(())
    }
    pub fn prepare_case_learning(
        &self,
        store: &Store,
        case: SocialCaseView,
        concept: Option<ConceptId>,
        observers: &BTreeMap<PersonId, u16>,
    ) -> Result<PreparedCaseLearning, Error> {
        if concept.is_some_and(|id| crate::storage::concept(store, id).is_none()) {
            return Err(Error::Unavailable);
        }
        case.validate_shape()?;
        let mut edits = Vec::new();
        for (observer, empathy) in observers {
            if observer.0 == 0 {
                return Err(Error::Unavailable);
            }
            let before = crate::storage::knowledge(store, *observer).cloned();
            let mut after = before.clone().unwrap_or_default();
            apply_case_learning(&mut after, &case, concept, *observer, *empathy);
            edits.push((*observer, before, after));
        }
        Ok(PreparedCaseLearning {
            before_case: crate::storage::case(store, case.id).cloned(),
            case,
            edits,
        })
    }
    pub fn remember_case(
        &self,
        store: &mut Store,
        case: SocialCaseView,
        concept: Option<ConceptId>,
        observers: &BTreeMap<PersonId, u16>,
    ) -> Result<(), Error> {
        let plan = self.prepare_case_learning(store, case, concept, observers)?;
        let mut batch = Batch::new();
        plan.stage_into(store, &mut batch)?;
        store.apply_batch(batch)
    }
}

impl Runtime {
    pub fn allows_access(
        &self,
        store: &Store,
        actor: PersonId,
        inventory: InventoryId,
        authority: &AuthorityFacts,
    ) -> bool {
        crate::storage::institutions(store).map(|(_, row)| row).filter(|i|i.status==InstitutionStatus::Active).any(|i|i.concepts.iter().any(|id| {
            let Some(rule)=crate::storage::concept(store, id) else{return false;};let Some(office)=i.charter.offices.iter().find(|o|o.slot==rule.authority) else{return false;};
            matches!(rule.effect,SocialEffect::Access {inventory:target,allowed:true} if target==inventory) && i.office_effective(office,authority) && i.grants.get(id).is_some_and(|people|people.contains(&actor))
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Runtime, Store, AuthorityFacts) {
        let people = BTreeSet::from([PersonId(11), PersonId(12), PersonId(13)]);
        let rule = InstitutionalRule {
            subjects: Subjects::members(),
            when: RuleExpression::Fact(SocialCondition::Local),
            effect: SocialEffect::Access {
                inventory: InventoryId(20),
                allowed: true,
            },
            authority: 0,
        };
        let charter = InstitutionCharter {
            purpose: "Chosen shared store".into(),
            jurisdiction: AreaId(1),
            founders: people.clone(),
            open_membership: true,
            procedure: DecisionProcedure {
                proposers: Subjects::members(),
                voters: Subjects::members(),
                minimum_support: 2,
            },
            offices: vec![OfficeMandate {
                slot: 0,
                holder: PersonId(11),
                inventories: BTreeSet::from([InventoryId(20), InventoryId(21)]),
                sites: BTreeSet::from([SiteId(22)]),
                powers: BTreeSet::from([
                    OfficePower::Access,
                    OfficePower::Delivery,
                    OfficePower::Labor,
                ]),
                minimum_support: 2,
            }],
            rules: vec![rule.clone()],
            contributions: Vec::new(),
            evidence: BTreeSet::new(),
        };
        let institution = Institution {
            id: InstitutionId(2),
            origin: None,
            charter,
            members: people.clone(),
            declined: BTreeSet::new(),
            invited: BTreeSet::new(),
            status: InstitutionStatus::Active,
            concepts: vec![ConceptId(3)],
            office_support: BTreeMap::from([(0, people.clone())]),
            grants: BTreeMap::new(),
            applied: BTreeMap::new(),
            funding: Vec::new(),
            revision: 0,
        };
        let mut schema = game_ecs::Schema::new();
        crate::storage::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        insert_concept(&mut store, ConceptId(3), Concept { rule }).unwrap();
        insert_institution(&mut store, institution.id, institution).unwrap();
        (Runtime::default(), store, AuthorityFacts { alive: people })
    }
    fn case(outcome: SocialOutcome) -> SocialCaseView {
        SocialCaseView {
            id: CaseId(40),
            kind: SocialCaseKind::Exclusion,
            subject: PersonId(12),
            counterpart: PersonId(11),
            area: AreaId(1),
            inventory: Some(InventoryId(20)),
            good: Good::Food,
            need_band: 2,
            reciprocal: false,
            outcome,
            observed_at: SimTime(1),
        }
    }
    #[test]
    fn support_and_liveness_determine_office_power_without_replacing_the_person() {
        let (_world, mut store, mut authority) = fixture();
        let i = crate::storage::institution_mut(&mut store, InstitutionId(2)).unwrap();
        let office = i.charter.offices[0].clone();
        assert!(i.office_effective(&office, &authority));
        authority.alive.remove(&PersonId(12));
        authority.alive.remove(&PersonId(13));
        assert!(!i.office_effective(&office, &authority));
        assert_eq!(i.charter.offices[0].holder, PersonId(11));
        assert!(i.members.contains(&PersonId(12)));
        let subjects = Subjects {
            members: false,
            named: BTreeSet::from([PersonId(13)]),
            offices: BTreeSet::from([0]),
        };
        assert_eq!(
            i.subjects(&subjects, &authority),
            BTreeSet::from([PersonId(13)])
        );
    }
    #[test]
    fn conditions_use_only_the_observers_known_cases_and_stocks() {
        let (world, mut store, _) = fixture();
        let stocks = BTreeMap::new();
        let input = ConditionInput {
            observer: PersonId(11),
            subject: PersonId(12),
            subject_local: true,
            own_hunger: 999_999,
            known_stocks: &stocks,
        };
        let expression = RuleExpression::Fact(SocialCondition::HungerAtLeast(200_000));
        let i = crate::storage::institution(&store, InstitutionId(2))
            .unwrap()
            .clone();
        assert!(!world.condition_holds(&store, &i, &input, &expression));
        world
            .remember_case(
                &mut store,
                case(SocialOutcome::Unresolved),
                None,
                &BTreeMap::from([(PersonId(13), 500)]),
            )
            .unwrap();
        assert!(!world.condition_holds(&store, &i, &input, &expression));
        world
            .remember_case(
                &mut store,
                case(SocialOutcome::Unresolved),
                None,
                &BTreeMap::from([(PersonId(11), 500)]),
            )
            .unwrap();
        assert!(world.condition_holds(&store, &i, &input, &expression));
        assert!(world.condition_holds(
            &store,
            &i,
            &input,
            &RuleExpression::Fact(SocialCondition::Excluded {
                inventory: InventoryId(20)
            })
        ));
    }
    #[test]
    fn chosen_quorum_ignores_outsiders_and_atomically_adopts_and_invalidates_stale_proposals() {
        let (world, mut store, authority) = fixture();
        let change = InstitutionChange::AddRule(
            crate::storage::concept(&store, ConceptId(3))
                .unwrap()
                .clone(),
        );
        let proposal = RuleProposal {
            id: RuleProposalId(30),
            institution: InstitutionId(2),
            proposer: PersonId(11),
            change: change.clone(),
            votes: BTreeMap::from([(PersonId(11), true), (PersonId(99), true)]),
            base_revision: 0,
            status: RuleProposalStatus::Proposed,
        };
        insert_proposal(&mut store, proposal.id, proposal.clone()).unwrap();
        assert_eq!(
            world
                .proposal_outcome(&store, proposal.id, &authority)
                .unwrap(),
            RuleProposalStatus::Proposed
        );
        crate::storage::proposal_mut(&mut store, proposal.id)
            .unwrap()
            .votes
            .insert(PersonId(12), true);
        insert_proposal(
            &mut store,
            RuleProposalId(31),
            RuleProposal {
                id: RuleProposalId(31),
                ..proposal.clone()
            },
        )
        .unwrap();
        let before = store.snapshot().unwrap();
        let prepared = world
            .prepare_resolution(&store, proposal.id, &authority, Some(ConceptId(32)))
            .unwrap();
        assert_eq!(store.snapshot().unwrap(), before);
        prepared.commit(&world, &mut store).unwrap();
        assert_eq!(
            crate::storage::institution(&store, InstitutionId(2))
                .unwrap()
                .revision,
            1
        );
        assert_eq!(
            crate::storage::institution(&store, InstitutionId(2))
                .unwrap()
                .charter
                .rules
                .len(),
            2
        );
        assert_eq!(
            crate::storage::proposal(&store, RuleProposalId(31))
                .unwrap()
                .status,
            RuleProposalStatus::Rejected
        );
    }
    #[test]
    fn stale_prepared_decision_changes_nothing() {
        let (world, mut store, authority) = fixture();
        insert_proposal(
            &mut store,
            RuleProposalId(30),
            RuleProposal {
                id: RuleProposalId(30),
                institution: InstitutionId(2),
                proposer: PersonId(11),
                change: InstitutionChange::Dissolve,
                votes: BTreeMap::from([(PersonId(11), true), (PersonId(12), true)]),
                base_revision: 0,
                status: RuleProposalStatus::Proposed,
            },
        )
        .unwrap();
        let plan = world
            .prepare_resolution(&store, RuleProposalId(30), &authority, None)
            .unwrap();
        crate::storage::institution_mut(&mut store, InstitutionId(2))
            .unwrap()
            .revision = 1;
        let before = store.snapshot().unwrap();
        assert_eq!(plan.commit(&world, &mut store), Err(Error::Unavailable));
        assert_eq!(store.snapshot().unwrap(), before);
    }
    #[test]
    fn policy_returns_a_real_duty_request_once_and_records_only_its_receipt() {
        let (world, mut store, authority) = fixture();
        let rule = InstitutionalRule {
            subjects: Subjects::members(),
            when: RuleExpression::Fact(SocialCondition::Member),
            effect: SocialEffect::Delivery {
                source: InventoryId(20),
                destination: InventoryId(21),
                quantity: Quantity(1_000),
                beneficiary: Beneficiary::Office(0),
                due_after: SimDuration(120_000),
            },
            authority: 0,
        };
        insert_concept(&mut store, ConceptId(4), Concept { rule: rule.clone() }).unwrap();
        let i = crate::storage::institution_mut(&mut store, InstitutionId(2)).unwrap();
        i.charter.rules.push(rule);
        i.concepts.push(ConceptId(4));
        let i = i.clone();
        let stocks = BTreeMap::new();
        let input = ConditionInput {
            observer: PersonId(11),
            subject: PersonId(12),
            subject_local: true,
            own_hunger: 0,
            known_stocks: &stocks,
        };
        let intent = world
            .rule_intent(&store, &i, 1, &input, &authority, true, SimTime(60_000))
            .unwrap();
        assert!(matches!(
            intent,
            RuleIntent::Duty {
                term: AgreementTerm::Delivery {
                    deadline: SimTime(180_000),
                    quantity: Quantity(1_000),
                    ..
                },
                ..
            }
        ));
        assert!(
            crate::storage::institution(&store, i.id)
                .unwrap()
                .applied
                .is_empty()
        );
        world
            .register_duty(&mut store, &intent, AgreementId(50))
            .unwrap();
        assert!(
            world
                .rule_intent(&store, &i, 1, &input, &authority, true, SimTime(120_000))
                .is_none()
        );
    }
    #[test]
    fn unsupported_access_and_unknown_precedents_do_not_affect_outcomes() {
        let (world, mut store, mut authority) = fixture();
        let i = crate::storage::institution(&store, InstitutionId(2))
            .unwrap()
            .clone();
        let stocks = BTreeMap::new();
        let input = ConditionInput {
            observer: PersonId(11),
            subject: PersonId(12),
            subject_local: true,
            own_hunger: 0,
            known_stocks: &stocks,
        };
        let intent = world
            .rule_intent(&store, &i, 0, &input, &authority, true, SimTime(0))
            .unwrap();
        assert!(world.apply_access_intent(&mut store, &intent).unwrap());
        assert!(world.allows_access(&store, PersonId(12), InventoryId(20), &authority));
        authority.alive.clear();
        assert!(!world.allows_access(&store, PersonId(12), InventoryId(20), &authority));
        let prior = case(SocialOutcome::Shared);
        world
            .remember_case(
                &mut store,
                prior.clone(),
                Some(ConceptId(3)),
                &BTreeMap::from([(PersonId(13), 500)]),
            )
            .unwrap();
        let next = SocialCaseView {
            id: CaseId(41),
            ..prior
        };
        assert_eq!(world.precedent_score(&store, PersonId(11), &next), 0);
        assert_eq!(world.precedent_score(&store, PersonId(13), &next), 600);
        let saved = serde_json::to_vec(&store.snapshot().unwrap()).unwrap();
        let restored = Store::restore(
            store.schema().clone(),
            serde_json::from_slice(&saved).unwrap(),
        )
        .unwrap();
        assert_eq!(store, restored);
    }
}

#[cfg(test)]
#[path = "institutions/command_tests.rs"]
mod command_tests;

/// Only the new case and the memories of its actual witnesses are captured.
pub struct PreparedCaseLearning {
    before_case: Option<SocialCaseView>,
    case: SocialCaseView,
    edits: Vec<(PersonId, Option<Knowledge>, Knowledge)>,
}
impl PreparedCaseLearning {
    pub fn verify(&self, store: &Store) -> Result<(), Error> {
        if crate::storage::case(store, self.case.id) != self.before_case.as_ref()
            || self.edits.iter().any(|(actor, before, _)| {
                crate::storage::knowledge(store, *actor) != before.as_ref()
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn also_remember(&mut self, store: &Store, actor: PersonId) {
        if let Some((_, _, after)) = self.edits.iter_mut().find(|(id, _, _)| *id == actor) {
            after.cases.insert(self.case.id);
        } else {
            let before = crate::storage::knowledge(store, actor).cloned();
            let mut after = before.clone().unwrap_or_default();
            after.cases.insert(self.case.id);
            self.edits.push((actor, before, after));
        }
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        self.verify(store)?;
        batch.insert(StableId(self.case.id.0), self.case);
        for (actor, _, after) in self.edits {
            batch.insert(StableId(actor.0), after);
        }
        Ok(())
    }
}
