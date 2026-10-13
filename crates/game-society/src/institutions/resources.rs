//! Resource demand for social cases from closing owner receipts and bounded
//! native institutional maintenance. Reservation does not record any case.
use super::*;
use game_ecs::Store;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InstitutionResourceDemand {
    pub ids: u64,
    pub events: u64,
}
/// Facts for exactly the unreconciled receipt suffix. Labor-attributed output
/// already has a cooperation receipt, so raw production cannot add it twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseReceipt {
    Labor { actor: PersonId, site: SiteId },
    Ledger,
    Production { actor: PersonId, site: SiteId },
    Support,
}
/// Classify the unreconciled opening receipt suffix for its existing case budget.
/// This deliberately counts repeated support against opening exclusions: reservation
/// precedes ordered receiving effects, which may later clear that exclusion.
pub fn pending_case_receipt_demand(
    store: &Store,
    receipts: &[(
        crate::reactions::SocialReceipt,
        crate::reactions::SubjectFacts,
    )],
) -> Result<u64, Error> {
    let qualified = receipts
        .iter()
        .filter_map(|(receipt, subject)| pending_case_receipt(store, receipt, *subject))
        .collect::<Vec<_>>();
    case_receipt_demand(&qualified)
}
fn pending_case_receipt(
    store: &Store,
    receipt: &crate::reactions::SocialReceipt,
    subject: crate::reactions::SubjectFacts,
) -> Option<CaseReceipt> {
    use crate::reactions::{CooperationTarget, SocialReceipt};
    let excluded = |actor, source| {
        crate::storage::exclusions(store, actor).is_some_and(|row| row.stores.contains_key(&source))
    };
    match *receipt {
        SocialReceipt::Performed {
            actor,
            target: CooperationTarget::Labor { site, .. },
            ..
        } => Some(CaseReceipt::Labor { actor, site }),
        SocialReceipt::Performed {
            target: CooperationTarget::Delivery { asset },
            ..
        } if asset.area.is_some() => Some(CaseReceipt::Ledger),
        SocialReceipt::Defaulted { .. } if subject.area.is_some() => Some(CaseReceipt::Ledger),
        SocialReceipt::Produced {
            actor,
            site,
            custodian: Some(owner),
            ..
        } if owner != actor => Some(CaseReceipt::Production { actor, site }),
        SocialReceipt::Ate { actor, source } | SocialReceipt::SupportReceived { actor, source }
            if excluded(actor, source) =>
        {
            Some(CaseReceipt::Support)
        }
        SocialReceipt::Transferred {
            actor,
            source,
            good: Good::Food,
            destination_holder: Some(holder),
        } if holder == actor && excluded(actor, source) => Some(CaseReceipt::Support),
        _ => None,
    }
}

