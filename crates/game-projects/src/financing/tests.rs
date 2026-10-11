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
            ProjectKind::Workshop {
                recipe: game_workshop::RecipeId("toolmaking".into()),
            }
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
fn definitions() -> Definitions {
    Definitions::from_content(serde_json::from_value(serde_json::json!({
        "id":"test/recipes", "version":1,"data":{"default_recipe":"toolmaking","recipes":{"toolmaking":{
            "input_good":"Timber","input_quantity":2000,"output_good":"Tool","output_quantity":1000,"duration":60000
        }}}
    })).unwrap()).unwrap()
}
fn workshop_facts(now: u64) -> WorkshopFundingFacts {
    WorkshopFundingFacts {
        now: SimTime(now),
        financier: PersonId(9),
        materials_source: InventoryId(10),
        input_source: InventoryId(10),
        repayment_destination: InventoryId(11),
        materials_good: Good::Timber,
        input_good: Good::Timber,
        repayment_good: Good::Tool,
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
    let definitions = definitions();
    let recipe = definitions
        .recipe(&definitions.catalog.default_recipe)
        .unwrap();
    let mut plan = project(false);
    let terms = available(plan.prepare_workshop_funding(
        &rules,
        &definitions,
        workshop_facts(100),
        SimDuration(30_000),
    ));
    let funding = 4 * 30_000 + 120_000;
    let production =
        (rules.construction_labor.0 + 3 * recipe.duration.0 + 10 * 30_000) * 2 + 120_000;
    assert_eq!(terms.next_search, SimTime(600_100));
    assert_eq!(
        terms.terms,
        vec![
            AgreementTerm::Delivery {
                debtor: PersonId(9),
                creditor: PersonId(2),
                source: InventoryId(10),
                destination: InventoryId(4),
                quantity: Quantity(rules.construction_timber.0 + 3 * recipe.input_quantity.0),
                deadline: SimTime(100 + funding)
            },
            AgreementTerm::Delivery {
                debtor: PersonId(2),
                creditor: PersonId(9),
                source: InventoryId(6),
                destination: InventoryId(11),
                quantity: Quantity(3 * recipe.output_quantity.0),
                deadline: SimTime(100 + production)
            },
        ]
    );
    plan.enterprise = Some(EnterpriseId(12));
    assert_eq!(
        plan.workshop_funding_inputs(&definitions).unwrap(),
        Quantity(3 * recipe.input_quantity.0)
    );
}
#[test]
fn workshop_future_windows_and_quantities_cannot_wrap_or_saturate_into_promises() {
    let plan = project(false);
    let rules = Rules::default();
    for (now, loading) in [(u64::MAX - 1, 30_000), (0, u64::MAX)] {
        assert_eq!(
            plan.prepare_workshop_funding(
                &rules,
                &definitions(),
                workshop_facts(now),
                SimDuration(loading)
            ),
            FundingChoice::Unavailable(FundingUnavailable::UnrepresentableTime)
        );
    }
    for output in [false, true] {
        let mut definitions = definitions();
        let recipe = definitions
            .catalog
            .recipes
            .get_mut(&definitions.catalog.default_recipe)
            .unwrap();
        if output {
            recipe.output_quantity = Quantity(u64::MAX);
        } else {
            recipe.input_quantity = Quantity(u64::MAX);
        }
        assert_eq!(
            plan.prepare_workshop_funding(
                &rules,
                &definitions,
                workshop_facts(0),
                SimDuration(30_000)
            ),
            FundingChoice::Unavailable(FundingUnavailable::UnrepresentableQuantity)
        );
    }
}
#[test]
fn distinct_construction_and_production_goods_require_distinct_supply_terms() {
    let mut definitions = definitions();
    let recipe = definitions
        .catalog
        .recipes
        .get_mut(&definitions.catalog.default_recipe)
        .unwrap();
    recipe.input_good = Good::Food;
    let mut plan = project(false);
    plan.input = InventoryId(12);
    let mut facts = workshop_facts(100);
    facts.input_good = Good::Food;
    assert_eq!(
        plan.prepare_workshop_funding(&Rules::default(), &definitions, facts, SimDuration(30_000)),
        FundingChoice::Unavailable(FundingUnavailable::InvalidParticipants)
    );
    facts.input_source = InventoryId(13);
    let terms = available(plan.prepare_workshop_funding(
        &Rules::default(),
        &definitions,
        facts,
        SimDuration(30_000),
    ));
    assert_eq!(terms.terms.len(), 3);
    assert!(terms.terms.iter().any(|term|matches!(term,AgreementTerm::Delivery{source,destination,quantity,..} if *source==InventoryId(10) && *destination==plan.materials && *quantity==Quantity(5000))));
    assert!(terms.terms.iter().any(|term|matches!(term,AgreementTerm::Delivery{source,destination,quantity,..} if *source==InventoryId(13) && *destination==plan.input && *quantity==Quantity(6000))));
}

fn remembered_stock(id: u64, good: Good, quantity: u64) -> StockKnowledge {
    StockKnowledge {
        inventory: InventoryId(id),
        area: AreaId(3),
        good,
        quantity: Quantity(quantity),
        accessible: false,
        observed_at: SimTime(1),
        learned_at: SimTime(1),
        source: KnowledgeSource::Observed(PersonId(2)),
    }
}
#[test]
fn remembered_financing_preserves_default_shared_lot_order_and_combines_once() {
    let plan = project(false);
    let rows = [
        remembered_stock(12, Good::Timber, 20_000),
        remembered_stock(10, Good::Timber, 20_000),
    ];
    let supplies = rows
        .iter()
        .map(|known| RememberedFinancingStock {
            known,
            custodian: Some(PersonId(9)),
            custodian_area: Some(AreaId(3)),
        })
        .collect::<Vec<_>>();
    let repayments = [FinancingRepaymentLot {
        inventory: InventoryId(11),
        good: Good::Tool,
        area: Some(AreaId(3)),
        custodian: Some(PersonId(9)),
    }];
    let found = available(plan.prepare_remembered_workshop_funding(
        &Rules::default(),
        &definitions(),
        RememberedWorkshopFunding {
            seeks_independence: true,
            ambition: 800,
            now: SimTime(1),
            loading: MINUTE,
            balances: FundingBalances {
                materials: Quantity(0),
                input: Quantity(0),
            },
            supplies: &supplies,
            repayments: &repayments,
        },
    ));
    let expected = available(plan.prepare_workshop_funding(
        &Rules::default(),
        &definitions(),
        workshop_facts(1),
        MINUTE,
    ));
    assert_eq!(found, expected);
    assert_eq!(found.terms.len(), 2);
    assert!(matches!(
        found.terms[0],
        AgreementTerm::Delivery {
            source: InventoryId(10),
            destination: InventoryId(4),
            quantity: Quantity(11_000),
            ..
        }
    ));
}
#[test]
fn remembered_financing_uses_alternate_recipe_goods_with_one_financier() {
    let mut plan = project(false);
    plan.input = InventoryId(13);
    let mut defs = definitions();
    let selected = defs
        .declare_recipe(game_workshop::Recipe {
            input_good: Good::Food,
            input_quantity: Quantity(1_000),
            output_good: Good::Binding,
            output_quantity: Quantity(1_000),
            duration: MINUTE,
        })
        .unwrap();
    plan.kind = ProjectKind::Workshop { recipe: selected };
    let rows = [
        remembered_stock(10, Good::Timber, 5_000),
        remembered_stock(14, Good::Food, 3_000),
    ];
    let mut supplies = rows
        .iter()
        .map(|known| RememberedFinancingStock {
            known,
            custodian: Some(PersonId(9)),
            custodian_area: Some(AreaId(3)),
        })
        .collect::<Vec<_>>();
    let repayments = [FinancingRepaymentLot {
        inventory: InventoryId(15),
        good: Good::Binding,
        area: Some(AreaId(3)),
        custodian: Some(PersonId(9)),
    }];
    let prepare = |supplies: &[RememberedFinancingStock<'_>], balances| {
        plan.prepare_remembered_workshop_funding(
            &Rules::default(),
            &defs,
            RememberedWorkshopFunding {
                seeks_independence: true,
                ambition: 800,
                now: SimTime(1),
                loading: MINUTE,
                balances,
                supplies,
                repayments: &repayments,
            },
        )
    };
    let balances = FundingBalances {
        materials: Quantity(0),
        input: Quantity(0),
    };
    let found = available(prepare(&supplies, balances));
    assert_eq!(found.parties, BTreeSet::from([PersonId(2), PersonId(9)]));
    assert_eq!(found.terms.len(), 3);
    assert!(found.terms.iter().any(|term| matches!(
        term,
        AgreementTerm::Delivery {
            source: InventoryId(10),
            destination: InventoryId(4),
            quantity: Quantity(5_000),
            ..
        }
    )));
    assert!(found.terms.iter().any(|term| matches!(
        term,
        AgreementTerm::Delivery {
            source: InventoryId(14),
            destination: InventoryId(13),
            quantity: Quantity(3_000),
            ..
        }
    )));
    assert!(found.terms.iter().any(|term| matches!(
        term,
        AgreementTerm::Delivery {
            source: InventoryId(6),
            destination: InventoryId(15),
            quantity: Quantity(3_000),
            ..
        }
    )));
    supplies[0].custodian = Some(PersonId(8));
    assert!(matches!(
        prepare(&supplies, balances),
        FundingChoice::Unavailable(_)
    ));
    supplies[0].custodian = Some(PersonId(9));
    assert!(matches!(
        prepare(
            &supplies,
            FundingBalances {
                materials: Quantity(5_000),
                input: Quantity(3_000)
            }
        ),
        FundingChoice::Unavailable(_)
    ));
}
