use game_economy::core_interval::{CoreChoiceOutcome, CoreHost, CoreRunEvent};
use game_economy_agriculture::Phase;
use game_lots::Place;
use game_personal_choice::{Person, Preference};
use std::path::Path;

fn farm(quantum: u64) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = std::fs::read_to_string(
        root.join("content/scenarios/economy-core-seasonal-seeds.toml"),
    )
    .unwrap()
    .replace(
        "tick_millis = 900000",
        &format!("tick_millis = {quantum}\ndecision_period_millis = 250\nelapsed_spoilage = true"),
    )
    .replace("[cultivation]", "[cultivation]\ngrowth_unit_millis = 1000")
    .replace("[environment]", "[environment]\nseason_unit_millis = 1000")
    .replace(
        "policies = [{ person = 1, willing = true }]",
        "policies = [{ person = 1, willing = true }, { person = 2, willing = true }, { person = 3, willing = true }]",
    )
    .replace(
        "season_effects = [{ season = \"warm\", weight = 1000 }]",
        "",
    )
    .replace(
        "[cultivation]",
        r#"
[[people]]
id = 2
region = 1
health = 100
hunger = 0
willing_to_work = true
seek_meals = false

[[people]]
id = 3
region = 1
health = 100
hunger = 0
willing_to_work = true
seek_meals = false

[cultivation]"#,
    );
    let path = std::env::temp_dir().join(format!(
        "legacy-elapsed-cultivation-{}-{quantum}.toml",
        std::process::id(),
    ));
    std::fs::write(&path, source).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    let mut state = loaded.unwrap().snapshot();
    let farm = state.agriculture.as_mut().unwrap();
    let grain = farm.fields[0].crop;
    farm.crops
        .iter_mut()
        .find(|crop| crop.id == grain)
        .unwrap()
        .growth_ticks = 3;
    farm.growth_clock.as_mut().unwrap().unit_millis = 100;
    let provenance = farm.provenance.as_mut().unwrap();
    let mut neighbor = provenance.profiles[0].clone();
    neighbor.person = 2;
    std::sync::Arc::make_mut(&mut provenance.profiles).push(neighbor);

    let mut host = CoreHost::resume(state).unwrap();
    host.enable_personal_choices(
        (1..=3)
            .map(|id| Person {
                id,
                idle: 0,
                act_on_tie: false,
                preferences: vec![Preference {
                    domain: 11,
                    enabled: id != 3,
                    bias: 10,
                    weight: 0,
                }],
                actions: vec![],
            })
            .collect(),
    )
    .unwrap();
    host
}

