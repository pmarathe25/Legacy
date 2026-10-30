use game_parallel::World;
use game_parallel_autonomy::{Actor, Event, Outcome};
use game_parallel_fused_physical::Kernel;

fn fixture() -> World {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/enrollment.toml")).unwrap();
    world.routes.clear();
    let autonomy = world.autonomy.as_mut().unwrap();
    autonomy.actors = [3, 2, 1]
        .into_iter()
        .map(|person| Actor {
            person,
            enabled: true,
            next_review: 0,
            travel_caution: 0,
            hunger_urgency: 0,
        })
        .collect();
    autonomy.events = vec![Event {
        tick: 0,
        person: 2,
        source: 1,
        destination: 2,
        outcome: Outcome::Rejected,
        reason: Some("authored historical refusal".into()),
    }];
    world.validate().unwrap();
    world
}

#[test]
fn route_free_reviews_preserve_import_order_and_newborn_activation_across_reload() {
    for workers in [1, 4] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut reference = fixture();
                let mut kernel = Kernel::compile(reference.clone()).unwrap();
                assert_eq!(kernel.export(), reference);
                for tick in 0..18 {
                    let abandoned = kernel.prepare().unwrap();
                    assert_eq!(kernel.export(), reference);
                    drop(abandoned);
                    reference.step().unwrap();
                    kernel.step().unwrap();
                    assert_eq!(kernel.export(), reference);
                    if tick == 7 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                assert!(reference.autonomy.as_ref().unwrap().actors.len() > 3);
            });
    }
}

#[test]
fn late_failure_keeps_imported_receipts_and_review_clocks_unchanged() {
    let mut world = fixture();
    world.families.as_mut().unwrap().totals.partnerships = u64::MAX;
    let mut kernel = Kernel::compile(world.clone()).unwrap();
    for _ in 0..2 {
        assert_eq!(kernel.prepare().err(), Some("family counter overflow"));
        assert_eq!(kernel.export(), world);
    }
}
