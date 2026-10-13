//! Agreements describe promises and chosen procedures, never physical transfers.
use game_core::*;
use game_ecs::{Batch, StableId, Store};
mod storage;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub use storage::*;
mod transitions;
pub use transitions::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgreementTerm {
    SiteAccess {
        grantor: PersonId,
        grantee: PersonId,
        site: SiteId,
        allowed: bool,
    },
    OutputShare {
        debtor: PersonId,
        creditor: PersonId,
        site: SiteId,
        good: Good,
        numerator: u64,
        denominator: u64,
        destination: InventoryId,
        pay_by: SimTime,
        ends_at: SimTime,
    },
    ConstructionLabor {
        debtor: PersonId,
        creditor: PersonId,
        project: ProjectId,
        millis: u64,
        deadline: SimTime,
    },
    Delivery {
        debtor: PersonId,
        creditor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        deadline: SimTime,
    },
    Labor {
        debtor: PersonId,
        creditor: PersonId,
        site: SiteId,
        millis: u64,
        deadline: SimTime,
    },
    Access {
        grantor: PersonId,
        grantee: PersonId,
        inventory: InventoryId,
        allowed: bool,
    },
}
impl AgreementTerm {
    pub fn parties(&self) -> [PersonId; 2] {
        match *self {
            Self::OutputShare {
                debtor, creditor, ..
            }
            | Self::ConstructionLabor {
                debtor, creditor, ..
            }
            | Self::Delivery {
                debtor, creditor, ..
            }
            | Self::Labor {
                debtor, creditor, ..
            } => [debtor, creditor],
            Self::SiteAccess {
                grantor, grantee, ..
            }
            | Self::Access {
                grantor, grantee, ..
            } => [grantor, grantee],
        }
    }
    pub fn amount(&self) -> u64 {
        match *self {
            Self::OutputShare { .. } => 0,
            Self::Delivery { quantity, .. } => quantity.0,
            Self::Labor { millis, .. } | Self::ConstructionLabor { millis, .. } => millis,
            Self::Access { .. } | Self::SiteAccess { .. } => 1,
        }
    }
    pub fn deadline(&self) -> Option<SimTime> {
        match *self {
            Self::OutputShare {
                pay_by: deadline, ..
            }
            | Self::Delivery { deadline, .. }
            | Self::Labor { deadline, .. }
            | Self::ConstructionLabor { deadline, .. } => Some(deadline),
            Self::Access { .. } | Self::SiteAccess { .. } => None,
        }
    }
}

