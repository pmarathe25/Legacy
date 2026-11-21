use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_economy_player::interval::Player;
use game_lots::Place;
use std::path::Path;

fn meadow(quantum: u64) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut state = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-wild-plants.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    state.core.physical.tick_millis = quantum;
    state
        .gathering
        .as_mut()
        .unwrap()
        .wild_plants
        .as_mut()
        .unwrap()
        .period_millis = 120;
    CoreHost::resume(state).unwrap()
}

#[test]
fn wild_population_and_seed_banks_follow_elapsed_deadlines_across_save_and_workers() {
    let run = |workers, quantum, steps: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = meadow(quantum);
                let mut observed = Vec::new();
                for &step in steps {
                    let advanced = host.advance_continuous(step, &[]).unwrap();
                    assert_eq!(advanced.failure, None);
                    observed.extend(advanced.events.into_iter().filter_map(|event| match event {
                        CoreRunEvent::WildPlants(cycle) => Some(cycle),
                        _ => None,
                    }));
                    let saved = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&saved).unwrap()).unwrap();
                }
                assert_eq!(
                    observed.iter().map(|row| row.at_millis).collect::<Vec<_>>(),
                    (1..=8).map(|cycle| cycle * 120).collect::<Vec<_>>()
                );
                let gathering = host.snapshot().gathering.unwrap();
                let wild = gathering.wild_plants.as_ref().unwrap();
                assert!(wild.stands[0].cohorts[0].adults > 90);
                assert!(wild.stands[1].cohorts[1].adults > 90);
                for (stand, resource) in wild.stands.iter().zip(&gathering.nodes) {
                    assert_eq!(stand.available(), resource.available);
                    assert!(stand.available() <= resource.capacity);
                }
                (observed, gathering)
            })
    };
    assert_eq!(run(1, 1000, &[973]), run(6, 333, &[23, 107, 293, 550]));
}

#[test]
fn a_real_wild_seed_withdrawal_preserves_its_source_batch_and_a_failed_one_does_not_deplete_it() {
    let mut state = meadow(5000).snapshot();
    state.core.rules.carrying_capacity = 1;
    state
        .core
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Person(1))
        .unwrap()
        .capacity = 1;
    let mut host = CoreHost::resume(state).unwrap();
    let mut player = Player::new(&mut host, 1).unwrap();
    let receipt = player.gather(10).unwrap().unwrap();
    let batch = receipt.batch.expect("wild seed provenance");
    assert!(player.gather(10).unwrap().is_none());
    assert!(player.gather(20).is_err());
    let snapshot = host.snapshot();
    let cargo = snapshot
        .core
        .physical
        .custody
        .containers
        .iter()
        .find(|row| row.place == Place::Person(1))
        .unwrap();
    assert_eq!(cargo.used, 1);
    assert_eq!(cargo.lots[0].batch, Some(batch));
    let wild = snapshot.gathering.unwrap().wild_plants.unwrap();
    assert_eq!(wild.stands[0].available(), 199);
    assert_eq!(wild.stands[1].available(), 200);
    for cohort in &wild.stands[0].cohorts {
        assert_eq!(
            cohort.seeds,
            if cohort.strain.batch == batch {
                99
            } else {
                100
            }
        );
    }
}

#[test]
fn a_gathered_wild_strain_affects_the_crop_that_uses_its_actual_seeds() {
    use game_economy::core_interval::CoreCropPhase;
    use std::collections::BTreeMap;

    let mut host = meadow(5000);
    let mut player = Player::new(&mut host, 1).unwrap();
    let mut collected = BTreeMap::new();
    for _ in 0..20 {
        let seed = player.gather(10).unwrap().unwrap();
        *collected.entry(seed.batch.unwrap()).or_insert(0) += seed.quantity;
    }
    let batch = collected.into_iter().find(|row| row.1 >= 10).unwrap().0;
    assert_eq!(
        player.cultivate(1, Some(batch)).unwrap(),
        Some(CoreCropPhase::Growing)
    );
    let farm = host.snapshot().agriculture.unwrap();
    let germination = &farm.provenance.unwrap().plantings[0];
    assert_eq!(germination.planter, Some(1));
    assert_eq!(germination.seeds[0].batch, Some(batch));
    assert_eq!(farm.fields[0].remaining, if batch == 150 { 12 } else { 36 });
}
