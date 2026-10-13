use super::*;
use game_agreements::{Agreement, AgreementStatus, AgreementTerm};

pub(crate) fn definitions() -> game_workshop::Definitions {
    game_workshop::Definitions::from_content(game_content::Document {
        id: "test/workshops".into(),
        version: 1,
        data: game_workshop::Catalog {
            default_recipe: game_workshop::RecipeId("tools".into()),
            recipes: BTreeMap::from([(
                game_workshop::RecipeId("tools".into()),
                game_workshop::Recipe {
                    input_good: Good::Timber,
                    input_quantity: Quantity(2_000),
                    output_good: Good::Tool,
                    output_quantity: Quantity(1_000),
                    duration: MINUTE,
                },
            )]),
        },
    })
    .unwrap()
}
fn schema() -> game_ecs::Schema {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
}
fn fixture() -> (Runtime, Store, game_workshop::Definitions) {
    (
        Runtime {
            rules: Rules {
                construction_labor: SimDuration(100),
                ..Rules::default()
            },
            last_event: 0,
        },
        Store::with_schema(schema()),
        definitions(),
    )
}
fn snapshot(runtime: &Runtime, store: &Store) -> (Runtime, game_ecs::Snapshot) {
    (runtime.clone(), store.snapshot().unwrap())
}
fn actor(now: u64) -> ActorFacts {
    ActorFacts {
        actor: PersonId(3),
        now: SimTime(now),
        area: Some(AreaId(2)),
        capable: true,
        knows_project: true,
    }
}
fn construction(now: u64, materials: u64) -> ConstructionFacts {
    ConstructionFacts {
        actor: actor(now),
        land_accessible: true,
        tool_available: true,
        materials_available: Quantity(materials),
    }
}
fn propose(
    world: &mut Runtime,
    store: &mut Store,
    definitions: &game_workshop::Definitions,
    finish: u64,
    farm: bool,
) {
    let people = BTreeSet::from([PersonId(3), PersonId(8)]);
    let mut stocks = BTreeMap::from([
        (
            InventoryId(4),
            StockFact {
                good: Good::Timber,
                area: Some(AreaId(2)),
            },
        ),
        (
            InventoryId(5),
            StockFact {
                good: Good::Tool,
                area: Some(AreaId(2)),
            },
        ),
        (
            InventoryId(6),
            StockFact {
                good: Good::Timber,
                area: Some(AreaId(2)),
            },
        ),
        (
            InventoryId(7),
            StockFact {
                good: Good::Tool,
                area: Some(AreaId(2)),
            },
        ),
    ]);
    let action = if farm {
        stocks.insert(
            InventoryId(6),
            StockFact {
                good: Good::Seed,
                area: Some(AreaId(2)),
            },
        );
        stocks.insert(
            InventoryId(7),
            StockFact {
                good: Good::Food,
                area: Some(AreaId(2)),
            },
        );
        stocks.insert(
            InventoryId(9),
            StockFact {
                good: Good::Water,
                area: Some(AreaId(2)),
            },
        );
        ProjectAction::ProposeFarm {
            plot: SiteId(10),
            materials: InventoryId(4),
            tool: InventoryId(5),
            seed: InventoryId(6),
            water: InventoryId(9),
            food: InventoryId(7),
            beneficiaries: BTreeSet::from([PersonId(3)]),
            workers: BTreeSet::from([PersonId(3), PersonId(8)]),
            finish_by: SimTime(finish),
        }
    } else {
        ProjectAction::Propose {
            recipe: game_workshop::RecipeId("tools".into()),
            area: AreaId(2),
            materials: InventoryId(4),
            tool: InventoryId(5),
            input: InventoryId(6),
            output: InventoryId(7),
            beneficiaries: BTreeSet::from([PersonId(3)]),
            workers: BTreeSet::from([PersonId(3), PersonId(8)]),
            finish_by: SimTime(finish),
        }
    };
    world
        .prepare_proposal(
            store,
            definitions,
            ProjectId(11),
            &action,
            ProposalFacts {
                actor: actor(0),
                people: &people,
                stocks: &stocks,
                land_area: Some(AreaId(2)),
                land_accessible: true,
            },
        )
        .unwrap()
        .commit(world, store, definitions)
        .unwrap();
}
fn backing(status: AgreementStatus) -> Agreement {
    Agreement {
        id: AgreementId(12),
        proposer: PersonId(8),
        parties: BTreeSet::from([PersonId(3), PersonId(8)]),
        accepted: BTreeSet::from([PersonId(3), PersonId(8)]),
        terms: vec![AgreementTerm::Delivery {
            debtor: PersonId(8),
            creditor: PersonId(3),
            source: InventoryId(13),
            destination: InventoryId(4),
            quantity: Quantity(5_000),
            deadline: SimTime(200),
        }],
        procedure: None,
        obligations: Vec::new(),
        status,
        proposed_at: SimTime(0),
        revision: 0,
    }
}
#[test]
fn support_recognizes_actual_accepted_terms_without_turning_receivables_into_material() {
    let (mut world, mut store, definitions) = fixture();
    propose(&mut world, &mut store, &definitions, 200, false);
    assert!(
        world
            .prepare_support(
                &store,
                ProjectId(11),
                &backing(AgreementStatus::Rejected),
                actor(0)
            )
            .is_err()
    );
    world
        .prepare_support(
            &store,
            ProjectId(11),
            &backing(AgreementStatus::Proposed),
            actor(0),
        )
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    assert!(
        crate::project(&store, ProjectId(11))
            .unwrap()
            .support
            .contains(&AgreementId(12))
    );
    assert!(
        world
            .prepare_construction(&store, ProjectId(11), construction(0, 0))
            .is_err()
    );
    assert!(
        !crate::project(&store, ProjectId(11))
            .unwrap()
            .materials_consumed
    );
    assert!(constructions(&store).next().is_none());
}
#[test]
fn construction_consumes_once_credits_real_elapsed_labor_and_resumes_at_remaining_deadline() {
    let (mut world, mut store, definitions) = fixture();
    propose(&mut world, &mut store, &definitions, 200, false);
    let effects = world
        .prepare_construction(&store, ProjectId(11), construction(0, 5_000))
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    assert!(effects.contains(&Effect::Consume {
        actor: PersonId(3),
        source: InventoryId(4),
        quantity: Quantity(5_000)
    }));
    world
        .prepare_advance(&store, SimTime(0), SimTime(40), &[])
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    world
        .prepare_interrupt(&store, PersonId(3))
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    assert_eq!(
        crate::project(&store, ProjectId(11)).unwrap().status,
        ProjectStatus::Ready
    );
    let effects = world
        .prepare_construction(&store, ProjectId(11), construction(60, 0))
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    assert!(!effects.iter().any(|e| matches!(e, Effect::Consume { .. })));
    assert_eq!(
        crate::construction(&store, PersonId(3)).unwrap().completes,
        SimTime(120)
    );
    world
        .prepare_advance(&store, SimTime(60), SimTime(120), &[])
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert_eq!(
        crate::project(&store, ProjectId(11)).unwrap().progress_ms,
        100
    );
    assert_eq!(
        crate::project(&store, ProjectId(11)).unwrap().contributions[&PersonId(3)],
        100
    );
}
#[test]
fn completion_describes_the_asset_and_enterprise_without_inventing_a_stock_or_owner() {
    for farm in [false, true] {
        let (mut world, mut store, definitions) = fixture();
        propose(&mut world, &mut store, &definitions, 200, farm);
        world
            .prepare_construction(&store, ProjectId(11), construction(0, 5_000))
            .unwrap()
            .commit(&mut world, &mut store, &definitions)
            .unwrap();
        world
            .prepare_advance(&store, SimTime(0), SimTime(100), &[])
            .unwrap()
            .commit(&mut world, &mut store)
            .unwrap();
        let effects = world
            .prepare_completion(
                &store,
                &definitions,
                PersonId(3),
                CompletionFacts {
                    now: SimTime(100),
                    capable: true,
                    asset_available: true,
                    site: SiteId(20),
                    enterprise: EnterpriseId(21),
                },
            )
            .unwrap()
            .commit(&mut world, &mut store, &definitions)
            .unwrap();
        assert_eq!(
            crate::project(&store, ProjectId(11)).unwrap().status,
            ProjectStatus::Completed
        );
        let enterprise = &crate::enterprise(&store, EnterpriseId(21)).unwrap();
        assert!(enterprise.inputs.is_empty());
        assert!(enterprise.produced.is_empty());
        match &effects[0] {
            Effect::Completed {
                asset: AssetEffect::Farm { plot, .. },
                site,
                ..
            } => {
                assert!(farm);
                assert_eq!(*plot, SiteId(10));
                assert_eq!(*site, SiteId(10));
            }
            Effect::Completed {
                asset: AssetEffect::Workshop(asset),
                ..
            } => {
                assert!(!farm);
                assert_eq!(
                    definitions.recipe(&asset.recipe).unwrap().input_quantity,
                    Quantity(2_000)
                );
                assert_eq!(
                    definitions.recipe(&asset.recipe).unwrap().output_quantity,
                    Quantity(1_000)
                );
            }
            _ => panic!("missing concrete asset effect"),
        }
    }
}
#[test]
fn cancellation_and_deadline_failure_stop_jobs_without_refunding_inputs_or_changing_agreements() {
    let (mut world, mut store, definitions) = fixture();
    propose(&mut world, &mut store, &definitions, 150, false);
    world
        .prepare_support(
            &store,
            ProjectId(11),
            &backing(AgreementStatus::Active),
            actor(0),
        )
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    world
        .prepare_construction(&store, ProjectId(11), construction(0, 5_000))
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    let mut cancelled = world.clone();
    let mut cancel_store = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
    assert!(
        cancelled
            .prepare_cancel(
                &cancel_store,
                ProjectId(11),
                ActorFacts {
                    actor: PersonId(8),
                    ..actor(1)
                }
            )
            .is_err()
    );
    let effects = cancelled
        .prepare_cancel(&cancel_store, ProjectId(11), actor(1))
        .unwrap()
        .commit(&mut cancelled, &mut cancel_store, &definitions)
        .unwrap();
    assert_eq!(
        crate::project(&cancel_store, ProjectId(11)).unwrap().status,
        ProjectStatus::Cancelled
    );
    assert!(
        crate::project(&cancel_store, ProjectId(11))
            .unwrap()
            .materials_consumed
    );
    assert!(
        crate::project(&cancel_store, ProjectId(11))
            .unwrap()
            .support
            .contains(&AgreementId(12))
    );
    assert!(effects.contains(&Effect::Interrupt { actor: PersonId(3) }));
    let effects = world
        .prepare_reconcile(&store, &definitions, SimTime(150), &[], &[], &[], 0)
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    assert_eq!(
        crate::project(&store, ProjectId(11)).unwrap().status,
        ProjectStatus::Failed
    );
    assert_eq!(
        crate::project(&store, ProjectId(11)).unwrap().blocker,
        Some(ProjectBlocker::Deadline)
    );
    assert!(constructions(&store).next().is_none());
    assert!(effects.contains(&Effect::Failed {
        project: ProjectId(11)
    }));
}
#[test]
fn missing_tool_blocks_readiness_and_insufficient_time_or_land_cannot_begin() {
    let (mut world, mut store, definitions) = fixture();
    propose(&mut world, &mut store, &definitions, 90, true);
    assert!(
        world
            .prepare_construction(&store, ProjectId(11), construction(0, 5_000))
            .is_err()
    );
    crate::project_mut(&mut store, ProjectId(11))
        .unwrap()
        .finish_by = SimTime(200);
    assert!(
        world
            .prepare_construction(
                &store,
                ProjectId(11),
                ConstructionFacts {
                    land_accessible: false,
                    ..construction(0, 5_000)
                }
            )
            .is_err()
    );
    world
        .prepare_reconcile(
            &store,
            &definitions,
            SimTime(0),
            &[ProjectCondition {
                project: ProjectId(11),
                sponsor_tool: false,
                materials_available: Quantity(5_000),
                usable_worker_tools: BTreeSet::new(),
            }],
            &[],
            &[],
            0,
        )
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    assert_eq!(
        crate::project(&store, ProjectId(11)).unwrap().blocker,
        Some(ProjectBlocker::Tool)
    );
}
#[test]
fn stale_prepared_change_and_batched_account_overflow_have_no_partial_effects() {
    let (mut world, mut store, definitions) = fixture();
    propose(&mut world, &mut store, &definitions, 200, false);
    let prepared = world
        .prepare_construction(&store, ProjectId(11), construction(0, 5_000))
        .unwrap();
    crate::project_mut(&mut store, ProjectId(11))
        .unwrap()
        .support
        .insert(AgreementId(99));
    let before = snapshot(&world, &store);
    assert!(
        prepared
            .commit(&mut world, &mut store, &definitions)
            .is_err()
    );
    assert_eq!(snapshot(&world, &store), before);
    world
        .prepare_construction(&store, ProjectId(11), construction(0, 5_000))
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    world
        .prepare_advance(&store, SimTime(0), SimTime(100), &[])
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    world
        .prepare_completion(
            &store,
            &definitions,
            PersonId(3),
            CompletionFacts {
                now: SimTime(100),
                capable: true,
                asset_available: true,
                site: SiteId(20),
                enterprise: EnterpriseId(21),
            },
        )
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    crate::enterprise_mut(&mut store, EnterpriseId(21))
        .unwrap()
        .produced
        .insert(Good::Tool, u64::MAX - 1);
    let before = snapshot(&world, &store);
    assert!(
        world
            .prepare_outputs(
                &store,
                &definitions,
                &[
                    (SiteId(20), Good::Tool, Quantity(1)),
                    (SiteId(20), Good::Tool, Quantity(1))
                ],
                Some(4)
            )
            .is_err()
    );
    assert_eq!(snapshot(&world, &store), before);
}
#[test]
fn current_restore_and_chunked_construction_produce_identical_owned_state() {
    let (mut initial, mut store, definitions) = fixture();
    propose(&mut initial, &mut store, &definitions, 200, false);
    initial
        .prepare_construction(&store, ProjectId(11), construction(0, 5_000))
        .unwrap()
        .commit(&mut initial, &mut store, &definitions)
        .unwrap();
    let mut one = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
    initial
        .prepare_advance(&one, SimTime(0), SimTime(100), &[])
        .unwrap()
        .commit(&mut initial, &mut one)
        .unwrap();
    let mut partial_runtime = Runtime {
        rules: initial.rules.clone(),
        last_event: 0,
    };
    partial_runtime
        .prepare_advance(&store, SimTime(0), SimTime(37), &[])
        .unwrap()
        .commit(&mut partial_runtime, &mut store)
        .unwrap();
    let encoded = serde_json::to_vec(&snapshot(&partial_runtime, &store)).unwrap();
    let (mut loaded, rows): (Runtime, game_ecs::Snapshot) =
        serde_json::from_slice(&encoded).unwrap();
    let mut restored = Store::restore(schema(), rows).unwrap();
    loaded
        .prepare_advance(&restored, SimTime(37), SimTime(100), &[])
        .unwrap()
        .commit(&mut loaded, &mut restored)
        .unwrap();
    assert_eq!(snapshot(&initial, &one), snapshot(&loaded, &restored));
}