/// Parties choose this procedure when accepting the original agreement.
/// Absence leaves amendments unresolved; it does not invent a fallback law.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmendmentProcedure {
    pub eligible_proposers: BTreeSet<PersonId>,
    pub approvers: BTreeSet<PersonId>,
    pub threshold: usize,
    /// Named approvers required for particular term indices, in addition to quorum.
    pub term_approvers: BTreeMap<u16, BTreeSet<PersonId>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgreementAction {
    Report {
        agreement: AgreementId,
        recipient: PersonId,
    },
    Propose {
        parties: BTreeSet<PersonId>,
        terms: Vec<AgreementTerm>,
        procedure: Option<AmendmentProcedure>,
    },
    Counteroffer {
        agreement: AgreementId,
        terms: Vec<AgreementTerm>,
        procedure: Option<AmendmentProcedure>,
    },
    Accept {
        agreement: AgreementId,
    },
    Reject {
        agreement: AgreementId,
    },
    Withdraw {
        agreement: AgreementId,
    },
    Amend {
        agreement: AgreementId,
        replacements: BTreeMap<u16, AgreementTerm>,
    },
    Vote {
        amendment: AmendmentId,
        support: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgreementStatus {
    Proposed,
    Active,
    Fulfilled,
    Rejected,
    Withdrawn,
    Breached,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendmentStatus {
    Proposed,
    Adopted,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationView {
    pub id: ObligationId,
    pub term: AgreementTerm,
    pub performed: u64,
    #[serde(default)]
    pub accrued: u64,
    #[serde(default)]
    pub share_remainder: u64,
    #[serde(default)]
    pub share_scale: u64,
    pub defaulted: bool,
    #[serde(default)]
    pub cargo: BTreeMap<InventoryId, u64>,
    #[serde(default)]
    pub observed_at: SimTime,
    #[serde(default)]
    pub learned_at: SimTime,
    #[serde(default)]
    pub source: Option<KnowledgeSource>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgreementView {
    pub id: AgreementId,
    pub proposer: PersonId,
    pub parties: BTreeSet<PersonId>,
    pub accepted: BTreeSet<PersonId>,
    pub status: AgreementStatus,
    pub terms: Vec<AgreementTerm>,
    pub obligations: Vec<ObligationView>,
    pub procedure: Option<AmendmentProcedure>,
    pub amendments: Vec<AmendmentView>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub observed_at: SimTime,
    #[serde(default)]
    pub learned_at: SimTime,
    #[serde(default)]
    pub source: Option<KnowledgeSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmendmentView {
    pub id: AmendmentId,
    pub proposer: PersonId,
    pub replacements: BTreeMap<u16, AgreementTerm>,
    pub votes: BTreeMap<PersonId, bool>,
    pub status: AmendmentStatus,
    #[serde(default)]
    pub base_revision: u64,
}

impl ObligationView {
    pub fn amount(&self) -> u64 {
        if matches!(self.term, AgreementTerm::OutputShare { .. }) {
            self.accrued
        } else {
            self.term.amount()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agreement {
    pub id: AgreementId,
    pub proposer: PersonId,
    pub parties: BTreeSet<PersonId>,
    pub accepted: BTreeSet<PersonId>,
    pub terms: Vec<AgreementTerm>,
    pub procedure: Option<AmendmentProcedure>,
    pub obligations: Vec<ObligationId>,
    pub status: AgreementStatus,
    pub proposed_at: SimTime,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obligation {
    pub id: ObligationId,
    pub agreement: AgreementId,
    pub term: AgreementTerm,
    pub performed: u64,
    #[serde(default)]
    pub accrued: u64,
    #[serde(default)]
    pub share_remainder: u64,
    #[serde(default)]
    pub share_scale: u64,
    pub defaulted: bool,
    /// Conserved cargo loaded from the agreed source, attributed once.
    pub cargo: BTreeMap<InventoryId, u64>,
}
impl Obligation {
    pub fn amount(&self) -> u64 {
        if matches!(self.term, AgreementTerm::OutputShare { .. }) {
            self.accrued
        } else {
            self.term.amount()
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Amendment {
    pub id: AmendmentId,
    pub agreement: AgreementId,
    pub proposer: PersonId,
    pub replacements: BTreeMap<u16, AgreementTerm>,
    pub votes: BTreeMap<PersonId, bool>,
    pub status: AmendmentStatus,
    pub base_revision: u64,
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a
}
/// A changed future share never rewrites the fraction already owed.
pub fn accrue_share(
    amount: u64,
    remainder: u64,
    scale: u64,
    numerator: u64,
    denominator: u64,
    quantity: u128,
) -> Result<(u64, u64, u64), Error> {
    if denominator == 0 {
        return Err(Error::Unavailable);
    }
    let product = quantity
        .checked_mul(u128::from(numerator))
        .ok_or(Error::Overflow)?;
    let whole = u64::try_from(product / u128::from(denominator)).map_err(|_| Error::Overflow)?;
    let fraction = (product % u128::from(denominator)) as u64;
    let scale = if remainder == 0 {
        1
    } else if scale == 0 {
        denominator
    } else {
        scale
    };
    let divisor = gcd(remainder, scale);
    let old_remainder = remainder / divisor;
    let old_scale = scale / divisor;
    if fraction == 0 {
        return Ok((
            amount.checked_add(whole).ok_or(Error::Overflow)?,
            old_remainder,
            old_scale,
        ));
    }
    let divisor = gcd(fraction, denominator);
    let fraction = fraction / divisor;
    let denominator = denominator / divisor;
    let common = (old_scale / gcd(old_scale, denominator))
        .checked_mul(denominator)
        .ok_or(Error::Overflow)?;
    let total = u128::from(old_remainder) * u128::from(common / old_scale)
        + u128::from(fraction) * u128::from(common / denominator);
    let extra = u64::try_from(total / u128::from(common)).map_err(|_| Error::Overflow)?;
    let remaining = (total % u128::from(common)) as u64;
    let divisor = gcd(remaining, common);
    Ok((
        amount
            .checked_add(whole)
            .and_then(|v| v.checked_add(extra))
            .ok_or(Error::Overflow)?,
        remaining / divisor,
        common / divisor,
    ))
}
pub fn stream_ended(term: &AgreementTerm, time: SimTime) -> bool {
    !matches!(term,AgreementTerm::OutputShare {ends_at,..} if *ends_at>time)
}

/// One body's eligible work during one shared-clock interval. The caller supplies
/// physical eligibility; the ledger allocates that time once across promises.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkTarget {
    Site(SiteId),
    Project(ProjectId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkAssignment {
    pub actor: PersonId,
    pub target: WorkTarget,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkCredit {
    pub obligation: ObligationId,
    pub agreement: AgreementId,
    pub actor: PersonId,
    pub millis: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct ObligationPatch {
    before: Obligation,
    after: Obligation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedLabor {
    assignments: Vec<WorkAssignment>,
    obligation_members: BTreeSet<ObligationId>,
    pending_before: BTreeMap<ObligationId, Option<PendingWork>>,
    patches: Vec<ObligationPatch>,
    pending: BTreeMap<ObligationId, (u64, u64)>,
    statuses: BTreeMap<AgreementId, AgreementStatus>,
    agreements_before: BTreeMap<AgreementId, Agreement>,
    read_obligations: BTreeMap<ObligationId, Obligation>,
    pub credits: Vec<WorkCredit>,
}
impl Ledger<'_> {
    pub fn status_for(&self, id: AgreementId, now: SimTime) -> Result<AgreementStatus, Error> {
        let agreement = self.agreements().get(&id).ok_or(Error::Unavailable)?;
        let obligations = agreement
            .obligations
            .iter()
            .map(|id| self.obligations().get(id).ok_or(Error::Unavailable))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(
            if obligations
                .iter()
                .all(|o| o.performed >= o.amount() && stream_ended(&o.term, now))
            {
                AgreementStatus::Fulfilled
            } else if obligations.iter().any(|o| o.defaulted) {
                AgreementStatus::Breached
            } else {
                AgreementStatus::Active
            },
        )
    }
    pub fn prepare_labor(
        &self,
        from: SimTime,
        to: SimTime,
        assignments: &[WorkAssignment],
    ) -> Result<PreparedLabor, Error> {
        self.prepare_labor_selected(from, to, assignments, false)
    }
    fn prepare_labor_selected(
        &self,
        from: SimTime,
        to: SimTime,
        assignments: &[WorkAssignment],
        allow_distinct_targets: bool,
    ) -> Result<PreparedLabor, Error> {
        let elapsed = to.0.checked_sub(from.0).ok_or(Error::Overflow)?;
        let mut actors = BTreeSet::new();
        if !allow_distinct_targets && assignments.iter().any(|a| !actors.insert(a.actor)) {
            return Err(Error::Unavailable);
        }
        let mut assignments = assignments.to_vec();
        assignments.sort_by_key(|a| {
            (
                matches!(a.target, WorkTarget::Project(_)),
                a.actor,
                a.target,
            )
        });
        let mut plan = PreparedLabor {
            assignments: assignments.clone(),
            obligation_members: BTreeSet::new(),
            pending_before: BTreeMap::new(),
            patches: Vec::new(),
            pending: BTreeMap::new(),
            statuses: BTreeMap::new(),
            agreements_before: BTreeMap::new(),
            read_obligations: BTreeMap::new(),
            credits: Vec::new(),
        };
        for assignment in assignments {
            let mut remaining = elapsed;
            for old in self.obligations().values() {
                let requested = match old.term {
                    AgreementTerm::Labor {
                        debtor,
                        site,
                        millis,
                        ..
                    } if debtor == assignment.actor
                        && assignment.target == WorkTarget::Site(site) =>
                    {
                        Some(millis)
                    }
                    AgreementTerm::ConstructionLabor {
                        debtor,
                        project,
                        millis,
                        ..
                    } if debtor == assignment.actor
                        && assignment.target == WorkTarget::Project(project) =>
                    {
                        Some(millis)
                    }
                    _ => None,
                };
                let Some(requested) = requested else {
                    continue;
                };
                plan.obligation_members.insert(old.id);
                plan.read_obligations.insert(old.id, old.clone());
                let outstanding = requested
                    .checked_sub(old.performed)
                    .ok_or(Error::Unavailable)?;
                let credit = remaining.min(outstanding);
                remaining -= credit;
                if credit == 0 {
                    continue;
                }
                if !self.agreements().contains_key(&old.agreement) {
                    return Err(Error::Unavailable);
                }
                let mut new = old.clone();
                new.performed = new.performed.checked_add(credit).ok_or(Error::Overflow)?;
                let pending = self.pending_work().get(&old.id).copied().unwrap_or(0);
                plan.pending_before.insert(
                    old.id,
                    self.store.get::<PendingWork>(StableId(old.id.0)).cloned(),
                );
                plan.pending.insert(
                    old.id,
                    (pending, pending.checked_add(credit).ok_or(Error::Overflow)?),
                );
                plan.credits.push(WorkCredit {
                    obligation: old.id,
                    agreement: old.agreement,
                    actor: assignment.actor,
                    millis: credit,
                });
                plan.patches.push(ObligationPatch {
                    before: old.clone(),
                    after: new,
                });
            }
        }
        let overrides = plan
            .patches
            .iter()
            .map(|p| (p.after.id, &p.after))
            .collect::<BTreeMap<_, _>>();
        for id in plan
            .credits
            .iter()
            .map(|c| c.agreement)
            .collect::<BTreeSet<_>>()
        {
            let agreement = self.agreements().get(&id).ok_or(Error::Unavailable)?;
            let mut fulfilled = true;
            let mut breached = false;
            for obligation in &agreement.obligations {
                let before = self
                    .obligations()
                    .get(obligation)
                    .ok_or(Error::Unavailable)?;
                plan.read_obligations.insert(*obligation, before.clone());
                let after = overrides.get(obligation).copied().unwrap_or(before);
                fulfilled &= after.performed >= after.amount() && stream_ended(&after.term, to);
                breached |= after.defaulted;
            }
            plan.agreements_before.insert(id, agreement.clone());
            plan.statuses.insert(
                id,
                if fulfilled {
                    AgreementStatus::Fulfilled
                } else if breached {
                    AgreementStatus::Breached
                } else {
                    AgreementStatus::Active
                },
            );
        }
        Ok(plan)
    }
}
impl PreparedLabor {
    /// Reject a stale plan before changing any owned ledger record.
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PermissionAsset {
    Inventory(InventoryId),
    Site(SiteId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventoryTerms {
    pub good: Good,
    pub capacity: Quantity,
}
/// Relevant asset facts, supplied by their owners. No custody state is duplicated
/// in this ledger, and authorizing a promise does not apply the permission.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TermsContext {
    pub now: SimTime,
    pub inventories: BTreeMap<InventoryId, InventoryTerms>,
    pub sites: BTreeSet<SiteId>,
    pub projects: BTreeSet<ProjectId>,
    pub outputs: BTreeMap<(SiteId, Good), InventoryId>,
    pub grants: BTreeSet<(PermissionAsset, PersonId)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PermissionEffect {
    pub asset: PermissionAsset,
    pub grantor: PersonId,
    pub grantee: PersonId,
    pub allowed: bool,
}
pub fn validate_terms(
    parties: &BTreeSet<PersonId>,
    terms: &[AgreementTerm],
    context: &TermsContext,
    future: bool,
) -> Result<(), Error> {
    if terms.is_empty() || terms.len() > 16 {
        return Err(Error::Unavailable);
    }
    for term in terms {
        let [a, b] = term.parties();
        if a == b
            || !parties.contains(&a)
            || !parties.contains(&b)
            || future && term.amount() == 0 && !matches!(term, AgreementTerm::OutputShare { .. })
            || future && term.deadline().is_some_and(|d| d <= context.now)
        {
            return Err(Error::Unavailable);
        }
        let valid = match term {
            AgreementTerm::Delivery {
                source,
                destination,
                quantity,
                ..
            } => context
                .inventories
                .get(source)
                .zip(context.inventories.get(destination))
                .is_some_and(|(a, b)| {
                    source != destination && a.good == b.good && quantity.0 <= b.capacity.0
                }),
            AgreementTerm::Labor { site, .. } => context.sites.contains(site),
            AgreementTerm::ConstructionLabor { project, .. } => context.projects.contains(project),
            AgreementTerm::Access {
                grantor, inventory, ..
            } => context
                .grants
                .contains(&(PermissionAsset::Inventory(*inventory), *grantor)),
            AgreementTerm::SiteAccess { grantor, site, .. } => context
                .grants
                .contains(&(PermissionAsset::Site(*site), *grantor)),
            AgreementTerm::OutputShare {
                site,
                good,
                numerator,
                denominator,
                destination,
                pay_by,
                ends_at,
                ..
            } => {
                *numerator > 0
                    && *denominator > 0
                    && numerator <= denominator
                    && pay_by >= ends_at
                    && (!future || *ends_at > context.now)
                    && context
                        .outputs
                        .get(&(*site, *good))
                        .is_some_and(|source| source != destination)
                    && context
                        .inventories
                        .get(destination)
                        .is_some_and(|i| i.good == *good)
            }
        };
        if !valid {
            return Err(Error::Unavailable);
        }
    }
    Ok(())
}
pub fn permission_effect(term: &AgreementTerm) -> Option<PermissionEffect> {
    match *term {
        AgreementTerm::Access {
            grantor,
            grantee,
            inventory,
            allowed,
        } => Some(PermissionEffect {
            asset: PermissionAsset::Inventory(inventory),
            grantor,
            grantee,
            allowed,
        }),
        AgreementTerm::SiteAccess {
            grantor,
            grantee,
            site,
            allowed,
        } => Some(PermissionEffect {
            asset: PermissionAsset::Site(site),
            grantor,
            grantee,
            allowed,
        }),
        _ => None,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedActivation {
    before: Agreement,
    after: Agreement,
    obligations: Vec<Obligation>,
    pub permissions: Vec<PermissionEffect>,
}
impl Ledger<'_> {
    pub fn prepare_activation(
        &self,
        id: AgreementId,
        obligations: &[ObligationId],
        context: &TermsContext,
    ) -> Result<PreparedActivation, Error> {
        let before = self
            .agreements()
            .get(&id)
            .ok_or(Error::Unavailable)?
            .clone();
        if before.status != AgreementStatus::Proposed
            || before.accepted != before.parties
            || !before.obligations.is_empty()
            || obligations.len() != before.terms.len()
            || obligations.iter().copied().collect::<BTreeSet<_>>().len() != obligations.len()
            || obligations
                .iter()
                .any(|id| self.obligations().contains_key(id))
        {
            return Err(Error::Unavailable);
        }
        activation_for(&before, obligations, context)
    }
}

fn activation_for(
    before: &Agreement,
    obligations: &[ObligationId],
    context: &TermsContext,
) -> Result<PreparedActivation, Error> {
    if before.status != AgreementStatus::Proposed
        || before.accepted != before.parties
        || !before.obligations.is_empty()
        || obligations.len() != before.terms.len()
        || obligations.iter().any(|id| id.0 == 0)
        || obligations.iter().copied().collect::<BTreeSet<_>>().len() != obligations.len()
    {
        return Err(Error::Unavailable);
    }
    let before = before.clone();
    validate_terms(&before.parties, &before.terms, context, true)?;
    let mut after = before.clone();
    after.status = AgreementStatus::Active;
    after.obligations = obligations.to_vec();
    let mut permissions = Vec::new();
    let obligations = before
        .terms
        .iter()
        .zip(obligations)
        .map(|(term, id)| {
            let permission = permission_effect(term);
            let performed = u64::from(permission.is_some());
            if let Some(permission) = permission {
                permissions.push(permission);
            }
            Obligation {
                id: *id,
                agreement: before.id,
                term: term.clone(),
                performed,
                accrued: 0,
                share_remainder: 0,
                share_scale: 0,
                defaulted: false,
                cargo: BTreeMap::new(),
            }
        })
        .collect();
    let mut plan = PreparedActivation {
        before,
        after,
        obligations,
        permissions,
    };
    if plan
        .obligations
        .iter()
        .all(|o| o.performed >= o.amount() && stream_ended(&o.term, context.now))
    {
        plan.after.status = AgreementStatus::Fulfilled;
    }
    Ok(plan)
}

impl PreparedActivation {
    /// The coordinator must first validate/apply every permission effect in the
    /// same transaction. This commits only economic records, never asset access.
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        if agreement(store, self.before.id) != Some(&self.before)
            || self
                .obligations
                .iter()
                .any(|o| store.contains(StableId(o.id.0)))
        {
            return Err(Error::Unavailable);
        }
        let mut batch = Batch::new();
        for row in self.obligations {
            batch.insert(StableId(row.id.0), row);
        }
        batch.insert(StableId(self.after.id.0), self.after);
        store.apply_batch(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ledger(terms: Vec<AgreementTerm>) -> Store {
        let mut state = fixture();
        let id = AgreementId(1);
        let parties = BTreeSet::from([PersonId(11), PersonId(12)]);
        let obligations = terms
            .iter()
            .enumerate()
            .map(|(i, term)| {
                let id = ObligationId(i as u64 + 2);
                insert_obligation(
                    &mut state,
                    id,
                    Obligation {
                        id,
                        agreement: AgreementId(1),
                        term: term.clone(),
                        performed: 0,
                        accrued: 0,
                        share_remainder: 0,
                        share_scale: 0,
                        defaulted: false,
                        cargo: BTreeMap::new(),
                    },
                )
                .unwrap();
                id
            })
            .collect();
        insert_agreement(
            &mut state,
            id,
            Agreement {
                id,
                proposer: PersonId(11),
                parties: parties.clone(),
                accepted: parties,
                terms,
                procedure: None,
                obligations,
                status: AgreementStatus::Active,
                proposed_at: SimTime(0),
                revision: 0,
            },
        )
        .unwrap();
        state
    }
    fn work(millis: u64) -> AgreementTerm {
        AgreementTerm::Labor {
            debtor: PersonId(11),
            creditor: PersonId(12),
            site: SiteId(20),
            millis,
            deadline: SimTime(200_000),
        }
    }
    #[test]
    fn shares_preserve_exact_prior_fraction_when_future_terms_change() {
        let a = accrue_share(0, 0, 0, 1, 3, 1).unwrap();
        assert_eq!(a, (0, 1, 3));
        let b = accrue_share(a.0, a.1, a.2, 1, 2, 1).unwrap();
        assert_eq!(b, (0, 5, 6));
        assert_eq!(accrue_share(b.0, b.1, b.2, 1, 2, 1).unwrap(), (1, 1, 3));
    }
    #[test]
    fn shares_are_chunk_invariant_and_reject_overflow() {
        let mut split = (0, 0, 0);
        for q in [7, 1, 3, 8, 2] {
            split = accrue_share(split.0, split.1, split.2, 2, 7, q).unwrap();
        }
        assert_eq!(split, accrue_share(0, 0, 0, 2, 7, 21).unwrap());
        assert_eq!(split, (6, 0, 1));
        assert_eq!(accrue_share(u64::MAX, 0, 0, 1, 1, 1), Err(Error::Overflow));
        assert_eq!(accrue_share(0, 0, 0, 1, 0, 1), Err(Error::Unavailable));
    }
    #[test]
    fn one_exclusive_interval_is_allocated_once_across_promises_and_slices() {
        let state = ledger(vec![work(90_000), work(100_000)]);
        let assignment = WorkAssignment {
            actor: PersonId(11),
            target: WorkTarget::Site(SiteId(20)),
        };
        let mut whole = state.clone();
        let plan = Ledger::new(&whole)
            .prepare_labor(SimTime(0), SimTime(120_000), &[assignment])
            .unwrap();
        assert_eq!(plan.credits.iter().map(|c| c.millis).sum::<u64>(), 120_000);
        plan.commit(&mut whole).unwrap();
        assert_eq!(
            crate::obligation(&whole, ObligationId(2))
                .unwrap()
                .performed,
            90_000
        );
        assert_eq!(
            crate::obligation(&whole, ObligationId(3))
                .unwrap()
                .performed,
            30_000
        );
        let mut split = state;
        Ledger::new(&split)
            .prepare_labor(SimTime(0), SimTime(33_001), &[assignment])
            .unwrap()
            .commit(&mut split)
            .unwrap();
        Ledger::new(&split)
            .prepare_labor(SimTime(33_001), SimTime(120_000), &[assignment])
            .unwrap()
            .commit(&mut split)
            .unwrap();
        assert_eq!(whole, split);
    }
    #[test]
    fn duplicate_actor_or_stale_prepared_labor_never_partially_credits() {
        let mut state = ledger(vec![work(90_000), work(100_000)]);
        let assignment = WorkAssignment {
            actor: PersonId(11),
            target: WorkTarget::Site(SiteId(20)),
        };
        assert_eq!(
            Ledger::new(&state).prepare_labor(
                SimTime(0),
                SimTime(60_000),
                &[
                    assignment,
                    WorkAssignment {
                        actor: assignment.actor,
                        target: WorkTarget::Project(ProjectId(30))
                    }
                ]
            ),
            Err(Error::Unavailable)
        );
        let plan = Ledger::new(&state)
            .prepare_labor(SimTime(0), SimTime(60_000), &[assignment])
            .unwrap();
        obligation_mut(&mut state, ObligationId(3))
            .unwrap()
            .performed = 1;
        let before = state.clone();
        assert_eq!(plan.commit(&mut state), Err(Error::Unavailable));
        assert_eq!(state, before);
    }
    #[test]
    fn activation_prepares_permissions_without_owning_or_modifying_assets() {
        let term = AgreementTerm::Access {
            grantor: PersonId(11),
            grantee: PersonId(12),
            inventory: InventoryId(21),
            allowed: true,
        };
        let mut state = ledger(vec![term]);
        for id in obligations(&state).map(|(id, _)| id).collect::<Vec<_>>() {
            state.remove::<Obligation>(StableId(id.0)).unwrap();
        }
        let agreement = agreement_mut(&mut state, AgreementId(1)).unwrap();
        agreement.obligations.clear();
        agreement.status = AgreementStatus::Proposed;
        let mut context = TermsContext {
            now: SimTime(1),
            ..TermsContext::default()
        };
        context
            .grants
            .insert((PermissionAsset::Inventory(InventoryId(21)), PersonId(11)));
        let before = state.clone();
        let prepared = Ledger::new(&state)
            .prepare_activation(AgreementId(1), &[ObligationId(31)], &context)
            .unwrap();
        assert_eq!(state, before);
        assert_eq!(
            prepared.permissions,
            vec![PermissionEffect {
                asset: PermissionAsset::Inventory(InventoryId(21)),
                grantor: PersonId(11),
                grantee: PersonId(12),
                allowed: true
            }]
        );
        prepared.commit(&mut state).unwrap();
        assert_eq!(
            crate::agreement(&state, AgreementId(1)).unwrap().status,
            AgreementStatus::Fulfilled
        );
        assert_eq!(
            crate::obligation(&state, ObligationId(31))
                .unwrap()
                .performed,
            1
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProductionFact {
    pub actor: PersonId,
    pub site: SiteId,
    pub good: Good,
    pub quantity: Quantity,
    pub time: SimTime,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShareCredit {
    pub obligation: ObligationId,
    pub actor: PersonId,
    pub quantity: Quantity,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedProduction {
    patches: Vec<ObligationPatch>,
    pub credits: Vec<ShareCredit>,
}
impl Ledger<'_> {
    pub fn prepare_production(&self, fact: ProductionFact) -> Result<PreparedProduction, Error> {
        let mut plan = PreparedProduction {
            patches: Vec::new(),
            credits: Vec::new(),
        };
        for old in self.obligations().values() {
            if let AgreementTerm::OutputShare {
                site,
                good,
                numerator,
                denominator,
                ends_at,
                ..
            } = old.term
                && site == fact.site
                && good == fact.good
                && fact.time < ends_at
            {
                let (amount, remainder, scale) = accrue_share(
                    old.accrued,
                    old.share_remainder,
                    old.share_scale,
                    numerator,
                    denominator,
                    u128::from(fact.quantity.0),
                )?;
                let credit = amount - old.accrued;
                let mut after = old.clone();
                after.accrued = amount;
                after.share_remainder = remainder;
                after.share_scale = scale;
                plan.patches.push(ObligationPatch {
                    before: old.clone(),
                    after,
                });
                if credit > 0 {
                    plan.credits.push(ShareCredit {
                        obligation: old.id,
                        actor: fact.actor,
                        quantity: Quantity(credit),
                    });
                }
            }
        }
        Ok(plan)
    }
    pub fn defaults_due(&self, now: SimTime) -> Vec<ObligationId> {
        self.obligations()
            .values()
            .filter(|o| {
                !o.defaulted
                    && o.performed < o.amount()
                    && o.term.deadline().is_some_and(|d| d <= now)
            })
            .map(|o| o.id)
            .collect()
    }
}
impl PreparedProduction {
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        if self
            .patches
            .iter()
            .any(|p| obligation(store, p.before.id) != Some(&p.before))
        {
            return Err(Error::Unavailable);
        }
        let mut batch = Batch::new();
        for patch in self.patches {
            batch.insert(StableId(patch.after.id.0), patch.after);
        }
        store.apply_batch(batch)
    }
}

#[cfg(test)]
mod obligation_tests {
    use super::*;
    #[test]
    fn production_creates_owed_shares_and_expiry_keeps_unpaid_defaulted_debt() {
        let id = AgreementId(1);
        let obligation = ObligationId(2);
        let actor = PersonId(3);
        let creditor = PersonId(4);
        let term = AgreementTerm::OutputShare {
            debtor: actor,
            creditor,
            site: SiteId(5),
            good: Good::Food,
            numerator: 1,
            denominator: 3,
            destination: InventoryId(6),
            pay_by: SimTime(120_000),
            ends_at: SimTime(90_000),
        };
        let parties = BTreeSet::from([actor, creditor]);
        let mut state = fixture();
        insert_agreement(
            &mut state,
            id,
            Agreement {
                id,
                proposer: actor,
                parties: parties.clone(),
                accepted: parties,
                terms: vec![term.clone()],
                procedure: None,
                obligations: vec![obligation],
                status: AgreementStatus::Active,
                proposed_at: SimTime(0),
                revision: 0,
            },
        )
        .unwrap();
        insert_obligation(
            &mut state,
            obligation,
            Obligation {
                id: obligation,
                agreement: id,
                term,
                performed: 0,
                accrued: 0,
                share_remainder: 0,
                share_scale: 0,
                defaulted: false,
                cargo: BTreeMap::new(),
            },
        )
        .unwrap();
        for time in [10_000, 20_000, 30_000] {
            let plan = Ledger::new(&state)
                .prepare_production(ProductionFact {
                    actor,
                    site: SiteId(5),
                    good: Good::Food,
                    quantity: Quantity(1),
                    time: SimTime(time),
                })
                .unwrap();
            plan.commit(&mut state).unwrap();
        }
        assert_eq!(crate::obligation(&state, obligation).unwrap().amount(), 1);
        assert_eq!(crate::obligation(&state, obligation).unwrap().performed, 0);
        let ended = Ledger::new(&state)
            .prepare_production(ProductionFact {
                actor,
                site: SiteId(5),
                good: Good::Food,
                quantity: Quantity(30),
                time: SimTime(90_000),
            })
            .unwrap();
        assert!(ended.credits.is_empty());
        ended.commit(&mut state).unwrap();
        assert_eq!(
            Ledger::new(&state).defaults_due(SimTime(120_000)),
            vec![obligation]
        );
        mark_default(&mut state, obligation, SimTime(120_000)).unwrap();
        assert_eq!(
            crate::agreement(&state, id).unwrap().status,
            AgreementStatus::Breached
        );
        assert_eq!(crate::obligation(&state, obligation).unwrap().amount(), 1);
        assert_eq!(crate::obligation(&state, obligation).unwrap().performed, 0);
        let saved = serde_json::to_vec(&state.snapshot().unwrap()).unwrap();
        let restored = Store::restore(
            state.schema().clone(),
            serde_json::from_slice(&saved).unwrap(),
        )
        .unwrap();
        assert_eq!(state, restored);
    }
}

#[cfg(test)]
mod transition_tests;

mod resources;
pub use resources::*;

mod judgments;
pub use judgments::*;

mod knowledge;
pub use knowledge::*;

mod receipt_validation;
mod receipts;
pub use receipt_validation::*;
pub use receipts::*;

mod communications;
pub use communications::*;
mod availability;
pub use availability::*;

#[cfg(test)]
fn fixture() -> Store {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    game_property::register_components(&mut schema).unwrap();
    Store::with_schema(schema)
}

mod choices;
pub use choices::*;

mod validation;
pub use validation::*;

#[cfg(test)]
mod native_flow_tests;

mod queries;
pub use queries::*;

mod interval_labor;
pub use interval_labor::*;
mod maintenance_inputs;
pub use maintenance_inputs::*;
