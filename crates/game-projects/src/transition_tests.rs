use super::*;
use game_agreements::{Agreement, AgreementStatus, AgreementTerm};

fn world() -> World {
    World {
        rules: Rules {
            construction_labor: SimDuration(100),
            ..Rules::default()
        },
        projects: BTreeMap::new(),
        enterprises: BTreeMap::new(),
        jobs: BTreeMap::new(),
        known_projects: BTreeMap::new(),
        known_enterprises: BTreeMap::new(),
        timber_hints: BTreeMap::new(),
        next_search: BTreeMap::new(),
        last_event: 0,
    }
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
fn project(world: &mut World, finish: u64, farm: bool) {
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
        .commit(world)
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
    let mut world = world();
    project(&mut world, 200, false);
    assert!(
        world
            .prepare_support(ProjectId(11), &backing(AgreementStatus::Rejected), actor(0))
            .is_err()
    );
    world
        .prepare_support(ProjectId(11), &backing(AgreementStatus::Proposed), actor(0))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert!(
        world.projects[&ProjectId(11)]
            .support
            .contains(&AgreementId(12))
    );
    assert!(
        world
            .prepare_construction(ProjectId(11), construction(0, 0))
            .is_err()
    );
    assert!(!world.projects[&ProjectId(11)].materials_consumed);
    assert!(world.jobs.is_empty());
}
#[test]
fn construction_consumes_once_credits_real_elapsed_labor_and_resumes_at_remaining_deadline() {
    let mut world = world();
    project(&mut world, 200, false);
    let effects = world
        .prepare_construction(ProjectId(11), construction(0, 5_000))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert!(effects.contains(&Effect::Consume {
        actor: PersonId(3),
        source: InventoryId(4),
        quantity: Quantity(5_000)
    }));
    world = world.advance(SimTime(0), SimTime(40), &[]).unwrap();
    world
        .prepare_interrupt(PersonId(3))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert_eq!(world.projects[&ProjectId(11)].status, ProjectStatus::Ready);
    let effects = world
        .prepare_construction(ProjectId(11), construction(60, 0))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert!(!effects.iter().any(|e| matches!(e, Effect::Consume { .. })));
    assert_eq!(world.jobs[&PersonId(3)].completes, SimTime(120));
    world = world.advance(SimTime(60), SimTime(120), &[]).unwrap();
    assert_eq!(world.projects[&ProjectId(11)].progress_ms, 100);
    assert_eq!(
        world.projects[&ProjectId(11)].contributions[&PersonId(3)],
        100
    );
}
#[test]
fn completion_describes_the_asset_and_enterprise_without_inventing_a_stock_or_owner() {
    for farm in [false, true] {
        let mut world = world();
        project(&mut world, 200, farm);
        world
            .prepare_construction(ProjectId(11), construction(0, 5_000))
            .unwrap()
            .commit(&mut world)
            .unwrap();
        world = world.advance(SimTime(0), SimTime(100), &[]).unwrap();
        let effects = world
            .prepare_completion(
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
            .commit(&mut world)
            .unwrap();
        assert_eq!(
            world.projects[&ProjectId(11)].status,
            ProjectStatus::Completed
        );
        let enterprise = &world.enterprises[&EnterpriseId(21)];
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
                assert_eq!(asset.input_quantity, Quantity(2_000));
                assert_eq!(asset.output_quantity, Quantity(1_000));
            }
            _ => panic!("missing concrete asset effect"),
        }
    }
}
#[test]
fn cancellation_and_deadline_failure_stop_jobs_without_refunding_inputs_or_changing_agreements() {
    let mut world = world();
    project(&mut world, 150, false);
    world
        .prepare_support(ProjectId(11), &backing(AgreementStatus::Active), actor(0))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    world
        .prepare_construction(ProjectId(11), construction(0, 5_000))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    let mut cancelled = world.clone();
    assert!(
        cancelled
            .prepare_cancel(
                ProjectId(11),
                ActorFacts {
                    actor: PersonId(8),
                    ..actor(1)
                }
            )
            .is_err()
    );
    let effects = cancelled
        .prepare_cancel(ProjectId(11), actor(1))
        .unwrap()
        .commit(&mut cancelled)
        .unwrap();
    assert_eq!(
        cancelled.projects[&ProjectId(11)].status,
        ProjectStatus::Cancelled
    );
    assert!(cancelled.projects[&ProjectId(11)].materials_consumed);
    assert!(
        cancelled.projects[&ProjectId(11)]
            .support
            .contains(&AgreementId(12))
    );
    assert!(effects.contains(&Effect::Interrupt { actor: PersonId(3) }));
    let effects = world
        .prepare_reconcile(SimTime(150), &[], &[], &[], 0)
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert_eq!(world.projects[&ProjectId(11)].status, ProjectStatus::Failed);
    assert_eq!(
        world.projects[&ProjectId(11)].blocker,
        Some(ProjectBlocker::Deadline)
    );
    assert!(world.jobs.is_empty());
    assert!(effects.contains(&Effect::Failed {
        project: ProjectId(11)
    }));
}
#[test]
fn missing_tool_blocks_readiness_and_insufficient_time_or_land_cannot_begin() {
    let mut world = world();
    project(&mut world, 90, true);
    assert!(
        world
            .prepare_construction(ProjectId(11), construction(0, 5_000))
            .is_err()
    );
    world.projects.get_mut(&ProjectId(11)).unwrap().finish_by = SimTime(200);
    assert!(
        world
            .prepare_construction(
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
        .commit(&mut world)
        .unwrap();
    assert_eq!(
        world.projects[&ProjectId(11)].blocker,
        Some(ProjectBlocker::Tool)
    );
}
#[test]
fn stale_prepared_change_and_batched_account_overflow_have_no_partial_effects() {
    let mut world = world();
    project(&mut world, 200, false);
    let prepared = world
        .prepare_construction(ProjectId(11), construction(0, 5_000))
        .unwrap();
    world
        .projects
        .get_mut(&ProjectId(11))
        .unwrap()
        .support
        .insert(AgreementId(99));
    let before = world.clone();
    assert!(prepared.commit(&mut world).is_err());
    assert_eq!(world, before);
    world
        .prepare_construction(ProjectId(11), construction(0, 5_000))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    world = world.advance(SimTime(0), SimTime(100), &[]).unwrap();
    world
        .prepare_completion(
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
        .commit(&mut world)
        .unwrap();
    world
        .enterprises
        .get_mut(&EnterpriseId(21))
        .unwrap()
        .produced
        .insert(Good::Tool, u64::MAX - 1);
    let before = world.clone();
    assert!(
        world
            .prepare_outputs(
                &[
                    (SiteId(20), Good::Tool, Quantity(1)),
                    (SiteId(20), Good::Tool, Quantity(1))
                ],
                Some(4)
            )
            .is_err()
    );
    assert_eq!(world, before);
}
#[test]
fn current_restore_and_chunked_construction_produce_identical_owned_state() {
    let mut initial = world();
    project(&mut initial, 200, false);
    initial
        .prepare_construction(ProjectId(11), construction(0, 5_000))
        .unwrap()
        .commit(&mut initial)
        .unwrap();
    let one = initial.advance(SimTime(0), SimTime(100), &[]).unwrap();
    let partial = initial.advance(SimTime(0), SimTime(37), &[]).unwrap();
    let loaded: World = serde_json::from_slice(&serde_json::to_vec(&partial).unwrap()).unwrap();
    assert_eq!(one, loaded.advance(SimTime(37), SimTime(100), &[]).unwrap());
}

#[test]
fn accepted_operating_terms_attach_without_creating_labor_or_payment_and_accounts_keep_defaults() {
    let mut world = world();
    project(&mut world, 200, false);
    world
        .prepare_construction(ProjectId(11), construction(0, 5_000))
        .unwrap()
        .commit(&mut world)
        .unwrap();
    world = world.advance(SimTime(0), SimTime(100), &[]).unwrap();
    world
        .prepare_completion(
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
        .commit(&mut world)
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
            .prepare_operation(EnterpriseId(21), &contract, worker, false)
            .is_err()
    );
    world
        .prepare_operation(EnterpriseId(21), &contract, worker, true)
        .unwrap()
        .commit(&mut world)
        .unwrap();
    let enterprise = &world.enterprises[&EnterpriseId(21)];
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
        let mut world = world();
        project(&mut world, 200, farm);
        world.validate_owned(SimTime(0)).unwrap();
        assert_eq!(world.boundary(SimTime(0), SimTime(500)), SimTime(200));
        world
            .prepare_construction(ProjectId(11), construction(0, 5_000))
            .unwrap()
            .commit(&mut world)
            .unwrap();
        assert_eq!(
            world.allocation_bound(SimTime(100)).unwrap(),
            if farm { 1 } else { 2 }
        );
        world.validate_owned(SimTime(0)).unwrap();
        let mut corrupt = world.clone();
        corrupt.jobs.clear();
        assert!(corrupt.validate_owned(SimTime(0)).is_err());
        let mut corrupt = world.clone();
        corrupt
            .projects
            .get_mut(&ProjectId(11))
            .unwrap()
            .progress_ms = 1;
        assert!(corrupt.validate_owned(SimTime(0)).is_err());
    }
}
