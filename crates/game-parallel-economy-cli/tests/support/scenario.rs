use super::{load, save, scenario};
use std::path::PathBuf;

#[test]
fn wildlife_receives_the_current_seasonal_exposure() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut state = scenario(
        &root.join("content/parallel/economy-wildlife.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    state.fauna.as_mut().unwrap().species[0].climate = Some(game_parallel_fauna::Climate {
        tolerance: 0,
        inherited_tolerance: 0,
        hardiness_locus: 1,
        damage_per_excess: 1,
    });
    let environment = state.environment.as_mut().unwrap();
    environment.seasons[0].duration = 1;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(state.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].health,
                    9
                );
                let mut restored = game_parallel_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].health,
                    7
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn animal_care_content_resolves_named_goods() {
    let config: game_parallel_economy::AnimalCareConfig = toml::from_str(
        r#"
        [[policies]]
        person = 4
        animal = 1
        good = "grain"
        quantity = 2
        willing = false
        health_at_most = 8
        pressured_by = 1
        pressure_threshold = 3
    "#,
    )
    .unwrap();
    let care = config.build(|name| (name == "grain").then_some(7)).unwrap();
    assert_eq!(care.policies[0].good, 7);
    assert_eq!(care.policies[0].pressure_threshold, Some(3));
}

#[test]
fn hunting_preferences_accept_optional_individual_targets() {
    let hunting: game_parallel_economy::Hunting = toml::from_str(
        r#"
        [[policies]]
        person = 1
        willing = true
        [[policies]]
        person = 2
        animal = 9
        willing = false
    "#,
    )
    .unwrap();
    assert_eq!(hunting.policies[0].animal, None);
    assert_eq!(hunting.policies[1].animal, Some(9));
}

#[test]
fn hunting_preferences_resolve_named_reserves() {
    let config: game_parallel_economy::HuntingConfig = toml::from_str(
        r#"
        [[policies]]
        person = 1
        willing = true
        hunger_at_least = 5
        reserve_good = "meat"
        reserve_below = 3
    "#,
    )
    .unwrap();
    let hunting = config.build(|name| (name == "meat").then_some(7)).unwrap();
    assert_eq!(hunting.policies[0].reserve_good, Some(7));
    assert_eq!(hunting.policies[0].reserve_below, Some(3));
}

#[test]
fn wildlife_climate_settings_are_loaded_from_content() {
    let text = r#"
        next_id = 1
        animals = []
        [[species]]
        id = 1
        name = "grazer"
        food_good = "grain"
        intake = 1
        hunger_per_tick = 1
        relief_per_unit = 1
        starvation_threshold = 10
        starvation_damage = 1
        max_health = 10
        adult_age = 1
        fertile_until = 10
        lifespan = 20
        gestation = 2
        conception_one_in = 4
        breeding_max_hunger = 3
        newborn_health = 10
        [species.climate]
        tolerance = 1
        inherited_tolerance = 4
        hardiness_locus = 1
        damage_per_excess = 2
        [species.hunting]
        good = "grain"
        quantity = 2
        success_per_thousand = 1000
    "#;
    let config: game_parallel_fauna::content::Config = toml::from_str(text).unwrap();
    let fauna = config.build(|name| (name == "grain").then_some(1)).unwrap();
    assert_eq!(
        fauna.species[0]
            .climate
            .as_ref()
            .unwrap()
            .inherited_tolerance,
        4
    );
    assert_eq!(fauna.species[0].hunting.as_ref().unwrap().good, 1);
}

#[test]
fn player_hunts_a_local_animal_into_actual_possessions() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state = scenario(
        &root.join("content/parallel/economy-wildlife.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let meat = state
        .physical
        .goods
        .iter()
        .find(|good| good.name == "raw-meat")
        .unwrap()
        .id;
    assert_eq!(
        state.fauna.as_ref().unwrap().species[0]
            .hunting
            .as_ref()
            .unwrap()
            .good,
        meat
    );
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    assert!(Player::new(&mut world).hunt_animal(1).unwrap());
    let view = Player::new(&mut world).view().unwrap();
    assert!(
        view.possessions
            .iter()
            .any(|lot| lot.good == meat && lot.quantity == 2 && lot.expires_at == Some(49))
    );
    assert_eq!(
        view.animals
            .iter()
            .find(|animal| animal.id == 1)
            .unwrap()
            .health,
        0
    );
    assert!(Player::new(&mut world).hunt_animal(1).is_err());
}

#[test]
fn wildlife_scenario_hungry_person_can_hunt_for_their_own_reserve() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut state = scenario(
        &root.join("content/parallel/economy-wildlife.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let meat = state.fauna.as_ref().unwrap().species[0]
        .hunting
        .as_ref()
        .unwrap()
        .good;
    state.people[1].hunger = 12;
    state.people[1].seek_meals = false;
    if let Some(social) = state.dynamics.as_mut().and_then(|d| d.social.as_mut()) {
        for participant in &mut social.participants {
            participant.willing = false;
        }
    }
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    let carried = state
        .physical
        .custody
        .containers
        .iter()
        .find(|c| c.place == game_parallel_lots::Place::Person(2))
        .unwrap();
    assert!(
        carried
            .lots
            .iter()
            .any(|lot| lot.good == meat && lot.quantity == 2)
    );
    assert_eq!(
        state
            .fauna
            .unwrap()
            .animals
            .iter()
            .filter(|animal| animal.health == 0)
            .count(),
        1
    );
}

#[test]
fn player_feeds_local_animals_only_with_real_carried_food() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let base = scenario(
        &root.join("content/parallel/economy-wildlife.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut initial = base.snapshot();
    let patch = initial
        .gathering
        .as_mut()
        .unwrap()
        .nodes
        .iter_mut()
        .find(|node| node.id == 3)
        .unwrap();
    patch.available = 0;
    patch.regrowth = 0;
    initial.fauna.as_mut().unwrap().animals[0].hunger = 4;
    initial.fauna.as_mut().unwrap().animals[0].social_response =
        Some(game_parallel_fauna::SocialResponse {
            feed_affinity: -2,
            minimum: -10,
            maximum: 10,
            capacity: 4,
        });
    let grain = initial.fauna.as_ref().unwrap().species[0].food_good;
    let bread = initial
        .physical
        .goods
        .iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                let before = world.snapshot();
                assert!(Player::new(&mut world).feed_animal(3, grain, 1).is_err());
                assert!(Player::new(&mut world).feed_animal(1, u32::MAX, 1).is_err());
                assert_eq!(world.snapshot(), before);
                assert!(Player::new(&mut world).feed_animal(1, bread, 1).unwrap());
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].opinions[0].affinity,
                    -2
                );
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].hunger,
                    5
                );
                assert!(!Player::new(&mut world).feed_animal(1, grain, 2).unwrap());
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].opinions[0].affinity,
                    -2
                );
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].hunger,
                    6
                );
                assert!(
                    Player::new(&mut world)
                        .transfer_stock(grain, 2, true)
                        .unwrap()
                );
                let saved = world.snapshot();
                let hunger = saved.fauna.as_ref().unwrap().animals[0].hunger;
                let mut restored = game_parallel_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(Player::new(&mut world).feed_animal(1, grain, 2).unwrap());
                assert!(Player::new(&mut restored).feed_animal(1, grain, 2).unwrap());
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state.fauna.as_ref().unwrap().animals[0].opinions[0].affinity,
                    -4
                );
                assert_eq!(state.fauna.as_ref().unwrap().animals[0].hunger, hunger - 1);
                assert!(
                    !Player::new(&mut world)
                        .view()
                        .unwrap()
                        .possessions
                        .iter()
                        .any(|lot| lot.good == grain)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn authored_wildlife_uses_named_food_and_exposes_only_local_observations() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy-wildlife.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let initial = world.snapshot();
                let species = &initial.fauna.as_ref().unwrap().species[0];
                assert_eq!(
                    initial
                        .physical
                        .goods
                        .iter()
                        .find(|g| g.id == species.food_good)
                        .unwrap()
                        .name,
                    "grain"
                );
                let view = Player::new(&mut world).view().unwrap();
                assert_eq!(
                    view.animals
                        .iter()
                        .map(|animal| animal.id)
                        .collect::<Vec<_>>(),
                    vec![1, 2]
                );
                let mut altered = initial.clone();
                let fauna = altered.fauna.as_mut().unwrap();
                fauna.animals[0].genome.alleles[0] = [999, 999];
                let father = fauna.animals[1].genome.clone();
                fauna.animals[0].pregnancy = Some(game_parallel_economy::AnimalPregnancy {
                    father: 2,
                    father_genome: father,
                    due: 3,
                });
                let mut other = game_parallel_economy::Economy::compile(altered).unwrap();
                assert_eq!(
                    Player::new(&mut other).view().unwrap().animals,
                    view.animals
                );
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state
                        .gathering
                        .as_ref()
                        .unwrap()
                        .nodes
                        .iter()
                        .find(|n| n.id == 3)
                        .unwrap()
                        .available,
                    9
                );
                assert_eq!(state.fauna.as_ref().unwrap().animals[2].hunger, 2);
                let mut restored = game_parallel_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_population_is_seeded_individual_and_worker_invariant() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy-generated.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let before = world.snapshot();
                assert_eq!(before.people.len(), 129);
                assert!(before.people.iter().any(|person| person.region == Some(1)));
                assert!(before.people.iter().any(|person| person.region == Some(2)));
                assert!(before.people.iter().any(|person| !person.willing_to_work));
                assert!(before.people.iter().any(|person| person.willing_to_work));
                assert!(before.people.iter().any(|person| !person.seek_meals));
                let events = world.advance().unwrap();
                assert!(!events.started.is_empty());
                let after = world.snapshot();
                assert!(
                    after
                        .physical
                        .work
                        .iter()
                        .all(|work| before.people.iter().any(|person| {
                            person.id == work.job.worker
                                && person.willing_to_work
                                && !person.controlled
                        }))
                );
                let saved = serde_json::to_string(&after).unwrap();
                let restored =
                    game_parallel_economy::Economy::compile(serde_json::from_str(&saved).unwrap())
                        .unwrap();
                assert_eq!(after, restored.snapshot());
                after
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn campaign_loads_its_per_strain_combat_transmission_rule() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let state = world.snapshot();
    assert_eq!(
        state.dynamics.unwrap().disease.unwrap().strains[0].combat_transmission_per_million,
        500_000
    );
}

