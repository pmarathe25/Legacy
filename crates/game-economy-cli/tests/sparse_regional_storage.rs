use game_economy::core_interval::CoreHost;
use game_economy_player::interval::Player;
use game_lots::Place;
use std::path::Path;

#[test]
fn stockless_regions_remain_reachable_and_jobs_keep_undeliverable_output() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scenario = root.join("content/scenarios/economy-sparse-regional-storage.toml");
    let run = |workers, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host =
                    game_economy_cli::core_scenario(&scenario, &root.join("content")).unwrap();
                let opening = host.snapshot();
                let physical = &opening.core.physical;
                assert_eq!(physical.region_ids.as_deref().unwrap(), [1, 2, 3]);
                assert!(physical.goods.len() > 100);
                let stock = |place: Place| matches!(place, Place::Stock { .. });
                assert_eq!(
                    physical
                        .custody
                        .containers
                        .iter()
                        .filter(|row| stock(row.place))
                        .count(),
                    2
                );
                let bread = physical
                    .goods
                    .iter()
                    .find(|good| good.name == "bread")
                    .unwrap()
                    .id;
                let recipe = physical
                    .recipes
                    .iter()
                    .find(|recipe| recipe.name == "fixture-tool")
                    .unwrap();
                let (recipe_id, tool) = (recipe.id, recipe.outputs[0].good);
                let (job, accepted) = host.start_controlled_work(1, recipe_id, 1).unwrap();
                assert!(accepted);
                for &elapsed in chunks {
                    assert_eq!(host.advance_continuous(elapsed, &[]).unwrap().failure, None);
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                let settled = host.snapshot();
                let location = &settled.core.physical.custody.containers;
                let output = location
                    .iter()
                    .find(|row| row.place == Place::JobOutput(job))
                    .unwrap();
                assert_eq!(
                    output
                        .lots
                        .iter()
                        .filter(|lot| lot.good == tool)
                        .map(|lot| lot.quantity)
                        .sum::<u64>(),
                    1
                );
                assert!(location.iter().all(|row| row.place
                    != (Place::Stock {
                        region: 1,
                        good: tool
                    })));
                let empty = location
                    .iter()
                    .find(|row| {
                        row.place
                            == (Place::Stock {
                                region: 3,
                                good: bread,
                            })
                    })
                    .unwrap();
                assert_eq!((empty.capacity, empty.lots.len()), (2, 0));
                let known = settled.knowledge.unwrap();
                assert!(known.minds.iter().all(|mind| {
                    mind.records
                        .iter()
                        .any(|row| row.region == 2 && row.food == 0)
                }));
                assert!(Player::new(&mut host, 1).unwrap().depart(2).unwrap());
                assert_eq!(host.advance_continuous(20, &[]).unwrap().failure, None);
                assert_eq!(host.snapshot().core.people[0].region, Some(2));
                assert!(!host.start_controlled_work(1, recipe_id, 1).unwrap().1);
                serde_json::to_vec(&host.snapshot()).unwrap()
            })
    };
    assert_eq!(
        run(1, &[7_200_005]),
        run(6, &[60_000, 1_222_222, 5_917_783])
    );
}
