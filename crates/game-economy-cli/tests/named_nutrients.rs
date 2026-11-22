use game_economy::core_interval::CoreHost;
use game_lots::Place;
use std::path::Path;

#[test]
fn an_authored_nutrient_changes_reserves_only_after_consumption() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scenario = content.join("scenarios/economy-named-nutrients.toml");
    let run = |workers, parts: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = game_economy_cli::core_scenario(&scenario, &content).unwrap();
                let opening = host.snapshot();
                let good = |name| {
                    opening
                        .core
                        .physical
                        .goods
                        .iter()
                        .find(|row| row.name == name)
                        .unwrap()
                        .id
                };
                let leaves = good("leafy-produce:folate-leaves");
                let bread = good("bread");
                assert!(!host.eat_controlled(2, leaves).unwrap());
                assert!(host.snapshot().core.nutrition.unwrap().people.is_empty());
                assert!(host.eat_controlled(1, leaves).unwrap());
                assert!(host.eat_controlled(2, bread).unwrap());
                let fed = host.snapshot();
                let nutrition = fed.core.nutrition.as_ref().unwrap();
                assert_eq!(nutrition.people[0].named, [(1, 8)]);
                assert!(nutrition.people[1].named.is_empty());
                assert!(fed.core.pending_food.is_empty());
                assert_eq!(
                    fed.core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|holder| holder.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                for &part in parts {
                    assert_eq!(host.advance_continuous(part, &[]).unwrap().failure, None);
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                let state = host.snapshot();
                let nutrition = state.core.nutrition.as_ref().unwrap();
                assert_eq!(nutrition.people[0].named, [(1, 2)]);
                assert_eq!(nutrition.people[1].named, [(1, 0)]);
                assert_eq!(
                    state
                        .core
                        .people
                        .iter()
                        .map(|row| row.health)
                        .collect::<Vec<_>>(),
                    [100, 91]
                );
                let mut forged = state.clone();
                forged.core.nutrition.as_mut().unwrap().people[0].named[0].0 = 999;
                assert!(CoreHost::resume(forged).is_err());
                state
            })
    };
    assert_eq!(run(1, &[350]), run(6, &[17, 83, 39, 160, 51]));
}
