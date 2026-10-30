use game_parallel::{Relationship, World};
use game_parallel_fused_physical::Kernel;
#[test]
fn paged_social_retention_preserves_initial_order_and_matches_reload() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))
    .unwrap();
    let social = world.social.as_mut().unwrap();
    social.rules.interval = 3;
    social.rules.relationship_retention = Some(1);
    social.relationships = vec![
        Relationship {
            a: 2,
            b: 3,
            familiarity: 2,
            encounters: 1,
            last_met: 0,
        },
        Relationship {
            a: 1,
            b: 2,
            familiarity: 2,
            encounters: 1,
            last_met: 0,
        },
    ];
    let mut kernel = Kernel::compile(world.clone()).unwrap();
    assert_eq!(kernel.export(), world);
    let discarded = kernel.prepare().unwrap();
    let current = kernel.prepare().unwrap();
    assert!(kernel.commit(discarded).is_err());
    assert_eq!(kernel.export(), world);
    kernel.commit(current).unwrap();
    world.step().unwrap();
    assert_eq!(kernel.export(), world);
    for tick in 2..=12 {
        kernel.step().unwrap();
        world.step().unwrap();
        assert_eq!(kernel.export(), world);
        if tick == 2 || tick == 7 {
            kernel = Kernel::compile(
                World::from_json(&serde_json::to_string(&kernel.export()).unwrap()).unwrap(),
            )
            .unwrap();
        }
    }
}
#[test]
fn late_family_failure_does_not_publish_paged_social_updates() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/enrollment.toml")).unwrap();
    world.families.as_mut().unwrap().totals.partnerships = u64::MAX;
    world.social.as_mut().unwrap().relationships = vec![Relationship {
        a: 1,
        b: 2,
        familiarity: 2,
        encounters: 1,
        last_met: 0,
    }];
    let mut kernel = Kernel::compile(world.clone()).unwrap();
    for _ in 0..2 {
        assert_eq!(kernel.prepare().err(), Some("family counter overflow"));
        assert_eq!(kernel.export(), world);
    }
}
