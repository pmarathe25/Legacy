use game_economy::core_interval::CoreHost;
use std::path::Path;

#[test]
fn connected_regions_can_cross_their_crops_on_the_elapsed_growth_clock() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scenario = root.join("content/scenarios/economy-pollen-corridor.toml");
    let opening = game_economy_cli::core_scenario(&scenario, &root.join("content"))
        .unwrap()
        .snapshot();
    let run = |workers, seed, growth_parts: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = opening.clone();
                state.core.seed = seed;
                let mut host = CoreHost::resume(state).unwrap();
                assert_eq!(host.advance_continuous(41, &[]).unwrap().failure, None);
                let mut state = host.snapshot();
                let farm = state.agriculture.as_mut().unwrap();
                assert_eq!(farm.provenance.as_ref().unwrap().plantings.len(), 2);
                for policy in &mut farm.policies {
                    policy.willing = false;
                }
                host = CoreHost::resume(state).unwrap();
                for &part in growth_parts {
                    assert_eq!(host.advance_continuous(part, &[]).unwrap().failure, None);
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                let farm = host.snapshot().agriculture.unwrap();
                let records = farm.provenance.unwrap().plantings;
                let first = &records[0].offspring.as_ref().unwrap()[0];
                assert_eq!(
                    (first.parent, first.pollen_parent, first.alleles),
                    (Some(11), Some(11), Some([0, 0]))
                );
                records[1].offspring.as_ref().unwrap()[0].clone()
            })
    };
    let (seed, child) = (1..32)
        .find_map(|seed| {
            let offspring = run(1, seed, &[86_400_019]);
            (offspring.pollen_parent == Some(11)).then_some((seed, offspring))
        })
        .expect("one physically seeded crop receives pollen across the corridor");
    assert_eq!(child.parent, Some(12));
    assert_eq!(child.alleles, Some([1000, 0]));
    assert_eq!(child, run(6, seed, &[19, 187, 86_399_813]));
}