#[test]
fn seed_override_controls_both_population_and_simulation_seed() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join("content/parallel/economy-generated.toml");
    let load_seed = |seed| {
        super::scenario_with_seed(&path, &root.join("content"), seed)
            .unwrap()
            .snapshot()
    };
    let authored = load_seed(None);
    assert_eq!(authored, load_seed(Some(41)));
    let other = load_seed(Some(42));
    assert_eq!(other.seed, 42);
    assert_ne!(authored.people, other.people);
    assert_eq!(other, load_seed(Some(42)));
}

#[test]
fn generated_dispositions_and_individual_overrides_interpret_experiences() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-generated.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut opinions = world.snapshot().opinions.unwrap();
    assert_eq!(opinions.sensitivities.len(), 128);
    opinions.learn_harm(&[(2, 1, 1), (3, 1, 1), (66, 1, 1)]);
    opinions.learn_help(&[(2, 1), (66, 1)]);
    opinions.learn_promises(&[(2, 1), (66, 1)]);
    let index = opinions.index();
    assert_eq!(index.affinity(2, 1), 2);
    assert_eq!(index.affinity(3, 1), -5);
    assert_eq!(index.affinity(66, 1), -2);
    assert_eq!(index.obligation(2, 1), 0);
    assert_eq!(index.obligation(66, 1), 4);
}

#[test]
fn generated_founders_have_individual_lives_and_actual_births_survive_reload() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy-generated.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let initial = world.snapshot();
                let family = initial.family.as_ref().unwrap();
                assert_eq!(family.members.len(), 129);
                assert_eq!(family.next_id, 130);
                assert!(family.members.iter().all(|member| member.partner.is_none()));
                assert!(
                    family
                        .members
                        .iter()
                        .any(|member| !member.reproduction_allowed)
                );
                assert!(
                    family
                        .members
                        .iter()
                        .any(|member| member.reproduction_allowed)
                );
                assert!(family.members.iter().any(|member| member.age == 18));
                assert!(family.members.iter().any(|member| member.age == 25));
                for _ in 0..2 {
                    world.advance().unwrap();
                }
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_parallel_economy::Economy::compile(serde_json::from_str(&saved).unwrap())
                        .unwrap();
                for _ in 0..5 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let family = state.family.as_ref().unwrap();
                assert!(family.totals.births > 0);
                assert!(
                    family
                        .members
                        .iter()
                        .any(|member| member.id >= 130 && member.parents.len() == 2)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_population_joins_the_ruler_campaign_without_revealing_remote_people() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = super::scenario_with_options(
                    &root.join("content/parallel/economy-campaign.toml"),
                    &root.join("content"),
                    Some(42),
                    Some(&root.join("content/parallel/campaign-population.toml")),
                )
                .unwrap();
                let initial = world.snapshot();
                assert_eq!(initial.people.len(), 130);
                assert_eq!(initial.family.as_ref().unwrap().next_id, 1128);
                let work = initial
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap();
                assert_eq!(work.preferences.len(), 129);
                assert!(
                    work.preferences
                        .iter()
                        .filter(|row| row.person >= 1000)
                        .all(|row| row.trust_weight == 1)
                );
                assert!(work.askers.iter().any(|row| row.person >= 1000));
                assert!(
                    initial
                        .dreams
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|row| row.person >= 1000)
                );
                let giving = initial.giving.as_ref().unwrap();
                assert_eq!(giving.responses.len(), 129);
                assert!(
                    giving
                        .responses
                        .iter()
                        .any(|p| p.person >= 1000 && p.willing)
                );
                assert!(
                    giving
                        .responses
                        .iter()
                        .any(|p| p.person >= 1000 && !p.willing)
                );
                assert!(giving.learning.iter().any(|p| p.person >= 1000));
                assert!(
                    giving
                        .learning
                        .iter()
                        .filter(|p| p.person >= 1000)
                        .all(|p| p.experiences == 0 && p.last_helper.is_none())
                );
                let farming = &initial.agriculture.as_ref().unwrap().policies;
                let search = &initial.food_search.as_ref().unwrap().people;
                assert_eq!(farming.len(), 129);
                assert_eq!(search.len(), 129);
                assert!(farming.iter().any(|p| p.person >= 1000 && p.willing));
                assert!(farming.iter().any(|p| p.person >= 1000 && !p.willing));
                assert!(search.iter().any(|p| p.person >= 1000 && p.enabled));
                assert!(search.iter().any(|p| p.person >= 1000 && !p.enabled));
                assert!(
                    search
                        .iter()
                        .filter(|p| p.person >= 1000)
                        .all(|p| p.target.is_none())
                );
                let dynamics = initial.dynamics.as_ref().unwrap();
                let social = dynamics.social.as_ref().unwrap();
                let claims = dynamics.claims.as_ref().unwrap();
                assert_eq!(claims.preferences.len(), 130);
                assert!(claims.claims.is_empty() && claims.site_claims.is_empty());
                assert!(
                    claims
                        .preferences
                        .iter()
                        .any(|p| p.person >= 1000 && p.assert_outputs)
                );
                assert!(
                    claims
                        .preferences
                        .iter()
                        .any(|p| p.person >= 1000 && !p.assert_outputs)
                );
                assert!(
                    claims
                        .preferences
                        .iter()
                        .any(|p| p.person >= 1000 && p.disregard_others)
                );
                assert!(
                    claims
                        .preferences
                        .iter()
                        .any(|p| p.person >= 1000 && !p.disregard_others)
                );
                assert_eq!(social.participants.len(), 130);
                assert!(
                    social
                        .participants
                        .iter()
                        .any(|p| p.person >= 1000 && p.willing)
                );
                assert!(
                    social
                        .participants
                        .iter()
                        .any(|p| p.person >= 1000 && !p.willing)
                );
                assert!(social.pressures.iter().any(|p| p.person >= 1000));
                assert!(social.relationships.is_empty());
                assert_eq!(dynamics.personality.as_ref().unwrap().people.len(), 130);
                let remote: Vec<_> = initial
                    .people
                    .iter()
                    .filter(|p| p.region == Some(2))
                    .map(|p| p.id)
                    .collect();
                assert!(!remote.is_empty());
                world.set_farming_choice(2, false).unwrap();
                {
                    let mut player = game_parallel_economy_player::Player::new(&mut world);
                    let view = player.view().unwrap();
                    assert_eq!(view.person.unwrap().id, 1);
                    assert!(view.nearby.iter().all(|id| !remote.contains(id)));
                    player.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(
                    state.agriculture.as_ref().unwrap().fields[0].phase,
                    game_parallel_economy::CropPhase::Growing
                );
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .social
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|event| event.a >= 1000 || event.b >= 1000)
                );
                let social = state.dynamics.as_ref().unwrap().social.as_ref().unwrap();
                assert!(state.physical.work.iter().all(|job| {
                    social
                        .events
                        .iter()
                        .all(|event| ![event.a, event.b].contains(&job.job.worker))
                }));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn generated_campaign(count: u64) -> game_parallel_economy::Economy {
    static NEXT_FILE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let profile = std::fs::read_to_string(root.join("content/parallel/campaign-population.toml"))
        .unwrap()
        .replace("count = 128", &format!("count = {count}"));
    let path = std::env::temp_dir().join(format!(
        "legacy-campaign-population-{}-{count}-{}.toml",
        std::process::id(),
        NEXT_FILE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, profile).unwrap();
    let world = super::scenario_with_options(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
        Some(42),
        Some(&path),
    );
    std::fs::remove_file(path).unwrap();
    world.unwrap()
}

#[test]
fn large_campaign_commits_and_reloads_identically_across_workers() {
    let initial = generated_campaign(20_000).snapshot();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                let stale = world.prepare().unwrap();
                world.advance().unwrap();
                let committed = world.snapshot();
                assert!(world.commit(stale).is_err());
                assert_eq!(world.snapshot(), committed);
                let mut restored = game_parallel_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                for _ in 0..2 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_campaign_replays_at_scale_across_workers() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = generated_campaign(2048);
                for _ in 0..7 {
                    world.advance().unwrap();
                }
                let state = world.snapshot();
                assert!(state.family.as_ref().unwrap().totals.births > 0);
                assert!(state.family.as_ref().unwrap().members.iter().any(|member| {
                    member.born.is_some()
                        && state
                            .agriculture
                            .as_ref()
                            .unwrap()
                            .policies
                            .iter()
                            .any(|policy| policy.person == member.id)
                }));
                let claims = state.dynamics.as_ref().unwrap().claims.as_ref().unwrap();
                assert!(state.family.as_ref().unwrap().members.iter().any(|member| {
                    member.born.is_some()
                        && claims.preferences.iter().any(|p| p.person == member.id)
                }));
                let giving = state.giving.as_ref().unwrap();
                assert!(state.family.as_ref().unwrap().members.iter().any(|member| {
                    member.born.is_some() && giving.responses.iter().any(|p| p.person == member.id)
                }));
                let work = state.autowork.as_ref().unwrap().requests.as_ref().unwrap();
                assert!(state.family.as_ref().unwrap().members.iter().any(|member| {
                    member.born.is_some() && work.preferences.iter().any(|p| p.person == member.id)
                }));
                let dreams = state.dreams.as_ref().unwrap();
                assert!(state.family.as_ref().unwrap().members.iter().any(|member| {
                    member.born.is_some() && dreams.learning.iter().any(|p| p.person == member.id)
                }));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_unwilling_person_can_respond_to_fear_without_a_felt_obligation() {
    let mut initial = generated_campaign(128).snapshot();
    let dynamics = initial.dynamics.as_mut().unwrap();
    let social = dynamics.social.as_ref().unwrap();
    let target = social
        .participants
        .iter()
        .find(|p| {
            p.person >= 1000
                && !p.willing
                && social.pressures.iter().any(|rule| rule.person == p.person)
        })
        .unwrap()
        .person;
    let region = initial
        .people
        .iter()
        .find(|p| p.id == target)
        .unwrap()
        .region;
    let requester = social
        .participants
        .iter()
        .find(|p| {
            p.person >= 1000
                && p.willing
                && p.person != target
                && initial
                    .people
                    .iter()
                    .any(|person| person.id == p.person && person.region == region)
        })
        .unwrap()
        .person;
    dynamics
        .personality
        .as_mut()
        .unwrap()
        .people
        .iter_mut()
        .find(|p| p.person == requester)
        .unwrap()
        .sociability = 1000;
    for person in &mut initial.people {
        person.willing_to_work = false;
    }
    for policy in &mut initial.agriculture.as_mut().unwrap().policies {
        policy.willing = false;
    }
    initial
        .opinions
        .as_mut()
        .unwrap()
        .records
        .push(game_parallel_economy::Opinion {
            observer: target,
            target: requester,
            trust: 0,
            fear: 10,
            obligation: 0,
            affinity: 0,
        });
    let mut world = game_parallel_economy::Economy::compile(initial).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    let social = state.dynamics.as_ref().unwrap().social.as_ref().unwrap();
    assert!(social.events.iter().any(|event| event.pressured == Some(target)
        && [event.a, event.b].contains(&requester)), "target={target} requester={requester} events={:?}", social.events);
    assert_eq!(
        state
            .opinions
            .as_ref()
            .unwrap()
            .index()
            .obligation(target, requester),
        0
    );
}

#[test]
fn generated_person_asserts_a_claim_only_after_their_real_output_arrives() {
    let mut initial = generated_campaign(128).snapshot();
    let claims = initial.dynamics.as_ref().unwrap().claims.as_ref().unwrap();
    let worker = initial
        .people
        .iter()
        .find(|person| {
            person.id >= 1000
                && person.region == Some(1)
                && person.willing_to_work
                && claims
                    .preferences
                    .iter()
                    .any(|p| p.person == person.id && p.assert_outputs)
        })
        .unwrap()
        .id;
    for person in &mut initial.people {
        person.willing_to_work = person.id == worker;
    }
    for policy in &mut initial.agriculture.as_mut().unwrap().policies {
        policy.willing = false;
    }
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .personality
        .as_mut()
        .unwrap()
        .people
        .iter_mut()
        .find(|p| p.person == worker)
        .unwrap()
        .diligence = 1000;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                assert!(
                    world
                        .snapshot()
                        .dynamics
                        .unwrap()
                        .claims
                        .unwrap()
                        .claims
                        .is_empty()
                );
                for _ in 0..5 {
                    world.advance().unwrap();
                }
                let state = world.snapshot();
                let flour = state
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "flour")
                    .unwrap()
                    .id;
                let stock = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|container| {
                        container.place
                            == game_parallel_lots::Place::Stock {
                                region: 1,
                                good: flour,
                            }
                    })
                    .unwrap();
                assert!(stock.used > 0);
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .claims
                        .as_ref()
                        .unwrap()
                        .claims
                        .iter()
                        .any(|claim| claim.person == worker
                            && claim.region == 1
                            && claim.good == flour)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_person_can_give_real_food_from_fear_without_feeling_obligated() {
    let mut initial = generated_campaign(128).snapshot();
    let giving = initial.giving.as_ref().unwrap();
    let target = giving
        .responses
        .iter()
        .find(|response| {
            response.person >= 1000
                && !response.willing
                && response.pressure_threshold == Some(3)
                && initial
                    .people
                    .iter()
                    .any(|p| p.id == response.person && p.region == Some(1))
        })
        .unwrap()
        .person;
    let person = initial.people.iter_mut().find(|p| p.id == target).unwrap();
    person.willing_to_work = false;
    person.seek_meals = false;
    let requester = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == game_parallel_lots::Place::Person(1))
        .unwrap();
    *requester =
        game_parallel_lots::Container::new(requester.place, requester.capacity, vec![]).unwrap();
    initial
        .agriculture
        .as_mut()
        .unwrap()
        .policies
        .iter_mut()
        .find(|p| p.person == target)
        .unwrap()
        .willing = false;
    initial
        .opinions
        .as_mut()
        .unwrap()
        .records
        .push(game_parallel_economy::Opinion {
            observer: target,
            target: 1,
            trust: 0,
            fear: 10,
            obligation: 0,
            affinity: 0,
        });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                let before = world.snapshot();
                let bread = before
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "bread")
                    .unwrap()
                    .id;
                let amount = |state: &game_parallel_economy::State, person| -> u64 {
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == game_parallel_lots::Place::Person(person))
                        .unwrap()
                        .lots
                        .iter()
                        .filter(|lot| lot.good == bread)
                        .map(|lot| lot.quantity)
                        .sum()
                };
                let before_sender = amount(&before, target);
                let before_recipient = amount(&before, 1);
                world.ask_food(1, target, 4).unwrap();
                world.advance().unwrap();
                let after = world.snapshot();
                assert_eq!(amount(&after, target), before_sender - 1);
                assert_eq!(amount(&after, 1), before_recipient + 1);
                assert_eq!(
                    after
                        .opinions
                        .as_ref()
                        .unwrap()
                        .index()
                        .obligation(target, 1),
                    0
                );
                after
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_person_can_refuse_requested_work_or_do_it_from_fear() {
    let mut initial = generated_campaign(128).snapshot();
    let requests = initial
        .autowork
        .as_ref()
        .unwrap()
        .requests
        .as_ref()
        .unwrap();
    let worker = requests
        .preferences
        .iter()
        .find(|preference| {
            preference.person >= 1000
                && preference.help_value < preference.effort
                && initial
                    .people
                    .iter()
                    .any(|person| person.id == preference.person && person.region == Some(1))
        })
        .unwrap()
        .person;
    for person in &mut initial.people {
        person.willing_to_work = false;
        person.seek_meals = false;
    }
    for policy in &mut initial.agriculture.as_mut().unwrap().policies {
        policy.willing = false;
    }
    initial
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .askers
        .clear();
    initial.goals = None;
    initial.dreams = None;
    let run = |workers, fear| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state
                    .opinions
                    .as_mut()
                    .unwrap()
                    .records
                    .push(game_parallel_economy::Opinion {
                        observer: worker,
                        target: 1,
                        trust: 0,
                        fear,
                        obligation: 0,
                        affinity: 0,
                    });
                let recipe = state
                    .physical
                    .recipes
                    .iter()
                    .find(|recipe| recipe.name == "mill-grain")
                    .unwrap()
                    .id;
                let mut world = game_parallel_economy::Economy::compile(state).unwrap();
                let request = world.ask_work(1, worker, recipe, 4).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let event = state
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap()
                    .events
                    .iter()
                    .find(|event| event.request == request)
                    .unwrap();
                if fear == 0 {
                    assert!(matches!(
                        event.outcome,
                        game_parallel_autowork::requests::Outcome::Refused
                    ));
                    assert!(
                        state
                            .physical
                            .work
                            .iter()
                            .all(|work| work.job.worker != worker)
                    );
                } else {
                    assert!(matches!(
                        event.outcome,
                        game_parallel_autowork::requests::Outcome::Started { .. }
                    ));
                    assert!(
                        state
                            .physical
                            .work
                            .iter()
                            .any(|work| work.job.worker == worker)
                    );
                }
                assert_eq!(
                    state
                        .opinions
                        .as_ref()
                        .unwrap()
                        .index()
                        .obligation(worker, 1),
                    0
                );
                state
            })
    };
    assert_eq!(run(1, 0), run(4, 0));
    assert_eq!(run(1, 10), run(4, 10));
}

