//! Record identity, referenced assets, personal chronology and conserved cargo.
use crate::*;
#[derive(Clone, Copy)]
pub struct ValidationContext<'a> {
    pub people: &'a BTreeSet<PersonId>,
    pub now: SimTime,
    pub next_id: u64,
    pub next_event: u64,
}
/// Asset existence and control are supplied by their actual feature owners.
pub trait TermAssets: OutputSources {
    fn site_exists(&self, site: SiteId) -> bool;
    fn site_custodian(&self, site: SiteId) -> Option<PersonId>;
    fn project_exists(&self, project: ProjectId) -> bool;
}
pub fn output_share_valid(
    store: &Store,
    assets: &impl OutputSources,
    term: &AgreementTerm,
) -> bool {
    let AgreementTerm::OutputShare {
        site,
        good,
        numerator,
        denominator,
        destination,
        pay_by,
        ends_at,
        ..
    } = term
    else {
        return false;
    };
    *numerator > 0
        && *denominator > 0
        && numerator <= denominator
        && pay_by >= ends_at
        && assets
            .output_source(*site, *good)
            .is_some_and(|source| source != *destination)
        && game_cargo::inventory(store, *destination).is_some_and(|row| row.good == *good)
}
pub fn validate_runtime(
    store: &Store,
    runtime: &Runtime,
    context: ValidationContext<'_>,
    assets: &impl TermAssets,
) -> Result<(), Error> {
    let w = Ledger::new(store);
    let fail = || Error::InvalidSave("invalid agreement state".into());
    let people = context.people;
    if runtime.last_event >= context.next_event
        || runtime.last_observation_event >= context.next_event
        || w.next_search().keys().any(|id| !people.contains(&id))
    {
        return Err(fail());
    }
    let valid_term = |term: &AgreementTerm, parties: &BTreeSet<PersonId>| {
        let [a, b] = term.parties();
        if a == b || !parties.contains(&a) || !parties.contains(&b) {
            return false;
        }
        match term {
            AgreementTerm::Delivery {
                source,
                destination,
                quantity,
                ..
            } => game_cargo::inventory(store, *(source))
                .zip(game_cargo::inventory(store, *(destination)))
                .is_some_and(|(a, b)| {
                    a.id != b.id && a.good == b.good && quantity.0 <= b.capacity.0
                }),
            AgreementTerm::Labor { site, .. } => assets.site_exists(*site),
            AgreementTerm::SiteAccess { grantor, site, .. } => {
                assets.site_custodian(*site) == Some(*grantor)
            }
            AgreementTerm::OutputShare { .. } => output_share_valid(store, assets, term),
            AgreementTerm::ConstructionLabor { project, .. } => assets.project_exists(*project),
            AgreementTerm::Access {
                grantor, inventory, ..
            } => {
                game_property::inventory_custodian(store, *inventory) == Some(*grantor)
                    || game_cargo::inventory(store, *(inventory)).is_some_and(|i| {
                        i.location == game_cargo::InventoryLocation::Carried(*grantor)
                    })
            }
        }
    };
    let mut referenced = BTreeSet::new();
    let mut cargo_totals: BTreeMap<InventoryId, u64> = BTreeMap::new();
    for (id, a) in w.agreements().iter() {
        if id != a.id
            || id.0 >= context.next_id
            || !(2..=8).contains(&a.parties.len())
            || !a.parties.is_subset(people)
            || !a.parties.contains(&a.proposer)
            || !a.accepted.is_subset(&a.parties)
            || !a.accepted.contains(&a.proposer)
            || a.proposed_at > context.now
            || a.terms.is_empty()
            || a.terms.len() > 16
            || a.terms.iter().any(|t| !valid_term(t, &a.parties))
        {
            return Err(fail());
        }
        if let Some(p) = &a.procedure
            && (p.eligible_proposers.is_empty()
                || !p.eligible_proposers.is_subset(&a.parties)
                || !p.approvers.is_subset(&a.parties)
                || p.threshold == 0
                || p.threshold > p.approvers.len()
                || p.term_approvers.iter().any(|(index, ids)| {
                    *index as usize >= a.terms.len() || ids.is_empty() || !ids.is_subset(&a.parties)
                }))
        {
            return Err(fail());
        }
        let active = matches!(
            a.status,
            AgreementStatus::Active | AgreementStatus::Fulfilled | AgreementStatus::Breached
        );
        if a.terms.iter().enumerate().any(|(index, term)| {
            term.amount() == 0
                && !matches!(term, AgreementTerm::OutputShare { .. })
                && (!active
                    || !w.amendments().values().any(|amendment| {
                        amendment.agreement == a.id
                            && amendment.status == AmendmentStatus::Adopted
                            && amendment.replacements.get(&(index as u16)) == Some(term)
                    }))
        }) {
            return Err(fail());
        }
        if (active && (a.obligations.len() != a.terms.len() || a.accepted != a.parties))
            || (!active && !a.obligations.is_empty())
        {
            return Err(fail());
        }
        for (index, obligation) in a.obligations.iter().enumerate() {
            let o = w.obligations().get(obligation).ok_or_else(fail)?;
            if !referenced.insert(*obligation) || o.agreement != id || o.term != a.terms[index] {
                return Err(fail());
            }
        }
        if active {
            let fulfilled = a.obligations.iter().all(|id| {
                w.obligations()[id].performed == w.obligations()[id].amount()
                    && stream_ended(&w.obligations()[id].term, context.now)
            });
            let breached = a.obligations.iter().any(|id| w.obligations()[id].defaulted);
            if (a.status == AgreementStatus::Fulfilled) != fulfilled
                || (!fulfilled && (a.status == AgreementStatus::Breached) != breached)
            {
                return Err(fail());
            }
        }
    }
    if referenced.len() != w.obligations().len() {
        return Err(fail());
    }
    if w.pending_work().iter().any(|(id, quantity)| {
        w.obligations().get(&id).is_none_or(|o| {
            !matches!(
                o.term,
                AgreementTerm::Labor { .. } | AgreementTerm::ConstructionLabor { .. }
            ) || *quantity == 0
                || *quantity > o.performed
        })
    }) {
        return Err(fail());
    }
    for (id, o) in w.obligations().iter() {
        if id != o.id
            || id.0 >= context.next_id
            || o.performed > o.amount()
            || o.amount() == 0 && o.defaulted
            || match o.term {
                AgreementTerm::OutputShare { denominator, .. } => {
                    o.share_remainder
                        >= if o.share_scale == 0 {
                            denominator
                        } else {
                            o.share_scale
                        }
                }
                _ => o.accrued != 0 || o.share_remainder != 0 || o.share_scale != 0,
            }
            || (o.defaulted && !o.term.deadline().is_some_and(|d| d <= context.now))
        {
            return Err(fail());
        }
        let mut total = 0_u64;
        for (inventory, quantity) in &o.cargo {
            let (debtor, source) = match o.term {
                AgreementTerm::Delivery { debtor, source, .. } => (debtor, source),
                AgreementTerm::OutputShare {
                    debtor, site, good, ..
                } => (debtor, assets.output_source(site, good).ok_or_else(fail)?),
                _ => return Err(fail()),
            };
            let stock = game_cargo::inventory(store, *(inventory)).ok_or_else(fail)?;
            if *quantity == 0
                || stock.location != game_cargo::InventoryLocation::Carried(debtor)
                || stock.good
                    != game_cargo::inventory(store, source)
                        .expect("inventory exists")
                        .good
            {
                return Err(fail());
            }
            total = total.checked_add(*quantity).ok_or_else(fail)?;
            let sum = cargo_totals.entry(*inventory).or_default();
            *sum = sum.checked_add(*quantity).ok_or_else(fail)?;
        }
        if total > o.amount() - o.performed {
            return Err(fail());
        }
    }
    if cargo_totals.iter().any(|(id, quantity)| {
        *quantity
            > game_cargo::inventory(store, *(id))
                .expect("inventory exists")
                .quantity
                .0
    }) {
        return Err(fail());
    }
    for (id, m) in w.amendments().iter() {
        let a = w.agreements().get(&m.agreement).ok_or_else(fail)?;
        let p = a.procedure.as_ref().ok_or_else(fail)?;
        if id != m.id
            || id.0 >= context.next_id
            || !p.eligible_proposers.contains(&m.proposer)
            || m.base_revision > a.revision
            || (m.status == AmendmentStatus::Proposed && m.base_revision != a.revision)
            || m.replacements.is_empty()
            || m.replacements.len() > 16
            || m.replacements.iter().any(|(index, term)| {
                *index as usize >= a.terms.len() || !valid_term(term, &a.parties)
            })
            || m.votes.keys().any(|actor| {
                !p.approvers.contains(actor)
                    && !p.term_approvers.values().any(|ids| ids.contains(actor))
            })
        {
            return Err(fail());
        }
    }
    for (actor, known) in w.knowledge().iter() {
        if !people.contains(&actor) {
            return Err(fail());
        }
        for (id, view) in known {
            if *id != view.id || !valid_knowledge(store, context.now, view, assets, people) {
                return Err(fail());
            }
        }
    }
    for (actor, report) in reports(store) {
        if actor != report.sender
            || !people.contains(&report.sender)
            || !people.contains(&report.recipient)
            || report.started_at > context.now
            || report.completes <= context.now
            || !valid_knowledge(store, context.now, &report.snapshot, assets, people)
        {
            return Err(fail());
        }
    }
    for (actor, _) in knowledges(store) {
        if !people.contains(&actor) {
            return Err(fail());
        }
    }
    Ok(())
}

