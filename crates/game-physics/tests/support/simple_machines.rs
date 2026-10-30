use game_core::*;
use game_physics::*;
fn config() -> Config {
    Config {
        materials: [(
            GoodId(1),
            Material {
                span: 32,
                limit: 100000,
                efficiency: 1000,
                supports: Default::default(),
            },
        )]
        .into(),
        supports: Default::default(),
    }
}
#[test]
fn a_crank_gear_drum_and_block_tackle_compose_without_creating_work() {
    let parts: Vec<_> = [
        (Mechanism::Crank, 2, 1),
        (Mechanism::Gear, 4, 12),
        (Mechanism::Drum, 1, 2),
        (Mechanism::Pulley, 4, 1),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (mechanism, drive, load))| Part {
        mechanism,
        good: GoodId(1),
        input: i.checked_sub(1),
        drive,
        load,
    })
    .collect();
    let links = transmissions(
        &config(),
        &parts,
        &Default::default(),
        [(GoodId(1), 4)].into_iter(),
    )
    .unwrap();
    let source = Effort {
        force: 100,
        travel: 12000,
    };
    let result = resolve(&links, source).unwrap();
    assert_eq!(
        result.outputs[3],
        Effort {
            force: 1200,
            travel: 1000
        }
    );
    assert_eq!(result.delivered_work, source.work());
    assert!(result.broken.is_empty());
    let mut invalid = parts.clone();
    invalid[2].mechanism = Mechanism::Lever;
    assert!(
        validate(&config(), &invalid).is_err(),
        "a rotary shaft cannot directly power a linear lever port"
    );
}
#[test]
fn simple_machine_geometry_and_finite_supports_are_required() {
    let mut config = config();
    config
        .supports
        .insert(Mechanism::Pulley, [(GoodId(2), 2)].into());
    let part = Part {
        mechanism: Mechanism::Pulley,
        good: GoodId(1),
        input: None,
        drive: 4,
        load: 1,
    };
    assert_eq!(
        needs(&config, std::slice::from_ref(&part)).unwrap()[&GoodId(2)],
        2
    );
    let links = transmissions(
        &config,
        std::slice::from_ref(&part),
        &Default::default(),
        [(GoodId(1), 1)].into_iter(),
    )
    .unwrap();
    let unpowered = resolve(
        &links,
        Effort {
            force: 100,
            travel: 1000,
        },
    )
    .unwrap();
    assert_eq!(unpowered.outputs[0].force, 0);
    assert_eq!(unpowered.delivered_work, 0);
    let links = transmissions(
        &config,
        &[part],
        &Default::default(),
        [(GoodId(1), 1), (GoodId(2), 2)].into_iter(),
    )
    .unwrap();
    assert_eq!(
        resolve(
            &links,
            Effort {
                force: 100,
                travel: 1000
            }
        )
        .unwrap()
        .outputs[0],
        Effort {
            force: 400,
            travel: 250
        }
    );
}
