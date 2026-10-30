use super::support;
use game_api::*;
use game_assembly::Simulation;
use game_content::Store;
use support::Pack;
fn named(sim: &Simulation, key: &str) -> u64 {
    sim.debug_host()
        .get::<game_openings::Opening>()
        .unwrap()
        .names[key]
}
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
        .replace("\r\n", "\n")
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
    let store = InventoryId(named(&sim, "store"));
    let plot = PlotId(named(&sim, "plot"));
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
    sim.submit(1, Command::Sow { plot, seed: store }).unwrap();
    run(&mut sim, SimTime(2000));
    sim.submit(
        2,
        Command::Harvest {
            plot,
            destination: store,
        },
    )
    .unwrap();
    run(&mut sim, SimTime(3000));
    assert_eq!(
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&store].amount(GoodId(6)),
        20
    );
    sim.submit(
        3,
        Command::Craft {
            recipe: "berry-box".into(),
            inventory: store,
        },
    )
    .unwrap();
    run(&mut sim, SimTime(4000));
    assert_eq!(
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&store].amount(GoodId(7)),
        1
    );
    // Remove the original grain so an existing Eat command must consume the new edible type.
    let grain = sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&store].amount(GoodId(1));
    sim.debug_host_mut()
        .dispatch(game_cargo::Consume {
            inventory: store,
            good: GoodId(1),
            quantity: grain,
        })
        .unwrap();
    run(&mut sim, SimTime(MINUTE.0 * 35));
    sim.submit(4, Command::Eat { inventory: store }).unwrap();
    assert_eq!(sim.observe().unwrap().body.unwrap().hunger, 0);
    assert_eq!(
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&store].amount(GoodId(6)),
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
    let store = InventoryId(named(&old, "store"));
    let plot = PlotId(named(&old, "plot"));
    old.submit(1, Command::Sow { plot, seed: store }).unwrap();
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
                plot,
                destination: store,
            },
        )
        .unwrap();
        run(s, SimTime(3000));
        assert_eq!(
            s.debug_host().get::<game_cargo::Cargo>().unwrap().0[&store].amount(GoodId(1)),
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
    let store = InventoryId(named(&sim, "store"));
    let destination = InventoryId(named(&sim, "pack"));
    let before = sim.save().unwrap();
    assert!(
        sim.submit(
            1,
            Command::Take {
                from: store,
                to: destination,
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
            from: store,
            to: destination,
            good: GoodId(2),
            quantity: 20,
            unauthorized: false,
        },
    )
    .unwrap();
    assert_eq!(
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&destination].mass,
        1000
    );
}