#[test]
fn accepted_operating_terms_attach_without_creating_labor_or_payment_and_accounts_keep_defaults() {
    let (mut world, mut store, definitions) = fixture();
    propose(&mut world, &mut store, &definitions, 200, false);
    world
        .prepare_construction(&store, ProjectId(11), construction(0, 5_000))
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    world
        .prepare_advance(&store, SimTime(0), SimTime(100), &[])
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    world
        .prepare_completion(
            &store,
            &definitions,
            PersonId(3),
            CompletionFacts {
                now: SimTime(100),
                capable: true,
                asset_available: true,
                site: SiteId(20),
                enterprise: EnterpriseId(21),
            },
        )
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    let mut contract = backing(AgreementStatus::Active);
    contract.terms = vec![AgreementTerm::Labor {
        debtor: PersonId(8),
        creditor: PersonId(3),
        site: SiteId(20),
        millis: 40,
        deadline: SimTime(200),
    }];
    let worker = ActorFacts {
        actor: PersonId(8),
        ..actor(100)
    };
    assert!(
        world
            .prepare_operation(&store, EnterpriseId(21), &contract, worker, false)
            .is_err()
    );
    world
        .prepare_operation(&store, EnterpriseId(21), &contract, worker, true)
        .unwrap()
        .commit(&mut world, &mut store, &definitions)
        .unwrap();
    let enterprise = &crate::enterprise(&store, EnterpriseId(21)).unwrap();
    assert!(enterprise.labor.is_empty());
    assert!(enterprise.produced.is_empty());
    let accounts = account_view(
        enterprise,
        &[
            AccountObligation {
                term: contract.terms[0].clone(),
                performed: 17,
                accrued: 0,
                defaulted: true,
                delivery_good: None,
            },
            AccountObligation {
                term: AgreementTerm::Delivery {
                    debtor: PersonId(3),
                    creditor: PersonId(8),
                    source: InventoryId(22),
                    destination: InventoryId(23),
                    quantity: Quantity(2_000),
                    deadline: SimTime(200),
                },
                performed: 500,
                accrued: 0,
                defaulted: true,
                delivery_good: Some(Good::Food),
            },
        ],
    )
    .unwrap();
    assert_eq!(accounts.promised_labor_ms, 40);
    assert_eq!(accounts.performed_labor_ms, 17);
    assert_eq!(accounts.promised_payments[&Good::Food], 2_000);
    assert_eq!(accounts.performed_payments[&Good::Food], 500);
    assert_eq!(accounts.defaults, 2);
}

