mod support;
use game_api::*;
use game_assembly::Simulation;
use game_content::Store;
use support::Pack;
fn run(sim: &mut Simulation, time: SimTime) {
    loop {
        let p = sim.advance(time, 128).unwrap();
        if p.stop == game_sim::Stop::Target {
            break;
        }
        assert_eq!(p.stop, game_sim::Stop::Budget);
    }
}
#[test]
fn files_add_scenarios_objects_traits_crops_and_recipes_without_a_rebuild() {
    let pack = Pack::new();
    pack.write(
        "objects/berries.toml",
        r#"id="objects/berries"
version=1
[data.objects.berries]
id=6
name="Berries"
mass=2
food={portion=2,relief=50}
[data.objects.berry-box]
id=7
name="Berry box"
mass=3
tags=["container"]
"#,
    );
    pack.write(
        "traits/curiosity.toml",
        r#"id="traits/curiosity"
version=1
[data.traits.curiosity]
name="Curiosity"
minimum=0
maximum=100
default=0
weights={goal=100}
"#,
    );
    pack.write(
        "agriculture/berries.toml",
        r#"id="agriculture/berries"
version=1
[data.crops.berry-bush]
name="Berry bush"
seed_object="grain-seed"
seed_input=5
output_object="berries"
yield_units=20
growth=1000
sow_labor=1000
harvest_labor=1000
minimum_warmth=1
minimum_rain=0
"#,
    );
    pack.write(
        "workshop/berry-box.toml",
        r#"id="workshop/berry-box"
version=1
[data.recipes.berry-box]
inputs={timber=3}
output="berry-box"
quantity=1
duration=1000
skill="craft"
"#,
    );
    let scenario = std::fs::read_to_string(pack.root.join("scenarios/kernel-fixture.toml"))
        .unwrap()
        .replace("id = \"kernel-fixture\"", "id = \"harbor\"")
        .replace("fixture = true", "fixture = false")
        .replace("Kernel fixture", "Harbor")
        .replace("name = \"Village\"", "name = \"Harbor\"")
        .replace("crop = \"grain\"", "crop = \"berry-bush\"")
        .replace(
            "key = \"ruler\"\ncount = 1",
            "key = \"ruler\"\ntraits = {curiosity = 50}\ncount = 1",
        );
    pack.write("scenarios/harbor.toml", &scenario);
    let mut store = Store::new(&pack.root);
    let menu = game_assembly::scenarios_with_content(&mut store).unwrap();
    assert!(menu.iter().any(|s| s.id == "harbor" && !s.fixture));
    let mut sim = Simulation::with_content("harbor", 9, 1, &mut store).unwrap();
    let ruler = sim.observe().unwrap().person.unwrap();
    let h = sim.debug_host();
    assert_eq!(
        h.get::<game_land::Land>().unwrap().0[&AreaId(1)].name,
        "Harbor"
    );
    assert_eq!(
        h.get::<game_traits::Traits>().unwrap().people[&ruler]["curiosity"],
        50
    );
    assert_eq!(
        h.get::<game_traits::Traits>().unwrap().weights(ruler)["goal"],
        150
    );
    sim.submit(
        1,
        Command::Sow {
            plot: PlotId(21),
            seed: InventoryId(19),
        },
    )
    .unwrap();
    run(&mut sim, SimTime(2000));
    sim.submit(
        2,
        Command::Harvest {
            plot: PlotId(21),
            destination: InventoryId(19),
        },
    )
    .unwrap();
    run(&mut sim, SimTime(3000));
    assert_eq!(
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&InventoryId(19)].amount(GoodId(6)),
        20
    );
    sim.submit(
        3,
        Command::Craft {
            recipe: "berry-box".into(),
            inventory: InventoryId(19),
        },
    )
    .unwrap();
    run(&mut sim, SimTime(4000));
    assert_eq!(
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&InventoryId(19)].amount(GoodId(7)),
        1
    );
    // Remove the original grain so an existing Eat command must consume the new edible type.
    let grain =
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&InventoryId(19)].amount(GoodId(1));
    sim.debug_host_mut()
        .dispatch(game_cargo::Consume {
            inventory: InventoryId(19),
            good: GoodId(1),
            quantity: grain,
        })
        .unwrap();
    run(&mut sim, SimTime(MINUTE.0 * 35));
    sim.submit(
        4,
        Command::Eat {
            inventory: InventoryId(19),
        },
    )
    .unwrap();
    assert_eq!(sim.observe().unwrap().body.unwrap().hunger, 0);
    assert_eq!(
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&InventoryId(19)].amount(GoodId(6)),
        18
    );
    let mut random_store = Store::new(&pack.root);
    assert_eq!(
        Simulation::with_content("random", 9, 1, &mut random_store)
            .unwrap()
            .debug_host()
            .get::<game_openings::Opening>()
            .unwrap()
            .scenario,
        "harbor"
    );
}
#[test]
fn resolved_content_is_saved_and_reloading_needs_no_files() {
    let pack = Pack::new();
    pack.edit(
        "agriculture/crops.toml",
        "growth = 86400000",
        "growth = 1000",
    );
    pack.edit(
        "agriculture/crops.toml",
        "sow_labor = 3600000",
        "sow_labor = 1000",
    );
    pack.edit(
        "agriculture/crops.toml",
        "harvest_labor = 3600000",
        "harvest_labor = 1000",
    );
    let mut old =
        Simulation::with_content("kernel-fixture", 1, 1, &mut Store::new(&pack.root)).unwrap();
    old.submit(
        1,
        Command::Sow {
            plot: PlotId(21),
            seed: InventoryId(19),
        },
    )
    .unwrap();
    run(&mut old, SimTime(500));
    let save = old.save().unwrap();
    pack.edit(
        "agriculture/crops.toml",
        "yield_units = 100",
        "yield_units = 777",
    );
    let fresh =
        Simulation::with_content("kernel-fixture", 1, 1, &mut Store::new(&pack.root)).unwrap();
    assert_eq!(
        fresh
            .debug_host()
            .get::<game_agriculture::Crops>()
            .unwrap()
            .items["grain"]
            .yield_units,
        777
    );
    assert_eq!(
        old.debug_host()
            .get::<game_agriculture::Crops>()
            .unwrap()
            .items["grain"]
            .yield_units,
        100
    );
    drop(pack);
    let mut loaded = Simulation::load(&save, 4).unwrap();
    for s in [&mut old, &mut loaded] {
        run(s, SimTime(2000));
        s.submit(
            2,
            Command::Harvest {
                plot: PlotId(21),
                destination: InventoryId(19),
            },
        )
        .unwrap();
        run(s, SimTime(3000));
        assert_eq!(
            s.debug_host().get::<game_cargo::Cargo>().unwrap().0[&InventoryId(19)]
                .amount(GoodId(1)),
            15_100
        );
    }
    assert_eq!(old.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn invalid_content_references_fail_before_world_mutation() {
    let pack = Pack::new();
    pack.edit(
        "agriculture/crops.toml",
        "output_object = \"grain\"",
        "output_object = \"missing-object\"",
    );
    assert!(Simulation::with_content("kernel-fixture", 1, 1, &mut Store::new(&pack.root)).is_err());
    pack.edit(
        "agriculture/crops.toml",
        "output_object = \"missing-object\"",
        "output_object = \"grain\"",
    );
    pack.edit(
        "scenarios/kernel-fixture.toml",
        "steward = \"ruler\"",
        "steward = \"unknown-person\"",
    );
    assert!(Simulation::with_content("kernel-fixture", 1, 1, &mut Store::new(&pack.root)).is_err());
}
#[test]
fn authored_mass_changes_carrying_limits() {
    let pack = Pack::new();
    pack.edit(
        "objects/base.toml",
        "name = \"Timber\"\nmass = 1",
        "name = \"Timber\"\nmass = 50",
    );
    let mut sim =
        Simulation::with_content("kernel-fixture", 1, 1, &mut Store::new(&pack.root)).unwrap();
    let before = sim.save().unwrap();
    assert!(
        sim.submit(
            1,
            Command::Take {
                from: InventoryId(19),
                to: InventoryId(20),
                good: GoodId(2),
                quantity: 21,
                unauthorized: false
            }
        )
        .is_err()
    );
    assert_eq!(before, sim.save().unwrap());
    sim.submit(
        1,
        Command::Take {
            from: InventoryId(19),
            to: InventoryId(20),
            good: GoodId(2),
            quantity: 20,
            unauthorized: false,
        },
    )
    .unwrap();
    assert_eq!(
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&InventoryId(20)].mass,
        1000
    );
}
