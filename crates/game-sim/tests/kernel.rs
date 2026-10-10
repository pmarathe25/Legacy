use game_core::{Error, SimDuration, SimTime};
use game_sim::*;

#[derive(Debug, Default, PartialEq, Eq)]
struct World {
    time: SimTime,
    publication: u64,
    value: u64,
    stop: Option<u8>,
}
impl ClockWorld for World {
    fn clock(&self) -> Clock {
        Clock {
            time: self.time,
            revision: self.publication,
        }
    }
    fn publish_clock(&mut self, clock: Clock) {
        self.time = clock.time;
        self.publication = clock.revision;
    }
}
struct Prepared {
    delta: u64,
    at: SimTime,
    publications: u64,
    stop: Option<u8>,
}
impl Transaction<World, u8> for Prepared {
    fn revision_budget(&self) -> u64 {
        self.publications
    }
    fn commit(self, world: &mut World) -> Option<u8> {
        assert_eq!(world.time, self.at, "closing time is visible to reactions");
        world.value += self.delta;
        world.publication += self.publications;
        self.stop
    }
}
fn prepare(
    world: &World,
    interval: Interval,
    _: &[PhaseKey],
    _: &Executor,
) -> Result<Prepared, Error> {
    let delta = interval.elapsed().0;
    world.value.checked_add(delta).ok_or(Error::Overflow)?;
    Ok(Prepared {
        delta,
        at: interval.to,
        publications: 0,
        stop: None,
    })
}
fn key(module: u32, phase: u16) -> PhaseKey {
    PhaseKey {
        module: ModuleId(module),
        phase: PhaseId(phase),
    }
}
fn boundary(_: &World, interval: Interval) -> Result<SimTime, Error> {
    Ok(SimTime(interval.from.0.checked_add(10).ok_or(Error::Overflow)?).min(interval.to))
}
fn stopped(world: &World, _: &Clock) -> Result<Option<u8>, Error> {
    Ok(world.stop)
}

#[test]
fn dependencies_have_stable_order_and_invalid_registration_is_rejected() {
    let create = |reverse| {
        let a = Module::<World, u8>::new(ModuleId(4)).phase(PhaseId(0), [key(2, 1)]);
        let b = Module::new(ModuleId(2))
            .phase(PhaseId(1), [key(2, 0)])
            .phase(PhaseId(0), []);
        let c = Module::new(ModuleId(1)).phase(PhaseId(0), []);
        let mut modules = vec![a, b, c];
        if reverse {
            modules.reverse();
        }
        Engine::new(modules, Execution::default()).unwrap()
    };
    assert_eq!(
        create(false).phases(),
        [key(1, 0), key(2, 0), key(2, 1), key(4, 0)]
    );
    assert_eq!(create(false).phases(), create(true).phases());
    assert!(
        Engine::<World, u8>::new(
            [Module::new(ModuleId(1)), Module::new(ModuleId(1))],
            Execution::default()
        )
        .is_err()
    );
    assert!(
        Engine::<World, u8>::new(
            [Module::new(ModuleId(1))
                .phase(PhaseId(0), [])
                .phase(PhaseId(0), [])],
            Execution::default()
        )
        .is_err()
    );
    assert!(
        Engine::<World, u8>::new(
            [Module::new(ModuleId(1)).phase(PhaseId(0), [key(9, 0)])],
            Execution::default()
        )
        .is_err()
    );
    assert!(
        Engine::<World, u8>::new(
            [Module::new(ModuleId(1))
                .phase(PhaseId(0), [key(1, 1)])
                .phase(PhaseId(1), [key(1, 0)])],
            Execution::default()
        )
        .is_err()
    );
}

