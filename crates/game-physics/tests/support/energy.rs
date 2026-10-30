#[cfg(test)]
mod tests {
    use game_physics::*;
    fn lever(input: Option<usize>, drive: u32, load: u32) -> Transmission {
        Transmission {
            input,
            drive,
            load,
            efficiency: 1000,
            limit: 1000000,
        }
    }
    #[test]
    fn compound_transmissions_trade_distance_for_force_without_creating_work() {
        let result = resolve(
            &[lever(None, 2, 1), lever(Some(0), 3, 1)],
            Effort {
                force: 100,
                travel: 600,
            },
        )
        .unwrap();
        assert_eq!(
            result.outputs[1],
            Effort {
                force: 600,
                travel: 100
            }
        );
        assert_eq!(result.delivered_work, 60000);
    }
    #[test]
    fn branches_share_one_source_instead_of_copying_its_energy() {
        let parts = [
            lever(None, 1, 1),
            lever(Some(0), 3, 1),
            lever(Some(0), 1, 2),
        ];
        let result = resolve(
            &parts,
            Effort {
                force: 101,
                travel: 600,
            },
        )
        .unwrap();
        assert!(result.delivered_work <= 60600);
        assert_eq!(result.outputs[1].force, 150);
        assert_eq!(result.outputs[2].force, 25);
    }
    #[test]
    fn a_strong_operator_uses_only_the_effort_needed_for_a_light_load() {
        let mut part = lever(None, 10, 1);
        part.limit = 5000;
        let (applied, result) = resolve_loaded(
            &[part],
            Effort {
                force: 10000,
                travel: 1000,
            },
            2000,
            0,
        )
        .unwrap();
        assert_eq!(applied.force, 200);
        assert_eq!(result.outputs[0].force, 2000);
        assert!(result.broken.is_empty());
        assert!(result.delivered_work <= applied.work());
    }
    #[test]
    fn weak_materials_fail_and_disconnect_downstream_parts() {
        let mut weak = lever(None, 10, 1);
        weak.limit = 500;
        let result = resolve(
            &[weak, lever(Some(0), 2, 1)],
            Effort {
                force: 100,
                travel: 1000,
            },
        )
        .unwrap();
        assert_eq!(result.broken, vec![0]);
        assert_eq!(result.outputs[1].work(), 0);
    }
    #[test]
    fn malformed_cycles_and_zero_dimensions_are_rejected() {
        assert!(resolve(&[lever(Some(0), 1, 1)], Effort::default()).is_err());
        assert!(resolve(&[lever(None, 0, 1)], Effort::default()).is_err());
    }
    #[test]
    fn rounding_friction_and_fanout_never_create_energy() {
        for force in [0, 1, 99, 100000, u64::MAX] {
            for travel in [0, 1, 97, 100000, u64::MAX] {
                let mut stage = lever(None, 7, 3);
                stage.efficiency = 777;
                stage.limit = u64::MAX;
                let result = resolve(
                    &[stage, lever(Some(0), 1, 1), lever(Some(0), 1, 1)],
                    Effort { force, travel },
                )
                .unwrap();
                assert!(result.delivered_work <= u128::from(force) * u128::from(travel));
            }
        }
    }
}
