use crate::*;
use game_cargo::{Inventory, InventoryLocation};
use game_logistics::Step;
fn actor() -> ActorFacts {
    ActorFacts {
        actor: PersonId(3),
        now: SimTime(0),
        area: Some(AreaId(2)),
        capable: true,
        knows_project: true,
    }
}
fn construction() -> ConstructionFacts {
    ConstructionFacts {
        actor: actor(),
        land_accessible: true,
        tool_available: true,
        materials_available: Quantity(5_000),
    }
}
fn resource(good: Good, quantity: u64) -> ResourceFacts {
    ResourceFacts {
        good,
        accessible: true,
        area: Some(AreaId(2)),
        actor_area: Some(AreaId(2)),
        unreserved: Quantity(quantity),
    }
}
fn fixture() -> (Runtime, Store, game_workshop::Definitions) {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_workshop::register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let plan = Project {
        kind: ProjectKind::Workshop {
            recipe: game_workshop::RecipeId("tools".into()),
        },
        id: ProjectId(11),
        sponsor: PersonId(3),
        area: AreaId(2),
        materials: InventoryId(1),
        tool: InventoryId(2),
        input: InventoryId(3),
        output: InventoryId(4),
        beneficiaries: BTreeSet::from([PersonId(3)]),
        workers: BTreeSet::from([PersonId(3)]),
        support: BTreeSet::new(),
        status: ProjectStatus::Ready,
        blocker: None,
        progress_ms: 0,
        contributions: BTreeMap::new(),
        materials_consumed: false,
        finish_by: SimTime(600_000),
        enterprise: None,
    };
    insert_project(&mut store, plan.id, plan.clone()).unwrap();
    insert_knowledge(
        &mut store,
        PersonId(3),
        Knowledge {
            projects: BTreeMap::from([(plan.id, plan.observed_view(SimTime(0)))]),
            ..Knowledge::default()
        },
    )
    .unwrap();
    for (id, kind, site) in [
        (
            21,
            ProjectKind::Farm {
                plot: SiteId(41),
                water: InventoryId(8),
            },
            SiteId(41),
        ),
        (22, plan.kind.clone(), SiteId(42)),
    ] {
        insert_enterprise(
            &mut store,
            EnterpriseId(id),
            Enterprise {
                id: EnterpriseId(id),
                project: ProjectId(11),
                kind,
                site,
                area: AreaId(2),
                sponsor: PersonId(3),
                beneficiaries: BTreeSet::from([PersonId(3)]),
                workers: BTreeSet::from([PersonId(3)]),
                status: EnterpriseStatus::Ready,
                blocker: None,
                inputs: BTreeMap::new(),
                produced: BTreeMap::new(),
                labor: BTreeMap::new(),
                operating_agreements: BTreeSet::new(),
            },
        )
        .unwrap();
    }
    game_workshop::insert_site(
        &mut store,
        game_workshop::Site {
            id: SiteId(42),
            recipe: game_workshop::RecipeId("tools".into()),
            area: AreaId(2),
            input: InventoryId(3),
            tool: InventoryId(2),
            output: InventoryId(4),
            access: BTreeSet::from([PersonId(3)]),
            last_granted: None,
        },
    )
    .unwrap();
    game_cargo::insert_inventory(
        &mut store,
        Inventory {
            id: InventoryId(4),
            location: InventoryLocation::Area(AreaId(2)),
            good: Good::Tool,
            quantity: Quantity(0),
            capacity: Quantity(10_000),
            access: BTreeSet::from([PersonId(3)]),
            last_granted: None,
            spoils_at: None,
        },
    )
    .unwrap();
    (
        Runtime::default(),
        store,
        crate::transition_tests::definitions(),
    )
}
#[test]
fn owner_composes_construction_procurement_cultivation_and_workshop_in_stable_order() {
    let (runtime, store, definitions) = fixture();
    let construction = [(ProjectId(11), construction())];
    let operations = vec![
        EnterpriseOperationFacts {
            enterprise: EnterpriseId(22),
            cultivation: None,
            input: Some(resource(Good::Timber, 2_000)),
            tool: Some(resource(Good::Tool, 1_000)),
            output: game_cargo::inventory(&store, InventoryId(4)),
        },
        EnterpriseOperationFacts {
            enterprise: EnterpriseId(21),
            cultivation: Some(7_u8),
            input: None,
            tool: None,
            output: None,
        },
    ];
    let mut reversed = operations.clone();
    reversed.reverse();
    let opening = store.snapshot().unwrap();
    let choose = |operations| {
        runtime.opportunities(
            &store,
            &definitions,
            OpportunityFacts {
                actor: actor(),
                has_mind: true,
                construction: &construction,
                procurement: Some(Step::Acquire(9)),
                enterprises: operations,
                stocks: &[],
                plots: &[],
                loading: MINUTE,
            },
        )
    };
    let expected = vec![
        ProjectOpportunity::Action(ProjectAction::Work {
            project: ProjectId(11),
        }),
        ProjectOpportunity::Procurement(Step::Acquire(9)),
        ProjectOpportunity::Cultivation(7),
        ProjectOpportunity::Workshop(SiteId(42)),
    ];
    assert_eq!(choose(&operations), expected);
    assert_eq!(choose(&reversed), expected);
    assert_eq!(store.snapshot().unwrap(), opening);
}
#[test]
fn owner_opportunity_gates_apply_to_all_paths_and_real_recipe_requirements() {
    let (runtime, store, definitions) = fixture();
    let construction = [(ProjectId(11), construction())];
    let operation = [EnterpriseOperationFacts {
        enterprise: EnterpriseId(22),
        cultivation: None::<u8>,
        input: Some(resource(Good::Timber, 1_999)),
        tool: Some(resource(Good::Tool, 1_000)),
        output: game_cargo::inventory(&store, InventoryId(4)),
    }];
    let choose = |actor, has_mind| {
        runtime.opportunities(
            &store,
            &definitions,
            OpportunityFacts {
                actor,
                has_mind,
                construction: &construction,
                procurement: Some(Step::Complete),
                enterprises: &operation,
                stocks: &[],
                plots: &[],
                loading: MINUTE,
            },
        )
    };
    assert_eq!(
        choose(actor(), true),
        vec![ProjectOpportunity::Action(ProjectAction::Work {
            project: ProjectId(11)
        })]
    );
    assert!(
        choose(
            ActorFacts {
                capable: false,
                ..actor()
            },
            true
        )
        .is_empty()
    );
    assert!(choose(actor(), false).is_empty());
    assert!(
        choose(
            ActorFacts {
                area: None,
                ..actor()
            },
            true
        )
        .is_empty()
    );
}
#[test]
fn project_tool_requirements_and_procurement_handoff_are_owned_exact_boundaries() {
    assert!(!tool_stock_usable(Some(resource(Good::Tool, 999))));
    assert!(tool_stock_usable(Some(resource(Good::Tool, 1_000))));
    assert!(!construction_tool_available(
        Some(resource(Good::Tool, 1_000)),
        false
    ));
    assert!(construction_tool_available(
        Some(resource(Good::Tool, 1_000)),
        true
    ));
    assert!(!tool_stock_usable(Some(resource(Good::Food, 1_000))));
    assert!(!tool_stock_usable(Some(ResourceFacts {
        accessible: false,
        ..resource(Good::Tool, 1_000)
    })));
    assert!(
        Procurement::active_step::<u8>(Step::Arrived {
            source: InventoryId(1),
            quantity: Quantity(1)
        })
        .is_none()
    );
    assert!(Procurement::active_step::<u8>(Step::Complete).is_none());
    let procurement = Procurement {
        delivery: game_logistics::Delivery::new(
            game_logistics::Requirement {
                good: Good::Timber,
                quantity: Quantity(1),
                source: None,
            },
            game_logistics::Destination::Inventory(InventoryId(1)),
        )
        .unwrap(),
        hints: Vec::new(),
        observed_at: SimTime(0),
    };
    assert_eq!(procurement.minimum_handoff(), Quantity(1));
    assert_eq!(
        Procurement::active_step(Step::Acquire(3_u8)),
        Some(Step::Acquire(3))
    );
}

