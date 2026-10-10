//! Prospective financing terms. These are choices, never published promises or
//! physical supplies. Every deadline and amount is prepared before publication.
use crate::{Project, ProjectKind, ProjectStatus, Rules, World};
use game_agreements::AgreementTerm;
use game_core::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug)]
pub struct FundingFacts {
    pub now: SimTime,
    pub financier: PersonId,
    pub source: InventoryId,
    pub repayment_destination: InventoryId,
}

#[derive(Clone, Copy, Debug)]
pub struct FundingCalendar {
    pub day: SimDuration,
    pub year_days: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FundingUnavailable {
    UnrepresentableTime,
    UnrepresentableQuantity,
    IneligibleProject,
    InvalidParticipants,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FundingProposal {
    pub parties: BTreeSet<PersonId>,
    pub terms: Vec<AgreementTerm>,
    pub next_search: SimTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FundingChoice {
    Available(FundingProposal),
    Unavailable(FundingUnavailable),
}

impl World {
    /// Structural closing bound for automatic funding choices. Contact,
    /// willingness, dated knowledge and agreement outcomes may change before
    /// review, so opening versions of those facts cannot exclude a candidate.
    /// No closing phase creates a project before the funding review.
    pub fn funding_proposal_bound(&self, next: SimTime) -> Result<u64, Error> {
        self.projects
            .values()
            .filter(|plan| {
                !matches!(
                    plan.status,
                    ProjectStatus::Failed | ProjectStatus::Cancelled
                ) && match plan.kind {
                    ProjectKind::Workshop => true,
                    ProjectKind::Farm { .. } => !plan.materials_consumed && plan.finish_by > next,
                }
            })
            .try_fold(0u64, |count, _| count.checked_add(1).ok_or(Error::Overflow))
    }
}

impl Project {
    /// Three recipe batches fund the authored first workshop repayment. A built
    /// enterprise needs no construction materials in its subsequent financing.
    pub fn workshop_funding_materials(
        &self,
        rules: &Rules,
    ) -> Result<Quantity, FundingUnavailable> {
        if self.kind != ProjectKind::Workshop {
            return Err(FundingUnavailable::IneligibleProject);
        }
        rules
            .recipe_timber
            .0
            .checked_mul(3)
            .and_then(|recipes| {
                recipes.checked_add(if self.enterprise.is_some() {
                    0
                } else {
                    rules.construction_timber.0
                })
            })
            .map(Quantity)
            .ok_or(FundingUnavailable::UnrepresentableQuantity)
    }

    fn funding_parties(
        &self,
        facts: FundingFacts,
    ) -> Result<BTreeSet<PersonId>, FundingUnavailable> {
        if matches!(
            self.status,
            ProjectStatus::Failed | ProjectStatus::Cancelled
        ) || self.sponsor.0 == 0
            || facts.financier.0 == 0
            || self.sponsor == facts.financier
            || facts.source.0 == 0
            || facts.repayment_destination.0 == 0
            || facts.source == self.materials
            || facts.repayment_destination == self.output
        {
            return Err(FundingUnavailable::InvalidParticipants);
        }
        Ok(BTreeSet::from([self.sponsor, facts.financier]))
    }

    pub fn prepare_farm_funding(
        &self,
        rules: &Rules,
        facts: FundingFacts,
        calendar: FundingCalendar,
    ) -> FundingChoice {
        let prepare = || -> Result<FundingProposal, FundingUnavailable> {
            let ProjectKind::Farm { plot, .. } = self.kind else {
                return Err(FundingUnavailable::IneligibleProject);
            };
            if self.materials_consumed
                || self.finish_by <= facts.now
                || calendar.day.0 == 0
                || calendar.year_days == 0
            {
                return Err(FundingUnavailable::IneligibleProject);
            }
            let parties = self.funding_parties(facts)?;
            let year = calendar
                .year_days
                .checked_mul(calendar.day.0)
                .ok_or(FundingUnavailable::UnrepresentableTime)?;
            let ends_at = facts
                .now
                .0
                .checked_add(year)
                .map(SimTime)
                .ok_or(FundingUnavailable::UnrepresentableTime)?;
            let pay_by = calendar
                .day
                .0
                .checked_mul(7)
                .and_then(|grace| ends_at.0.checked_add(grace))
                .map(SimTime)
                .ok_or(FundingUnavailable::UnrepresentableTime)?;
            let next_search = facts
                .now
                .0
                .checked_add(120_000)
                .map(SimTime)
                .ok_or(FundingUnavailable::UnrepresentableTime)?;
            Ok(FundingProposal {
                parties,
                terms: vec![
                    AgreementTerm::Delivery {
                        debtor: facts.financier,
                        creditor: self.sponsor,
                        source: facts.source,
                        destination: self.materials,
                        quantity: rules.construction_timber,
                        deadline: self.finish_by,
                    },
                    AgreementTerm::OutputShare {
                        debtor: self.sponsor,
                        creditor: facts.financier,
                        site: plot,
                        good: Good::Food,
                        numerator: 1,
                        denominator: 10,
                        destination: facts.repayment_destination,
                        pay_by,
                        ends_at,
                    },
                ],
                next_search,
            })
        };
        match prepare() {
            Ok(proposal) => FundingChoice::Available(proposal),
            Err(reason) => FundingChoice::Unavailable(reason),
        }
    }

    pub fn prepare_workshop_funding(
        &self,
        rules: &Rules,
        facts: FundingFacts,
        loading: SimDuration,
    ) -> FundingChoice {
        let prepare = || -> Result<FundingProposal, FundingUnavailable> {
            let parties = self.funding_parties(facts)?;
            let needed = self.workshop_funding_materials(rules)?;
            let repayment = rules
                .recipe_tools
                .0
                .checked_mul(3)
                .map(Quantity)
                .ok_or(FundingUnavailable::UnrepresentableQuantity)?;
            let funding_window = loading
                .0
                .checked_mul(4)
                .and_then(|v| v.checked_add(120_000))
                .ok_or(FundingUnavailable::UnrepresentableTime)?;
            let production_window = rules
                .recipe_time
                .0
                .checked_mul(3)
                .and_then(|v| rules.construction_labor.0.checked_add(v))
                .and_then(|v| {
                    loading
                        .0
                        .checked_mul(10)
                        .and_then(|loads| v.checked_add(loads))
                })
                .and_then(|v| v.checked_mul(2))
                .and_then(|v| v.checked_add(120_000))
                .ok_or(FundingUnavailable::UnrepresentableTime)?;
            let funding_deadline = facts
                .now
                .0
                .checked_add(funding_window)
                .map(SimTime)
                .ok_or(FundingUnavailable::UnrepresentableTime)?;
            let repayment_deadline = facts
                .now
                .0
                .checked_add(production_window)
                .map(SimTime)
                .ok_or(FundingUnavailable::UnrepresentableTime)?;
            let next_search = facts
                .now
                .0
                .checked_add(600_000)
                .map(SimTime)
                .ok_or(FundingUnavailable::UnrepresentableTime)?;
            Ok(FundingProposal {
                parties,
                terms: vec![
                    AgreementTerm::Delivery {
                        debtor: facts.financier,
                        creditor: self.sponsor,
                        source: facts.source,
                        destination: self.materials,
                        quantity: needed,
                        deadline: funding_deadline,
                    },
                    AgreementTerm::Delivery {
                        debtor: self.sponsor,
                        creditor: facts.financier,
                        source: self.output,
                        destination: facts.repayment_destination,
                        quantity: repayment,
                        deadline: repayment_deadline,
                    },
                ],
                next_search,
            })
        };
        match prepare() {
            Ok(proposal) => FundingChoice::Available(proposal),
            Err(reason) => FundingChoice::Unavailable(reason),
        }
    }
}

#[cfg(test)]
mod tests;
