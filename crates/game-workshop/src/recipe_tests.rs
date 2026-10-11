use super::*;
fn stock(id: u64, good: Good, quantity: u64) -> Inventory {
    Inventory {
        id: InventoryId(id),
        location: InventoryLocation::Area(AreaId(1)),
        good,
        quantity: Quantity(quantity),
        capacity: Quantity(100_000),
        access: BTreeSet::from([PersonId(1)]),
        last_granted: None,
        spoils_at: None,
    }
}
#[test]
fn selected_data_recipe_controls_actual_workshop_consumption_and_frozen_job_time() {
    let defs = Definitions::from_content(
        game_content::parse(include_str!("../../../content/workshop/temperate.toml")).unwrap(),
    )
    .unwrap();
    let mut input = stock(1, Good::Timber, 10_000);
    let tool = stock(2, Good::Tool, 1_000);
    let output = stock(3, Good::Binding, 0);
    let site = defs
        .bind(
            &RecipeId("cordage".into()),
            SiteBindings {
                id: SiteId(4),
                area: AreaId(1),
                input: &input,
                tool: &tool,
                output: &output,
                access: BTreeSet::from([PersonId(1)]),
            },
        )
        .unwrap();
    let start = defs
        .resolve_site(&site)
        .unwrap()
        .prepare_start(
            None,
            &input,
            &tool,
            &output,
            StartFacts {
                actor: PersonId(1),
                area: AreaId(1),
                capable: true,
                adult: true,
                fatigue_allows: true,
                site_access: true,
                occupied_by_other: false,
                input_usable: true,
                input_unreserved: input.quantity,
                tool_usable: true,
                rate: SkillRate::default(),
                body_rate: SkillRate::default(),
            },
        )
        .unwrap();
    assert_eq!(start.job.remaining, 1_800_000);
    start.consumption.unwrap().commit(&mut input).unwrap();
    assert_eq!(input.quantity, Quantity(9_000));
    assert_eq!(
        defs.resolve_site(&site).unwrap().recipe.output_quantity,
        Quantity(1_000)
    );
}
#[test]
fn normal_and_fixture_definitions_are_separate_and_saved_without_files() {
    let normal = Definitions::from_content(
        game_content::parse(include_str!("../../../content/workshop/temperate.toml")).unwrap(),
    )
    .unwrap();
    let fixture = Definitions::from_content(
        game_content::parse(include_str!("../../../content/workshop/fixtures.toml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        normal
            .recipe(&normal.catalog.default_recipe)
            .unwrap()
            .duration,
        SimDuration(3_600_000)
    );
    assert_eq!(
        fixture
            .recipe(&fixture.catalog.default_recipe)
            .unwrap()
            .duration,
        MINUTE
    );
    let saved = serde_json::to_vec(&normal).unwrap();
    let restored: Definitions = serde_json::from_slice(&saved).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, normal);
    let mut changed = restored;
    changed
        .catalog
        .recipes
        .get_mut(&changed.catalog.default_recipe)
        .unwrap()
        .duration = SimDuration(1);
    assert!(changed.validate().is_err());
}
#[test]
fn zero_work_unknown_defaults_and_incompatible_actual_stocks_are_rejected() {
    let mut defs = Definitions::from_content(
        game_content::parse(include_str!("../../../content/workshop/temperate.toml")).unwrap(),
    )
    .unwrap();
    let input = stock(1, Good::Food, 10_000);
    let tool = stock(2, Good::Tool, 1_000);
    let output = stock(3, Good::Tool, 0);
    assert!(
        defs.bind(
            &defs.catalog.default_recipe,
            SiteBindings {
                id: SiteId(4),
                area: AreaId(1),
                input: &input,
                tool: &tool,
                output: &output,
                access: BTreeSet::new()
            }
        )
        .is_err()
    );
    defs.catalog.default_recipe = RecipeId("missing".into());
    assert!(defs.catalog.validate().is_err());
    defs.catalog.default_recipe = RecipeId("toolmaking".into());
    defs.catalog
        .recipes
        .get_mut(&defs.catalog.default_recipe)
        .unwrap()
        .duration = SimDuration(0);
    assert!(defs.catalog.validate().is_err());
}

#[test]
fn selected_recipe_and_native_cargo_complete_without_game_assembly() {
    let definitions = Definitions::from_content(
        game_content::parse(include_str!("../../../content/workshop/fixtures.toml")).unwrap(),
    )
    .unwrap();
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    let mut store = game_ecs::Store::with_schema(schema);
    for row in [
        stock(1, Good::Timber, 10_000),
        stock(2, Good::Tool, 1_000),
        stock(3, Good::Tool, 0),
    ] {
        game_cargo::insert_inventory(&mut store, row).unwrap();
    }
    let declaration = definitions
        .bind(
            &RecipeId("toolmaking".into()),
            SiteBindings {
                id: SiteId(4),
                area: AreaId(1),
                input: game_cargo::inventory(&store, InventoryId(1)).unwrap(),
                tool: game_cargo::inventory(&store, InventoryId(2)).unwrap(),
                output: game_cargo::inventory(&store, InventoryId(3)).unwrap(),
                access: BTreeSet::from([PersonId(5)]),
            },
        )
        .unwrap();
    insert_site(&mut store, declaration).unwrap();
    let start = definitions
        .resolve_site(site(&store, SiteId(4)).unwrap())
        .unwrap()
        .prepare_start(
            None,
            game_cargo::inventory(&store, InventoryId(1)).unwrap(),
            game_cargo::inventory(&store, InventoryId(2)).unwrap(),
            game_cargo::inventory(&store, InventoryId(3)).unwrap(),
            StartFacts {
                actor: PersonId(5),
                area: AreaId(1),
                capable: true,
                adult: true,
                fatigue_allows: true,
                site_access: true,
                occupied_by_other: false,
                input_usable: true,
                input_unreserved: Quantity(10_000),
                tool_usable: true,
                rate: SkillRate::default(),
                body_rate: SkillRate::default(),
            },
        )
        .unwrap();
    start
        .consumption
        .unwrap()
        .commit(game_cargo::inventory_mut(&mut store, InventoryId(1)).unwrap())
        .unwrap();
    insert_job(&mut store, PersonId(5), start.job).unwrap();
    let progress = job(&store, PersonId(5))
        .unwrap()
        .evaluate(60_000, SkillRate::default(), true, true)
        .unwrap();
    job_mut(&mut store, PersonId(5))
        .unwrap()
        .apply_progress(progress);
    let production = job(&store, PersonId(5))
        .unwrap()
        .prepare_completion(
            definitions
                .resolve_site(site(&store, SiteId(4)).unwrap())
                .unwrap(),
            game_cargo::inventory(&store, InventoryId(3)).unwrap(),
            CompletionFacts {
                actor: PersonId(5),
                active: true,
                alive_at_start: true,
                equipped: true,
            },
        )
        .unwrap()
        .unwrap();
    production.commit_stocks(&definitions, &mut store).unwrap();
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(1))
            .unwrap()
            .quantity,
        Quantity(8_000)
    );
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(3))
            .unwrap()
            .quantity,
        Quantity(1_000)
    );
    assert!(job(&store, PersonId(5)).is_none());
    assert_eq!(
        site(&store, SiteId(4)).unwrap().last_granted,
        Some(PersonId(5))
    );
}

#[test]
fn fixture_recipe_identity_is_content_derived_and_round_trips() {
    let document =
        game_content::parse(include_str!("../../../content/workshop/fixtures.toml")).unwrap();
    let mut definitions = Definitions::from_content(document).unwrap();
    let original = definitions.content.clone();
    let recipe = Recipe {
        input_good: Good::Timber,
        input_quantity: Quantity(250),
        output_good: Good::Tool,
        output_quantity: Quantity(1_000),
        duration: SimDuration(30_000),
    };
    let id = definitions.declare_recipe(recipe.clone()).unwrap();
    assert_ne!(definitions.content, original);
    assert_eq!(definitions.declare_recipe(recipe.clone()).unwrap(), id);
    assert_eq!(definitions.recipe(&id).unwrap(), &recipe);
    definitions.validate().unwrap();
    let restored: Definitions =
        serde_json::from_str(&serde_json::to_string(&definitions).unwrap()).unwrap();
    assert_eq!(restored, definitions);
    let before = definitions.clone();
    assert!(
        definitions
            .declare_recipe(Recipe {
                input_quantity: Quantity(0),
                ..recipe
            })
            .is_err()
    );
    assert_eq!(definitions, before);
}