pub fn case_receipt_demand(receipts: &[CaseReceipt]) -> Result<u64, Error> {
    let attributed = receipts
        .iter()
        .filter_map(|receipt| match receipt {
            CaseReceipt::Labor { actor, site } => Some((*actor, *site)),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    receipts.iter().filter(|receipt|!matches!(receipt,CaseReceipt::Production{actor,site} if attributed.contains(&(*actor,*site))))
        .try_fold(0u64,|total,_|total.checked_add(1).ok_or(Error::Overflow))
}
impl Runtime {
    /// Active duties must fit in the simulation clock before closing work starts.
    pub fn preflight_rule_deadlines(&self, store: &Store, at: SimTime) -> Result<(), Error> {
        for (_, institution) in crate::storage::institutions(store)
            .filter(|(_, row)| row.status == InstitutionStatus::Active)
        {
            for rule in &institution.charter.rules {
                let due_after = match rule.effect {
                    SocialEffect::Labor { due_after, .. }
                    | SocialEffect::Delivery { due_after, .. } => Some(due_after),
                    _ => None,
                };
                if let Some(duration) = due_after {
                    at.0.checked_add(duration.0).ok_or(Error::Overflow)?;
                }
            }
        }
        Ok(())
    }

    pub fn closing_resource_demand(
        &self,
        store: &Store,
        population_after_births: u64,
        review: bool,
        ledger: game_agreements::ClosingReceiptDemand,
        existing_case_receipts: u64,
        direct_closing_receipts: u64,
    ) -> Result<InstitutionResourceDemand, Error> {
        let maintenance = if review {
            // Exclusion observations take at most eight known stocks per person.
            // Native construction can add one arrangement (institution+two rules)
            // and one amendment proposal per person; hearing each new exclusion
            // can add one decision case. These are existing review operators.
            let mut maintenance = population_after_births
                .checked_mul(20)
                .and_then(|n| n.checked_add(64))
                .ok_or(Error::Overflow)?;
            let unheard = crate::storage::cases(store)
                .map(|(_, row)| row)
                .filter(|case| {
                    case.kind == SocialCaseKind::Exclusion
                        && case.outcome == SocialOutcome::Unresolved
                        && !crate::storage::hearing(store, case.id).is_some()
                })
                .count() as u64;
            maintenance = maintenance.checked_add(unheard).ok_or(Error::Overflow)?;
            for institution in crate::storage::institutions(store).map(|(_, row)| row) {
                // A chosen subject set can combine 64 members, 64 named people,
                // and eight office holders. Rule adoption may run before duties;
                // sixteen is the validated charter rule count, not a new cap.
                let subjects = population_after_births.min(64 + 64 + 8);
                let rule_ids = subjects
                    .checked_mul(16)
                    .and_then(|n| n.checked_mul(2))
                    .ok_or(Error::Overflow)?;
                let funding = (institution.charter.contributions.len() as u64)
                    .checked_mul(2)
                    .ok_or(Error::Overflow)?;
                maintenance = maintenance
                    .checked_add(rule_ids)
                    .and_then(|n| n.checked_add(funding))
                    .ok_or(Error::Overflow)?;
            }
            // Pending adoption can allocate a concept. Actual votes are bounded by
            // the registry and no additional abstract institutions are fabricated.
            let proposals = crate::storage::proposals(store)
                .map(|(_, row)| row)
                .filter(|proposal| proposal.status == RuleProposalStatus::Proposed)
                .count() as u64;
            maintenance = maintenance.checked_add(proposals).ok_or(Error::Overflow)?;
            maintenance
        } else {
            0
        };
        let cases = ledger
            .defaults
            .checked_add(ledger.performed)
            .and_then(|n| n.checked_add(existing_case_receipts))
            .and_then(|n| n.checked_add(direct_closing_receipts))
            .ok_or(Error::Overflow)?;
        let ids = maintenance.checked_add(cases).ok_or(Error::Overflow)?;
        // Existing native reserve_social_ids requires four event slots beyond
        // its immediate operation. Keep that continuation margin once here.
        let events = ids.checked_add(4).ok_or(Error::Overflow)?;
        Ok(InstitutionResourceDemand { ids, events })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ledger_cases_pending_receipts_and_new_people_have_checked_additive_demand() {
        let world = Runtime::default();
        let mut schema = game_ecs::Schema::new();
        crate::storage::register_components(&mut schema).unwrap();
        let store = Store::with_schema(schema);
        let demand = world
            .closing_resource_demand(
                &store,
                16,
                true,
                game_agreements::ClosingReceiptDemand {
                    defaults: 1000,
                    performed: 7,
                    accrued: 9,
                },
                3,
                2,
            )
            .unwrap();
        assert_eq!(demand.ids, 16 * 20 + 64 + 1000 + 7 + 3 + 2);
        assert_eq!(demand.events, demand.ids + 4);
        let before = world
            .closing_resource_demand(&store, 15, true, Default::default(), 0, 0)
            .unwrap();
        let after = world
            .closing_resource_demand(&store, 16, true, Default::default(), 0, 0)
            .unwrap();
        assert_eq!(after.ids - before.ids, 20);
        assert_eq!(after.events - before.events, 20);
        assert_eq!(
            world
                .closing_resource_demand(&store, u64::MAX, false, Default::default(), 0, 0)
                .unwrap()
                .ids,
            0
        );
        assert_eq!(
            world.closing_resource_demand(
                &store,
                u64::MAX,
                true,
                game_agreements::ClosingReceiptDemand::default(),
                0,
                0
            ),
            Err(Error::Overflow)
        );
    }
    #[test]
    fn existing_labor_and_production_receipts_do_not_reserve_the_same_cooperation_twice() {
        let receipts = [
            CaseReceipt::Production {
                actor: PersonId(1),
                site: SiteId(2),
            },
            CaseReceipt::Labor {
                actor: PersonId(1),
                site: SiteId(2),
            },
            CaseReceipt::Ledger,
            CaseReceipt::Production {
                actor: PersonId(3),
                site: SiteId(2),
            },
        ];
        assert_eq!(case_receipt_demand(&receipts).unwrap(), 3);
    }
    #[test]
    fn only_active_duty_deadlines_are_preflighted_without_mutation() {
        use crate::policy_fixtures::{institution, labor, seed_institution, world};
        let (runtime, mut store) = world();
        seed_institution(
            &mut store,
            institution(10, vec![labor()], InstitutionStatus::Proposed),
        );
        let opening = store.snapshot().unwrap();
        runtime
            .preflight_rule_deadlines(&store, SimTime(u64::MAX))
            .unwrap();
        assert_eq!(store.snapshot().unwrap(), opening);
        crate::storage::institution_mut(&mut store, InstitutionId(10))
            .unwrap()
            .status = InstitutionStatus::Active;
        let active = store.snapshot().unwrap();
        runtime
            .preflight_rule_deadlines(&store, SimTime(u64::MAX - 180_000))
            .unwrap();
        assert_eq!(
            runtime.preflight_rule_deadlines(&store, SimTime(u64::MAX - 180_000 + 1)),
            Err(Error::Overflow)
        );
        assert_eq!(store.snapshot().unwrap(), active);
        crate::storage::institution_mut(&mut store, InstitutionId(10))
            .unwrap()
            .status = InstitutionStatus::Dissolved;
        runtime
            .preflight_rule_deadlines(&store, SimTime(u64::MAX))
            .unwrap();
    }
    #[test]
    fn raw_pending_receipts_preserve_existing_qualification_and_opening_budget() {
        use crate::reactions::{CaseAsset, CooperationTarget, SocialReceipt, SubjectFacts};
        let (_, mut store) = crate::policy_fixtures::world();
        crate::insert_exclusions(
            &mut store,
            PersonId(1),
            crate::Exclusions {
                stores: BTreeMap::from([(InventoryId(4), CaseId(31))]),
            },
        )
        .unwrap();
        let before = store.snapshot().unwrap();
        let asset = CaseAsset {
            area: Some(AreaId(3)),
            inventory: InventoryId(4),
            good: Good::Food,
        };
        let remote = CaseAsset {
            area: None,
            ..asset
        };
        let labor = SocialReceipt::Performed {
            actor: PersonId(1),
            counterpart: PersonId(2),
            agreement: AgreementId(30),
            target: CooperationTarget::Labor {
                site: SiteId(20),
                asset: remote,
            },
        };
        let production = SocialReceipt::Produced {
            actor: PersonId(1),
            site: SiteId(20),
            asset,
            custodian: Some(PersonId(2)),
        };
        let cases = [
            (labor, Some(AreaId(3)), 1),
            (
                SocialReceipt::Performed {
                    actor: PersonId(1),
                    counterpart: PersonId(2),
                    agreement: AgreementId(30),
                    target: CooperationTarget::Delivery { asset },
                },
                Some(AreaId(3)),
                1,
            ),
            (
                SocialReceipt::Performed {
                    actor: PersonId(1),
                    counterpart: PersonId(2),
                    agreement: AgreementId(30),
                    target: CooperationTarget::Delivery { asset: remote },
                },
                Some(AreaId(3)),
                0,
            ),
            (
                SocialReceipt::Defaulted {
                    actor: PersonId(1),
                    counterpart: PersonId(2),
                    agreement: AgreementId(30),
                    asset: remote,
                },
                Some(AreaId(3)),
                1,
            ),
            (
                SocialReceipt::Defaulted {
                    actor: PersonId(1),
                    counterpart: PersonId(2),
                    agreement: AgreementId(30),
                    asset,
                },
                None,
                0,
            ),
            (production, Some(AreaId(3)), 1),
            (
                SocialReceipt::Produced {
                    actor: PersonId(1),
                    site: SiteId(20),
                    asset,
                    custodian: Some(PersonId(1)),
                },
                Some(AreaId(3)),
                0,
            ),
            (
                SocialReceipt::Produced {
                    actor: PersonId(1),
                    site: SiteId(20),
                    asset,
                    custodian: None,
                },
                Some(AreaId(3)),
                0,
            ),
            (
                SocialReceipt::Ate {
                    actor: PersonId(1),
                    source: InventoryId(4),
                },
                Some(AreaId(3)),
                1,
            ),
            (
                SocialReceipt::Ate {
                    actor: PersonId(1),
                    source: InventoryId(5),
                },
                Some(AreaId(3)),
                0,
            ),
            (
                SocialReceipt::Transferred {
                    actor: PersonId(1),
                    source: InventoryId(4),
                    good: Good::Food,
                    destination_holder: Some(PersonId(1)),
                },
                Some(AreaId(3)),
                1,
            ),
            (
                SocialReceipt::Transferred {
                    actor: PersonId(1),
                    source: InventoryId(4),
                    good: Good::Food,
                    destination_holder: Some(PersonId(2)),
                },
                Some(AreaId(3)),
                0,
            ),
            (
                SocialReceipt::Transferred {
                    actor: PersonId(1),
                    source: InventoryId(4),
                    good: Good::Timber,
                    destination_holder: Some(PersonId(1)),
                },
                Some(AreaId(3)),
                0,
            ),
            (
                SocialReceipt::Transferred {
                    actor: PersonId(1),
                    source: InventoryId(4),
                    good: Good::Food,
                    destination_holder: None,
                },
                Some(AreaId(3)),
                0,
            ),
        ];
        for (receipt, area, count) in cases {
            let subject = SubjectFacts {
                actor: PersonId(1),
                area,
                hunger: 300_000,
            };
            assert_eq!(
                pending_case_receipt_demand(&store, &[(receipt, subject)]).unwrap(),
                count
            );
        }
        let subject = SubjectFacts {
            actor: PersonId(1),
            area: Some(AreaId(3)),
            hunger: 300_000,
        };
        assert_eq!(
            pending_case_receipt_demand(&store, &[(production, subject), (labor, subject)])
                .unwrap(),
            1
        );
        let support = SocialReceipt::Ate {
            actor: PersonId(1),
            source: InventoryId(4),
        };
        assert_eq!(
            pending_case_receipt_demand(&store, &[(support, subject); 3]).unwrap(),
            3
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
}