#[test]
fn communities_develop_social_histories_and_preserve_them_across_workers_and_saving() {
    for id in ["rivermeet", "bracken-coast", "ashridge"] {
        let mut a = Simulation::new(id, 21, 1).unwrap();
        let mut b = Simulation::load(&a.save().unwrap(), 4).unwrap();
        let initial = a.observe().unwrap();
        assert!(initial.neighbors.len() >= 4);
        let ruler = initial.person.unwrap();
        let before = a.save().unwrap();
        a.observe().unwrap();
        assert_eq!(before, a.save().unwrap());
        run(&mut a, SimTime(6 * HOUR.0));
        for at in [1, 480_001, 3_000_000, 6_000_019, 6 * HOUR.0] {
            run(&mut b, SimTime(at));
            b = Simulation::load(&b.save().unwrap(), 2).unwrap();
        }
        assert_eq!(a.save().unwrap(), b.save().unwrap(), "{id}");
        let state = a
            .debug_host()
            .get::<game_social_life::SocialLife>()
            .unwrap();
        assert!(
            state.people.values().any(|p| p.encounters > 0),
            "{id}: no social life"
        );
        assert_eq!(
            state.people[&ruler].encounters, 0,
            "NPCs cannot reserve the player"
        );
        assert!(
            a.debug_host()
                .get::<game_physiology::Bodies>()
                .unwrap()
                .people
                .iter()
                .all(|(p, b)| *p == ruler || b.alive),
            "{id}: autonomous residents must eat"
        );
    }
}
#[test]
fn public_company_is_timed_cancellable_and_visible_only_locally() {
    let mut sim = Simulation::new("rivermeet", 7, 1).unwrap();
    let view = sim.observe().unwrap();
    let other = view
        .neighbors
        .iter()
        .find(|p| p.name == "Mira Reed")
        .unwrap()
        .person;
    let result = sim.submit(1, Command::Talk { other }).unwrap();
    assert_eq!(sim.submit(1, Command::Talk { other }).unwrap(), result);
    assert_eq!(sim.observe().unwrap().action.as_deref(), Some("socialize"));
    let before = sim.save().unwrap();
    assert!(sim.submit(2, Command::Rest).is_err());
    assert_eq!(before, sim.save().unwrap());
    run(&mut sim, SimTime(120_000));
    sim.submit(2, Command::Cancel).unwrap();
    assert!(
        sim.debug_host()
            .get::<game_social_life::SocialLife>()
            .unwrap()
            .meetings
            .get(&view.person.unwrap())
            .is_none()
    );
    assert!(
        !sim.debug_host()
            .get::<game_social_life::SocialLife>()
            .unwrap()
            .meetings
            .contains_key(&other)
    );
    let names = &sim
        .debug_host()
        .get::<game_openings::Opening>()
        .unwrap()
        .names;
    let destination = AreaId(names["ridge"]);
    sim.submit(3, Command::Move { destination }).unwrap();
    assert!(sim.observe().unwrap().neighbors.is_empty());
    run(&mut sim, SimTime(420_000));
    assert!(sim.observe().unwrap().neighbors.is_empty());
    assert!(sim.submit(4, Command::Talk { other }).is_err());
}
#[test]
fn new_craft_chains_transform_real_supplies_into_food() {
    let mut sim = Simulation::new("rivermeet", 9, 1).unwrap();
    let inventory = InventoryId(
        sim.debug_host()
            .get::<game_openings::Opening>()
            .unwrap()
            .names["green-store"],
    );
    let objects = sim.debug_host().get::<game_objects::Objects>().unwrap();
    let flour = objects.resolve("flour").unwrap();
    let bread = objects.resolve("bread").unwrap();
    sim.submit(
        1,
        Command::Craft {
            recipe: "mill-grain".into(),
            inventory,
        },
    )
    .unwrap();
    run(&mut sim, SimTime(10 * MINUTE.0));
    assert_eq!(
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(flour),
        16
    );
    sim.submit(
        2,
        Command::Craft {
            recipe: "bake-bread".into(),
            inventory,
        },
    )
    .unwrap();
    let save = sim.save().unwrap();
    let mut loaded = Simulation::load(&save, 3).unwrap();
    for s in [&mut sim, &mut loaded] {
        run(s, SimTime(30 * MINUTE.0));
        let stock = &s.debug_host().get::<game_cargo::Cargo>().unwrap().0[&inventory];
        assert_eq!(stock.amount(flour), 4);
        assert_eq!(stock.amount(bread), 12);
    }
    assert_eq!(sim.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn default_history_develops_livelihoods_interests_and_unprescribed_social_arrangements() {
    assert_eq!(
        game_assembly::default_scenario(&mut Store::standard()).unwrap(),
        "generated"
    );
    let mut sim = Simulation::new("generated", 1, 1).unwrap();
    let host = sim.debug_host();
    let opening = host.get::<game_openings::Opening>().unwrap();
    let history = opening.generated.as_ref().unwrap();
    assert_eq!(host.now(), history.until);
    assert_eq!(
        history.until.0,
        game_worldgen::EPOCH.0 + history.settings.history.0
    );
    let initial = opening.definition.as_ref().unwrap();
    assert!(initial.institutions.is_empty());
    assert!(initial.bonds.is_empty());
    let institutions = host.get::<game_institutions::Institutions>().unwrap();
    assert!(
        host.get::<game_livelihoods::Livelihoods>()
            .unwrap()
            .achieved
            .values()
            .any(|n| *n > 0)
    );
    assert!(
        !host
            .get::<game_horticulture::Gardens>()
            .unwrap()
            .plots
            .is_empty()
    );
    // Leisure is optional: this short history may be occupied by other chosen work.
    assert!(host.get::<game_pastimes::Pastimes>().unwrap().enabled);
    assert!(
        host.get::<game_social_life::SocialLife>()
            .unwrap()
            .people
            .values()
            .any(|p| p.encounters > 0)
    );
    for (id, institution) in &institutions.groups {
        assert!(institution.name.ends_with("Hearth") || institution.name.ends_with("Company"));
        assert!(institution.offices.is_empty());
        // Recorded practices need not remain current permission: custodians can withdraw consent.
        // Departures retire the departing person's old practices, preserving the historical group.
        for rule in &institution.rules {
            for person in &rule.users {
                assert_eq!(
                    host.get::<game_communities::Communities>().unwrap().members[person],
                    *id
                );
            }
        }
    }
    let next = SimTime(history.until.0 + HOUR.0);
    let before = sim.save().unwrap();
    let mut loaded = Simulation::load(&before, 4).unwrap();
    run(&mut sim, next);
    run(&mut loaded, next);
    assert_eq!(sim.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn history_uses_the_same_live_rules_with_zero_duration_and_worker_variations() {
    let pack = Pack::new();
    pack.edit(
        "worldgen/default.toml",
        "history = 172800000",
        "history = 0",
    );
    let empty = Simulation::with_content("generated", 1, 1, &mut Store::new(&pack.root)).unwrap();
    assert_eq!(empty.now(), game_worldgen::EPOCH);
    assert!(
        empty
            .debug_host()
            .get::<game_institutions::Institutions>()
            .unwrap()
            .groups
            .is_empty()
    );
    assert!(
        empty
            .debug_host()
            .get::<game_relationships::Social>()
            .unwrap()
            .relationships
            .is_empty()
    );
    let a = Simulation::new("generated", 13, 1).unwrap();
    let b = Simulation::new("generated", 13, 4).unwrap();
    assert_eq!(a.save().unwrap(), b.save().unwrap());
    let c = Simulation::new("generated", 14, 1).unwrap();
    assert_ne!(
        a.debug_host()
            .get::<game_openings::Opening>()
            .unwrap()
            .definition,
        c.debug_host()
            .get::<game_openings::Opening>()
            .unwrap()
            .definition
    );
}

#[test]
fn older_save_manifests_gain_only_empty_new_owners() {
    let sim = game_assembly::Simulation::new("kernel-fixture", 7, 1).unwrap();
    let mut old: serde_json::Value = serde_json::from_str(&sim.save().unwrap()).unwrap();
    old["plugins"]
        .as_object_mut()
        .unwrap()
        .remove("communication");
    old["plugins"].as_object_mut().unwrap().remove("discourse");
    old["resources"]
        .as_object_mut()
        .unwrap()
        .remove("communication.experience");
    let loaded = game_assembly::Simulation::load(&old.to_string(), 4).unwrap();
    assert_eq!(
        loaded
            .debug_host()
            .get::<game_communication::Communication>()
            .unwrap(),
        &Default::default()
    );
    assert_eq!(loaded.observe().unwrap(), sim.observe().unwrap());
    let save = loaded.save().unwrap();
    assert_eq!(
        game_assembly::Simulation::load(&save, 1)
            .unwrap()
            .save()
            .unwrap(),
        save
    );
}