#[test]
fn generated_work_learning_is_an_individual_inclination_not_an_aspiration() {
    let initial = generated_campaign(128).snapshot();
    let dreams = initial.dreams.as_ref().unwrap();
    assert!(dreams.inherit_births);
    assert!(dreams.learning.iter().any(|row| row.person == 2));
    assert!(dreams.learning.iter().any(|row| row.person >= 1000));
    assert!(dreams.learning.iter().all(|row| row.max_aspirations == 3));
    let learner = dreams
        .learning
        .iter()
        .find(|row| {
            row.person >= 1000 && !dreams.people.iter().any(|dream| dream.person == row.person)
        })
        .unwrap()
        .person;
    let restored: game_parallel_economy::State =
        serde_json::from_str(&serde_json::to_string(&initial).unwrap()).unwrap();
    assert!(
        restored
            .dreams
            .unwrap()
            .learning
            .iter()
            .any(|row| row.person == learner)
    );
    assert_eq!(generated_campaign(128).snapshot(), initial);
}

#[test]
fn authored_aspiration_learning_overrides_a_generated_disposition() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let contents =
        std::fs::read_to_string(root.join("content/parallel/economy-campaign.toml")).unwrap();
    let path = std::env::temp_dir().join(format!(
        "legacy-aspiration-override-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, format!("{contents}\n[[aspiration_learning]]\nperson = 1000\nmax_aspirations = 1\npriority = 17\nbatch = 2\nreview_interval = 5\n")).unwrap();
    let world = super::scenario_with_options(
        &path,
        &root.join("content"),
        Some(42),
        Some(&root.join("content/parallel/campaign-population.toml")),
    );
    std::fs::remove_file(path).unwrap();
    let state = world.unwrap().snapshot();
    let learning: Vec<_> = state
        .dreams
        .as_ref()
        .unwrap()
        .learning
        .iter()
        .filter(|row| row.person == 1000)
        .collect();
    assert_eq!(learning.len(), 1);
    assert_eq!(
        (
            learning[0].max_aspirations,
            learning[0].priority,
            learning[0].batch
        ),
        (1, 17, 2)
    );
}

#[test]
fn generated_aspiration_can_lead_to_an_actual_work_request() {
    let mut initial = generated_campaign(128).snapshot();
    let requests = initial
        .autowork
        .as_ref()
        .unwrap()
        .requests
        .as_ref()
        .unwrap();
    let asker = requests
        .askers
        .iter()
        .find(|asker| {
            asker.person >= 1000
                && initial
                    .dreams
                    .as_ref()
                    .unwrap()
                    .people
                    .iter()
                    .any(|dream| dream.person == asker.person)
        })
        .unwrap()
        .person;
    initial
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .askers
        .retain(|row| row.person == asker);
    initial
        .dreams
        .as_mut()
        .unwrap()
        .people
        .retain(|row| row.person == asker);
    for person in &mut initial.people {
        person.willing_to_work = false;
        person.seek_meals = false;
    }
    for policy in &mut initial.agriculture.as_mut().unwrap().policies {
        policy.willing = false;
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                let mut observed = false;
                for _ in 0..8 {
                    world.advance().unwrap();
                    let state = world.snapshot();
                    observed |= state
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .next_id
                        > 1;
                }
                assert!(observed);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_person_learns_help_only_from_an_actual_food_transfer() {
    let initial = generated_campaign(128).snapshot();
    let target = initial
        .giving
        .as_ref()
        .unwrap()
        .learning
        .iter()
        .find(|p| {
            p.person >= 1000
                && initial
                    .people
                    .iter()
                    .any(|person| person.id == p.person && person.region == Some(1))
        })
        .unwrap()
        .person;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                let bread = world
                    .snapshot()
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "bread")
                    .unwrap()
                    .id;
                assert!(world.give(1, target, bread, 1).unwrap());
                let learned = world.sharing_learning(target).unwrap();
                assert_eq!((learned.experiences, learned.last_helper), (1, Some(1)));
                assert!(!world.give(1, target, bread, 999).unwrap());
                let learned = world.sharing_learning(target).unwrap();
                assert_eq!((learned.experiences, learned.last_helper), (1, Some(1)));
                assert!(world.give(1, target, bread, 1).unwrap());
                assert_eq!(world.sharing_learning(target).unwrap().experiences, 2);
                let mut state = world.snapshot();
                let person = state.people.iter_mut().find(|p| p.id == target).unwrap();
                person.hunger = 15;
                person.seek_meals = false;
                person.willing_to_work = false;
                state
                    .agriculture
                    .as_mut()
                    .unwrap()
                    .policies
                    .iter_mut()
                    .find(|p| p.person == target)
                    .unwrap()
                    .willing = false;
                let holder = state
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|c| c.place == game_parallel_lots::Place::Person(target))
                    .unwrap();
                *holder = game_parallel_lots::Container::new(holder.place, holder.capacity, vec![])
                    .unwrap();
                let mut world = game_parallel_economy::Economy::compile(state).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert!(
                    state
                        .giving
                        .as_ref()
                        .unwrap()
                        .requests
                        .iter()
                        .any(|request| request.requester == target && request.target == 1)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_hungry_person_can_ask_a_previously_met_acquaintance() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = generated_campaign(128);
                world.advance().unwrap();
                let mut state = world.snapshot();
                let relationships = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .social
                    .as_ref()
                    .unwrap()
                    .relationships;
                let preferences = &state.giving.as_ref().unwrap().acquaintance_asking;
                let actor = relationships
                    .iter()
                    .flat_map(|r| [r.a, r.b])
                    .find(|id| {
                        *id >= 1000 && preferences.iter().any(|p| p.person == *id && p.enabled)
                    })
                    .unwrap();
                let known: Vec<_> = relationships
                    .iter()
                    .filter_map(|r| {
                        if r.a == actor {
                            Some(r.b)
                        } else if r.b == actor {
                            Some(r.a)
                        } else {
                            None
                        }
                    })
                    .collect();
                let person = state.people.iter_mut().find(|p| p.id == actor).unwrap();
                person.hunger = 15;
                person.seek_meals = false;
                person.willing_to_work = false;
                state
                    .agriculture
                    .as_mut()
                    .unwrap()
                    .policies
                    .iter_mut()
                    .find(|p| p.person == actor)
                    .unwrap()
                    .willing = false;
                let holder = state
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|c| c.place == game_parallel_lots::Place::Person(actor))
                    .unwrap();
                *holder = game_parallel_lots::Container::new(holder.place, holder.capacity, vec![])
                    .unwrap();
                let mut world = game_parallel_economy::Economy::compile(state).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let request = state
                    .giving
                    .as_ref()
                    .unwrap()
                    .requests
                    .iter()
                    .find(|r| r.requester == actor)
                    .unwrap();
                assert!(known.contains(&request.target));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn population_profiles_seed_personal_acquaintance_preferences() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let profile = std::fs::read_to_string(root.join("content/parallel/campaign-population.toml"))
        .unwrap()
        .replace(
            "prefer_affinity = true",
            "prefer_affinity = true\nmax_affinity = -1\naffinity_weight = -2\navoid_last = false",
        );
    let path = std::env::temp_dir().join(format!(
        "legacy-personal-acquaintance-profile-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, profile).unwrap();
    let world = super::scenario_with_options(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
        Some(42),
        Some(&path),
    )
    .unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let rows = &state.giving.as_ref().unwrap().acquaintance_asking;
    assert!(!rows.is_empty());
    assert!(rows.iter().all(
        |row| (row.max_affinity, row.affinity_weight, row.avoid_last)
            == (Some(-1), Some(-2), Some(false))
    ));
}

fn hungry_generated_campaign(count: u64) -> game_parallel_economy::State {
    let mut world = generated_campaign(count);
    world.advance().unwrap();
    let mut state = world.snapshot();
    for person in &mut state.people {
        if person.id >= 1000 {
            person.hunger = 15;
            person.seek_meals = false;
            person.willing_to_work = false;
        }
    }
    for policy in &mut state.agriculture.as_mut().unwrap().policies {
        if policy.person >= 1000 {
            policy.willing = false;
        }
    }
    for holder in &mut state.physical.custody.containers {
        if matches!(holder.place, game_parallel_lots::Place::Person(id) if id >= 1000) {
            *holder =
                game_parallel_lots::Container::new(holder.place, holder.capacity, vec![]).unwrap();
        }
    }
    state
}

#[test]
fn many_hungry_people_ask_known_acquaintances_deterministically() {
    let initial = hungry_generated_campaign(2048);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let requests = &state.giving.as_ref().unwrap().requests;
                assert!(requests.len() > 100);
                let mut actors: Vec<_> = requests.iter().map(|request| request.requester).collect();
                actors.sort_unstable();
                actors.dedup();
                assert_eq!(actors.len(), requests.len());
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn generated_acquaintance_requests_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000u64);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let initial = hungry_generated_campaign(count);
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                let start = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert!(world.snapshot().giving.unwrap().requests.len() > (count / 20) as usize);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} generated={count} hungry_tick_seconds={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore = "performance measurement"]
fn generated_campaign_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000u64);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = generated_campaign(count);
                let start = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let elapsed = start.elapsed().as_secs_f64();
                let snapshot = world.snapshot();
                assert!(
                    snapshot
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .social
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|event| event.a >= 1000 || event.b >= 1000)
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} generated={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
    #[cfg(feature = "profile")]
    if std::env::var_os("LEGACY_PROFILE").is_some() {
        for (name, measurement) in game_diagnostics::snapshot() {
            if (name.starts_with("economy.") || name.starts_with("family."))
                && measurement.calls > 0
            {
                println!(
                    "phase={name} calls={} mean_ms={:.3}",
                    measurement.calls,
                    measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0
                );
            }
        }
    }
}

