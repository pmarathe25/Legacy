use crate::*;
use game_agreements::AgreementTerm;
use game_core::*;
use game_missions::MissionObjective;
use std::collections::BTreeMap;

fn add_usage(
    usage: &mut BTreeMap<(InstitutionId, InventoryId), u128>,
    institution: InstitutionId,
    source: InventoryId,
    quantity: Quantity,
) -> Result<(), Error> {
    let entry = usage.entry((institution, source)).or_default();
    *entry = entry
        .checked_add(u128::from(quantity.0))
        .ok_or(Error::Overflow)?;
    Ok(())
}
fn delivery_valid(
    term: &AgreementTerm,
    debtor: PersonId,
    creditor: PersonId,
    facts: &ProposalFacts,
    earliest: SimTime,
) -> bool {
    matches!(term, AgreementTerm::Delivery { debtor:a,creditor:b,source,destination,quantity,deadline }
        if *a == debtor && *b == creditor && a != b && source != destination && quantity.0 > 0
        && *deadline > earliest && facts.inventories.get(source).zip(facts.inventories.get(destination))
        .is_some_and(|(from,to)|from.good==to.good))
}

impl Proposal {
    /// Validate named, scoped facts only. Future production is allowed: no
    /// available balance is inferred or reserved by a diplomatic promise.
    pub fn validate(&self, facts: &ProposalFacts, now: SimTime) -> Result<(), Error> {
        if !(2..=8).contains(&self.participants.len())
            || !(1..=16).contains(&self.terms.len())
            || self.ratify_by <= now
            || !self
                .participants
                .values()
                .any(|p| p.representative == self.proposer)
        {
            return Err(Error::Unavailable);
        }
        for (id, p) in &self.participants {
            let Some(rep) = facts.representatives.get(id) else {
                return Err(Error::Unavailable);
            };
            if id.0 == 0
                || *id != p.institution
                || p.representative.0 == 0
                || !facts.known_people.contains(&p.representative)
                || !facts.known_areas.contains(&p.settlement)
                || p.settlement != rep.settlement
                || p.representative != rep.representative
                || !rep.may_negotiate
            {
                return Err(Error::Unavailable);
            }
        }
        let mut deliveries = BTreeMap::new();
        let mut supplies = BTreeMap::new();
        for term in &self.terms {
            let provider = term.provider();
            let beneficiary = term.beneficiary();
            let Some(a) = self.participants.get(&provider) else {
                return Err(Error::Unavailable);
            };
            let Some(b) = self.participants.get(&beneficiary) else {
                return Err(Error::Unavailable);
            };
            let rights = &facts.representatives[&provider];
            if provider == beneficiary {
                return Err(Error::Unavailable);
            }
            match term {
                Term::Delivery { term, .. } => {
                    if !delivery_valid(
                        term,
                        a.representative,
                        b.representative,
                        facts,
                        self.ratify_by,
                    ) {
                        return Err(Error::Unavailable);
                    }
                    let AgreementTerm::Delivery {
                        source, quantity, ..
                    } = term
                    else {
                        unreachable!()
                    };
                    add_usage(&mut deliveries, provider, *source, *quantity)?;
                }
                Term::Passage { areas, until, .. } => {
                    if areas.is_empty()
                        || areas.len() > 16
                        || !areas.is_subset(&facts.known_areas)
                        || !areas.is_subset(&rights.passage_areas)
                        || *until <= self.ratify_by
                    {
                        return Err(Error::Unavailable);
                    }
                }
                Term::Protection { patrol, .. } => {
                    let MissionObjective::Patrol { areas, until } = &patrol.objective else {
                        return Err(Error::Unavailable);
                    };
                    let people = patrol
                        .recruits
                        .iter()
                        .copied()
                        .chain([patrol.leader])
                        .collect::<std::collections::BTreeSet<_>>();
                    if areas.is_empty()
                        || areas.len() > 16
                        || !areas.is_subset(&facts.known_areas)
                        || !areas.is_subset(&rights.protection_areas)
                        || *until <= self.ratify_by
                        || patrol.deadline < *until
                        || patrol.home != a.settlement
                        || people.is_empty()
                        || people.len() > 32
                        || !people.is_subset(&facts.known_people)
                        || patrol.supplies.len() > 32
                    {
                        return Err(Error::Unavailable);
                    }
                    for supply in &patrol.supplies {
                        if !people.contains(&supply.carrier)
                            || supply.quantity.0 == 0
                            || !facts.inventories.contains_key(&supply.source)
                        {
                            return Err(Error::Unavailable);
                        }
                        add_usage(&mut supplies, provider, supply.source, supply.quantity)?;
                    }
                    for (worker, terms) in &patrol.compensation {
                        if !people.contains(worker) || terms.len() > 8 {
                            return Err(Error::Unavailable);
                        }
                        for payment in terms {
                            if !delivery_valid(
                                payment,
                                a.representative,
                                *worker,
                                facts,
                                self.ratify_by,
                            ) {
                                return Err(Error::Unavailable);
                            }
                            let AgreementTerm::Delivery {
                                source, quantity, ..
                            } = payment
                            else {
                                unreachable!()
                            };
                            add_usage(&mut deliveries, provider, *source, *quantity)?;
                        }
                    }
                }
            }
        }
        if deliveries.iter().any(|((institution, source), q)| {
            facts.representatives[institution]
                .delivery_caps
                .get(source)
                .is_none_or(|cap| *q > u128::from(cap.0))
        }) || supplies.iter().any(|((institution, source), q)| {
            facts.representatives[institution]
                .supply_caps
                .get(source)
                .is_none_or(|cap| *q > u128::from(cap.0))
        }) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}

pub(crate) fn authoritative_actor(
    proposal: &Proposal,
    institution: InstitutionId,
    actor: PersonId,
    facts: &ProposalFacts,
) -> bool {
    proposal
        .participants
        .get(&institution)
        .zip(facts.representatives.get(&institution))
        .is_some_and(|(p, f)| {
            p.representative == actor
                && f.representative == actor
                && f.settlement == p.settlement
                && f.may_negotiate
        })
}
