use game_api::{Good, Quantity, SimDuration};
use game_assembly::{Execution, Simulation, scenarios::Config};
use game_content::{DefinitionSource, Document};

fn authored() -> Document<game_openings::Catalog> {
    game_content::parse(include_str!("../../../content/openings/temperate.toml")).unwrap()
}
fn config(document: Document<game_openings::Catalog>) -> Config {
    Config {
        openings: DefinitionSource::Inline { document },
        ..Config::default()
    }
}
#[test]
fn added_opening_id_and_edited_stock_name_are_actual_normal_play() {
    let mut document = authored();
    let mut opening = document.data.openings["settlement-founder"].clone();
    opening.label = "New ford founder".into();
    document.data.openings.insert("new-ford".into(), opening);
    let settlement = document.data.settlements.get_mut("ordinary").unwrap();
    settlement.areas[0].name = "New ford".into();
    settlement
        .stocks
        .iter_mut()
        .find(|stock| stock.key == "timber")
        .unwrap()
        .quantity = Quantity(27_000);
    let sim = Simulation::playable_scenario("new-ford", 21, Execution::default(), config(document))
        .unwrap();
    let view = sim.canonical_state();
    assert_eq!(view.state.scenario.id, "new-ford");
    assert!(!view.state.scenario.debug_fixture);
    assert_eq!(view.state.areas[0].name, "New ford");
    assert!(
        game_cargo::inventories(view.components)
            .any(|(_, stock)| stock.good == Good::Timber && stock.quantity == Quantity(27_000))
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}
#[test]
fn all_three_openings_and_populated_startup_retain_resolved_definitions() {
    for id in ["settlement-founder", "established-ruler", "child-successor"] {
        let sim = Simulation::playable_scenario(id, 22, Execution::default(), config(authored()))
            .unwrap();
        let state = sim.canonical_state();
        state.state.openings.as_ref().unwrap().validate().unwrap();
        state.state.workshops.validate().unwrap();
        for (_, site) in game_workshop::sites(state.components) {
            assert!(state.state.workshops.resolve_site(site).is_ok());
        }
        assert_eq!(
            state,
            Simulation::load(&sim.save().unwrap(), Execution::default())
                .unwrap()
                .canonical_state()
        );
    }
    let sim = Simulation::populated_world(
        22,
        Execution::default(),
        config(authored()),
        game_openings::PopulationPlan {
            people: 30,
            settlement_size: 15,
        },
    )
    .unwrap();
    assert_eq!(
        sim.canonical_state()
            .state
            .openings
            .as_ref()
            .unwrap()
            .catalog
            .settlements["populated"]
            .areas[0]
            .name,
        "Reach{n} settlement"
    );
}
#[test]
fn save_resume_does_not_read_deleted_authored_source_files() {
    let dir = std::env::temp_dir().join(format!("legacy-authored-content-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("openings.toml");
    std::fs::write(
        &path,
        include_str!("../../../content/openings/temperate.toml"),
    )
    .unwrap();
    let mut sim = Simulation::playable_scenario(
        "settlement-founder",
        23,
        Execution::default(),
        Config {
            openings: DefinitionSource::File { path: path.clone() },
            ..Config::default()
        },
    )
    .unwrap();
    let save = sim.save().unwrap();
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(dir).unwrap();
    let mut restored = Simulation::load(
        &save,
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    sim.advance(SimDuration(60_000)).unwrap();
    restored.advance(SimDuration(60_000)).unwrap();
    assert_eq!(sim.canonical_state(), restored.canonical_state());
}

#[test]
fn edited_recipe_controls_normal_work_and_is_saved_with_the_selected_site() {
    let mut recipes: Document<game_workshop::Catalog> =
        game_content::parse(include_str!("../../../content/workshop/temperate.toml")).unwrap();
    let recipe_id = recipes.data.default_recipe.clone();
    let recipe = recipes.data.recipes.get_mut(&recipe_id).unwrap();
    recipe.input_quantity = Quantity(750);
    recipe.duration = SimDuration(30_000);
    let opening_definitions = authored();
    let craftsperson = opening_definitions.data.settlements["ordinary"].workshops[0].craftsperson;
    let mut sim = Simulation::playable_scenario(
        "established-ruler",
        25,
        Execution::default(),
        Config {
            openings: DefinitionSource::Inline {
                document: opening_definitions,
            },
            workshops: DefinitionSource::Inline { document: recipes },
            ..Config::default()
        },
    )
    .unwrap();
    let (site, actor, input, output) = {
        let state = sim.canonical_state();
        let (_, site) = game_workshop::sites(state.components).next().unwrap();
        assert_eq!(site.recipe, recipe_id);
        let actor = state.state.people[craftsperson].id;
        let tool = game_cargo::inventory(state.components, site.tool).unwrap();
        assert!(
            site.access.contains(&actor),
            "authored craftsperson has site access"
        );
        assert!(
            tool.access.contains(&actor),
            "authored craftsperson has physical tool access"
        );
        assert!(
            tool.quantity.0 >= 1_000,
            "production has a real usable tool"
        );
        assert_eq!(
            game_spatial::resolve(state.components, game_ecs::StableId(actor.0)).unwrap(),
            game_api::Location::AtArea(site.area)
        );
        (site.id, actor, site.input, site.output)
    };
    // Startup may have begun this person's work. Measure a fresh batch, rather
    // than resuming its already consumed inputs.
    sim.debug_actor_command(actor, game_api::Command::Cancel)
        .unwrap();
    assert!(game_workshop::job(sim.canonical_state().components, actor).is_none());
    let before = game_cargo::inventory(sim.canonical_state().components, input)
        .unwrap()
        .quantity;
    sim.debug_actor_command(actor, game_api::Command::Work { site })
        .unwrap();
    let state = sim.canonical_state();
    assert_eq!(
        game_workshop::job(state.components, actor)
            .unwrap()
            .remaining,
        30_000
    );
    assert_eq!(
        game_cargo::inventory(state.components, input)
            .unwrap()
            .quantity
            .0,
        before.0 - 750
    );
    let saved = sim.save().unwrap();
    let mut restored = Simulation::load(
        &saved,
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    assert_eq!(state, restored.canonical_state());
    assert_eq!(
        restored
            .canonical_state()
            .state
            .workshops
            .recipe(&recipe_id)
            .unwrap()
            .input_quantity,
        Quantity(750)
    );
    let job = game_workshop::job(state.components, actor).unwrap();
    let invocation = state.state.invocation.as_ref().unwrap();
    assert!(
        invocation
            .control
            .active(state.components)
            .is_none_or(|active| active.actor != actor)
    );
    assert!(
        game_invocation::burden(state.components, actor)
            .is_none_or(|person| person.temporary_identity == 0 && person.permanent_identity == 0)
    );
    let rate = game_invocation::compose_work_rate(job.rate, job.body_rate, 0, 0).unwrap();
    let elapsed = SimDuration(job.completion_after(rate).unwrap());
    let output_before = game_cargo::inventory(state.components, output)
        .unwrap()
        .quantity;
    let first_new_event = state.next_event;
    sim.advance(elapsed).unwrap();
    restored.advance(elapsed).unwrap();
    let finished = sim.canonical_state();
    assert_eq!(
        game_cargo::inventory(finished.components, input)
            .unwrap()
            .quantity
            .0,
        before.0 - 750
    );
    assert_eq!(
        game_cargo::inventory(finished.components, output)
            .unwrap()
            .quantity
            .0,
        output_before.0 + 1_000
    );
    assert_eq!(finished.events.iter().filter(|event| event.sequence >= first_new_event &&
        matches!(event.event, game_api::EventKind::Produced { site: produced, actor: worker, .. } if produced == site && worker == actor)).count(), 1);
    assert_eq!(finished, restored.canonical_state());
}

#[test]
fn added_workshop_binding_creates_a_real_site_and_preserves_chronological_history() {
    let mut document = authored();
    let settlement = document.data.settlements.get_mut("ordinary").unwrap();
    let mut workshop = settlement.workshops[0].clone();
    workshop.key = "second-toolmaking".into();
    workshop.historical_batches = 1;
    settlement.workshops.push(workshop);
    let sim = Simulation::playable_scenario(
        "established-ruler",
        27,
        Execution::default(),
        config(document),
    )
    .unwrap();
    assert_eq!(
        game_workshop::sites(sim.canonical_state().components).count(),
        2
    );
    assert!(
        sim.canonical_state()
            .events
            .windows(2)
            .all(|pair| pair[0].at <= pair[1].at)
    );
    let restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    assert_eq!(sim.canonical_state(), restored.canonical_state());
}
