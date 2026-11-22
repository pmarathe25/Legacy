use game_economy::core_interval::CoreHost;
use game_economy_player::interval::Player;
use game_lots::Place;
use std::path::Path;

#[test]
fn named_inherited_vigor_limits_actual_work_without_assigning_it() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let source = content.join("scenarios/economy-named-work-vigor.toml");
    let run = |workers, partitions: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = game_economy_cli::core_scenario(&source, &content).unwrap();
                let initial = host.snapshot();
                let physical = &initial.core.physical;
                let recipe = physical
                    .recipes
                    .iter()
                    .find(|row| row.name == "fixture-tool")
                    .unwrap();
                assert!(
                    Player::new(&mut host, 1)
                        .unwrap()
                        .start_work(recipe.id, 1)
                        .unwrap()
                        .is_none()
                );
                let job = Player::new(&mut host, 2)
                    .unwrap()
                    .start_work(recipe.id, 1)
                    .unwrap()
                    .unwrap();
                for &elapsed in partitions {
                    let outcome = host.advance_continuous(elapsed, &[]).unwrap();
                    assert_eq!(outcome.failure, None);
                    let saved = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&saved).unwrap()).unwrap();
                }
                let world = host.snapshot();
                let work = &world.core.physical.work;
                assert_eq!(work.len(), 1);
                assert_eq!(work[0].job.worker, 2);
                let holder = world
                    .core
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::JobInput(job))
                    .unwrap();
                assert_eq!(holder.used, 5);
                assert_eq!(
                    world.work_capabilities.as_ref().unwrap().vigor_locus,
                    Some(27)
                );
                let mut invalid = world.clone();
                invalid.work_capabilities.as_mut().unwrap().vigor_locus = Some(99);
                assert!(
                    CoreHost::resume(invalid)
                        .err()
                        .unwrap()
                        .contains("known family locus")
                );
                world
            })
    };
    assert_eq!(run(1, &[60000]), run(6, &[17, 1298, 58685]));
}