#[test]
fn authored_scenario_advances_and_resumes_identically_across_worker_counts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let first = world.advance().unwrap();
                assert_eq!(first.started.len(), 1);
                assert_eq!(world.snapshot().goals.as_ref().unwrap().people.len(), 1);
                let state = world.snapshot();
                let field = &state.agriculture.as_ref().unwrap().fields[0];
                assert_eq!(field.phase, game_parallel_economy::CropPhase::Growing);
                assert_eq!(field.remaining, 24);
                let seed = state
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "grain-seed")
                    .unwrap()
                    .id;
                let stock = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| {
                        row.place
                            == game_parallel_lots::Place::Stock {
                                region: 1,
                                good: seed,
                            }
                    })
                    .unwrap();
                assert_eq!(stock.used, 20);
                let path = std::env::temp_dir().join(format!(
                    "legacy-economy-{}-{workers}.json",
                    std::process::id()
                ));
                save(&world, &path).unwrap();
                let mut restored = load(&path).unwrap();
                std::fs::remove_file(path).unwrap();
                for _ in 0..4 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn authored_campaign_starts_a_controlled_ruler_and_a_real_family() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy-campaign.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let state = world.snapshot();
                assert!(state.people.iter().find(|p| p.id == 1).unwrap().controlled);
                assert_eq!(state.campaign.as_ref().unwrap().lineage.current, Some(1));
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let saved = world.snapshot();
                assert_eq!(saved.family.as_ref().unwrap().totals.births, 1);
                assert!(
                    saved
                        .food_search
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|p| p.person == 3 && p.enabled)
                );
                assert!(
                    saved
                        .gathering
                        .as_ref()
                        .unwrap()
                        .policies
                        .iter()
                        .any(|p| p.person == 3 && p.pressured_by.is_none())
                );
                let newborn_purpose = saved
                    .purpose
                    .as_ref()
                    .unwrap()
                    .people
                    .iter()
                    .find(|p| p.person == 3)
                    .unwrap();
                assert_eq!(newborn_purpose.deprivation, 0);
                assert!(newborn_purpose.eligible_at > saved.physical.custody.tick);
                assert!(
                    saved
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .personality
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|p| p.person == 3)
                );
                assert!(
                    saved
                        .knowledge
                        .as_ref()
                        .unwrap()
                        .minds
                        .iter()
                        .any(|mind| mind.person == 3)
                );
                assert!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .any(|c| c.place == game_parallel_lots::Place::Person(3))
                );
                game_parallel_economy::Economy::compile(saved.clone()).unwrap();
                saved
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn ruler_view_excludes_remote_people_stock_and_other_minds() {
    use game_parallel_economy_knowledge::Record;
    use game_parallel_economy_player::Player;
    use game_parallel_lots::{Container, Lot, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut baseline = world.snapshot();
    baseline
        .people
        .iter_mut()
        .find(|p| p.id == 2)
        .unwrap()
        .region = Some(2);
    let mut hidden = baseline.clone();
    hidden.giving.as_mut().unwrap().learning[0].experiences = 2;
    hidden
        .opinions
        .as_mut()
        .unwrap()
        .records
        .push(game_parallel_economy::Opinion {
            observer: 2,
            target: 1,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: 8,
        });
    hidden
        .knowledge
        .as_mut()
        .unwrap()
        .minds
        .iter_mut()
        .find(|m| m.person == 2)
        .unwrap()
        .records
        .push(Record {
            region: 2,
            food: 999,
            observed_at: 0,
            witness: 2,
            learned_from: None,
        });
    let stock = hidden
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| matches!(c.place, Place::Stock { region: 2, .. }))
        .unwrap();
    let good = match stock.place {
        Place::Stock { good, .. } => good,
        _ => unreachable!(),
    };
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good,
            quantity: 7,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut first = game_parallel_economy::Economy::compile(baseline).unwrap();
    let mut second = game_parallel_economy::Economy::compile(hidden).unwrap();
    let mut player = Player::new(&mut first);
    assert_eq!(
        player.view().unwrap(),
        Player::new(&mut second).view().unwrap()
    );
    assert_eq!(player.view().unwrap().person.unwrap().id, 1);
    assert!(player.view().unwrap().nearby.is_empty());
    let conditions = player.view().unwrap().conditions.unwrap();
    assert_eq!(conditions.season.unwrap().name, "growing season");
    assert_eq!(conditions.local_hazard.unwrap().region, 1);
    assert!(player.past_knowledge(2).is_err());
    assert!(player.meet(2).is_err());
    assert_eq!(player.view().unwrap().tick, 0);
}

