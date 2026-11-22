use game_economy::core_interval::{CoreHost, CoreRunEvent};
use std::path::Path;

#[test]
fn travelers_may_prefer_food_proximity_staying_or_reconsidering() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let path = content.join("scenarios/economy-personal-food-travel.toml");
    let run = |workers, slices: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = game_economy_cli::core_scenario(&path, &content).unwrap();
                let mut departures = Vec::new();
                for &elapsed in slices {
                    let advance = host.advance_continuous(elapsed, &[]).unwrap();
                    assert_eq!(advance.failure, None);
                    for event in advance.events {
                        if let CoreRunEvent::FoodSearch(rows) = event {
                            departures
                                .extend(rows.into_iter().map(|row| (row.person, row.departed)));
                        }
                    }
                    let checkpoint = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&checkpoint).unwrap()).unwrap();
                }
                let world = host.snapshot();
                let locations: Vec<_> =
                    world.core.people.iter().map(|p| (p.id, p.region)).collect();
                assert_eq!(
                    locations,
                    [
                        (1, Some(1)),
                        (2, Some(3)),
                        (3, Some(2)),
                        (4, Some(1)),
                        (5, Some(4)),
                    ]
                );
                let starts = |person| {
                    departures
                        .iter()
                        .filter(|&&row| row == (person, true))
                        .count()
                };
                assert_eq!((starts(2), starts(3), starts(4), starts(5)), (2, 1, 0, 1));
                let search = &world.food_search.as_ref().unwrap().search;
                assert_eq!(
                    search.people.iter().find(|p| p.person == 5).unwrap().target,
                    None
                );
                (departures, world)
            })
    };
    assert_eq!(run(1, &[100]), run(6, &[7, 12, 16, 4, 2, 59]));
}