#[test]
fn input_stops_and_transaction_stops_preserve_actual_boundary_metadata() {
    let mut module = Module::new(ModuleId(2));
    module.boundary = Some(boundary);
    module.input_stop = Some(stopped);
    let engine = Engine::new([module], Execution::default()).unwrap();
    let mut world = World {
        stop: Some(7),
        ..World::default()
    };
    let result = engine
        .advance(&mut world, SimDuration(100), prepare)
        .unwrap();
    assert_eq!(result.requested, SimTime(100));
    assert_eq!(result.reached, SimTime(0));
    assert_eq!(result.input, Some(7));
    assert_eq!(world.value, 0);
    world.stop = None;
    let result = engine
        .advance(&mut world, SimDuration(100), |_, interval, _, _| {
            Ok(Prepared {
                delta: interval.elapsed().0,
                at: interval.to,
                publications: 2,
                stop: Some(9),
            })
        })
        .unwrap();
    assert_eq!(result.reached, SimTime(10));
    assert_eq!(result.revision, 3);
    assert_eq!(result.input, Some(9));
    assert_eq!(world.value, 10);
}

#[test]
fn failed_preparation_and_overflow_publish_no_partial_state_or_clock() {
    let engine = Engine::<World, u8>::new([], Execution::default()).unwrap();
    let mut world = World {
        value: u64::MAX,
        ..World::default()
    };
    assert_eq!(
        engine.advance(&mut world, SimDuration(1), prepare),
        Err(Error::Overflow)
    );
    assert_eq!(world.time, SimTime(0));
    assert_eq!(world.publication, 0);
    assert_eq!(world.value, u64::MAX);
    world.value = 0;
    world.publication = u64::MAX - 1;
    assert_eq!(
        engine.advance(&mut world, SimDuration(1), |_, interval, _, _| Ok(
            Prepared {
                delta: 1,
                at: interval.to,
                publications: 1,
                stop: None,
            }
        )),
        Err(Error::Overflow)
    );
    assert_eq!(world.time, SimTime(0));
    assert_eq!(world.value, 0);
    world.time = SimTime(u64::MAX);
    assert_eq!(
        engine.advance(&mut world, SimDuration(1), prepare),
        Err(Error::Overflow)
    );
}

#[test]
fn stalled_or_skipped_boundaries_are_rejected_without_a_fake_time_step() {
    let mut module = Module::<World, u8>::new(ModuleId(1));
    module.boundary = Some(|_, interval| Ok(interval.from));
    let engine = Engine::new([module], Execution::default()).unwrap();
    let mut world = World::default();
    assert!(engine.advance(&mut world, SimDuration(1), prepare).is_err());
    assert_eq!(world, World::default());
    let mut module = Module::<World, u8>::new(ModuleId(1));
    module.boundary = Some(|_, interval| Ok(SimTime(interval.to.0 + 1)));
    let engine = Engine::new([module], Execution::default()).unwrap();
    assert!(engine.advance(&mut world, SimDuration(1), prepare).is_err());
    assert_eq!(world, World::default());
}

#[test]
fn keyed_results_and_errors_are_independent_of_workers_and_submission_order() {
    let rows: Vec<_> = (0..512_u64).rev().map(|key| (key, key)).collect();
    for workers in [1, 4] {
        for reverse_jobs in [false, true] {
            let executor = Executor::new(Execution {
                workers,
                reverse_jobs,
            })
            .unwrap();
            let results = executor.map_keyed(&rows, |value| Ok(value * 3)).unwrap();
            assert_eq!(
                results,
                (0..512).map(|key| (key, key * 3)).collect::<Vec<_>>()
            );
            let failure = executor.map_keyed(&rows, |value| {
                if *value == 10 {
                    Err(Error::Unavailable)
                } else if *value == 400 {
                    Err(Error::Overflow)
                } else {
                    Ok(*value)
                }
            });
            assert_eq!(failure, Err(Error::Unavailable));
            assert!(executor.map_keyed(&[(1, ()), (1, ())], |_| Ok(())).is_err());
        }
    }
}