#[test]
fn composed_new_proposals_keep_farm_before_workshop_without_assembly() {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    let store = Store::with_schema(schema);
    let definitions = crate::transition_tests::definitions();
    let rows = [
        (1, Good::Timber, 0),
        (2, Good::Tool, 1_000),
        (3, Good::Seed, 0),
        (4, Good::Water, 0),
        (5, Good::Food, 0),
        (6, Good::Tool, 0),
    ]
    .into_iter()
    .map(|(id, good, quantity)| StockKnowledge {
        inventory: InventoryId(id),
        area: AreaId(2),
        good,
        quantity: Quantity(quantity),
        accessible: true,
        observed_at: SimTime(0),
        learned_at: SimTime(0),
        source: KnowledgeSource::Observed(PersonId(3)),
    })
    .collect::<Vec<_>>();
    let stocks = rows
        .iter()
        .map(|known| WorkshopStock {
            known,
            at_area: Some(AreaId(2)),
            custodian: Some(PersonId(3)),
        })
        .collect::<Vec<_>>();
    let plots = [RememberedPlot {
        id: SiteId(50),
        area: AreaId(2),
        accessible: true,
    }];
    let result = Runtime::default().opportunities::<u8>(
        &store,
        &definitions,
        OpportunityFacts {
            actor: actor(),
            has_mind: true,
            construction: &[],
            procurement: None,
            enterprises: &[],
            stocks: &stocks,
            plots: &plots,
            loading: MINUTE,
        },
    );
    assert!(matches!(
        result.as_slice(),
        [
            ProjectOpportunity::Action(ProjectAction::ProposeFarm { .. }),
            ProjectOpportunity::Action(ProjectAction::Propose { .. })
        ]
    ));
}
