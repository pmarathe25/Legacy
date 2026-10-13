//! Prospective financing terms. These are choices, never published promises or
//! physical supplies. Every deadline and amount is prepared before publication.
use crate::{Project, ProjectKind, ProjectStatus, Rules};
use game_agreements::AgreementTerm;
use game_core::*;
use game_workshop::Definitions;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug)]
pub struct FundingFacts {
    pub now: SimTime,
    pub financier: PersonId,
    pub source: InventoryId,
    pub repayment_destination: InventoryId,
}

#[derive(Clone, Copy, Debug)]
pub struct WorkshopFundingFacts {
    pub now: SimTime,
    pub financier: PersonId,
    pub materials_source: InventoryId,
    pub input_source: InventoryId,
    pub repayment_destination: InventoryId,
    pub materials_good: Good,
    pub input_good: Good,
    pub repayment_good: Good,
}

/// Personal supply memory plus current custody/locality facts.
#[derive(Clone, Copy)]
pub struct RememberedFinancingStock<'a> {
    pub known: &'a StockKnowledge,
    pub custodian: Option<PersonId>,
    pub custodian_area: Option<AreaId>,
}
#[derive(Clone, Copy)]
pub struct FinancingRepaymentLot {
    pub inventory: InventoryId,
    pub good: Good,
    pub area: Option<AreaId>,
    pub custodian: Option<PersonId>,
}
#[derive(Clone, Copy)]
pub struct FundingBalances {
    pub materials: Quantity,
    pub input: Quantity,
}

pub struct RememberedWorkshopFunding<'a> {
    pub seeks_independence: bool,
    pub ambition: u16,
    pub now: SimTime,
    pub loading: SimDuration,
    pub balances: FundingBalances,
    pub supplies: &'a [RememberedFinancingStock<'a>],
    pub repayments: &'a [FinancingRepaymentLot],
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

impl Project {
    pub fn workshop_funding_inputs(
        &self,
        definitions: &Definitions,
    ) -> Result<Quantity, FundingUnavailable> {
        let ProjectKind::Workshop { recipe } = &self.kind else {
            return Err(FundingUnavailable::IneligibleProject);
        };
        let recipe = definitions
            .recipe(recipe)
            .map_err(|_| FundingUnavailable::IneligibleProject)?;
        let output = Quantity(
            recipe
                .output_quantity
                .0
                .checked_mul(3)
                .ok_or(FundingUnavailable::UnrepresentableQuantity)?,
        );
        recipe
            .production_requirement(output)
            .map(|required| required.input)
            .map_err(|_| FundingUnavailable::UnrepresentableQuantity)
    }

