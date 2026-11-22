use game_economy::core_interval::CoreHost;
use game_lots::Place;
use std::path::Path;

#[test]
fn authored_goods_fit_real_cargo_storage_and_work_reservations() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scene = content.join("scenarios/economy-physical-footprints.toml");
    let run = |workers, parts: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = game_economy_cli::core_scenario(&scene, &content).unwrap();
                let start = host.snapshot();
                let good = |name| {
                    start
                        .core
                        .physical
                        .goods
                        .iter()
                        .find(|good| good.name == name)
                        .unwrap()
                        .id
                };
                let timber = good("timber");
                let tool = good("tool");
                let recipe = start
                    .core
                    .physical
                    .recipes
                    .iter()
                    .find(|recipe| {
                        recipe
                            .inputs
                            .iter()
                            .any(|input| input.good == timber && input.quantity == 5)
                            && recipe.outputs.iter().any(|output| output.good == tool)
                    })
                    .unwrap()
                    .id;
                let holdings = |host: &CoreHost, place| {
                    let state = host.snapshot();
                    let holder = state
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == place)
                        .unwrap();
                    let quantity = holder.lots.iter().map(|lot| lot.quantity).sum::<u64>();
                    (holder.capacity, holder.used, quantity)
                };
                assert!(
                    !host
                        .carry_at_controlled_store(1, 1, good("bread"), 2, false)
                        .unwrap()
                );
                assert!(
                    host.carry_at_controlled_store(1, 1, timber, 1, true)
                        .unwrap()
                );
                assert_eq!(holdings(&host, Place::Inventory(1)), (7, 7, 3));
                let carrier = host.carry_locally_autonomously(&[]).unwrap();
                assert!(
                    carrier
                        .events
                        .iter()
                        .any(|row| row.person == 2 && row.moved && row.quantity == 1)
                );
                assert_eq!(holdings(&host, Place::Person(2)), (7, 5, 2));

                let (job, started) = host.start_controlled_work(1, recipe, 1).unwrap();
                assert!(started);
                assert_eq!(holdings(&host, Place::JobInput(job)), (15, 15, 5));
                assert_eq!(holdings(&host, Place::JobOutput(job)), (5, 0, 0));
                for &part in parts {
                    assert_eq!(host.advance_continuous(part, &[]).unwrap().failure, None);
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                let stock = Place::Stock {
                    region: 1,
                    good: tool,
                };
                assert_eq!(holdings(&host, stock), (40, 5, 1));
                host.snapshot()
            })
    };
    assert_eq!(
        run(1, &[7_200_000]),
        run(6, &[173_001, 3_526_999, 3_500_000])
    );
}