#[test]
fn no_module_and_zero_duration_need_no_domain_specific_fallback() {
    let engine = Engine::<World, u8>::new([], Execution::default()).unwrap();
    let mut world = World::default();
    engine.advance(&mut world, SimDuration(0), prepare).unwrap();
    assert_eq!(world, World::default());
    engine
        .advance(&mut world, SimDuration(17), prepare)
        .unwrap();
    assert_eq!(world.time, SimTime(17));
    assert_eq!(world.value, 17);
}

#[test]
fn boundaries_receive_the_current_clipped_window_in_stable_module_order() {
    let create = |reverse| {
        let mut first = Module::<World, u8>::new(ModuleId(1));
        first.boundary = Some(|_, interval| Ok(SimTime(interval.from.0 + 7).min(interval.to)));
        let mut second = Module::new(ModuleId(2));
        second.boundary = Some(|_, interval| {
            assert!(
                interval.elapsed().0 <= 7,
                "earlier module must clip this predictor"
            );
            Ok(SimTime(interval.from.0 + 3).min(interval.to))
        });
        let mut modules = vec![first, second];
        if reverse {
            modules.reverse();
        }
        Engine::new(modules, Execution::default()).unwrap()
    };
    let world = World::default();
    assert_eq!(
        create(false).next_boundary(&world, SimTime(100)).unwrap(),
        SimTime(3)
    );
    assert_eq!(
        create(false).next_boundary(&world, SimTime(100)),
        create(true).next_boundary(&world, SimTime(100))
    );
    assert_eq!(
        create(false).next_boundary(&world, SimTime(0)).unwrap(),
        SimTime(0)
    );
    let mut advanced = World::default();
    create(true)
        .advance(&mut advanced, SimDuration(100), prepare)
        .unwrap();
    assert_eq!(advanced.time, SimTime(100));
    assert_eq!(advanced.value, 100);
    assert_eq!(advanced.publication, 34);
    let mut larger = Module::<World, u8>::new(ModuleId(2));
    larger.boundary = Some(|_, _| Ok(SimTime(8)));
    let mut first = Module::new(ModuleId(1));
    first.boundary = Some(|_, _| Ok(SimTime(7)));
    let engine = Engine::new([larger, first], Execution::default()).unwrap();
    assert!(engine.next_boundary(&world, SimTime(100)).is_err());
}

#[test]
fn existing_batches_reuse_the_same_bounded_pool() {
    let executor = Executor::new(Execution {
        workers: 3,
        reverse_jobs: false,
    })
    .unwrap();
    assert_eq!(executor.pool().current_num_threads(), 3);
    assert_eq!(executor.install(rayon::current_num_threads), 3);
    assert_eq!(executor.install(|| executor.pool().install(|| 17)), 17);
}

#[test]
fn one_interval_driver_commits_one_boundary_and_checks_input_before_any_commit() {
    let mut module = Module::<World, u8>::new(ModuleId(1));
    module.boundary = Some(boundary);
    module.input_stop = Some(stopped);
    let engine = Engine::new([module], Execution::default()).unwrap();
    let mut world = World::default();
    let one = engine
        .advance_one(&mut world, SimDuration(100), prepare)
        .unwrap();
    assert_eq!(one.requested, SimTime(100));
    assert_eq!(one.reached, SimTime(10));
    assert_eq!(world.value, 10);
    assert_eq!(world.publication, 1);
    world.stop = Some(4);
    let stopped = engine
        .advance_one(&mut world, SimDuration(90), prepare)
        .unwrap();
    assert_eq!(stopped.input, Some(4));
    assert_eq!(stopped.reached, SimTime(10));
    assert_eq!(world.value, 10);
    world.stop = None;
    engine
        .advance(&mut world, SimDuration(90), prepare)
        .unwrap();
    assert_eq!(world.time, SimTime(100));
    assert_eq!(world.value, 100);
    assert_eq!(world.publication, 10);
}
