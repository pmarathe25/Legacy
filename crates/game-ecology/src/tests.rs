use super::*;
use game_environment::Weather;

fn world(kind: ResourceKind, remaining: u64) -> World {
    let source = Resource {
        id: ResourceId(1),
        area: AreaId(2),
        kind,
        remaining: Quantity(remaining),
        capacity: Quantity(20_000),
        daily_growth: Quantity(250),
        growth_remainder: 0,
        decay_permille: 0,
        soil_water: 75,
        water_capacity: 120,
        optimal_water: 50,
        minimum_temperature: 0,
        optimum_temperature: 20,
        contamination: 0,
        runoff_contamination: 0,
        labor_per_unit: SimDuration(60_000),
        workers: 2,
        access: BTreeSet::from([PersonId(3), PersonId(4), PersonId(5)]),
        custodian: Some(PersonId(3)),
    };
    let plot = Plot {
        id: SiteId(9),
        area: AreaId(2),
        fertility: 900,
        soil_water: 75,
        water_capacity: 120,
        access: BTreeSet::from([PersonId(3)]),
        custodian: Some(PersonId(3)),
    };
    World {
        rules: EcologyRules::default(),
        resources: BTreeMap::from([(source.id, source)]),
        plots: BTreeMap::from([(plot.id, plot)]),
        jobs: BTreeMap::new(),
        known: BTreeMap::new(),
        known_plots: BTreeMap::new(),
        day: 0,
        next_request: BTreeMap::new(),
    }
}
fn day(number: u64, temp: i32) -> DayFacts {
    DayFacts {
        day: number,
        day_of_year: number - 1,
        weather: Weather {
            temperature: temp,
            rain: 30,
            evaporation: 0,
        },
    }
}
fn facts() -> GatherFacts {
    GatherFacts {
        now: SimTime(10),
        area: AreaId(2),
        capable: true,
        adult: true,
        carrying_room: Quantity(5_000),
        tool: Some(InventoryId(6)),
    }
}
fn closing(now: SimTime) -> CollectionFacts {
    CollectionFacts {
        now,
        area: AreaId(2),
        capable: true,
        cargo_available: true,
        tool_available: true,
    }
}
#[test]
fn renewal_is_finite_and_changes_no_custody_permissions_or_personal_knowledge() {
    let world = world(ResourceKind::Coppice, 0);
    let before = world.clone();
    let next = world.advance(&day(1, 20)).unwrap();
    assert_eq!(next.resources[&ResourceId(1)].remaining, Quantity(250));
    assert_eq!(next.plots[&SiteId(9)].fertility, 901);
    assert_eq!(
        next.resources[&ResourceId(1)].access,
        world.resources[&ResourceId(1)].access
    );
    assert!(next.known.is_empty());
    assert_eq!(world, before);
    let cold = world.advance(&day(1, -5)).unwrap();
    assert_eq!(cold.resources[&ResourceId(1)].remaining, Quantity(0));
    assert_eq!(cold.plots[&SiteId(9)].fertility, 900);
    assert_eq!(next.advance(&day(1, 20)).unwrap(), next);
}
#[test]
fn land_recovery_configuration_is_independent_of_crop_rules() {
    let mut world = world(ResourceKind::Herbs, 0);
    world.rules = EcologyRules {
        base_temperature: 30,
        soil_recovery: 7,
    };
    let cool = world.advance(&day(1, 20)).unwrap();
    assert_eq!(cool.plots[&SiteId(9)].fertility, 900);
    let warm = world.advance(&day(1, 40)).unwrap();
    assert_eq!(warm.plots[&SiteId(9)].fertility, 907);
}
#[test]
fn actual_reservations_worker_slots_and_cargo_limits_bound_collection_plans() {
    let mut world = world(ResourceKind::Spring, 2_000);
    let first = world
        .prepare_gather(PersonId(3), ResourceId(1), Quantity(1_000), false, facts())
        .unwrap();
    assert_eq!(first.good, Good::Water);
    assert!(first.tool.is_none());
    assert_eq!(first.completes, SimTime(60_010));
    world.jobs.insert(PersonId(3), first.job(InventoryId(7)));
    let second = world
        .prepare_gather(PersonId(4), ResourceId(1), Quantity(1_000), false, facts())
        .unwrap();
    assert_eq!(
        world.prepare_gather(PersonId(4), ResourceId(1), Quantity(2_000), false, facts()),
        Err(Error::Unavailable)
    );
    world.jobs.insert(PersonId(4), second.job(InventoryId(8)));
    let before = world.clone();
    assert_eq!(
        world.prepare_gather(PersonId(5), ResourceId(1), Quantity(1_000), false, facts()),
        Err(Error::Unavailable)
    );
    assert_eq!(world, before);
    assert_eq!(
        world.prepare_gather(
            PersonId(3),
            ResourceId(1),
            Quantity(1_000),
            false,
            GatherFacts {
                carrying_room: Quantity(999),
                ..facts()
            }
        ),
        Err(Error::Unavailable)
    );
}
#[test]
fn capable_child_can_collect_small_herbs_but_needs_maturity_and_tool_for_timber() {
    let child = GatherFacts {
        adult: false,
        tool: None,
        ..facts()
    };
    let herbs = world(ResourceKind::Herbs, 4_000);
    herbs
        .prepare_gather(PersonId(3), ResourceId(1), Quantity(1_000), false, child)
        .unwrap();
    assert_eq!(
        herbs.prepare_gather(PersonId(3), ResourceId(1), Quantity(2_000), false, child),
        Err(Error::Unavailable)
    );
    let forest = world(ResourceKind::Coppice, 4_000);
    assert_eq!(
        forest.prepare_gather(PersonId(3), ResourceId(1), Quantity(1_000), false, child),
        Err(Error::Unavailable)
    );
    assert_eq!(
        forest.prepare_gather(
            PersonId(3),
            ResourceId(1),
            Quantity(1_000),
            false,
            GatherFacts {
                tool: None,
                ..facts()
            }
        ),
        Err(Error::Unavailable)
    );
}
#[test]
fn permission_is_distinct_from_physical_possibility_and_closing_refusal_moves_no_goods() {
    let mut world = world(ResourceKind::Coppice, 2_000);
    world
        .resources
        .get_mut(&ResourceId(1))
        .unwrap()
        .access
        .remove(&PersonId(4));
    assert_eq!(
        world.prepare_gather(PersonId(4), ResourceId(1), Quantity(1_000), false, facts()),
        Err(Error::Unavailable)
    );
    let attempt = world
        .prepare_gather(PersonId(4), ResourceId(1), Quantity(1_000), true, facts())
        .unwrap();
    let job = attempt.job(InventoryId(7));
    let before = world.clone();
    assert_eq!(
        prepare_collection(
            &world.resources[&ResourceId(1)],
            PersonId(4),
            &job,
            CollectionFacts {
                tool_available: false,
                ..closing(job.completes)
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(
        prepare_collection(
            &world.resources[&ResourceId(1)],
            PersonId(4),
            &job,
            CollectionFacts {
                capable: false,
                ..closing(job.completes)
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(
        prepare_collection(
            &world.resources[&ResourceId(1)],
            PersonId(4),
            &job,
            closing(SimTime(job.completes.0 - 1))
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(world, before);
    let effect = prepare_collection(
        &world.resources[&ResourceId(1)],
        PersonId(4),
        &job,
        closing(job.completes),
    )
    .unwrap();
    assert_eq!(
        effect.remaining.0 + effect.quantity.0,
        world.resources[&ResourceId(1)].remaining.0
    );
    assert_eq!(effect.destination, InventoryId(7));
    assert!(effect.contamination.is_none());
}
#[test]
fn water_quality_effect_uses_committed_renewed_source_without_owning_health_lots() {
    let mut world = world(ResourceKind::Spring, 1_000);
    let source = world.resources.get_mut(&ResourceId(1)).unwrap();
    source.contamination = 1_000;
    source.daily_growth = Quantity(1_000);
    let renewed = world.advance(&day(1, 20)).unwrap();
    assert_eq!(renewed.resources[&ResourceId(1)].remaining, Quantity(2_000));
    assert_eq!(renewed.resources[&ResourceId(1)].contamination, 500);
    let plan = renewed
        .prepare_gather(PersonId(3), ResourceId(1), Quantity(1_000), false, facts())
        .unwrap();
    let effect = prepare_collection(
        &renewed.resources[&ResourceId(1)],
        PersonId(3),
        &plan.job(InventoryId(7)),
        closing(plan.completes),
    )
    .unwrap();
    assert_eq!(effect.contamination, Some(500));
    assert_eq!(effect.remaining, Quantity(1_000));
}
#[test]
fn current_restore_pending_jobs_and_fractional_renewal_continue_without_rerolling() {
    let mut world = world(ResourceKind::Berries, 1_000);
    let source = world.resources.get_mut(&ResourceId(1)).unwrap();
    source.daily_growth = Quantity(1);
    source.soil_water = 1;
    let plan = world
        .prepare_gather(PersonId(3), ResourceId(1), Quantity(1_000), false, facts())
        .unwrap();
    world.jobs.insert(PersonId(3), plan.job(InventoryId(7)));
    let first = world
        .advance(&DayFacts {
            weather: Weather {
                temperature: 10,
                rain: 0,
                evaporation: 0,
            },
            ..day(1, 10)
        })
        .unwrap();
    assert!(first.resources[&ResourceId(1)].growth_remainder > 0);
    let loaded: World = serde_json::from_slice(&serde_json::to_vec(&first).unwrap()).unwrap();
    assert_eq!(
        first.advance(&day(2, 20)).unwrap(),
        loaded.advance(&day(2, 20)).unwrap()
    );
    assert_eq!(
        prepare_collection(
            &first.resources[&ResourceId(1)],
            PersonId(3),
            &first.jobs[&PersonId(3)],
            closing(plan.completes)
        )
        .unwrap(),
        prepare_collection(
            &loaded.resources[&ResourceId(1)],
            PersonId(3),
            &loaded.jobs[&PersonId(3)],
            closing(plan.completes)
        )
        .unwrap()
    );
}
#[test]
fn malformed_or_overflowing_inputs_fail_without_partial_renewal_or_reservation() {
    let mut world = world(ResourceKind::Spring, 2_000);
    let before = world.clone();
    assert!(world.advance(&day(2, 20)).is_err());
    assert_eq!(world, before);
    world
        .resources
        .get_mut(&ResourceId(1))
        .unwrap()
        .optimal_water = 0;
    assert!(world.advance(&day(1, 20)).is_err());
    let source = world.resources.get_mut(&ResourceId(1)).unwrap();
    source.optimal_water = 50;
    source.capacity = Quantity(u64::MAX);
    source.remaining = Quantity(u64::MAX);
    source.labor_per_unit = SimDuration(u64::MAX);
    assert_eq!(
        world.prepare_gather(
            PersonId(3),
            ResourceId(1),
            Quantity(u64::MAX),
            false,
            GatherFacts {
                carrying_room: Quantity(u64::MAX),
                ..facts()
            }
        ),
        Err(Error::Overflow)
    );
    assert!(world.jobs.is_empty());
}
