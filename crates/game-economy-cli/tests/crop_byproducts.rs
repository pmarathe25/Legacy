use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_economy_agriculture::Phase;
use game_lots::Place;
use std::path::Path;

#[test]
fn harvest_places_all_products_in_real_stock_or_keeps_the_field_ripe() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scene = content.join("scenarios/economy-crop-byproducts.toml");
    let run = |workers, parts: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = game_economy_cli::core_scenario(&scene, &content).unwrap();
                let initial = host.snapshot();
                let good = |name| {
                    initial
                        .core
                        .physical
                        .goods
                        .iter()
                        .find(|good| good.name == name)
                        .unwrap()
                        .id
                };
                let grain = good("grain");
                let straw = good("crop-residues:straw");
                assert_eq!(
                    host.cultivate_controlled(2, 2, None).unwrap(),
                    Some(Phase::Growing)
                );
                let mut harvested = Vec::new();
                for &part in parts {
                    let advanced = host.advance_continuous(part, &[]).unwrap();
                    assert_eq!(advanced.failure, None);
                    for event in advanced.events {
                        if let CoreRunEvent::HarvestProducts { at_millis, events } = event {
                            harvested.extend(events.into_iter().map(|row| (at_millis, row)));
                        }
                    }
                    let checkpoint = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&checkpoint).unwrap()).unwrap();
                }
                assert!(host.cultivate_controlled(2, 2, None).unwrap().is_none());
                let state = host.snapshot();
                let stock = |region, good| {
                    let place = Place::Stock { region, good };
                    state
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|holder| holder.place == place)
                        .unwrap()
                        .lots
                        .iter()
                        .map(|lot| lot.quantity)
                        .sum::<u64>()
                };
                assert_eq!(stock(1, grain), 100);
                assert_eq!(stock(1, straw), 8);
                assert_eq!(stock(2, grain), 0);
                assert_eq!(stock(2, straw), 0);
                assert_eq!(
                    state.agriculture.as_ref().unwrap().fields[1].phase,
                    Phase::Ripe
                );
                assert_eq!(harvested.len(), 1);
                let (at, row) = &harvested[0];
                assert_eq!((*at, row.person, row.field), (86_400_000, 1, 1));
                assert_eq!(
                    row.products
                        .iter()
                        .map(|amount| (amount.good, amount.quantity))
                        .collect::<Vec<_>>(),
                    [(grain, 100), (straw, 8)]
                );
                let mut controlled = state;
                controlled.core.people[0].controlled = true;
                host = CoreHost::resume(controlled).unwrap();
                assert!(host.start_controlled_work(1, 210103, 1).unwrap().1);
                assert_eq!(
                    host.advance_continuous(7_200_000, &[]).unwrap().failure,
                    None
                );
                let finished = host.snapshot();
                let mat = good("reed-mat");
                let product = |good| {
                    finished
                        .core
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|holder| holder.place == (Place::Stock { region: 1, good }))
                        .unwrap()
                        .lots
                        .iter()
                        .map(|lot| lot.quantity)
                        .sum::<u64>()
                };
                assert_eq!(product(mat), 1);
                assert_eq!(product(straw), 0);
                (finished, harvested)
            })
    };
    assert_eq!(run(1, &[86_400_001]), run(6, &[13, 20_000_000, 66_399_988]));
}
