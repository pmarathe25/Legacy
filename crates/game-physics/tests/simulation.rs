use game_core::random;
use game_physics::*;
#[test]
fn loaded_control_does_not_confuse_an_overflowing_capacity_probe_with_insufficient_power() {
    let parts = [Transmission {
        input: None,
        drive: 3,
        load: 1,
        efficiency: 1000,
        limit: u64::MAX,
    }];
    for load in [999, u64::MAX - 2] {
        let (applied, result) = resolve_loaded(
            &parts,
            Effort {
                force: u64::MAX,
                travel: 300,
            },
            load,
            0,
        )
        .unwrap();
        assert!(result.outputs[0].force >= load);
        assert!(result.broken.is_empty());
        assert_eq!(applied.force, load.div_ceil(3));
    }
}
#[test]
fn seeded_network_simulations_conserve_work_across_branching_friction_and_load_control() {
    let mut cases = 0;
    let mut loaded = 0;
    let mut broken = 0;
    for seed in [1, 31, 77, 819, 2026] {
        for trial in 0..4000 {
            let n = 1 + (random(seed, trial, 1) % 32) as usize;
            let parts: Vec<_> = (0..n)
                .map(|i| {
                    let r = random(seed, trial, 10 + i as u64);
                    Transmission {
                        input: if i == 0 || r.is_multiple_of(5) {
                            None
                        } else {
                            Some(r as usize % i)
                        },
                        drive: 1 + (r % 1000) as u32,
                        load: 1 + ((r >> 10) % 1000) as u32,
                        efficiency: ((r >> 20) % 1001) as u16,
                        limit: if r.is_multiple_of(7) {
                            u64::MAX
                        } else {
                            1 + (r >> 30) % 100000
                        },
                    }
                })
                .collect();
            let f = [0, 1, 1000, 64000, u64::MAX][(trial % 5) as usize];
            let t = [0, 1, 1000, 1000000, u64::MAX][((trial / 5) % 5) as usize];
            let source = Effort {
                force: f,
                travel: t,
            };
            let result = resolve(&parts, source).unwrap();
            assert!(result.delivered_work <= source.work());
            broken += result.broken.len();
            cases += 1;
            if trial % 4 == 0 {
                let (applied, result) =
                    resolve_loaded(&parts, source, 1 + random(seed, trial, 77) % 100000, n - 1)
                        .unwrap();
                assert!(applied.force <= source.force);
                assert!(result.delivered_work <= applied.work());
                loaded += 1;
            }
        }
    }
    if let Some(folder) = std::env::var_os("LEGACY_PHYSICS_REPORT_DIR") {
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(std::path::PathBuf::from(folder).join("networks.json"),format!("{{\"seeds\":5,\"networks\":{cases},\"loaded_trials\":{loaded},\"broken_components\":{broken},\"work_conservation\":true}}\n")).unwrap();
    }
    assert_eq!(cases, 20000);
    assert_eq!(loaded, 5000);
}

#[path = "support/energy.rs"]
mod energy;
#[path = "support/simple_machines.rs"]
mod simple_machines;
