use super::*;
#[derive(Default)]
struct Known {
    stocks: BTreeMap<InventoryId, StockKnowledge>,
    owners: BTreeMap<InventoryId, PersonId>,
    work: bool,
}
impl FinancingKnowledge for Known {
    fn stocks(&self) -> impl Iterator<Item = &StockKnowledge> {
        self.stocks.values()
    }
    fn stock(&self, id: InventoryId) -> Option<&StockKnowledge> {
        self.stocks.get(&id)
    }
    fn custodian(&self, id: InventoryId) -> Option<PersonId> {
        self.owners.get(&id).copied()
    }
    fn has_work_access(&self) -> bool {
        self.work
    }
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
        source: KnowledgeSource::Observed(PersonId(1)),
    }
}
fn definitions(input: Good, output: Good) -> Definitions {
    Definitions::from_content(serde_json::from_value(serde_json::json!({"id":"test/recipes","version":1,"data":{
        "default_recipe":"test-recipe","recipes":{"test-recipe":{"input_good":input,"input_quantity":2000,
        "output_good":output,"output_quantity":1000,"duration":60000}}}})).unwrap()).unwrap()
}
fn fixture() -> (
    Runtime,
    Store,
    Definitions,
    Known,
    FinancingActorFacts,
    Vec<AgreementTerm>,
) {
    let runtime = Runtime::default();
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let plan = ProjectView {
        kind: ProjectKind::Workshop {
            recipe: game_workshop::RecipeId("test-recipe".into()),
        },
        id: ProjectId(30),
        sponsor: PersonId(2),
        area: AreaId(1),
        materials: InventoryId(13),
        tool: InventoryId(12),
        input: InventoryId(13),
        output: InventoryId(20),
        beneficiaries: BTreeSet::from([PersonId(2)]),
        workers: BTreeSet::from([PersonId(2)]),
        support: BTreeSet::new(),
        status: ProjectStatus::SeekingSupport,
        blocker: None,
        progress_ms: 0,
        materials_consumed: false,
        finish_by: SimTime(300000),
        enterprise: None,
        observed_at: SimTime(0),
    };
    insert_knowledge(
        &mut store,
        PersonId(1),
        Knowledge {
            projects: BTreeMap::from([(plan.id, plan)]),
            ..Knowledge::default()
        },
    )
    .unwrap();
    let known = Known {
        stocks: BTreeMap::from([
            (InventoryId(10), stock(10, Good::Tool, 0)),
            (InventoryId(11), stock(11, Good::Timber, 20000)),
            (InventoryId(12), stock(12, Good::Tool, 1000)),
        ]),
        owners: BTreeMap::from([
            (InventoryId(10), PersonId(1)),
            (InventoryId(11), PersonId(1)),
            (InventoryId(12), PersonId(2)),
        ]),
        work: false,
    };
    let actor = FinancingActorFacts {
        actor: PersonId(1),
        now: SimTime(0),
        seeks_independence: true,
        desired_good: Good::Tool,
        ambition: 800,
        risk_tolerance: 500,
    };
    let terms = vec![
        AgreementTerm::Delivery {
            debtor: PersonId(2),
            creditor: PersonId(1),
            source: InventoryId(20),
            destination: InventoryId(10),
            quantity: Quantity(3000),
            deadline: SimTime(720000),
        },
        AgreementTerm::Delivery {
            debtor: PersonId(1),
            creditor: PersonId(2),
            source: InventoryId(11),
            destination: InventoryId(13),
            quantity: Quantity(11000),
            deadline: SimTime(300000),
        },
    ];
    (
        runtime,
        store,
        definitions(Good::Timber, Good::Tool),
        known,
        actor,
        terms,
    )
}
#[test]
fn merged_finance_preserves_scalar_autonomy_reserve_and_exact_time_boundary() {
    let (runtime, store, definitions, mut known, mut actor, mut terms) = fixture();
    let before = store.snapshot().unwrap();
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        2000
    );
    known.work = true;
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        800
    );
    known.work = false;
    actor.ambition = 599;
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0
    );
    actor.ambition = 800;
    actor.seeks_independence = false;
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0
    );
    actor.seeks_independence = true;
    if let AgreementTerm::Delivery { deadline, .. } = &mut terms[0] {
        *deadline = SimTime(719999);
    }
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0
    );
    if let AgreementTerm::Delivery { deadline, .. } = &mut terms[0] {
        *deadline = SimTime(720000);
    }
    known.stocks.get_mut(&InventoryId(11)).unwrap().quantity = Quantity(15999);
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0
    );
    known.stocks.get_mut(&InventoryId(11)).unwrap().quantity = Quantity(16000);
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        2000
    );
    known.stocks.get_mut(&InventoryId(10)).unwrap().quantity = Quantity(1000);
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn arbitrary_recipe_input_requires_its_own_good_and_destination() {
    let (runtime, mut store, _, mut known, actor, mut terms) = fixture();
    let recipes = definitions(Good::Food, Good::Tool);
    knowledge_mut(&mut store, actor.actor)
        .unwrap()
        .projects
        .get_mut(&ProjectId(30))
        .unwrap()
        .input = InventoryId(14);
    assert_eq!(
        runtime.financing_utility(&store, &recipes, actor, &terms, &known),
        0,
        "timber cannot provide recipe food"
    );
    if let AgreementTerm::Delivery { quantity, .. } = &mut terms[1] {
        *quantity = Quantity(5000);
    }
    known
        .stocks
        .insert(InventoryId(15), stock(15, Good::Food, 6000));
    known.owners.insert(InventoryId(15), PersonId(1));
    terms.push(AgreementTerm::Delivery {
        debtor: actor.actor,
        creditor: PersonId(2),
        source: InventoryId(15),
        destination: InventoryId(14),
        quantity: Quantity(6000),
        deadline: SimTime(300000),
    });
    assert_eq!(
        runtime.financing_utility(&store, &recipes, actor, &terms, &known),
        2000
    );
    if let AgreementTerm::Delivery { destination, .. } = &mut terms[2] {
        *destination = InventoryId(13);
    }
    assert_eq!(
        runtime.financing_utility(&store, &recipes, actor, &terms, &known),
        0,
        "food sent to the construction stock cannot fund a separate recipe stock"
    );
}
#[test]
fn one_shared_endpoint_pledge_is_not_credited_twice_and_all_source_spending_is_counted() {
    let (runtime, store, definitions, known, actor, mut terms) = fixture();
    if let AgreementTerm::Delivery { quantity, .. } = &mut terms[1] {
        *quantity = Quantity(6000);
    }
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0,
        "production and construction together need eleven units"
    );
    if let AgreementTerm::Delivery { quantity, .. } = &mut terms[1] {
        *quantity = Quantity(11000);
    }
    terms.push(AgreementTerm::Delivery {
        debtor: actor.actor,
        creditor: PersonId(9),
        source: InventoryId(11),
        destination: InventoryId(99),
        quantity: Quantity(5001),
        deadline: SimTime(300000),
    });
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0,
        "another pledge spends the retained construction reserve"
    );
}
#[test]
fn non_tool_recipe_output_can_serve_an_explicit_material_independence_goal() {
    let (runtime, store, _, mut known, mut actor, terms) = fixture();
    let recipes = definitions(Good::Timber, Good::Food);
    actor.desired_good = Good::Food;
    known.stocks.get_mut(&InventoryId(10)).unwrap().good = Good::Food;
    assert_eq!(
        runtime.financing_utility(&store, &recipes, actor, &terms, &known),
        2000
    );
    actor.desired_good = Good::Tool;
    assert_eq!(
        runtime.financing_utility(&store, &recipes, actor, &terms, &known),
        0
    );
}
#[test]
fn missing_personal_plan_and_unknown_or_unrelated_supply_do_not_expose_live_world() {
    let (runtime, mut store, definitions, mut known, actor, terms) = fixture();
    let baseline = runtime.financing_utility(&store, &definitions, actor, &terms, &known);
    known
        .stocks
        .insert(InventoryId(99), stock(99, Good::Timber, u64::MAX));
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        baseline
    );
    known.stocks.remove(&InventoryId(11));
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0
    );
    known
        .stocks
        .insert(InventoryId(11), stock(11, Good::Timber, 20000));
    remove_knowledge(&mut store, actor.actor).unwrap();
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0
    );
}
#[test]
fn invalid_or_unrepresentable_recipes_are_not_opportunities_and_do_not_mutate_rows() {
    let (runtime, store, mut definitions, known, actor, terms) = fixture();
    let before = store.snapshot().unwrap();
    let recipe = definitions
        .catalog
        .recipes
        .get_mut(&game_workshop::RecipeId("test-recipe".into()))
        .unwrap();
    recipe.input_quantity = Quantity(u64::MAX);
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0
    );
    let recipe = definitions
        .catalog
        .recipes
        .get_mut(&game_workshop::RecipeId("test-recipe".into()))
        .unwrap();
    recipe.input_quantity = Quantity(2000);
    recipe.duration = SimDuration(u64::MAX);
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0
    );
    definitions
        .catalog
        .recipes
        .get_mut(&game_workshop::RecipeId("test-recipe".into()))
        .unwrap()
        .output_quantity = Quantity(0);
    assert_eq!(
        runtime.financing_utility(&store, &definitions, actor, &terms, &known),
        0
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