#[test]
fn owned_validation_detects_lifecycle_corruption_and_the_asset_bound_is_kind_specific() {
    for farm in [false, true] {
        let (mut world, mut store, definitions) = fixture();
        propose(&mut world, &mut store, &definitions, 200, farm);
        world
            .validate_owned(&store, &definitions, SimTime(0))
            .unwrap();
        assert_eq!(
            world.boundary(&store, SimTime(0), SimTime(500)),
            SimTime(200)
        );
        world
            .prepare_construction(&store, ProjectId(11), construction(0, 5_000))
            .unwrap()
            .commit(&mut world, &mut store, &definitions)
            .unwrap();
        assert_eq!(
            world.allocation_bound(&store, SimTime(100)).unwrap(),
            if farm { 1 } else { 2 }
        );
        world
            .validate_owned(&store, &definitions, SimTime(0))
            .unwrap();
        let mut missing = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
        remove_construction(&mut missing, PersonId(3)).unwrap();
        assert!(
            world
                .validate_owned(&missing, &definitions, SimTime(0))
                .is_err()
        );
        let mut wrong = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
        crate::project_mut(&mut wrong, ProjectId(11))
            .unwrap()
            .progress_ms = 1;
        assert!(
            world
                .validate_owned(&wrong, &definitions, SimTime(0))
                .is_err()
        );
    }
}

