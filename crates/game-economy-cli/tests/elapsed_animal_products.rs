use game_economy::AnimalProductOutcome;
use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_lots::Place;
use std::path::Path;

#[test]
fn fleece_grows_on_ecological_deadlines_and_only_an_accepted_yield_resets_it() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scene = content.join("scenarios/economy-elapsed-animal-products.toml");
    let run = |workers, quantum, parts: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = game_economy_cli::core_scenario(&scene, &content)
                    .unwrap()
                    .snapshot();
                state.core.physical.tick_millis = quantum;
                let mut host = CoreHost::resume(state).unwrap();
                let mut products = Vec::new();
                for &part in parts {
                    let advanced = host.advance_continuous(part, &[]).unwrap();
                    assert_eq!(advanced.failure, None);
                    for event in advanced.events {
                        if let CoreRunEvent::ElapsedAnimalProducts { at_millis, events } = event {
                            products.extend(
                                events
                                    .into_iter()
                                    .map(|row| (at_millis, row.animal, row.outcome)),
                            );
                        }
                    }
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                assert_eq!(
                    products,
                    [
                        (204, 1, AnimalProductOutcome::Collected),
                        (204, 2, AnimalProductOutcome::Refused),
                        (204, 3, AnimalProductOutcome::NoCapacity),
                    ]
                );
                let saved = host.snapshot();
                assert_eq!(
                    saved
                        .animal_products
                        .unwrap()
                        .progress
                        .into_iter()
                        .map(|row| row.growth)
                        .collect::<Vec<_>>(),
                    [0, 2, 2]
                );
                let used = |person| {
                    saved
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(person))
                        .unwrap()
                        .used
                };
                assert_eq!((used(1), used(2), used(3)), (6, 0, 8));
                products
            })
    };
    assert_eq!(run(1, 37, &[205]), run(6, 159, &[29, 72, 100, 4]));
}