#[test]
fn suspended_control_hides_the_rulers_view_and_rejects_actions() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .campaign
        .as_mut()
        .unwrap()
        .lineage
        .invocation
        .as_mut()
        .unwrap()
        .control_until = Some(2);
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    let view = player.view().unwrap();
    assert!(view.person.is_none());
    assert!(
        view.nearby.is_empty()
            && view.possessions.is_empty()
            && view.local_stock.is_empty()
            && view.food_reports.is_empty()
    );
    assert!(player.eat(31).is_err());
    assert!(player.start_expedition(2, 1).is_err());
    player.advance().unwrap();
    player.advance().unwrap();
    assert_eq!(player.view().unwrap().person.unwrap().id, 1);
}

#[test]
fn player_can_offer_more_than_they_own_and_make_a_separate_work_request() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-work-practices.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    player
        .set_work_outcome_policy(Some(game_parallel_economy::WorkOutcomePolicy {
            wait_ticks: 2,
            missing_trust: 1,
            missing_affinity: -2,
        }))
        .unwrap();
    assert_eq!(
        player
            .view()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .outcome_policy
            .unwrap()
            .missing_affinity,
        -2
    );
    player.set_work_outcome_policy(None).unwrap();
    player
        .set_work_practice_evaluation(Some(game_parallel_economy::WorkPracticeEvaluation {
            base: 1,
            affinity_weight: -2,
            trust_weight: 3,
            threshold: 4,
        }))
        .unwrap();
    assert_eq!(
        player
            .view()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .evaluation
            .unwrap()
            .affinity_weight,
        -2
    );
    player.set_work_practice_choice("approve", false).unwrap();
    assert!(
        player
            .view()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .evaluation
            .is_none()
    );
    let view = player.view().unwrap();
    let bread = view.possessions[0].good;
    let offered = view.possessions[0].quantity + 100;
    let recipe = player
        .catalog()
        .unwrap()
        .1
        .into_iter()
        .find(|recipe| recipe.name == "fixture-tool")
        .unwrap()
        .id;
    assert!(
        player
            .propose_work_practice(2, recipe, bread, offered, 12)
            .unwrap()
    );
    let view = player.view().unwrap();
    assert_eq!(
        view.practices
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .authored
            .as_ref()
            .unwrap()
            .quantity,
        offered
    );
    player.advance().unwrap();
    let request = player.ask_work(2, recipe, 8).unwrap();
    assert_eq!(
        player
            .view()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .engagement
            .as_ref()
            .unwrap()
            .request,
        request
    );
}

#[test]
fn player_can_choose_signed_promise_trust_reactions() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    assert_eq!(player.promise_trust_reactions().unwrap(), (None, None));
    player.set_promise_trust_reactions(Some((-2, 3))).unwrap();
    assert_eq!(
        player.promise_trust_reactions().unwrap(),
        (Some(-2), Some(3))
    );
    let state = world.snapshot();
    let mut restored = game_parallel_economy::Economy::compile(
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
    )
    .unwrap();
    let mut player = Player::new(&mut restored);
    assert_eq!(
        player.promise_trust_reactions().unwrap(),
        (Some(-2), Some(3))
    );
    player.set_promise_trust_reactions(None).unwrap();
    assert_eq!(player.promise_trust_reactions().unwrap(), (None, None));
}

#[test]
fn player_can_make_a_promise_and_later_choose_real_delivery() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let good = player.view().unwrap().possessions[0].good;
    let id = player.promise(2, good, 1, 8).unwrap();
    player.advance().unwrap();
    assert_eq!(
        player.view().unwrap().promises[0].status,
        game_parallel_agreements::Status::Active
    );
    assert!(player.fulfill_promise(id).unwrap());
    assert_eq!(
        player.view().unwrap().promises[0].status,
        game_parallel_agreements::Status::Fulfilled
    );
}

#[test]
fn distrust_can_initiate_tactical_conflict_with_a_controlled_person() {
    use game_parallel_economy_combat::{Order, Reaction, Response};
    use game_parallel_economy_player::Player;
    use game_parallel_opinions::{Confrontation, Opinion};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let initial = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                for person in &mut state.people {
                    person.willing_to_work = false;
                    person.seek_meals = false;
                }
                for member in &mut state.family.as_mut().unwrap().members {
                    member.pregnancy = None;
                    member.reproduction_allowed = false;
                }
                let opinions = state.opinions.as_mut().unwrap();
                opinions
                    .records
                    .retain(|row| (row.observer, row.target) != (2, 1));
                opinions.records.push(Opinion {
                    observer: 2,
                    target: 1,
                    trust: -3,
                    fear: 0,
                    obligation: 0,
                    affinity: 0,
                });
                let sensitivity = opinions
                    .sensitivities_mut()
                    .iter_mut()
                    .find(|p| p.person == 2)
                    .unwrap();
                sensitivity.confrontation = Some(Confrontation {
                    trust_at_most: -2,
                    fear_at_most: 0,
                    interval: 1,
                    enabled: true,
                });
                state.combat.as_mut().unwrap().reactions.push(Reaction {
                    person: 1,
                    response: Response::Fight,
                    retreat_at_or_below: None,
                    pursue: false,
                });
                let mut world = game_parallel_economy::Economy::compile(state).unwrap();
                Player::new(&mut world).rest().unwrap();
                assert!(
                    world
                        .snapshot()
                        .combat
                        .as_ref()
                        .unwrap()
                        .conflict
                        .encounters
                        .is_empty()
                );
                let before = world.snapshot().people[0].health;
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.people[0].health, before,
                    "new fights begin after the existing round"
                );
                let combat = state.combat.as_ref().unwrap();
                assert_eq!(combat.conflict.encounters.len(), 1);
                let fight = &combat.conflict.encounters[0];
                assert_eq!(
                    fight.fighters.iter().find(|f| f.person == 1).unwrap().order,
                    Order::Defend
                );
                assert!(matches!(
                    fight.fighters.iter().find(|f| f.person == 2).unwrap().order,
                    Order::Attack { target: 1 }
                ));
                assert_eq!(combat.tactics.battles[0].encounter, fight.id);
                assert!(Player::new(&mut world).view().unwrap().battle.is_some());
                let mut restored = game_parallel_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert!(world.snapshot().people[0].health < before);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn player_fights_on_the_authored_generated_map() {
    use game_parallel_economy_combat::Order;
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let id = player.attack(2).unwrap();
    assert_eq!(player.view().unwrap().battle.unwrap().encounter, id);
    assert!(player.rest().is_err());
    player.advance().unwrap();
    assert!(player.view().unwrap().person.unwrap().health < 100);
    player.combat_order(Order::Retreat).unwrap();
    player.advance().unwrap();
    assert!(player.view().unwrap().battle.is_none());
}

#[test]
fn controlled_worker_can_receive_and_independently_respond_to_real_work_requests() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy-campaign.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let recipe = Player::new(&mut world)
                    .catalog()
                    .unwrap()
                    .1
                    .into_iter()
                    .find(|r| r.name == "fixture-tool")
                    .unwrap()
                    .id;
                let declined = world.ask_work(2, 1, recipe, 15).unwrap();
                {
                    let mut player = Player::new(&mut world);
                    assert!(
                        player
                            .view()
                            .unwrap()
                            .pending_work_requests
                            .iter()
                            .any(|r| r.id == declined)
                    );
                    assert!(!player.respond_work_request(declined, false).unwrap());
                    assert!(
                        !player
                            .view()
                            .unwrap()
                            .pending_work_requests
                            .iter()
                            .any(|r| r.id == declined)
                    );
                }
                let request = world.ask_work(2, 1, recipe, 15).unwrap();
                let saved = world.snapshot();
                let mut restored = game_parallel_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(
                    Player::new(&mut world)
                        .respond_work_request(request, true)
                        .unwrap()
                );
                assert!(
                    Player::new(&mut restored)
                        .respond_work_request(request, true)
                        .unwrap()
                );
                assert_eq!(world.snapshot(), restored.snapshot());
                let state = world.snapshot();
                let requests = state.autowork.as_ref().unwrap().requests.as_ref().unwrap();
                assert!(
                    requests
                        .work_links
                        .iter()
                        .any(|link| link.request == request && link.worker == 1)
                );
                assert!(!requests.pending.iter().any(|row| row.id == request));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn controlled_work_response_requires_a_real_admitted_start() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    let recipe = state
        .physical
        .recipes
        .iter()
        .find(|r| r.name == "fixture-tool")
        .unwrap()
        .id;
    let tool = state
        .physical
        .goods
        .iter()
        .find(|g| g.name == "tool")
        .unwrap()
        .id;
    state.work_tools.push(game_parallel_economy::WorkTool {
        recipe,
        good: tool,
        quantity: 100_000,
    });
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let request = world.ask_work(2, 1, recipe, 15).unwrap();
    let before = world.snapshot();
    assert!(world.respond_work_request(2, request, true).is_err());
    assert_eq!(world.snapshot(), before);
    assert!(
        !Player::new(&mut world)
            .respond_work_request(request, true)
            .unwrap()
    );
    let state = world.snapshot();
    let requests = state.autowork.as_ref().unwrap().requests.as_ref().unwrap();
    assert!(requests.pending.iter().any(|row| row.id == request));
    assert!(!requests.work_links.iter().any(|row| row.request == request));
}