pub fn valid_knowledge(
    store: &Store,
    now: SimTime,
    view: &AgreementView,
    assets: &impl TermAssets,
    people: &BTreeSet<PersonId>,
) -> bool {
    let world = Ledger::new(store);
    let Some(actual) = world.agreements().get(&view.id) else {
        return false;
    };
    let exists = |actor| people.contains(&actor);
    let source_valid = |source: &Option<KnowledgeSource>| {
        source.as_ref().is_none_or(|source| match source {
            KnowledgeSource::Observed(actor) | KnowledgeSource::Reported(actor) => exists(*actor),
            KnowledgeSource::Recorded(_) => false,
        })
    };
    if view.parties != actual.parties
        || view.proposer != actual.proposer
        || !view.accepted.is_subset(&view.parties)
        || view.terms.is_empty()
        || view.terms.len() > 16
        || view.revision > actual.revision
        || view.observed_at > view.learned_at
        || view.learned_at > now
        || !source_valid(&view.source)
    {
        return false;
    }
    let valid_term = |term: &AgreementTerm| {
        let [a, b] = term.parties();
        if a == b || !view.parties.contains(&a) || !view.parties.contains(&b) {
            return false;
        }
        match term {
            AgreementTerm::Delivery {
                source,
                destination,
                ..
            } => {
                source != destination
                    && game_cargo::inventory(store, *(source)).is_some()
                    && game_cargo::inventory(store, *(destination)).is_some()
            }
            AgreementTerm::Labor { site, .. } => assets.site_exists(*site),
            AgreementTerm::SiteAccess { site, .. } => assets.site_exists(*site),
            AgreementTerm::OutputShare { .. } => output_share_valid(store, assets, term),
            AgreementTerm::ConstructionLabor { project, .. } => assets.project_exists(*project),
            AgreementTerm::Access { inventory, .. } => {
                game_cargo::inventory(store, *(inventory)).is_some()
            }
        }
    };
    if view.terms.iter().any(|term| !valid_term(term)) {
        return false;
    }
    let mut obligations = BTreeSet::new();
    for obligation in &view.obligations {
        if !obligations.insert(obligation.id)
            || !world
                .obligations()
                .get(&obligation.id)
                .is_some_and(|o| o.agreement == view.id)
            || !valid_term(&obligation.term)
            || obligation.performed > obligation.amount()
            || match obligation.term {
                AgreementTerm::OutputShare { denominator, .. } => {
                    obligation.share_remainder
                        >= if obligation.share_scale == 0 {
                            denominator
                        } else {
                            obligation.share_scale
                        }
                }
                _ => {
                    obligation.accrued != 0
                        || obligation.share_remainder != 0
                        || obligation.share_scale != 0
                }
            }
            || obligation.observed_at > obligation.learned_at
            || obligation.learned_at > now
            || !source_valid(&obligation.source)
            || obligation.defaulted
                && !obligation
                    .term
                    .deadline()
                    .is_some_and(|at| at <= obligation.observed_at)
            || obligation
                .cargo
                .iter()
                .any(|(id, qty)| !game_cargo::inventory(store, *(id)).is_some() || *qty == 0)
            || obligation
                .cargo
                .values()
                .try_fold(0_u64, |sum, qty| sum.checked_add(*qty))
                .is_none_or(|sum| sum > obligation.amount() - obligation.performed)
        {
            return false;
        }
    }
    let mut amendments = BTreeSet::new();
    for proposal in &view.amendments {
        if !amendments.insert(proposal.id)
            || !world
                .amendments()
                .get(&proposal.id)
                .is_some_and(|a| a.agreement == view.id)
            || proposal.base_revision > view.revision
            || proposal.replacements.is_empty()
            || proposal
                .replacements
                .iter()
                .any(|(index, term)| *index as usize >= view.terms.len() || !valid_term(term))
            || proposal
                .votes
                .keys()
                .any(|actor| !view.parties.contains(actor))
        {
            return false;
        }
    }
    true
}