#[test]
fn owner_selects_known_construction_and_cancel_commands_and_local_observations() {
    let (mut runtime, mut store, definitions) = fixture();
    propose(&mut runtime, &mut store, &definitions, 200, false);
    runtime
        .prepare_observation(
            &store,
            ObservationFacts {
                now: SimTime(0),
                observers: &[Observer {
                    actor: PersonId(3),
                    area: Some(AreaId(2)),
                }],
                accounts_available: false,
            },
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(
        runtime.construction_candidates(&store, PersonId(3)),
        vec![ProjectId(11)]
    );
    assert!(
        runtime
            .construction_candidates(&store, PersonId(8))
            .is_empty()
    );
    let commands =
        runtime.available_actions(&store, actor(0), &[(ProjectId(11), construction(0, 5_000))]);
    assert_eq!(
        commands,
        vec![
            ProjectAction::Work {
                project: ProjectId(11)
            },
            ProjectAction::Cancel {
                project: ProjectId(11)
            }
        ]
    );
    let opening = store.snapshot().unwrap();
    assert_eq!(
        runtime.remembered_view(&store, PersonId(3)).projects.len(),
        1
    );
    assert!(
        runtime
            .remembered_view(&store, PersonId(8))
            .projects
            .is_empty()
    );
    assert_eq!(store.snapshot().unwrap(), opening);
}

#[test]
fn farm_financing_search_uses_personal_supply_memory_and_owned_retry_deadline() {
    let (mut runtime, mut store, definitions) = fixture();
    propose(&mut runtime, &mut store, &definitions, 1_000, true);
    let rows = [
        StockKnowledge {
            inventory: InventoryId(20),
            area: AreaId(2),
            good: Good::Timber,
            quantity: Quantity(5_000),
            accessible: false,
            observed_at: SimTime(0),
            learned_at: SimTime(0),
            source: KnowledgeSource::Observed(PersonId(3)),
        },
        StockKnowledge {
            inventory: InventoryId(21),
            area: AreaId(2),
            good: Good::Food,
            quantity: Quantity(0),
            accessible: false,
            observed_at: SimTime(0),
            learned_at: SimTime(0),
            source: KnowledgeSource::Observed(PersonId(3)),
        },
    ];
    let supplies = rows
        .iter()
        .map(|known| RememberedFinancingStock {
            known,
            custodian: Some(PersonId(8)),
            custodian_area: Some(AreaId(2)),
        })
        .collect::<Vec<_>>();
    let facts = || FundingSearchFacts {
        now: SimTime(0),
        ruler: PersonId(99),
        capable: true,
        knows_project: true,
        farm_independence: true,
        workshop_independence: true,
        ambition: 700,
        loading: SimDuration(10),
        calendar: Some(FundingCalendar {
            day: SimDuration(100),
            year_days: 10,
        }),
        materials_usable: false,
        balances: FundingBalances {
            materials: Quantity(0),
            input: Quantity(0),
        },
        supplies: &supplies,
        repayments: &[],
    };
    let opening = snapshot(&runtime, &store);
    let FundingChoice::Available(funding) =
        runtime.search_funding(&store, &definitions, ProjectId(11), facts())
    else {
        panic!("eligible remembered farm funding");
    };
    assert_eq!(funding.parties, BTreeSet::from([PersonId(3), PersonId(8)]));
    assert_eq!(snapshot(&runtime, &store), opening);
    runtime
        .record_funding_search(&mut store, PersonId(3), &funding)
        .unwrap();
    assert!(matches!(
        runtime.search_funding(&store, &definitions, ProjectId(11), facts()),
        FundingChoice::Unavailable(_)
    ));
    assert_eq!(
        requests(&store, PersonId(3)).unwrap().next_search,
        funding.next_search
    );
}

#[test]
fn project_procurement_respects_fresh_contact_and_keeps_unknown_supply_hints() {
    let (mut runtime, mut store, definitions) = fixture();
    propose(&mut runtime, &mut store, &definitions, 200, false);
    let observed = project(&store, ProjectId(11))
        .unwrap()
        .observed_view(SimTime(0));
    insert_knowledge(
        &mut store,
        PersonId(3),
        Knowledge {
            projects: BTreeMap::from([(ProjectId(11), observed)]),
            timber_hints: BTreeSet::from([AreaId(5)]),
            ..Knowledge::default()
        },
    )
    .unwrap();
    let target = runtime
        .procurement_target(&store, &definitions, PersonId(3), &[])
        .unwrap();
    let stocks = BTreeMap::from([(
        InventoryId(4),
        StockKnowledge {
            inventory: InventoryId(4),
            area: AreaId(2),
            good: Good::Timber,
            quantity: Quantity(0),
            accessible: true,
            observed_at: SimTime(0),
            learned_at: SimTime(0),
            source: KnowledgeSource::Observed(PersonId(3)),
        },
    )]);
    let mut destination = game_logistics::KnownDestination {
        destination: game_logistics::Destination::Inventory(InventoryId(4)),
        location: Location::AtArea(AreaId(2)),
        observed_at: SimTime(10),
        accessible: true,
        room: None,
        direct_transfer: true,
    };
    assert!(
        runtime
            .procurement(
                &store,
                target,
                ProcurementFacts {
                    actor: PersonId(3),
                    stocks: &stocks,
                    destination: &destination,
                    observed_location: &Location::AtArea(AreaId(2)),
                    now: SimTime(10),
                    actual_quantity: Quantity(5_000),
                }
            )
            .is_none()
    );
    destination.observed_at = SimTime(0);
    let procurement = runtime
        .procurement(
            &store,
            target,
            ProcurementFacts {
                actor: PersonId(3),
                stocks: &stocks,
                destination: &destination,
                observed_location: &Location::AtArea(AreaId(2)),
                now: SimTime(10),
                actual_quantity: Quantity(5_000),
            },
        )
        .unwrap();
    assert_eq!(procurement.delivery.requirement.quantity, Quantity(5_000));
    assert_eq!(procurement.hints, vec![AreaId(5)]);
}

#[test]
fn completed_workshop_asset_composes_receiving_owner_without_assembly() {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_workshop::register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let (mut runtime, _, definitions) = fixture();
    propose(&mut runtime, &mut store, &definitions, 200, false);
    for (id, good, quantity) in [
        (4, Good::Timber, 5_000),
        (5, Good::Tool, 1_000),
        (6, Good::Timber, 4_000),
        (7, Good::Tool, 0),
    ] {
        game_cargo::insert_inventory(
            &mut store,
            game_cargo::Inventory {
                id: InventoryId(id),
                good,
                quantity: Quantity(quantity),
                capacity: Quantity(10_000),
                location: game_cargo::InventoryLocation::Area(AreaId(2)),
                access: BTreeSet::from([PersonId(3)]),
                last_granted: None,
                spoils_at: None,
            },
        )
        .unwrap();
    }
    let construction = runtime
        .prepare_construction(&store, ProjectId(11), construction(0, 5_000))
        .unwrap();
    let consumptions = construction
        .effects()
        .iter()
        .filter_map(|effect| {
            if let Effect::Consume {
                source, quantity, ..
            } = effect
            {
                Some((*source, *quantity))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut batch = Batch::new();
    let cargo = game_cargo::prepare_consumptions(&store, &consumptions)
        .unwrap()
        .stage_guarded_into(&store, &mut batch)
        .unwrap();
    let projects = construction
        .stage_into(&runtime, &store, &definitions, &mut batch)
        .unwrap();
    cargo.validate(&store, &batch).unwrap();
    projects
        .validate_batch(&runtime, &store, &definitions, &batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    projects.finish(&mut runtime);
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(4))
            .unwrap()
            .quantity,
        Quantity(0)
    );
    runtime
        .prepare_labor_advance(&store, SimTime(0), SimTime(100), &[])
        .unwrap()
        .commit(&mut runtime, &mut store)
        .unwrap();
    let completion = runtime
        .prepare_completion(
            &store,
            &definitions,
            PersonId(3),
            CompletionFacts {
                now: SimTime(100),
                capable: true,
                asset_available: true,
                site: SiteId(12),
                enterprise: EnterpriseId(13),
            },
        )
        .unwrap();
    let asset = completion
        .effects()
        .iter()
        .find_map(|effect| {
            if let Effect::Completed {
                asset: AssetEffect::Workshop(asset),
                ..
            } = effect
            {
                Some(asset)
            } else {
                None
            }
        })
        .unwrap();
    let receiver = definitions
        .prepare_site_establishment(
            &store,
            &asset.recipe,
            game_workshop::SiteBindings {
                id: asset.site,
                area: asset.area,
                input: game_cargo::inventory(&store, asset.input).unwrap(),
                tool: game_cargo::inventory(&store, asset.tool).unwrap(),
                output: game_cargo::inventory(&store, asset.output).unwrap(),
                access: asset.access.clone(),
            },
        )
        .unwrap();
    let mut batch = Batch::new();
    let receiver = receiver.stage(&definitions, &store, &mut batch).unwrap();
    let projects = completion
        .stage_into(&runtime, &store, &definitions, &mut batch)
        .unwrap();
    receiver.validate(&definitions, &store, &batch).unwrap();
    projects
        .validate_batch(&runtime, &store, &definitions, &batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    projects.finish(&mut runtime);
    assert_eq!(
        game_workshop::site(&store, SiteId(12)).unwrap().recipe,
        definitions.catalog.default_recipe
    );
    assert_eq!(
        enterprise(&store, EnterpriseId(13)).unwrap().site,
        SiteId(12)
    );
}
