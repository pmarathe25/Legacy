use super::*;
use std::collections::BTreeMap;

fn project(farm: bool) -> Project {
    Project {
        kind: if farm {
            ProjectKind::Farm {
                plot: SiteId(7),
                water: InventoryId(8),
            }
        } else {
            ProjectKind::Workshop
        },
        id: ProjectId(1),
        sponsor: PersonId(2),
        area: AreaId(3),
        materials: InventoryId(4),
        tool: InventoryId(5),
        input: InventoryId(4),
        output: InventoryId(6),
        beneficiaries: BTreeSet::from([PersonId(2)]),
        workers: BTreeSet::from([PersonId(2)]),
        support: BTreeSet::new(),
        status: ProjectStatus::Blocked,
        blocker: None,
        progress_ms: 0,
        contributions: BTreeMap::new(),
        materials_consumed: false,
        finish_by: SimTime(u64::MAX),
        enterprise: None,
    }
}
fn facts(now: u64) -> FundingFacts {
    FundingFacts {
        now: SimTime(now),
        financier: PersonId(9),
        source: InventoryId(10),
        repayment_destination: InventoryId(11),
    }
}
fn available(choice: FundingChoice) -> FundingProposal {
    let FundingChoice::Available(proposal) = choice else {
        panic!("expected available finance")
    };
    proposal
}

#[test]
fn representable_farm_terms_preserve_delivery_harvest_share_and_retry() {
    let plan = project(true);
    let rules = Rules::default();
    let terms = available(plan.prepare_farm_funding(
        &rules,
        facts(100),
        FundingCalendar {
            day: SimDuration(1_000),
            year_days: 365,
        },
    ));
    assert_eq!(terms.parties, BTreeSet::from([PersonId(2), PersonId(9)]));
    assert_eq!(terms.next_search, SimTime(120_100));
    assert_eq!(
        terms.terms,
        vec![
            AgreementTerm::Delivery {
                debtor: PersonId(9),
                creditor: PersonId(2),
                source: InventoryId(10),
                destination: InventoryId(4),
                quantity: rules.construction_timber,
                deadline: SimTime(u64::MAX)
            },
            AgreementTerm::OutputShare {
                debtor: PersonId(2),
                creditor: PersonId(9),
                site: SiteId(7),
                good: Good::Food,
                numerator: 1,
                denominator: 10,
                destination: InventoryId(11),
                pay_by: SimTime(372_100),
                ends_at: SimTime(365_100)
            },
        ]
    );
}

#[test]
fn unrepresentable_farm_promise_and_retry_are_explicit_choices_without_mutation() {
    let plan = project(true);
    let before = plan.clone();
    let rules = Rules::default();
    for (now, day, year_days) in [
        (u64::MAX - 360_000, 1_000, 365),
        (u64::MAX - 10, 1, 4),
        (0, u64::MAX, 4),
    ] {
        assert_eq!(
            plan.prepare_farm_funding(
                &rules,
                facts(now),
                FundingCalendar {
                    day: SimDuration(day),
                    year_days
                }
            ),
            FundingChoice::Unavailable(FundingUnavailable::UnrepresentableTime)
        );
    }
    assert_eq!(plan, before);
}

#[test]
fn farm_end_may_fit_but_repayment_grace_must_fit_too() {
    let now = u64::MAX - 370_000;
    assert_eq!(
        project(true).prepare_farm_funding(
            &Rules::default(),
            facts(now),
            FundingCalendar {
                day: SimDuration(1_000),
                year_days: 365
            }
        ),
        FundingChoice::Unavailable(FundingUnavailable::UnrepresentableTime)
    );
}

#[test]
fn workshop_terms_preserve_three_batches_and_completed_enterprise_materials() {
    let rules = Rules::default();
    let mut plan = project(false);
    let terms = available(plan.prepare_workshop_funding(&rules, facts(100), SimDuration(30_000)));
    let funding = 4 * 30_000 + 120_000;
    let production =
        (rules.construction_labor.0 + 3 * rules.recipe_time.0 + 10 * 30_000) * 2 + 120_000;
    assert_eq!(terms.next_search, SimTime(600_100));
    assert_eq!(
        terms.terms,
        vec![
            AgreementTerm::Delivery {
                debtor: PersonId(9),
                creditor: PersonId(2),
                source: InventoryId(10),
                destination: InventoryId(4),
                quantity: Quantity(rules.construction_timber.0 + 3 * rules.recipe_timber.0),
                deadline: SimTime(100 + funding)
            },
            AgreementTerm::Delivery {
                debtor: PersonId(2),
                creditor: PersonId(9),
                source: InventoryId(6),
                destination: InventoryId(11),
                quantity: Quantity(3 * rules.recipe_tools.0),
                deadline: SimTime(100 + production)
            },
        ]
    );
    plan.enterprise = Some(EnterpriseId(12));
    assert_eq!(
        plan.workshop_funding_materials(&rules).unwrap(),
        Quantity(3 * rules.recipe_timber.0)
    );
}

#[test]
fn workshop_future_windows_and_quantities_cannot_wrap_or_saturate_into_promises() {
    let plan = project(false);
    assert_eq!(
        plan.prepare_workshop_funding(&Rules::default(), facts(u64::MAX - 1), SimDuration(30_000)),
        FundingChoice::Unavailable(FundingUnavailable::UnrepresentableTime)
    );
    assert_eq!(
        plan.prepare_workshop_funding(&Rules::default(), facts(0), SimDuration(u64::MAX)),
        FundingChoice::Unavailable(FundingUnavailable::UnrepresentableTime)
    );
    assert_eq!(
        plan.prepare_workshop_funding(
            &Rules {
                recipe_timber: Quantity(u64::MAX),
                ..Rules::default()
            },
            facts(0),
            SimDuration(30_000)
        ),
        FundingChoice::Unavailable(FundingUnavailable::UnrepresentableQuantity)
    );
    assert_eq!(
        plan.prepare_workshop_funding(
            &Rules {
                recipe_tools: Quantity(u64::MAX),
                ..Rules::default()
            },
            facts(0),
            SimDuration(30_000)
        ),
        FundingChoice::Unavailable(FundingUnavailable::UnrepresentableQuantity)
    );
}

#[test]
fn funding_bound_includes_completed_workshops_and_excludes_closed_or_consumed_plans() {
    let mut world = World {
        rules: Rules::default(),
        projects: BTreeMap::new(),
        enterprises: BTreeMap::new(),
        jobs: BTreeMap::new(),
        known_projects: BTreeMap::new(),
        known_enterprises: BTreeMap::new(),
        timber_hints: BTreeMap::new(),
        next_search: BTreeMap::new(),
        last_event: 0,
    };
    for (id, farm, status, consumed, finish) in [
        (1, false, ProjectStatus::Blocked, false, u64::MAX),
        (2, false, ProjectStatus::Completed, true, 0),
        (3, true, ProjectStatus::Blocked, false, u64::MAX),
        (4, true, ProjectStatus::Completed, true, 0),
        (5, false, ProjectStatus::Cancelled, false, u64::MAX),
        (6, false, ProjectStatus::Failed, false, u64::MAX),
        (7, true, ProjectStatus::Blocked, false, 60_000),
    ] {
        let mut plan = project(farm);
        plan.id = ProjectId(id);
        plan.status = status;
        plan.materials_consumed = consumed;
        plan.finish_by = SimTime(finish);
        world.projects.insert(plan.id, plan);
    }
    assert_eq!(world.funding_proposal_bound(SimTime(60_000)).unwrap(), 3);
    assert_eq!(world.allocation_bound(SimTime(60_000)).unwrap(), 0);
}
