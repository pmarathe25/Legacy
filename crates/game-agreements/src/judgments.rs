//! Personal agreement judgments use dated knowledge and supplied local facts.
use crate::*;

#[derive(Clone, Copy, Debug)]
pub struct OfferActorFacts {
    pub actor: PersonId,
    pub now: SimTime,
    pub hunger: u64,
    pub empathy: u16,
    pub risk_tolerance: u16,
    pub ambition: u16,
    pub hedonism: u16,
    /// Already evaluated by the project feature for these terms.
    pub financing_utility: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareProspect {
    /// Production owners supply the expected quantity of this promised share.
    PlannedFood {
        expected_share: u128,
    },
    Established,
    Unknown,
}
#[derive(Clone, Copy, Debug)]
pub struct SpeakerFacts {
    pub locally_present: bool,
    pub trust: i32,
    pub observed_invocation_penalty: i64,
}
/// Narrow factual queries. Neither proposals nor relationships disclose stock,
/// projects, work permissions or another person's current symptoms remotely.
pub trait OfferKnowledge {
    fn stock(&self, inventory: InventoryId) -> Option<&StockKnowledge>;
    fn work_site_access(&self, site: SiteId) -> bool;
    fn project_known(&self, project: ProjectId) -> bool;
    fn share_prospect(&self, site: SiteId, numerator: u64, denominator: u64) -> ShareProspect;
    fn speaker(&self, person: PersonId) -> Option<SpeakerFacts>;
}
fn add(score: &mut i64, change: i64) -> Result<(), Error> {
    *score = score.checked_add(change).ok_or(Error::Overflow)?;
    Ok(())
}
fn bounded_amount(amount: u64) -> i64 {
    i64::try_from(amount).unwrap_or(i64::MAX / 32).min(100_000)
}
pub fn offer_score(
    actor: OfferActorFacts,
    terms: &[AgreementTerm],
    knowledge: &impl OfferKnowledge,
) -> Result<i64, Error> {
    let mut score = actor.financing_utility;
    for term in terms {
        if term.amount() == 0 && !matches!(term, AgreementTerm::OutputShare { .. }) {
            continue;
        }
        match *term {
            AgreementTerm::OutputShare {
                creditor,
                debtor,
                site,
                numerator,
                denominator,
                ..
            } => {
                if denominator == 0 {
                    return Err(Error::InvalidConfiguration(
                        "zero output share denominator".into(),
                    ));
                }
                let value = match knowledge.share_prospect(site, numerator, denominator) {
                    ShareProspect::PlannedFood { expected_share } => {
                        let estimate = (expected_share / 100).min(20_000) as i64;
                        estimate * i64::from(actor.risk_tolerance) / 1_000
                    }
                    ShareProspect::Established => (numerator
                        .saturating_mul(u64::from(actor.ambition) + u64::from(actor.hedonism))
                        / denominator)
                        .min(2_000) as i64,
                    ShareProspect::Unknown => -2_000,
                };
                if creditor == actor.actor {
                    add(&mut score, value)?;
                }
                if debtor == actor.actor {
                    add(&mut score, -value / 2)?;
                }
            }
            AgreementTerm::SiteAccess {
                grantee, allowed, ..
            } if grantee == actor.actor => {
                add(&mut score, if allowed { 1_000 } else { -1_000 })?;
            }
            AgreementTerm::Delivery {
                debtor,
                creditor,
                source,
                quantity,
                deadline,
                ..
            } => {
                if debtor == actor.actor {
                    if knowledge
                        .stock(source)
                        .is_none_or(|stock| !stock.accessible || stock.quantity < quantity)
                        || deadline.0.saturating_sub(actor.now.0) < 120_000
                    {
                        add(&mut score, -10_000)?;
                    }
                    add(&mut score, -bounded_amount(quantity.0 / 2))?;
                }
                if creditor == actor.actor {
                    add(&mut score, bounded_amount(quantity.0))?;
                }
            }
            AgreementTerm::Labor {
                debtor,
                site,
                millis,
                deadline,
                ..
            } if debtor == actor.actor => {
                if !knowledge.work_site_access(site)
                    || deadline.0.saturating_sub(actor.now.0) < millis
                {
                    add(&mut score, -10_000)?;
                }
                add(&mut score, -bounded_amount(millis / 60))?;
            }
            AgreementTerm::ConstructionLabor {
                debtor,
                project,
                millis,
                deadline,
                ..
            } if debtor == actor.actor => {
                if !knowledge.project_known(project)
                    || deadline.0.saturating_sub(actor.now.0) < millis
                {
                    add(&mut score, -10_000)?;
                }
                add(&mut score, -bounded_amount(millis / 60))?;
            }
            AgreementTerm::Access {
                grantee,
                inventory,
                allowed,
                ..
            } if grantee == actor.actor => {
                if let Some(stock) = knowledge.stock(inventory) {
                    let utility = if stock.good == Good::Food && actor.hunger > 120_000 {
                        3_000
                    } else {
                        1_000
                    };
                    add(&mut score, if allowed { utility } else { -utility })?;
                } else {
                    add(&mut score, -2_000)?;
                }
            }
            _ => {}
        }
    }
    let speakers: BTreeSet<_> = terms
        .iter()
        .filter(|term| !matches!(term, AgreementTerm::Access { .. }))
        .map(AgreementTerm::parties)
        .filter(|[debtor, creditor]| *debtor != actor.actor && *creditor == actor.actor)
        .map(|[debtor, _]| debtor)
        .collect();
    for speaker in speakers {
        if let Some(facts) = knowledge
            .speaker(speaker)
            .filter(|facts| facts.locally_present)
        {
            let trust = i64::from(facts.trust).clamp(-1_000, 1_000);
            let penalty = facts
                .observed_invocation_penalty
                .checked_mul(1_000 - trust)
                .ok_or(Error::Overflow)?
                / 1_000;
            add(&mut score, -penalty)?;
        }
    }
    add(&mut score, i64::from(actor.empathy) - 500)?;
    Ok(score)
}
pub fn needs_proposal_response(actor: PersonId, offer: &AgreementView) -> bool {
    offer.status == AgreementStatus::Proposed
        && offer.parties.contains(&actor)
        && !offer.accepted.contains(&actor)
}
/// Only the parties named in the chosen procedure can assess pending changes.
pub fn pending_amendments(
    actor: PersonId,
    offer: &AgreementView,
) -> impl Iterator<Item = &AmendmentView> {
    let authorized = offer.parties.contains(&actor)
        && offer.procedure.as_ref().is_some_and(|procedure| {
            procedure.approvers.contains(&actor)
                || procedure
                    .term_approvers
                    .values()
                    .any(|ids| ids.contains(&actor))
        });
    offer.amendments.iter().filter(move |change| {
        authorized
            && change.status == AmendmentStatus::Proposed
            && change.base_revision == offer.revision
            && !change.votes.contains_key(&actor)
    })
}
pub fn amended_terms(
    offer: &AgreementView,
    change: &AmendmentView,
) -> Result<Vec<AgreementTerm>, Error> {
    let mut terms = offer.terms.clone();
    for (index, term) in &change.replacements {
        let destination = terms
            .get_mut(usize::from(*index))
            .ok_or_else(|| Error::InvalidConfiguration("amendment term index is absent".into()))?;
        *destination = term.clone();
    }
    Ok(terms)
}
pub fn proposal_answer(
    actor: PersonId,
    offer: &AgreementView,
    score: i64,
    contact_eligible: bool,
) -> Option<AgreementAction> {
    if !contact_eligible || !needs_proposal_response(actor, offer) {
        return None;
    }
    if score >= 0 {
        return Some(AgreementAction::Accept {
            agreement: offer.id,
        });
    }
    if score >= -2_000 && offer.proposer != actor {
        let terms = offer
            .terms
            .iter()
            .map(|term| match *term {
                AgreementTerm::Labor {
                    debtor,
                    creditor,
                    site,
                    millis,
                    deadline,
                } if debtor == actor => AgreementTerm::Labor {
                    debtor,
                    creditor,
                    site,
                    millis: (millis / 2).max(1),
                    deadline,
                },
                _ => term.clone(),
            })
            .collect::<Vec<_>>();
        if terms != offer.terms {
            return Some(AgreementAction::Counteroffer {
                agreement: offer.id,
                terms,
                procedure: offer.procedure.clone(),
            });
        }
    }
    Some(AgreementAction::Reject {
        agreement: offer.id,
    })
}
pub fn amendment_support(previous: i64, revised: i64, empathy: u16) -> Result<bool, Error> {
    let concession = i64::from(empathy.saturating_sub(700)) * 5;
    Ok(revised.checked_add(concession).ok_or(Error::Overflow)? >= previous)
}
#[derive(Clone, Copy, Debug)]
pub struct KnownSiteRight {
    pub site: SiteId,
    pub area: AreaId,
    pub accessible: bool,
}
pub struct LocalExchangeFacts<'a> {
    pub actor: PersonId,
    pub autonomous: bool,
    pub area: Option<AreaId>,
    pub hunger: u64,
    /// Existing personal stock memory, already in stable inventory order.
    pub stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
    /// Sorted by stable site ID, preserving personal work-site order.
    pub sites: &'a [KnownSiteRight],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExchangeProposal {
    pub actor: PersonId,
    pub action: AgreementAction,
    /// The caller publishes this only after the proposal succeeds.
    pub next_search: SimTime,
}
impl Ledger<'_> {
    pub fn local_exchange(
        &self,
        now: SimTime,
        facts: LocalExchangeFacts<'_>,
        presence: &impl Presence,
    ) -> Option<ExchangeProposal> {
        if !facts.autonomous
            || self
                .next_search()
                .get(&facts.actor)
                .is_some_and(|at| *at > now)
            || self.knowledge().get(&facts.actor).is_some_and(|known| {
                known.values().any(|offer| {
                    offer.parties.contains(&facts.actor)
                        && matches!(
                            offer.status,
                            AgreementStatus::Proposed | AgreementStatus::Active
                        )
                })
            })
        {
            return None;
        }
        let area = facts.area?;
        if facts.hunger < 180_000
            || facts.stocks.values().any(|stock| {
                stock.area == area
                    && stock.good == Good::Food
                    && stock.accessible
                    && stock.quantity.0 >= 1_000
            })
        {
            return None;
        }
        let food = facts.stocks.values().find(|stock| {
            stock.area == area
                && stock.good == Good::Food
                && stock.quantity.0 >= 1_000
                && !stock.accessible
                && game_property::inventory_custodian(self.store, stock.inventory).is_some()
        })?;
        let owner = game_property::inventory_custodian(self.store, food.inventory)?;
        if contact_area(presence, owner) != Some(area) {
            return None;
        }
        let site = facts
            .sites
            .iter()
            .filter(|site| site.area == area && site.accessible)
            .min_by_key(|site| site.site)?;
        let deadline = SimTime(now.0.checked_add(180_000)?);
        Some(ExchangeProposal {
            actor: facts.actor,
            action: AgreementAction::Propose {
                parties: BTreeSet::from([facts.actor, owner]),
                terms: vec![
                    AgreementTerm::Access {
                        grantor: owner,
                        grantee: facts.actor,
                        inventory: food.inventory,
                        allowed: true,
                    },
                    AgreementTerm::Labor {
                        debtor: facts.actor,
                        creditor: owner,
                        site: site.site,
                        millis: 60_000,
                        deadline,
                    },
                ],
                procedure: None,
            },
            next_search: SimTime(now.0.saturating_add(300_000)),
        })
    }
}
#[cfg(test)]
mod tests;
/// Only these personal estimates and speaker facts can affect the judgment.
pub fn share_estimate_requests(
    terms: &[AgreementTerm],
) -> impl Iterator<Item = (SiteId, u64, u64)> + '_ {
    terms.iter().filter_map(|term| match *term {
        AgreementTerm::OutputShare {
            site,
            numerator,
            denominator,
            ..
        } => Some((site, numerator, denominator)),
        _ => None,
    })
}
pub fn speaker_fact_requests(actor: PersonId, terms: &[AgreementTerm]) -> BTreeSet<PersonId> {
    terms
        .iter()
        .filter(|term| !matches!(term, AgreementTerm::Access { .. }))
        .map(AgreementTerm::parties)
        .filter(|[debtor, creditor]| *debtor != actor && *creditor == actor)
        .map(|[speaker, _]| speaker)
        .collect()
}

/// A remembered enterprise and a locally established field are independent
/// factual evidence that a promised production site is established.
pub fn share_prospect_from_sources(
    planned: Option<u128>,
    remembered_established: bool,
    locally_established: bool,
) -> ShareProspect {
    share_prospect(planned, remembered_established || locally_established)
}