#[test]
fn planting_harvest_and_seed_provenance_publish_at_the_personal_decision() {
    let run = |workers, quantum, steps: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = farm(quantum);
                let mut actions = Vec::new();
                let mut choices = Vec::new();
                for &step in steps {
                    let advanced = host.advance_continuous(step, &[]).unwrap();
                    assert_eq!(advanced.failure, None);
                    for event in advanced.events {
                        match event {
                            CoreRunEvent::ElapsedCultivation { at_millis, fields } => {
                                actions.extend(fields.into_iter().map(|row| (at_millis, row)));
                            }
                            CoreRunEvent::PersonalChoices(rows) => choices.extend(
                                rows.into_iter()
                                    .filter(|row| row.key.is_some_and(|key| key.domain == 11))
                                    .map(|row| (row.actor, row.outcome)),
                            ),
                            _ => {}
                        }
                    }
                    let snapshot = host.snapshot();
                    if snapshot.core.boundary_millis == Some(0) && step == 81 {
                        let owner = snapshot.agriculture.as_ref().unwrap();
                        assert_eq!(owner.fields[0].phase, Phase::Growing);
                        let record = &owner.provenance.as_ref().unwrap().plantings[0];
                        assert_eq!(record.planted_at_millis, Some(0));
                        assert_eq!(record.seeds[0].batch, Some(12));
                    }
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap(),
                    )
                    .unwrap();
                }
                assert_eq!(
                    actions
                        .iter()
                        .map(|(time, action)| (*time, action.phase))
                        .collect::<Vec<_>>(),
                    [(0, Phase::Fallow), (500, Phase::Ripe)],
                );
                assert_eq!(choices.len(), 4);
                assert_eq!(
                    choices
                        .iter()
                        .filter(|row| row.1 == CoreChoiceOutcome::Accepted)
                        .count(),
                    2
                );
                assert_eq!(
                    choices
                        .iter()
                        .filter(|row| row.1 == CoreChoiceOutcome::Denied)
                        .count(),
                    2
                );
                assert!(choices.iter().all(|row| row.0 != 3));

                let snapshot = host.snapshot();
                let owner = snapshot.agriculture.unwrap();
                let field = &owner.fields[0];
                assert_eq!(field.phase, Phase::Fallow);
                assert!(owner.provenance.unwrap().plantings.is_empty());
                let crop = owner
                    .crops
                    .iter()
                    .find(|crop| crop.id == field.crop)
                    .unwrap();
                let stock = |good| {
                    snapshot
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Stock { region: 1, good })
                        .unwrap()
                };
                assert_eq!(stock(crop.output).used, 100);
                let seeds = &stock(crop.seed).lots;
                assert_eq!(seeds.iter().map(|row| row.quantity).sum::<u64>(), 20);
                assert!(!seeds.iter().any(|row| row.batch == Some(12)));
                assert!(
                    seeds
                        .iter()
                        .any(|row| row.batch.is_some_and(|batch| batch > 12))
                );
                (actions, choices, seeds.clone())
            })
    };
    assert_eq!(run(1, 800, &[735]), run(6, 400, &[81, 300, 125, 229]));
}

#[test]
fn a_player_can_plant_a_carried_seed_batch_and_harvest_the_field() {
    use game_economy_player::interval::Player;
    use game_lots::Container;

    let mut state = farm(800).snapshot();
    state.core.people[0].controlled = true;
    let owner = state.agriculture.as_ref().unwrap();
    let crop = owner
        .crops
        .iter()
        .find(|crop| crop.id == owner.fields[0].crop)
        .unwrap();
    let seed_stock = Place::Stock {
        region: 1,
        good: crop.seed,
    };
    let stock = state
        .core
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == seed_stock)
        .unwrap();
    let mut lots = stock.lots.clone();
    let index = lots.iter().position(|lot| lot.batch == Some(12)).unwrap();
    let seed = lots.remove(index);
    *stock = Container::new(stock.place, stock.capacity, lots).unwrap();
    let cargo = state
        .core
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Person(1))
        .unwrap();
    *cargo = Container::new(cargo.place, cargo.capacity, vec![seed]).unwrap();
    let mut host = CoreHost::resume(state).unwrap();

    let mut player = Player::new(&mut host, 1).unwrap();
    assert_eq!(
        player.view().unwrap().accessible_fields[0].phase,
        Phase::Fallow
    );
    assert!(player.cultivate(1234, Some(12)).is_err());
    assert_eq!(player.cultivate(1, Some(11)).unwrap(), None);
    assert_eq!(player.cultivate(1, Some(12)).unwrap(), Some(Phase::Growing));
    assert!(player.view().unwrap().possessions.is_empty());
    let planted = host.snapshot().agriculture.unwrap().provenance.unwrap();
    assert_eq!(planted.plantings[0].seeds[0].batch, Some(12));
    assert_eq!(planted.plantings[0].planter, Some(1));
    assert_eq!(host.advance_continuous(301, &[]).unwrap().failure, None);

    let mut player = Player::new(&mut host, 1).unwrap();
    assert!(player.cultivate(1, Some(12)).is_err());
    assert_eq!(player.cultivate(1, None).unwrap(), Some(Phase::Fallow));
    assert!(
        host.snapshot()
            .agriculture
            .unwrap()
            .provenance
            .unwrap()
            .plantings
            .is_empty()
    );
}
