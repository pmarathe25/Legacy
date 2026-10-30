use super::support;
use game_api::*;
use game_assembly::Simulation;
use game_content::Store;
use support::Pack;
fn run(s: &mut Simulation, at: SimTime) {
    while s.advance(at, 128).unwrap().stop == game_sim::Stop::Budget {}
}
fn await_gather(s: &mut Simulation, person: PersonId) -> SimTime {
    for _ in 0..360 {
        let at = s.debug_host().now().after(MINUTE).unwrap();
        run(s, at);
        if s.debug_host()
            .get::<game_ecology::Ecology>()
            .unwrap()
            .gathering
            .contains_key(&person)
        {
            return s.debug_host().get::<game_sim::Actions>().unwrap().0[&person]
                .until
                .unwrap();
        }
    }
    panic!("authored forager did not choose to gather within six hours");
}
#[test]
fn an_authored_forager_survives_from_real_patch_output_across_saving() {
    let mut original = Simulation::new("foraging-fixture", 73, 1).unwrap();
    let names = &original
        .debug_host()
        .get::<game_openings::Opening>()
        .unwrap()
        .names;
    let forager = PersonId(names["forager"]);
    let pack = InventoryId(names["foraging-pack"]);
    let patch = names["food-patch"];
    await_gather(&mut original, forager);
    let split = original.debug_host().now().0;
    let mut loaded = Simulation::load(&original.save().unwrap(), 4).unwrap();
    run(&mut original, SimTime(DAY.0 * 2));
    for time in [
        split + MINUTE.0,
        split + 17 * MINUTE.0 + 13,
        DAY.0,
        DAY.0 * 2,
    ] {
        run(&mut loaded, SimTime(time));
    }
    assert_eq!(original.save().unwrap(), loaded.save().unwrap());
    let host = original.debug_host();
    let body = &host.get::<game_physiology::Bodies>().unwrap().people[&forager];
    assert!(body.alive);
    assert_eq!(body.health, 100);
    assert!(body.hunger < 120);
    assert!(host.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock < 1000);
    let carried = host.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(GoodId(1));
    let depleted = 1000 - host.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock;
    // A spare meal is a valid choice; some real patch output must have been consumed.
    assert!(carried < depleted);

    assert!(host.get::<game_skills::Skills>().unwrap().levels[&forager]["forage"] > 0);
    assert_eq!(
        host.get::<game_characters::Minds>().unwrap().people[&forager].failures,
        0
    );
}
#[test]
fn editing_a_rule_profile_changes_new_decisions_and_existing_saves_keep_the_old_rules() {
    let pack = Pack::new();
    let mut old =
        Simulation::with_content("survival-fixture", 1, 1, &mut Store::new(&pack.root)).unwrap();
    let citizen = PersonId(
        old.debug_host()
            .get::<game_openings::Opening>()
            .unwrap()
            .names["citizen-1"],
    );
    run(&mut old, SimTime(11 * MINUTE.0));
    assert_eq!(
        old.debug_host().get::<game_sim::Actions>().unwrap().0[&citizen].kind,
        "rest"
    );
    let save = old.save().unwrap();
    pack.edit(
        "characters/fixture.toml",
        "rest_fatigue = 10",
        "rest_fatigue = 1000",
    );
    let mut fresh =
        Simulation::with_content("survival-fixture", 1, 1, &mut Store::new(&pack.root)).unwrap();
    run(&mut fresh, SimTime(11 * MINUTE.0));
    assert!(
        !fresh
            .debug_host()
            .get::<game_sim::Actions>()
            .unwrap()
            .0
            .contains_key(&citizen)
    );
    assert_eq!(
        fresh
            .debug_host()
            .get::<game_characters::Minds>()
            .unwrap()
            .rules
            .rest_fatigue,
        1000
    );
    drop(pack);
    let mut loaded = Simulation::load(&save, 4).unwrap();
    run(&mut old, SimTime(90 * MINUTE.0));
    run(&mut loaded, SimTime(90 * MINUTE.0));
    assert_eq!(old.save().unwrap(), loaded.save().unwrap());
    assert_eq!(
        loaded
            .debug_host()
            .get::<game_characters::Minds>()
            .unwrap()
            .rules
            .rest_fatigue,
        10
    );
}
#[test]
fn public_observation_and_speech_require_local_physical_access() {
    let mut sim = Simulation::new("kernel-fixture", 1, 1).unwrap();
    let names = &sim
        .debug_host()
        .get::<game_openings::Opening>()
        .unwrap()
        .names;
    let citizen = PersonId(names["citizen-1"]);
    let ruler = sim.observe().unwrap().person.unwrap();
    let forest = AreaId(names["forest"]);
    let patch = names["patch"];
    let before = sim.save().unwrap();
    assert!(sim.submit(1, Command::ObservePatch { patch }).is_err());
    assert_eq!(sim.save().unwrap(), before);
    sim.submit(
        1,
        Command::Move {
            destination: forest,
        },
    )
    .unwrap();
    run(&mut sim, SimTime(3 * MINUTE.0));
    sim.submit(2, Command::ObserveSurroundings).unwrap();
    assert!(
        sim.observe()
            .unwrap()
            .facts
            .contains_key(game_knowledge::patch_key(patch).as_str())
    );
    assert!(
        !game_knowledge::view(sim.debug_host(), citizen)
            .unwrap()
            .contains_key(game_knowledge::patch_key(patch).as_str())
    );
    let before = sim.save().unwrap();
    assert!(
        sim.submit(
            3,
            Command::Tell {
                recipient: citizen,
                key: game_knowledge::patch_key(patch)
            }
        )
        .is_err()
    );
    assert_eq!(sim.save().unwrap(), before);
    sim.submit(
        3,
        Command::Move {
            destination: AreaId(1),
        },
    )
    .unwrap();
    run(&mut sim, SimTime(6 * MINUTE.0));
    sim.submit(
        4,
        Command::Tell {
            recipient: citizen,
            key: game_knowledge::patch_key(patch),
        },
    )
    .unwrap();
    let now = sim.observe().unwrap().time;
    run(&mut sim, now);
    let fact = &game_knowledge::view(sim.debug_host(), citizen)
        .unwrap()
        .get(game_knowledge::patch_key(patch).as_str())
        .unwrap()
        .to_owned();
    assert_eq!(fact.source, ruler);
    assert_eq!(fact.observed, SimTime(3 * MINUTE.0));
    assert_eq!(fact.confidence, 90);
}