#[test]
fn player_request_can_be_refused_or_followed_from_personal_fear() {
    use game_parallel_economy_combat::Order;
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut initial = world.snapshot();
    for member in &mut initial.family.as_mut().unwrap().members {
        member.pregnancy = None;
        member.reproduction_allowed = false;
    }
    let mut world = game_parallel_economy::Economy::compile(initial).unwrap();
    let mut player = Player::new(&mut world);
    let recipe = player
        .catalog()
        .unwrap()
        .1
        .into_iter()
        .find(|r| r.name == "fixture-tool")
        .unwrap()
        .id;
    let first = player.ask_work(2, recipe, 15).unwrap();
    assert_eq!(player.view().unwrap().pending_work_requests[0].id, first);
    for _ in 0..8 {
        player.advance().unwrap();
        if player
            .view()
            .unwrap()
            .work_memories
            .iter()
            .any(|m| m.worker == 2 && m.failures == 1)
        {
            break;
        }
    }
    let view = player.view().unwrap();
    assert!(
        view.work_memories
            .iter()
            .any(|m| m.worker == 2 && m.failures == 1),
        "{view:?}"
    );
    player.attack(2).unwrap();
    player.advance().unwrap();
    player.combat_order(Order::Retreat).unwrap();
    player.advance().unwrap();
    let second = player.ask_work(2, recipe, 15).unwrap();
    let mut accepted = false;
    for _ in 0..10 {
        player.advance().unwrap();
        if player
            .view()
            .unwrap()
            .observed_work
            .iter()
            .any(|work| work.request == second && work.worker == 2)
        {
            accepted = true;
            break;
        }
    }
    let view = player.view().unwrap();
    assert!(accepted, "{view:?}");
}

#[test]
fn player_view_does_not_reveal_an_unobserved_infection_or_its_remedy() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let healthy = world.snapshot();
    let mut infected = healthy.clone();
    infected
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases
        .push(game_parallel_disease::Case {
            person: 1,
            strain: 1,
            infected_at: 0,
            source: None,
        });
    let mut healthy = game_parallel_economy::Economy::compile(healthy).unwrap();
    let mut infected = game_parallel_economy::Economy::compile(infected).unwrap();
    let healthy_view = Player::new(&mut healthy).view().unwrap();
    let infected_view = Player::new(&mut infected).view().unwrap();
    assert!(
        healthy_view.treatment.is_some(),
        "the player may see their own selected medicine"
    );
    assert_eq!(
        healthy_view, infected_view,
        "the unseen infection cannot alter the player's view"
    );
}

#[test]
fn player_treats_infection_with_authored_medicine_taken_from_stock() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases
        .push(game_parallel_disease::Case {
            person: 1,
            strain: 1,
            infected_at: 0,
            source: None,
        });
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    let treatment = player.view().unwrap().treatment.unwrap();
    let (medicine, quantity) = (treatment.medicine, treatment.quantity);
    assert!(player.transfer_stock(medicine, quantity, true).unwrap());
    assert!(player.treat().unwrap());
    assert_eq!(player.view().unwrap().treatment.unwrap(), treatment);
}

#[test]
fn player_can_treat_a_nearby_person_with_carried_medicine() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases
        .push(game_parallel_disease::Case {
            person: 2,
            strain: 1,
            infected_at: 0,
            source: None,
        });
    let medicine = state
        .physical
        .goods
        .iter()
        .find(|g| g.name == "medicine")
        .unwrap()
        .id;
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    assert!(player.transfer_stock(medicine, 1, true).unwrap());
    assert!(player.treat_other(2, medicine, 1).unwrap());
    assert!(
        !world
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .any(|c| c.person == 2)
    );
}

#[test]
fn player_can_cultivate_a_visible_field() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let field = player.view().unwrap().fields[0].id;
    assert!(player.farm(field).unwrap());
    assert_eq!(
        player.view().unwrap().fields[0].phase,
        game_parallel_economy::CropPhase::Growing
    );
}

#[test]
fn player_rest_advances_time_and_recovers_personal_fatigue() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .fatigue
        .as_mut()
        .unwrap()
        .people
        .push(game_parallel_fatigue::Person {
            id: 1,
            fatigue: 10,
            resting: false,
        });
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    player.rest().unwrap();
    let view = player.view().unwrap();
    assert_eq!(view.tick, 1);
    assert_eq!(view.fatigue, Some((6, true)));
}

#[test]
fn player_observes_and_recovers_local_deceased_possessions() {
    use game_parallel_economy_player::Player;
    use game_parallel_lots::{Container, Lot, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut state = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    state.people.iter_mut().find(|p| p.id == 2).unwrap().health = 0;
    state
        .family
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|p| p.id == 2)
        .unwrap()
        .alive = false;
    let good = state.physical.goods[0].id;
    let holder = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(
        Place::Person(2),
        holder.capacity,
        vec![Lot {
            good,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    assert!(
        player
            .view()
            .unwrap()
            .bodies
            .iter()
            .any(|body| body.person == 2 && body.possessions.iter().any(|lot| lot.good == good))
    );
    assert!(player.recover_possessions(2, good, 1).unwrap());
    assert!(
        player
            .view()
            .unwrap()
            .possessions
            .iter()
            .any(|lot| lot.good == good)
    );
}

#[test]
fn player_gathers_from_an_authored_local_resource_site() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let site = player
        .view()
        .unwrap()
        .resource_sites
        .into_iter()
        .find(|site| site.id == 1)
        .unwrap();
    assert_eq!(site.available, 20);
    assert!(player.gather(1).unwrap());
    assert_eq!(
        player
            .view()
            .unwrap()
            .resource_sites
            .into_iter()
            .find(|site| site.id == 1)
            .unwrap()
            .available,
        18
    );
    assert!(player.gather(2).is_err());
}

#[test]
fn player_sees_only_their_own_actual_journey() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    player.depart(2).unwrap();
    let view = player.view().unwrap();
    let journey = view.journey.unwrap();
    assert_eq!(
        (
            journey.person,
            journey.source,
            journey.destination,
            journey.remaining
        ),
        (1, 1, 2, 3)
    );
    assert!(view.nearby.is_empty());
    assert!(view.resource_sites.is_empty());
}

#[test]
fn player_can_give_carried_goods_without_creating_a_promise() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let before = player.view().unwrap();
    let good = before.possessions[0].good;
    assert!(player.give(2, good, 1).unwrap());
    let after = player.view().unwrap();
    assert_eq!(
        after.possessions[0].quantity,
        before.possessions[0].quantity - 1
    );
    assert!(after.promises.is_empty());
}

#[test]
fn player_can_ask_a_local_person_for_food_and_see_the_request() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    player.ask_food(2, 4).unwrap();
    let view = player.view().unwrap();
    assert_eq!(view.tick, 1);
    assert_eq!(view.food_requests.len(), 1);
    assert_eq!(
        (
            view.food_requests[0].requester,
            view.food_requests[0].target
        ),
        (1, 2)
    );
    assert!(player.ask_food(99, 4).is_err());
    assert_eq!(player.view().unwrap().tick, 1);
}

#[test]
fn player_can_request_a_personal_hauling_effort() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let bread = player
        .catalog()
        .unwrap()
        .0
        .into_iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    player.ask_haul(2, 2, 1, bread, 8).unwrap();
    let view = player.view().unwrap();
    assert_eq!(view.hauling_requests.len(), 1);
    assert_eq!(
        (
            view.hauling_requests[0].requester,
            view.hauling_requests[0].carrier
        ),
        (1, 2)
    );
    assert_eq!(view.tick, 1);
}

#[test]
fn player_request_can_lead_to_a_real_cross_region_delivery() {
    use game_parallel_economy_player::Player;
    use game_parallel_lots::{Container, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let original = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = original.snapshot();
    state.autowork = None;
    state.goals = None;
    state.dreams = None;
    state.agriculture = None;
    state.gathering = None;
    state.people[1].seek_meals = false;
    let holder = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(holder.place, holder.capacity, vec![]).unwrap();
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    let bread = player
        .catalog()
        .unwrap()
        .0
        .into_iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    player.ask_haul(2, 2, 1, bread, 20).unwrap();
    for _ in 0..15 {
        player.advance().unwrap();
    }
    let view = player.view().unwrap();
    assert!(
        view.local_stock
            .iter()
            .any(|lot| lot.good == bread && lot.quantity >= 2)
    );
}

#[test]
fn player_can_see_and_give_to_a_companion_on_the_same_journey() {
    use game_parallel_economy::Departure;
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    world
        .depart(&[
            Departure {
                person: 1,
                destination: 2,
            },
            Departure {
                person: 2,
                destination: 2,
            },
        ])
        .unwrap();
    let mut player = Player::new(&mut world);
    let view = player.view().unwrap();
    assert_eq!(view.nearby, vec![2]);
    assert!(player.give(2, view.possessions[0].good, 1).unwrap());
    assert_eq!(player.view().unwrap().journey.unwrap().remaining, 2);
}

#[test]
fn scenario_shelter_goods_resolve_from_content_names() {
    use crate::{EnvironmentConfig, resolve_environment};
    use std::collections::BTreeMap;
    let parse = |good: &str| {
        toml::from_str::<EnvironmentConfig>(&format!("regions = [{{ region = 1, damage = 2, shelter = 0, shelter_reduction = 1, shelter_good = \"{good}\" }}]")).unwrap()
    };
    let goods = BTreeMap::from([("reed-shelter-kit", 7)]);
    assert_eq!(
        resolve_environment(parse("reed-shelter-kit"), &goods)
            .unwrap()
            .regions[0]
            .shelter_good,
        Some(7)
    );
    assert!(resolve_environment(parse("missing"), &goods).is_err());
}

#[test]
fn authored_personal_promise_offers_resolve_named_goods() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy.toml"),
        &root.join("content"),
    )
    .unwrap();
    let state = world.snapshot();
    let offer = &state.agreements.as_ref().unwrap().offering[0];
    let good = state
        .physical
        .goods
        .iter()
        .find(|good| good.id == offer.good)
        .unwrap();
    assert_eq!(
        (offer.person, offer.recipient, good.name.as_str()),
        (2, 1, "bread")
    );
    assert!(!offer.willing);
}