    pub fn prepare_remembered_workshop_funding(
        &self,
        rules: &Rules,
        definitions: &Definitions,
        facts: RememberedWorkshopFunding<'_>,
    ) -> FundingChoice {
        let RememberedWorkshopFunding {
            seeks_independence,
            ambition,
            now,
            loading,
            balances,
            supplies,
            repayments,
        } = facts;
        let unavailable = || FundingChoice::Unavailable(FundingUnavailable::IneligibleProject);
        if !seeks_independence || ambition < 600 {
            return unavailable();
        }
        let ProjectKind::Workshop { recipe: id } = &self.kind else {
            return unavailable();
        };
        let Ok(recipe) = definitions.recipe(id) else {
            return unavailable();
        };
        let Ok(input_needed) = self.workshop_funding_inputs(definitions) else {
            return unavailable();
        };
        let construction = if self.enterprise.is_some() {
            Quantity(0)
        } else {
            rules.construction_timber
        };
        if self.materials == self.input {
            let Some(total) = construction.0.checked_add(input_needed.0) else {
                return FundingChoice::Unavailable(FundingUnavailable::UnrepresentableQuantity);
            };
            if balances.materials.0 >= total {
                return unavailable();
            }
        } else if balances.materials >= construction && balances.input >= input_needed {
            return unavailable();
        }
        let mut candidates = supplies.iter().collect::<Vec<_>>();
        candidates.sort_by_key(|row| row.known.inventory);
        for input in &candidates {
            let Some(financier) = input.custodian.filter(|owner| *owner != self.sponsor) else {
                continue;
            };
            let shared_materials = recipe.input_good == Good::Timber;
            if input.known.good != recipe.input_good
                || input.known.area != self.area
                || input.known.inventory == self.input
                || construction.0 > 0 && shared_materials && input.known.inventory == self.materials
                || input.custodian_area != Some(self.area)
            {
                continue;
            }
            let Some(needed_from_input) =
                input_needed
                    .0
                    .checked_add(if shared_materials { construction.0 } else { 0 })
            else {
                return FundingChoice::Unavailable(FundingUnavailable::UnrepresentableQuantity);
            };
            if input.known.quantity.0 < needed_from_input {
                continue;
            }
            let materials = if construction.0 == 0 || shared_materials {
                *input
            } else {
                let Some(materials) = candidates.iter().find(|row| {
                    row.known.good == Good::Timber
                        && row.known.area == self.area
                        && row.known.inventory != self.materials
                        && row.known.quantity >= construction
                        && row.custodian == Some(financier)
                        && row.custodian_area == Some(self.area)
                }) else {
                    continue;
                };
                *materials
            };
            let destination = repayments
                .iter()
                .filter(|lot| {
                    lot.good == recipe.output_good
                        && lot.area == Some(self.area)
                        && lot.inventory != self.tool
                        && lot.inventory != self.output
                        && lot.custodian == Some(financier)
                })
                .min_by_key(|lot| lot.inventory);
            let Some(destination) = destination else {
                continue;
            };
            let choice = self.prepare_workshop_funding(
                rules,
                definitions,
                WorkshopFundingFacts {
                    now,
                    financier,
                    materials_source: materials.known.inventory,
                    input_source: input.known.inventory,
                    repayment_destination: destination.inventory,
                    materials_good: materials.known.good,
                    input_good: input.known.good,
                    repayment_good: destination.good,
                },
                loading,
            );
            // Stable first qualifying financier preserves the original proposal choice.
            return choice;
        }
        unavailable()
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
        definitions: &Definitions,
        facts: WorkshopFundingFacts,
        loading: SimDuration,
    ) -> FundingChoice {
        let prepare = || -> Result<FundingProposal, FundingUnavailable> {
            let ProjectKind::Workshop { recipe: id } = &self.kind else {
                return Err(FundingUnavailable::IneligibleProject);
            };
            let recipe = definitions
                .recipe(id)
                .map_err(|_| FundingUnavailable::IneligibleProject)?;
            if facts.input_good != recipe.input_good
                || facts.repayment_good != recipe.output_good
                || (self.enterprise.is_none() && facts.materials_good != Good::Timber)
                || facts.input_source == self.input
                || (facts.materials_source == facts.input_source
                    && facts.materials_good != facts.input_good)
            {
                return Err(FundingUnavailable::InvalidParticipants);
            }
            let parties = self.funding_parties(FundingFacts {
                now: facts.now,
                financier: facts.financier,
                source: facts.input_source,
                repayment_destination: facts.repayment_destination,
            })?;
            let repayment = Quantity(
                recipe
                    .output_quantity
                    .0
                    .checked_mul(3)
                    .ok_or(FundingUnavailable::UnrepresentableQuantity)?,
            );
            let required = recipe
                .production_requirement(repayment)
                .map_err(|_| FundingUnavailable::UnrepresentableQuantity)?;
            let funding_window = loading
                .0
                .checked_mul(4)
                .and_then(|v| v.checked_add(120_000))
                .ok_or(FundingUnavailable::UnrepresentableTime)?;
            let production_window = required
                .work
                .0
                .checked_add(rules.construction_labor.0)
                .and_then(|v| {
                    loading
                        .0
                        .checked_mul(10)
                        .and_then(|loads| v.checked_add(loads))
                })
                .and_then(|v| v.checked_mul(2))
                .and_then(|v| v.checked_add(120_000))
                .ok_or(FundingUnavailable::UnrepresentableTime)?;
            let funding_deadline = SimTime(
                facts
                    .now
                    .0
                    .checked_add(funding_window)
                    .ok_or(FundingUnavailable::UnrepresentableTime)?,
            );
            let repayment_deadline = SimTime(
                facts
                    .now
                    .0
                    .checked_add(production_window)
                    .ok_or(FundingUnavailable::UnrepresentableTime)?,
            );
            let next_search = SimTime(
                facts
                    .now
                    .0
                    .checked_add(600_000)
                    .ok_or(FundingUnavailable::UnrepresentableTime)?,
            );
            let construction = if self.enterprise.is_some() {
                Quantity(0)
            } else {
                rules.construction_timber
            };
            let mut supplies = std::collections::BTreeMap::<(InventoryId, InventoryId), u64>::new();
            if construction.0 > 0 {
                if facts.materials_source.0 == 0 || facts.materials_source == self.materials {
                    return Err(FundingUnavailable::InvalidParticipants);
                }
                supplies.insert((facts.materials_source, self.materials), construction.0);
            }
            let total = supplies
                .entry((facts.input_source, self.input))
                .or_default();
            *total = total
                .checked_add(required.input.0)
                .ok_or(FundingUnavailable::UnrepresentableQuantity)?;
            let mut terms = supplies
                .into_iter()
                .map(|((source, destination), amount)| AgreementTerm::Delivery {
                    debtor: facts.financier,
                    creditor: self.sponsor,
                    source,
                    destination,
                    quantity: Quantity(amount),
                    deadline: funding_deadline,
                })
                .collect::<Vec<_>>();
            terms.push(AgreementTerm::Delivery {
                debtor: self.sponsor,
                creditor: facts.financier,
                source: self.output,
                destination: facts.repayment_destination,
                quantity: repayment,
                deadline: repayment_deadline,
            });
            Ok(FundingProposal {
                parties,
                terms,
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

/// Facts for autonomous financing. Selecting a counterparty and retry policy
/// belong to Projects; adapters supply personal memory and actual custody.
pub struct FundingSearchFacts<'a> {
    pub now: SimTime,
    pub ruler: PersonId,
    pub capable: bool,
    pub knows_project: bool,
    pub farm_independence: bool,
    pub workshop_independence: bool,
    pub ambition: u16,
    pub loading: SimDuration,
    pub calendar: Option<FundingCalendar>,
    pub materials_usable: bool,
    pub balances: FundingBalances,
    pub supplies: &'a [RememberedFinancingStock<'a>],
    pub repayments: &'a [FinancingRepaymentLot],
}
impl crate::Runtime {
    pub fn funding_candidates(&self, store: &game_ecs::Store, farm: bool) -> Vec<ProjectId> {
        crate::projects(store)
            .filter(|(_, plan)| {
                if farm {
                    matches!(plan.kind, ProjectKind::Farm { .. })
                        && !plan.materials_consumed
                        && !matches!(
                            plan.status,
                            ProjectStatus::Failed | ProjectStatus::Cancelled
                        )
                } else {
                    matches!(plan.kind, ProjectKind::Workshop { .. })
                        && plan.support.iter().all(|id| {
                            game_agreements::agreement(store, *id).is_some_and(|agreement| {
                                matches!(
                                    agreement.status,
                                    game_agreements::AgreementStatus::Fulfilled
                                        | game_agreements::AgreementStatus::Rejected
                                        | game_agreements::AgreementStatus::Withdrawn
                                )
                            })
                        })
                }
            })
            .map(|(id, _)| id)
            .collect()
    }
    pub fn search_funding(
        &self,
        store: &game_ecs::Store,
        definitions: &Definitions,
        id: ProjectId,
        facts: FundingSearchFacts<'_>,
    ) -> FundingChoice {
        let unavailable = || FundingChoice::Unavailable(FundingUnavailable::IneligibleProject);
        let Some(plan) = crate::project(store, id) else {
            return unavailable();
        };
        if plan.sponsor == facts.ruler
            || !facts.capable
            || matches!(
                plan.status,
                ProjectStatus::Failed | ProjectStatus::Cancelled
            )
            || crate::requests(store, plan.sponsor).is_some_and(|row| row.next_search > facts.now)
        {
            return unavailable();
        }
        if matches!(plan.kind, ProjectKind::Farm { .. }) {
            if !facts.knows_project
                || facts.materials_usable
                || !facts.farm_independence
                || facts.ambition < 600
                || plan.materials_consumed
                || plan.support.iter().any(|id| {
                    game_agreements::agreement(store, *id).is_some_and(|row| {
                        matches!(
                            row.status,
                            game_agreements::AgreementStatus::Proposed
                                | game_agreements::AgreementStatus::Active
                                | game_agreements::AgreementStatus::Breached
                        )
                    })
                })
            {
                return unavailable();
            }
            let offer = facts
                .supplies
                .iter()
                .filter(|stock| {
                    stock.known.good == Good::Timber
                        && stock.known.area == plan.area
                        && stock.known.quantity >= self.rules.construction_timber
                        && stock.known.inventory != plan.materials
                        && stock.custodian != Some(plan.sponsor)
                        && stock.custodian.is_some()
                        && stock.custodian_area == Some(plan.area)
                })
                .min_by_key(|stock| stock.known.inventory)
                .map(|stock| (stock.custodian.unwrap(), stock.known.inventory));
            let Some((financier, source)) = offer else {
                return unavailable();
            };
            let Some(destination) = facts
                .supplies
                .iter()
                .filter(|stock| {
                    stock.known.good == Good::Food
                        && stock.known.area == plan.area
                        && stock.known.inventory != plan.output
                        && stock.custodian == Some(financier)
                })
                .min_by_key(|stock| stock.known.inventory)
                .map(|stock| stock.known.inventory)
            else {
                return unavailable();
            };
            let Some(calendar) = facts.calendar else {
                return unavailable();
            };
            plan.prepare_farm_funding(
                &self.rules,
                FundingFacts {
                    now: facts.now,
                    financier,
                    source,
                    repayment_destination: destination,
                },
                calendar,
            )
        } else {
            if !plan.support.iter().all(|id| {
                game_agreements::agreement(store, *id).is_some_and(|row| {
                    matches!(
                        row.status,
                        game_agreements::AgreementStatus::Fulfilled
                            | game_agreements::AgreementStatus::Rejected
                            | game_agreements::AgreementStatus::Withdrawn
                    )
                })
            }) {
                return unavailable();
            }
            plan.prepare_remembered_workshop_funding(
                &self.rules,
                definitions,
                RememberedWorkshopFunding {
                    seeks_independence: facts.workshop_independence,
                    ambition: facts.ambition,
                    now: facts.now,
                    loading: facts.loading,
                    balances: facts.balances,
                    supplies: facts.supplies,
                    repayments: facts.repayments,
                },
            )
        }
    }
    pub fn record_funding_search(
        &self,
        store: &mut game_ecs::Store,
        actor: PersonId,
        funding: &FundingProposal,
    ) -> Result<(), Error> {
        crate::insert_requests(
            store,
            actor,
            crate::Requests {
                next_search: funding.next_search,
            },
        )
    }
}
