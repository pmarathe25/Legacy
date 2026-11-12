use game_work_capability::{Body, Capabilities, GrowthPoint, Requirement};

fn model() -> Capabilities {
    Capabilities {
        growth: vec![
            GrowthPoint {
                age: 0,
                strength: 0,
            },
            GrowthPoint {
                age: 10,
                strength: 100,
            },
        ],
        vigor_base: 500,
        vigor_weight: 1000,
        default_strength: 80,
        missing_strength: None,
        requirements: vec![Requirement {
            recipe: 1,
            strength: 20,
        }],
    }
}

#[test]
fn growth_and_vigor_constrain_tasks_without_making_choices() {
    let model = model();
    model.validate(&[1, 2]).unwrap();
    let child = Body {
        person: 1,
        age: 5,
        vigor: 500,
    };
    assert_eq!(model.strength(Some(child)), Some(50));
    assert!(model.permits(Some(child), 1));
    assert!(!model.permits(Some(child), 2));
    assert!(!model.permits(Some(Body { age: 0, ..child }), 1));
    assert!(!model.permits(
        Some(Body {
            age: 10,
            vigor: 0,
            ..child
        }),
        2
    ));
    assert!(!model.permits(None, 1));
    assert_eq!(
        serde_json::from_str::<Capabilities>(&serde_json::to_string(&model).unwrap()).unwrap(),
        model
    );
}

#[test]
fn bad_configuration_and_parallel_filtering_are_deterministic() {
    let mut invalid = model();
    assert!(invalid.validate(&[2]).is_err());
    invalid.growth[0].age = 1;
    assert!(invalid.validate(&[1, 2]).is_err());
    let model = model();
    let attempts: Vec<_> = (0..10000)
        .map(|id| {
            (
                Some(Body {
                    person: id + 1,
                    age: id % 20,
                    vigor: 500,
                }),
                1,
            )
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| model.permitted(&attempts))
    };
    assert_eq!(run(1), run(4));
}