#[test]
fn autonomous_food_plans_use_new_authored_edibles_and_their_actual_mass() {
    let pack = Pack::new();
    pack.write(
        "objects/berries.toml",
        r#"id="objects/berries"
version=1
[data.objects.berries]
id=6
name="Berries"
mass=50
food={portion=2,relief=80}
"#,
    );
    pack.edit(
        "scenarios/foraging-fixture.toml",
        "object = \"grain\"",
        "object = \"berries\"",
    );
    let mut sim =
        Simulation::with_content("foraging-fixture", 1, 1, &mut Store::new(&pack.root)).unwrap();
    let names = &sim
        .debug_host()
        .get::<game_openings::Opening>()
        .unwrap()
        .names;
    let forager = PersonId(names["forager"]);
    let container = InventoryId(names["foraging-pack"]);
    let patch = names["food-patch"];
    let finish = await_gather(&mut sim, forager);
    assert_eq!(
        sim.debug_host()
            .get::<game_ecology::Ecology>()
            .unwrap()
            .gathering[&forager]
            .quantity,
        2
    );
    assert_eq!(
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&container]
            .reserved
            .values()
            .sum::<u64>(),
        100
    );
    run(&mut sim, finish);
    assert_eq!(
        sim.debug_host()
            .get::<game_ecology::Ecology>()
            .unwrap()
            .patches[&patch]
            .stock,
        998
    );
    assert_eq!(
        sim.debug_host().get::<game_cargo::Cargo>().unwrap().0[&container].mass,
        0
    );
    assert_eq!(
        sim.debug_host()
            .get::<game_physiology::Bodies>()
            .unwrap()
            .people[&forager]
            .hunger,
        0
    );
}
