use game_parallel::{Person, World};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/seasons.toml")).unwrap()
}
#[test]
fn seasonal_growth_and_sheltered_exposure_follow_the_calendar_across_reload() {
    let mut kernel = Kernel::compile(world()).unwrap();
    for _ in 0..2 {
        kernel.step().unwrap();
    }
    let warm = kernel.export();
    assert_eq!(warm.regions[0].food, 22);
    assert!(warm.regions[0].people.iter().all(|p| p.health == 100));
    kernel =
        Kernel::compile(World::from_json(&serde_json::to_string(&warm).unwrap()).unwrap()).unwrap();
    kernel.step().unwrap();
    let cold = kernel.export();
    assert_eq!(cold.regions[0].food, 22);
    assert_eq!(cold.regions[0].people[0].health, 100);
    assert_eq!(cold.regions[0].people[1].health, 80);
    kernel.step().unwrap();
    kernel.step().unwrap();
    let next_warm = kernel.export();
    assert_eq!(next_warm.regions[0].food, 28);
    assert_eq!(next_warm.regions[0].people[1].health, 60);
}
#[test]
fn invalid_or_overflowing_calendars_are_rejected() {
    let mut state = world();
    state.environment.as_mut().unwrap().seasons[0].duration = 0;
    assert!(state.validate().is_err());
    let mut state = world();
    state.environment.as_mut().unwrap().seasons[0].duration = u64::MAX;
    assert!(state.validate().is_err());
    let mut state = world();
    state.environment.as_mut().unwrap().seasons[0].food_growth_per_thousand = 10_001;
    assert!(state.validate().is_err());
    let mut state = world();
    state.environment.as_mut().unwrap().seasons[1].name = "warm".into();
    assert!(state.validate().is_err());
}
#[test]
fn many_seasonal_exposures_are_worker_invariant_and_transactional() {
    let mut state = world();
    state.regions[0].people = (1..=5000)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=5 {
                    kernel.step().unwrap();
                    if tick == 3 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                kernel.export()
            })
    };
    let serial = run(1);
    assert_eq!(serial, run(4));
    serial.validate().unwrap();
    let mut failing =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    failing.environment = state.environment;
    failing.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(failing.clone()).unwrap();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), failing);
}

#[test]
fn seasonal_field_work_pauses_and_resumes_without_losing_seed_or_progress() {
    use game_parallel::CropPhase;
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/farming.toml")).unwrap();
    state.environment = world().environment;
    state.rules.food_growth = 0;
    state.rules.hunger_per_tick = 0;
    state.rules.recovery = 0;
    state.environment.as_mut().unwrap().regions.clear();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    kernel.step().unwrap();
    let before_cold = kernel.export();
    assert!(
        before_cold.agriculture.as_ref().unwrap().farms[0]
            .fields
            .iter()
            .all(|field| field.phase == CropPhase::Growing && field.work == 1)
    );
    assert_eq!(before_cold.regions[0].food, 6);
    kernel.step().unwrap();
    kernel.step().unwrap();
    let during_cold = kernel.export();
    assert_eq!(during_cold.agriculture, before_cold.agriculture);
    assert_eq!(during_cold.regions[0].food, 6);
    kernel =
        Kernel::compile(World::from_json(&serde_json::to_string(&during_cold).unwrap()).unwrap())
            .unwrap();
    kernel.step().unwrap();
    let ripe = kernel.export();
    assert!(
        ripe.agriculture.as_ref().unwrap().farms[0]
            .fields
            .iter()
            .all(|field| field.phase == CropPhase::Ripe)
    );
    kernel.step().unwrap();
    let harvested = kernel.export();
    assert!(
        harvested.agriculture.unwrap().farms[0]
            .fields
            .iter()
            .all(|field| field.phase == CropPhase::Fallow)
    );
    assert_eq!(harvested.regions[0].food, 18);
}