#[test]
fn authored_personal_offers_resolve_observed_work_by_recipe_name() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("content/parallel/economy.toml")).unwrap();
    let configured = source.replacen(
        "[[promise_offering]]",
        "[[promise_offering]]\nafter_work_recipe = \"fixture-tool\"",
        1,
    );
    let path =
        std::env::temp_dir().join(format!("legacy-observed-work-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let id = state.agreements.unwrap().offering[0]
        .after_work_recipe
        .unwrap();
    assert_eq!(
        state
            .physical
            .recipes
            .iter()
            .find(|r| r.id == id)
            .unwrap()
            .name,
        "fixture-tool"
    );
}

#[test]
fn authored_work_tools_resolve_recipe_and_good_names() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("content/parallel/economy.toml")).unwrap();
    let configured = format!(
        "{}\n[[work_tools]]\nrecipe = \"fixture-tool\"\ngood = \"tool\"\nquantity = 1\n",
        source
    );
    let path = std::env::temp_dir().join(format!("legacy-work-tools-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let tool = &state.work_tools[0];
    assert_eq!(
        state
            .physical
            .recipes
            .iter()
            .find(|r| r.id == tool.recipe)
            .unwrap()
            .name,
        "fixture-tool"
    );
    assert_eq!(
        state
            .physical
            .goods
            .iter()
            .find(|g| g.id == tool.good)
            .unwrap()
            .name,
        "tool"
    );
}

#[test]
fn authored_work_sites_resolve_recipes_and_preserve_kind_names() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("content/parallel/economy.toml")).unwrap();
    let configured = format!(
        "{}\n[work_sites]\n[[work_sites.sites]]\nid = 7\nregion = 1\nkind = \"forge\"\ncapacity = 2\n[[work_sites.requirements]]\nrecipe = \"fixture-tool\"\nkind = \"forge\"\nslots = 1\n",
        source
    );
    let path = std::env::temp_dir().join(format!("legacy-work-sites-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let sites = state.work_sites.unwrap();
    assert_eq!(sites.sites[0].kind, "forge");
    assert_eq!(sites.requirements[0].kind, "forge");
    assert_eq!(
        state
            .physical
            .recipes
            .iter()
            .find(|r| r.id == sites.requirements[0].recipe)
            .unwrap()
            .name,
        "fixture-tool"
    );
}

#[test]
fn ruler_observes_only_local_physical_work_sites() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/parallel/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{}\n[work_sites]\n[[work_sites.sites]]\nid = 7\nregion = 1\nkind = \"forge\"\ncapacity = 2\n[[work_sites.sites]]\nid = 8\nregion = 2\nkind = \"mill\"\ncapacity = 1\n[[work_sites.requirements]]\nrecipe = \"fixture-tool\"\nkind = \"forge\"\nslots = 1\n",
        source
    );
    let path = std::env::temp_dir().join(format!(
        "legacy-work-sites-view-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let player = game_parallel_economy_player::Player::new(&mut world);
    let (_, requirements) = player.work_requirements().unwrap();
    assert_eq!(requirements[0].kind, "forge");
    let view = player.view().unwrap();
    assert_eq!(view.work_sites.len(), 1);
    assert_eq!(
        (view.work_sites[0].site.id, view.work_sites[0].occupied),
        (7, 0)
    );
}

#[test]
fn authored_personal_practices_are_visible_only_to_their_owner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/parallel/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{source}\n[[giving.practices.minds]]\nperson = 1\ninvent = true\napprove = false\nwilling = false\nask_willing = true\nreporting = \"lie\"\nreport_attitude = \"doubt\"\nunanswered_report_belief = \"believes\"\nrequire_believed_ready = true\nvalue_learning = {{ value = 1, minimum = -4, maximum = 4, received = 2, gave = -1, unanswered = 1 }}\nshortage_threshold = 5\nminimum_shortages = 2\nreview_interval = 3\nproposal_lifetime = 8\n\n[[giving.practices.minds]]\nperson = 2\ninvent = false\napprove = true\nwilling = true\nshortage_threshold = 5\nminimum_shortages = 2\nreview_interval = 3\nproposal_lifetime = 8\n"
    );
    let configured = configured.replacen(
        "shortage_threshold = 5",
        "precedent = { same_person = 3, same_good = 2, quantity_difference = -1, received = 4, gave = -2 }\nshortage_threshold = 5",
        1,
    );
    let configured = configured.replace(
        "report_attitude = \"doubt\"",
        "report_attitude = { trust_at_least = 3 }",
    );
    let configured = configured.replacen(
        "reporting = \"lie\"",
        "reporting = \"lie\"\nproposal_affinity_weight = -3",
        1,
    );
    let configured = configured.replacen(
        "[[medical.policies]]",
        "[[medical.acquaintances]]\nperson = 1\nmedicine = \"medicine\"\nquantity = 1\npatient_health_at_most = 75\nwilling = false\npressure_threshold = 3\nurgency_weight = 2\naffinity_weight = -1\n\n[[medical.policies]]\npatient_health_at_most = 70",
        1,
    );
    let configured = configured.replacen("[[remedies]]", "[[remedies]]\nhealth_at_most = 65", 1);
    let configured = configured.replacen(
        "[[giving.responses]]",
        "[[giving.responses]]\naffinity_at_most = -2\naffinity_weight = -1",
        1,
    );
    let configured = format!(
        "{configured}\n[[giving.acquaintance_asking]]\nperson = 1\nenabled = true\nmin_hunger = 5\nretry_ticks = 2\nrequest_lifetime = 3\nmax_affinity = -1\naffinity_weight = -2\navoid_last = false\n"
    );
    let path = std::env::temp_dir().join(format!(
        "legacy-personal-practices-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        world.snapshot().dynamics.unwrap().treatment[0].health_at_most,
        Some(65)
    );
    assert_eq!(
        world.snapshot().giving.unwrap().responses[0].affinity_at_most,
        Some(-2)
    );
    let asking = world.snapshot().giving.unwrap().acquaintance_asking;
    assert_eq!(
        (
            asking[0].max_affinity,
            asking[0].affinity_weight,
            asking[0].avoid_last
        ),
        (Some(-1), Some(-2), Some(false))
    );
    assert_eq!(
        world.snapshot().medical.unwrap().policies[0].patient_health_at_most,
        Some(70)
    );
    assert_eq!(
        world.snapshot().medical.unwrap().acquaintances[0].affinity_weight,
        -1
    );
    let mut player = game_parallel_economy_player::Player::new(&mut world);
    let own = player.view().unwrap().practices.unwrap();
    assert_eq!(own.person, 1);
    assert!(!own.approve && !own.willing && own.ask_willing);
    assert_eq!(own.reporting, game_parallel_economy::PracticeReporting::Lie);
    assert_eq!(own.proposal_affinity_weight, -3);
    assert_eq!(
        own.report_attitude,
        game_parallel_economy::PracticeReportAttitude::TrustAtLeast(3)
    );
    assert!(own.require_believed_ready);
    assert_eq!(own.value_learning.as_ref().unwrap().value, 1);
    assert_eq!(own.precedent.as_ref().unwrap().same_person, 3);
    player.set_practice_precedent(None).unwrap();
    assert!(
        player
            .view()
            .unwrap()
            .practices
            .unwrap()
            .precedent
            .is_none()
    );
    let invalid = game_parallel_economy::PracticeValueLearning {
        value: 1,
        minimum: 2,
        maximum: 0,
        received: 0,
        gave: 0,
        unanswered: 0,
    };
    assert!(player.set_practice_value_learning(Some(invalid)).is_err());
    assert_eq!(
        player
            .view()
            .unwrap()
            .practices
            .unwrap()
            .value_learning
            .unwrap()
            .value,
        1
    );
    assert_eq!(
        own.unanswered_report_belief,
        Some(game_parallel_economy::PracticeBelief::Believes)
    );
    player.set_practice_unanswered_belief(None).unwrap();
    assert_eq!(
        player
            .view()
            .unwrap()
            .practices
            .unwrap()
            .unanswered_report_belief,
        None
    );
    player.set_practice_willingness(true).unwrap();
    assert!(player.view().unwrap().practices.unwrap().willing);
    assert!(player.decide_practice(2, 1, true).is_err());
    assert!(world.personal_practice(2).unwrap().willing);
    assert!(
        game_parallel_economy_cli_commands::handle(
            &mut world,
            &[
                "practice-precedent",
                "1",
                "3",
                "2",
                "-1",
                "4",
                "-2",
                "0",
                "0"
            ]
        )
        .unwrap()
    );
    assert_eq!(
        world
            .personal_practice(1)
            .unwrap()
            .precedent
            .as_ref()
            .unwrap()
            .same_person,
        3
    );
    assert!(
        game_parallel_economy_cli_commands::handle(&mut world, &["practice-precedent", "1", "off"])
            .unwrap()
    );
}

