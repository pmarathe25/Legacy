use super::*;
fn definitions() -> Definitions {
    Definitions::from_content(serde_json::from_value(serde_json::json!({
        "id":"test/recipes","version":1,"data":{"default_recipe":"toolmaking","recipes":{
            "toolmaking":{"input_good":"Timber","input_quantity":2000,"output_good":"Tool","output_quantity":1000,"duration":60000},
            "cordage":{"input_good":"Timber","input_quantity":1000,"output_good":"Binding","output_quantity":1000,"duration":30000}
        }}
    })).unwrap()).unwrap()
}
fn stock(id: u64, good: Good, quantity: u64) -> StockKnowledge {
    StockKnowledge {
        inventory: InventoryId(id),
        area: AreaId(1),
        good,
        quantity: Quantity(quantity),
        accessible: true,
        observed_at: SimTime(0),
        learned_at: SimTime(0),
        source: KnowledgeSource::Observed(PersonId(9)),
    }
}
fn store() -> Store {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    Store::with_schema(schema)
}
fn facts() -> WorkshopProspectFacts {
    WorkshopProspectFacts {
        actor: PersonId(9),
        area: AreaId(1),
        now: SimTime(0),
        capable: true,
        loading: SimDuration(60_000),
    }
}
#[test]
fn default_blueprint_preserves_order_deadline_and_remembered_amounts() {
    let rows = [
        stock(3, Good::Tool, 0),
        stock(2, Good::Tool, 1_000),
        stock(1, Good::Timber, 0),
    ];
    let stocks = rows
        .iter()
        .map(|known| WorkshopStock {
            known,
            at_area: Some(AreaId(1)),
            custodian: Some(PersonId(9)),
        })
        .collect::<Vec<_>>();
    let runtime = Runtime::default();
    let plans = runtime
        .workshop_prospects(&store(), &definitions(), facts(), &stocks)
        .unwrap();
    assert_eq!(
        plans,
        vec![ProjectAction::Propose {
            recipe: game_workshop::RecipeId("toolmaking".into()),
            area: AreaId(1),
            materials: InventoryId(1),
            tool: InventoryId(2),
            input: InventoryId(1),
            output: InventoryId(3),
            beneficiaries: BTreeSet::from([PersonId(9)]),
            workers: BTreeSet::from([PersonId(9)]),
            finish_by: SimTime(runtime.rules.construction_labor.0 * 4 + 600_000)
        }]
    );
}
#[test]
fn added_recipe_uses_its_goods_and_cooldown_capability_and_custody_are_gates() {
    let rows = [
        stock(1, Good::Timber, 20_000),
        stock(2, Good::Tool, 1_000),
        stock(3, Good::Tool, 0),
        stock(4, Good::Binding, 0),
    ];
    let mut stocks = rows
        .iter()
        .map(|known| WorkshopStock {
            known,
            at_area: Some(AreaId(1)),
            custodian: Some(PersonId(9)),
        })
        .collect::<Vec<_>>();
    let runtime = Runtime::default();
    let mut store = store();
    let defs = definitions();
    let plans = runtime
        .workshop_prospects(&store, &defs, facts(), &stocks)
        .unwrap();
    assert_eq!(plans.len(), 2);
    assert!(
        matches!(&plans[1],ProjectAction::Propose{recipe,output,..} if recipe.0=="cordage" && *output==InventoryId(4))
    );
    stocks[3].custodian = Some(PersonId(8));
    assert_eq!(
        runtime
            .workshop_prospects(&store, &defs, facts(), &stocks)
            .unwrap()
            .len(),
        1
    );
    assert!(
        runtime
            .workshop_prospects(
                &store,
                &defs,
                WorkshopProspectFacts {
                    capable: false,
                    ..facts()
                },
                &stocks
            )
            .unwrap()
            .is_empty()
    );
    insert_requests(
        &mut store,
        PersonId(9),
        Requests {
            next_search: SimTime(1),
        },
    )
    .unwrap();
    assert!(
        runtime
            .workshop_prospects(&store, &defs, facts(), &stocks)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn foreign_locality_and_deadline_overflow_do_not_create_proposals() {
    let rows = [
        stock(1, Good::Timber, 20_000),
        stock(2, Good::Tool, 1_000),
        stock(3, Good::Tool, 0),
    ];
    let stocks = rows
        .iter()
        .map(|known| WorkshopStock {
            known,
            at_area: None,
            custodian: Some(PersonId(9)),
        })
        .collect::<Vec<_>>();
    let runtime = Runtime::default();
    let defs = definitions();
    let store = store();
    assert!(
        runtime
            .workshop_prospects(&store, &defs, facts(), &stocks)
            .unwrap()
            .is_empty()
    );
    let stocks = rows
        .iter()
        .map(|known| WorkshopStock {
            known,
            at_area: Some(AreaId(1)),
            custodian: Some(PersonId(9)),
        })
        .collect::<Vec<_>>();
    assert!(
        runtime
            .workshop_prospects(
                &store,
                &defs,
                WorkshopProspectFacts {
                    now: SimTime(u64::MAX),
                    ..facts()
                },
                &stocks
            )
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unfinished_personal_plan_blocks_search_but_cancelled_history_does_not() {
    let rows = [
        stock(1, Good::Timber, 0),
        stock(2, Good::Tool, 1_000),
        stock(3, Good::Tool, 0),
    ];
    let stocks = rows
        .iter()
        .map(|known| WorkshopStock {
            known,
            at_area: Some(AreaId(1)),
            custodian: Some(PersonId(9)),
        })
        .collect::<Vec<_>>();
    let mut store = store();
    let runtime = Runtime::default();
    let definitions = definitions();
    let remembered = ProjectView {
        kind: ProjectKind::Workshop {
            recipe: game_workshop::RecipeId("toolmaking".into()),
        },
        id: ProjectId(10),
        sponsor: PersonId(9),
        area: AreaId(1),
        materials: InventoryId(1),
        tool: InventoryId(2),
        input: InventoryId(1),
        output: InventoryId(3),
        beneficiaries: BTreeSet::from([PersonId(9)]),
        workers: BTreeSet::from([PersonId(9)]),
        support: BTreeSet::new(),
        status: ProjectStatus::SeekingSupport,
        blocker: None,
        progress_ms: 0,
        materials_consumed: false,
        finish_by: SimTime(1_000_000),
        enterprise: None,
        observed_at: SimTime(0),
    };
    insert_knowledge(
        &mut store,
        PersonId(9),
        Knowledge {
            projects: BTreeMap::from([(ProjectId(10), remembered)]),
            ..Knowledge::default()
        },
    )
    .unwrap();
    assert!(
        runtime
            .workshop_prospects(&store, &definitions, facts(), &stocks)
            .unwrap()
            .is_empty()
    );
    knowledge_mut(&mut store, PersonId(9))
        .unwrap()
        .projects
        .get_mut(&ProjectId(10))
        .unwrap()
        .status = ProjectStatus::Cancelled;
    assert_eq!(
        runtime
            .workshop_prospects(&store, &definitions, facts(), &stocks)
            .unwrap()
            .len(),
        1
    );
    let mut reversed = stocks;
    reversed.reverse();
    assert_eq!(
        runtime
            .workshop_prospects(&store, &definitions, facts(), &reversed)
            .unwrap(),
        runtime
            .workshop_prospects(
                &store,
                &definitions,
                facts(),
                &reversed.iter().copied().rev().collect::<Vec<_>>()
            )
            .unwrap()
    );
}

#[test]
fn farm_choice_is_stable_and_requires_remembered_accessible_owned_supplies() {
    let rows = [
        stock(1, Good::Timber, 0),
        stock(2, Good::Tool, 1_000),
        stock(3, Good::Seed, 0),
        stock(4, Good::Water, 0),
        stock(5, Good::Food, 0),
    ];
    let mut stocks = rows
        .iter()
        .map(|known| WorkshopStock {
            known,
            at_area: Some(AreaId(1)),
            custodian: Some(PersonId(9)),
        })
        .collect::<Vec<_>>();
    let plots = [
        RememberedPlot {
            id: SiteId(10),
            area: AreaId(1),
            accessible: true,
        },
        RememberedPlot {
            id: SiteId(6),
            area: AreaId(1),
            accessible: true,
        },
    ];
    let runtime = Runtime::default();
    let mut store = store();
    let choices = runtime.farm_prospects(&store, facts(), &stocks, &plots);
    assert!(
        matches!(choices.as_slice(), [ProjectAction::ProposeFarm { plot, .. }] if *plot == SiteId(6))
    );
    stocks.reverse();
    assert_eq!(
        runtime.farm_prospects(&store, facts(), &stocks, &plots),
        choices
    );
    stocks[0].custodian = Some(PersonId(8));
    assert!(
        runtime
            .farm_prospects(&store, facts(), &stocks, &plots)
            .is_empty()
    );
    insert_requests(
        &mut store,
        PersonId(9),
        Requests {
            next_search: SimTime(1),
        },
    )
    .unwrap();
    assert!(
        runtime
            .farm_prospects(&store, facts(), &stocks, &plots)
            .is_empty()
    );
}