#[test]
fn authored_food_misreport_is_personal_and_player_can_change_it() {
    let authored: game_parallel_opinions::Sensitivity =
        toml::from_str("person = 1\nreport_reception = { trust_at_least = -3 }").unwrap();
    assert_eq!(
        authored.report_reception,
        Some(game_parallel_economy::ReportReception::TrustAtLeast(-3))
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/parallel/economy-campaign.toml")).unwrap();
    let configured = source.replace("\r\n", "\n").replace(
        "[[knowledge.minds]]\nperson = 1",
        "[[knowledge.minds]]\nperson = 1\nmisreport = { target = 2, food_delta = -7 }",
    );
    let configured = format!(
        "{configured}\n[[opinions.sensitivities]]\nperson = 1\nreport_reception = \"never\"\ntrust_report_discrepancy = -2\n[[opinions.sensitivities.relationship_cases]]\ntarget = 2\nreport_discrepancy_trust = 3\n"
    );
    let path =
        std::env::temp_dir().join(format!("legacy-food-misreport-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let snapshot = world.snapshot();
    let own = snapshot
        .opinions
        .as_ref()
        .unwrap()
        .sensitivities
        .iter()
        .find(|s| s.person == 1)
        .unwrap();
    assert_eq!(own.trust_report_discrepancy, Some(-2));
    assert_eq!(own.relationship_cases[0].report_discrepancy_trust, Some(3));
    let mut player = game_parallel_economy_player::Player::new(&mut world);
    assert_eq!(
        player.food_misreport().unwrap(),
        Some(game_parallel_economy::Misreport {
            target: Some(2),
            food_delta: -7
        })
    );
    assert_eq!(
        player.food_report_reception().unwrap(),
        Some(game_parallel_economy::ReportReception::Never)
    );
    player
        .set_food_report_reception(Some(game_parallel_economy::ReportReception::TrustAtLeast(
            -3,
        )))
        .unwrap();
    assert_eq!(
        player.food_report_reception().unwrap(),
        Some(game_parallel_economy::ReportReception::TrustAtLeast(-3))
    );
    player.set_food_misreport(None).unwrap();
    assert_eq!(player.food_misreport().unwrap(), None);
    assert_eq!(world.food_misreport(2).unwrap(), None);
}

#[test]
fn aspiration_help_recognition_is_configurable_and_private_to_the_ruler() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/parallel/economy-campaign.toml")).unwrap();
    let configured = source.replace("[[aspirations]]", "[[aspirations]]\nrecognize_help = true");
    let path = std::env::temp_dir().join(format!("legacy-goal-help-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    for _ in 0..4 {
        if !world.snapshot().goals.as_ref().unwrap().people.is_empty() {
            break;
        }
        world.advance().unwrap();
    }
    let mut state = world.snapshot();
    let other = state
        .goals
        .as_ref()
        .unwrap()
        .people
        .iter()
        .find(|goal| goal.person == 2)
        .unwrap()
        .clone();
    assert!(other.recognize_help);
    let mut own = other.clone();
    own.person = 1;
    state.goals.as_mut().unwrap().people.push(own);
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = game_parallel_economy_player::Player::new(&mut world);
    let view = player.view().unwrap();
    assert_eq!(view.goals.len(), 1);
    assert_eq!(view.goals[0].person, 1);
    player
        .set_goal_help_recognition(other.recipe, false)
        .unwrap();
    assert!(!player.view().unwrap().goals[0].recognize_help);
    assert!(
        world
            .snapshot()
            .goals
            .unwrap()
            .people
            .iter()
            .find(|goal| goal.person == 2)
            .unwrap()
            .recognize_help
    );
}

#[test]
fn scenario_loads_a_personal_choice_to_retry_after_an_offer() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/parallel/economy-campaign.toml")).unwrap();
    let configured = source.replace("[work_requests]", "[work_requests]\naskers = [{ person = 2, interval = 6, lifetime = 5, retry_after_offer = true }]");
    let path = std::env::temp_dir().join(format!("legacy-work-retry-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(world.snapshot().autowork.unwrap().requests.unwrap().askers[0].retry_after_offer);
}

#[test]
fn campaign_loads_personal_offers_for_observed_helpers() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let state = world.snapshot();
    let preference = &state.agreements.as_ref().unwrap().work_offering[0];
    assert_eq!(preference.person, 2);
    assert!(!preference.willing);
    assert!(preference.after_refusal);
    assert_eq!(
        state
            .physical
            .goods
            .iter()
            .find(|good| good.id == preference.good)
            .unwrap()
            .name,
        "bread"
    );
    let sensitivity = state
        .opinions
        .as_ref()
        .unwrap()
        .sensitivities
        .iter()
        .find(|row| row.person == 2)
        .unwrap();
    assert_eq!(
        (
            sensitivity.affinity_work_success,
            sensitivity.affinity_work_refusal
        ),
        (Some(2), Some(-1))
    );
    let worker = state
        .autowork
        .as_ref()
        .unwrap()
        .requests
        .as_ref()
        .unwrap()
        .preferences
        .iter()
        .find(|p| p.person == 2)
        .unwrap();
    let value = worker.offer_value.as_ref().unwrap();
    assert_eq!(
        (value.good, value.value_per_unit, value.max_quantity),
        (preference.good, 5, 1)
    );
}

#[test]
fn salvage_scenario_runs_through_real_player_and_individual_actions() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-salvage.toml"),
        &root.join("content"),
    )
    .unwrap();
    let recovered = {
        let mut player = game_parallel_economy_player::Player::new(&mut world);
        let good = player.building_blueprints().unwrap()[0].kit;
        assert!(player.build_site(1).unwrap());
        player.claim_site(1).unwrap();
        assert!(player.destroy_site(1).unwrap());
        assert_eq!(player.view().unwrap().debris[0].site, 1);
        player.salvage_site(1, good, 1).unwrap()
    };
    let state = world.snapshot();
    let events = &state.construction.as_ref().unwrap().salvage_events;
    assert_eq!(events.len(), 2);
    assert_eq!(events.iter().filter(|event| event.recovered).count(), 1);
    assert_eq!(
        events
            .iter()
            .any(|event| event.person == 1 && event.recovered),
        recovered
    );
    assert!(state.construction.as_ref().unwrap().materials.is_empty());
    assert_eq!(
        state.opinions.unwrap().index().affinity(1, 2),
        if recovered { 0 } else { -2 }
    );
}

#[test]
fn player_carries_dated_goods_between_local_stores_over_two_boundaries() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-salvage.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = game_parallel_economy_player::Player::new(&mut world);
    let before = player.view().unwrap();
    assert_eq!(before.local_stores.len(), 2);
    let good = before.local_stores[0].lots[0].good;
    assert!(player.move_local_store(1, good, 1, false).unwrap());
    let carrying = player.view().unwrap();
    assert!(
        carrying
            .possessions
            .iter()
            .any(|lot| lot.good == good && lot.quantity == 1)
    );
    assert!(carrying.local_stores[1].lots.is_empty());
    assert!(player.move_local_store(2, good, 1, true).unwrap());
    let delivered = player.view().unwrap();
    assert_eq!(delivered.tick, before.tick + 2);
    assert!(
        delivered.local_stores[1]
            .lots
            .iter()
            .any(|lot| lot.good == good && lot.quantity == 1)
    );
}

#[test]
fn ruler_can_build_an_authored_site_from_an_actual_kit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/parallel/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{}\n[work_sites]\nsites = []\nrequirements = []\n[construction]\n[[construction.blueprints]]\nid = 1\nname = \"shelter workshop\"\nkind = \"workshop\"\ncapacity = 1\nkit = \"reed-shelter-kit\"\nquantity = 1\nsalvage = 1\n[[construction.demolition]]\nperson = 2\nsite = 10\nwilling = false\npressured_by = 1\npressure_threshold = 3\n[[construction.goal_builders]]\nperson = 2\nenabled = false\n",
        source
    );
    let path =
        std::env::temp_dir().join(format!("legacy-construction-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let preference = &state.construction.as_ref().unwrap().demolition[0];
    assert_eq!(
        (preference.person, preference.site, preference.pressured_by),
        (2, 10, Some(1))
    );
    assert_eq!(
        state.construction.as_ref().unwrap().goal_builders[0].person,
        2
    );
    let mut player = game_parallel_economy_player::Player::new(&mut world);
    assert_eq!(
        player.building_blueprints().unwrap()[0].name,
        "shelter workshop"
    );
    assert_eq!(player.view().unwrap().claim_built_sites, Some(false));
    player.set_built_site_claim_choice(true).unwrap();
    assert!(player.build_site(1).unwrap());
    let view = player.view().unwrap();
    assert_eq!(view.work_sites.len(), 1);
    assert_eq!(view.work_sites[0].site.kind, "workshop");
    let site = view.work_sites[0].site.id;
    assert_eq!(player.view().unwrap().site_claims, vec![site]);
    assert!(player.claim_site(site).is_err());
    assert!(player.destroy_site(site).unwrap());
    let debris = player.view().unwrap().debris;
    assert_eq!(debris.len(), 1);
    assert_eq!(debris[0].site, site);
    let good = debris[0].lots[0].good;
    assert!(player.salvage_site(site, good, 1).unwrap());
    assert!(
        player
            .view()
            .unwrap()
            .possessions
            .iter()
            .any(|lot| lot.good == good)
    );
    assert_eq!(player.view().unwrap().site_claims, vec![site]);
    player.withdraw_site_claim(site).unwrap();
    assert!(player.view().unwrap().site_claims.is_empty());
}

#[test]
fn ruler_can_assert_and_withdraw_personal_claims_without_seeing_other_minds() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let good = world
        .snapshot()
        .physical
        .goods
        .iter()
        .find(|g| g.name == "grain")
        .unwrap()
        .id;
    world.claim_stock(2, 1, good).unwrap();
    {
        let mut player = game_parallel_economy_player::Player::new(&mut world);
        let before = player.view().unwrap();
        assert!(before.claims.is_empty());
        player.claim_stock(good).unwrap();
        let after = player.view().unwrap();
        assert_eq!(after.tick, before.tick);
        assert_eq!(
            after.claims,
            vec![game_parallel_economy_player::ClaimView { region: 1, good }]
        );
        assert!(player.claim_stock(good).is_err());
    }
    let path =
        std::env::temp_dir().join(format!("legacy-personal-claim-{}.json", std::process::id()));
    save(&world, &path).unwrap();
    let mut restored = load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let mut player = game_parallel_economy_player::Player::new(&mut restored);
    assert_eq!(player.view().unwrap().claims.len(), 1);
    player.withdraw_claim(1, good).unwrap();
    assert!(player.view().unwrap().claims.is_empty());
    assert!(player.withdraw_claim(1, good).is_err());
    let state = restored.snapshot();
    let claims = &state
        .dynamics
        .as_ref()
        .unwrap()
        .claims
        .as_ref()
        .unwrap()
        .claims;
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].person, 2);
}

#[test]
fn authored_site_claims_remain_personal_and_private_to_the_ruler() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/parallel/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{source}\n[work_sites]\nsites = [{{ id = 7, region = 1, kind = \"workshop\", capacity = 1 }}]\nrequirements = []\n[[dynamics.claims.site_claims]]\nperson = 2\nsite = 7\n[[opinions.sensitivities]]\nperson = 1\naffinity_site_use = 3\naffinity_site_destruction = -2\n[[opinions.sensitivities.relationship_cases]]\ntarget = 2\nsite_use = -4\n"
    );
    let path = std::env::temp_dir().join(format!("legacy-site-claims-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let snapshot = world.snapshot();
    let sensitivity = snapshot
        .opinions
        .as_ref()
        .unwrap()
        .sensitivities
        .iter()
        .find(|s| s.person == 1)
        .unwrap();
    assert_eq!(
        (
            sensitivity.affinity_site_use,
            sensitivity.affinity_site_destruction
        ),
        (Some(3), Some(-2))
    );
    assert_eq!(sensitivity.relationship_cases[0].site_use, Some(-4));
    let mut player = game_parallel_economy_player::Player::new(&mut world);
    assert!(player.view().unwrap().site_claims.is_empty());
    player.claim_site(7).unwrap();
    assert_eq!(player.view().unwrap().site_claims, vec![7]);
    player.withdraw_site_claim(7).unwrap();
    assert!(player.view().unwrap().site_claims.is_empty());
    let snapshot = world.snapshot();
    assert_eq!(
        snapshot
            .dynamics
            .as_ref()
            .unwrap()
            .claims
            .as_ref()
            .unwrap()
            .site_claims[0]
            .person,
        2
    );
}
