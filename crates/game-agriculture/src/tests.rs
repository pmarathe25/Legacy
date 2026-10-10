use super::*;

fn world() -> World {
    let crop = CropRules::grain();
    let field = Field {
        id: SiteId(1),
        area: AreaId(2),
        access: BTreeSet::from([PersonId(3)]),
        custodian: Some(PersonId(3)),
        seed: InventoryId(4),
        tool: InventoryId(5),
        water: InventoryId(6),
        food: InventoryId(7),
        phase: CropPhase::Growing,
        soil_fertility: 900,
        soil_water: 75,
        water_capacity: 120,
        preparation_ms: crop.prepare_labor.0,
        planting_ms: crop.plant_labor.0,
        harvest_ms: 0,
        degree_days_milli: 0,
        stress_days: 0,
        tending: 1_000,
        contributions: BTreeMap::new(),
    };
    World {
        crop: CropRules {
            planting_last_day: 9,
            base_temperature: 0,
            ripe_degree_days: 20,
            ..crop
        },
        fields: BTreeMap::from([(field.id, field)]),
        jobs: BTreeMap::new(),
        day: 0,
    }
}
fn day(number: u64, rain: u64) -> DayFacts {
    DayFacts {
        day: number,
        day_of_year: number - 1,
        weather: Weather {
            temperature: 10,
            rain,
            evaporation: 2,
        },
    }
}
fn begin() -> BeginFacts {
    BeginFacts {
        actor: PersonId(3),
        now: SimTime(0),
        area: AreaId(2),
        capable: true,
        tool_available: true,
        field_available: true,
        seed_available: Quantity(1_000),
        water_available: Quantity(1_000),
        outputs_accessible: true,
        food_room: Quantity(u64::MAX),
        seed_room: Quantity(u64::MAX),
        day_of_year: 0,
        temperature_c: 10,
    }
}
fn closing(now: SimTime) -> CompletionFacts {
    CompletionFacts {
        now,
        eligible: true,
        outputs_accessible: true,
        food_room: Quantity(u64::MAX),
        seed_room: Quantity(u64::MAX),
    }
}
#[test]
fn crop_growth_and_harvest_describe_outputs_without_economy_or_stock_storage() {
    let world = world();
    let before = world.clone();
    let first = world
        .advance(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(world, before);
    assert_eq!(first.fields[&SiteId(1)].phase, CropPhase::Growing);
    let ripe = first
        .advance(SimTime(60_000), SimTime(120_000), Some(&day(2, 10)))
        .unwrap();
    assert_eq!(ripe.fields[&SiteId(1)].phase, CropPhase::Ripe);
    let job = Cultivation {
        field: SiteId(1),
        operation: FarmOperation::Harvest,
        labor_ms: ripe.crop.harvest_labor.0,
        completes: SimTime(120_000),
    };
    let effect = prepare_completion(
        &ripe.fields[&SiteId(1)],
        &job,
        &ripe.crop,
        closing(job.completes),
    )
    .unwrap();
    assert_eq!(effect.harvest.unwrap().food, Quantity(5_400_000));
    assert_eq!(effect.harvest.unwrap().seed, Quantity(1_800));
    assert_eq!(effect.field.phase, CropPhase::Fallow);
    assert_eq!(effect.field.soil_fertility, 880);
    let blocked = prepare_completion(
        &ripe.fields[&SiteId(1)],
        &job,
        &ripe.crop,
        CompletionFacts {
            seed_room: Quantity(0),
            ..closing(job.completes)
        },
    )
    .unwrap();
    assert!(blocked.harvest.is_none());
    assert_eq!(blocked.field.phase, CropPhase::Ripe);
    assert_eq!(blocked.field.harvest_ms, ripe.crop.harvest_labor.0 - 1);
}
#[test]
fn crop_failure_and_fallow_recovery_consume_supplied_weather_only() {
    let mut world = world();
    world.crop.failure_stress_days = 2;
    world.fields.get_mut(&SiteId(1)).unwrap().soil_water = 0;
    let first = world
        .advance(SimTime(0), SimTime(60_000), Some(&day(1, 0)))
        .unwrap();
    let failed = first
        .advance(SimTime(60_000), SimTime(120_000), Some(&day(2, 0)))
        .unwrap();
    assert_eq!(failed.fields[&SiteId(1)].phase, CropPhase::Failed);
    let job = Cultivation {
        field: SiteId(1),
        operation: FarmOperation::Fallow,
        labor_ms: failed.crop.fallow_labor.0,
        completes: SimTime(120_000),
    };
    let mut field = prepare_completion(
        &failed.fields[&SiteId(1)],
        &job,
        &failed.crop,
        closing(job.completes),
    )
    .unwrap()
    .field;
    field.soil_water = 50;
    let before = field.soil_fertility;
    grow(&mut field, &failed.crop, &day(3, 0).weather).unwrap();
    assert_eq!(field.soil_fertility, before + failed.crop.soil_recovery);
}
#[test]
fn planting_consumes_seed_once_and_resuming_credit_requires_actual_remaining_work() {
    let world = world();
    let mut field = world.fields[&SiteId(1)].clone();
    field.phase = CropPhase::Prepared;
    field.planting_ms = 0;
    assert_eq!(
        prepare_begin(
            &field,
            &world.crop,
            FarmOperation::Plant,
            BeginFacts {
                seed_available: Quantity(0),
                ..begin()
            }
        ),
        Err(Error::Unavailable)
    );
    let effect = prepare_begin(&field, &world.crop, FarmOperation::Plant, begin()).unwrap();
    assert_eq!(
        effect.consumed,
        Some(InputConsumption {
            inventory: field.seed,
            quantity: world.crop.seed_input
        })
    );
    assert_eq!(effect.field.phase, CropPhase::Planting);
    let active = World {
        fields: BTreeMap::from([(field.id, effect.field)]),
        jobs: BTreeMap::from([(PersonId(3), effect.job)]),
        ..world
    };
    let half = active
        .advance(SimTime(0), SimTime(3_600_000), None)
        .unwrap();
    let resumed = prepare_begin(
        &half.fields[&SiteId(1)],
        &half.crop,
        FarmOperation::Plant,
        BeginFacts {
            now: SimTime(3_600_000),
            seed_available: Quantity(0),
            ..begin()
        },
    )
    .unwrap();
    assert!(resumed.consumed.is_none());
    assert_eq!(resumed.job.completes, SimTime(7_200_000));
    assert_eq!(resumed.field.contributions[&PersonId(3)], 3_600_000);
    assert_eq!(field.phase, CropPhase::Prepared);
}
#[test]
fn actual_cultivation_labor_and_current_restore_are_identical_across_slices() {
    let mut world = world();
    world.fields.get_mut(&SiteId(1)).unwrap().phase = CropPhase::Planting;
    world.fields.get_mut(&SiteId(1)).unwrap().planting_ms = 0;
    world.crop.plant_labor = SimDuration(120_000);
    world.jobs.insert(
        PersonId(3),
        Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Plant,
            labor_ms: 0,
            completes: SimTime(120_000),
        },
    );
    let whole = world
        .advance(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    let early = world.advance(SimTime(0), SimTime(17_001), None).unwrap();
    let restored: World = serde_json::from_slice(&serde_json::to_vec(&early).unwrap()).unwrap();
    let split = restored
        .advance(SimTime(17_001), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(whole, split);
    assert_eq!(whole.fields[&SiteId(1)].contributions[&PersonId(3)], 60_000);
    assert_eq!(whole.fields[&SiteId(1)].phase, CropPhase::Planting);
}
#[test]
fn physical_ineligibility_and_early_completion_cannot_produce_or_mutate_anything() {
    let mut world = world();
    world.fields.get_mut(&SiteId(1)).unwrap().phase = CropPhase::Ripe;
    let field = &world.fields[&SiteId(1)];
    let before = world.clone();
    assert_eq!(
        prepare_begin(
            field,
            &world.crop,
            FarmOperation::Harvest,
            BeginFacts {
                capable: false,
                ..begin()
            }
        ),
        Err(Error::Unavailable)
    );
    let job = Cultivation {
        field: SiteId(1),
        operation: FarmOperation::Harvest,
        labor_ms: 0,
        completes: SimTime(10),
    };
    assert_eq!(
        prepare_completion(field, &job, &world.crop, closing(SimTime(9))),
        Err(Error::Unavailable)
    );
    assert_eq!(
        prepare_completion(
            field,
            &job,
            &world.crop,
            CompletionFacts {
                eligible: false,
                ..closing(SimTime(10))
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(world, before);
}
#[test]
fn duplicate_labor_repeated_days_and_overflow_never_partially_change_fields() {
    let mut world = world();
    let job = Cultivation {
        field: SiteId(1),
        operation: FarmOperation::Tend,
        labor_ms: 0,
        completes: SimTime(60_000),
    };
    world.jobs.insert(PersonId(3), job.clone());
    world.jobs.insert(PersonId(8), job);
    let before = world.clone();
    assert!(world.advance(SimTime(0), SimTime(1), None).is_err());
    assert_eq!(world, before);
    world.jobs.clear();
    let first = world
        .advance(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert!(
        first
            .advance(SimTime(60_000), SimTime(61_000), Some(&day(1, 10)))
            .is_err()
    );
    let mut field = world.fields[&SiteId(1)].clone();
    field.degree_days_milli = u64::MAX;
    let before = field.clone();
    assert_eq!(
        grow(&mut field, &world.crop, &day(1, 10).weather),
        Err(Error::Overflow)
    );
    assert_eq!(field, before);
}
#[test]
fn crop_calendar_validation_has_no_dependency_on_climate_state() {
    Config::default().validate(365).unwrap();
    assert!(Config::default().validate(2).is_err());
    assert!(
        Config {
            crop: CropRules {
                prepare_labor: SimDuration(0),
                ..CropRules::grain()
            }
        }
        .validate(365)
        .is_err()
    );
}

#[test]
fn owned_save_validation_rejects_phase_job_deadline_and_exclusive_labor_corruption() {
    let mut valid = world();
    valid.fields.get_mut(&SiteId(1)).unwrap().tending = 0;
    valid.jobs.insert(
        PersonId(3),
        Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Tend,
            labor_ms: 0,
            completes: SimTime(valid.crop.tend_labor.0),
        },
    );
    valid.validate(SimTime(0), 365).unwrap();
    let mut bad = valid.clone();
    bad.fields.get_mut(&SiteId(1)).unwrap().phase = CropPhase::Prepared;
    assert!(bad.validate(SimTime(0), 365).is_err());
    let mut bad = valid.clone();
    bad.jobs.get_mut(&PersonId(3)).unwrap().completes.0 += 1;
    assert!(bad.validate(SimTime(0), 365).is_err());
    let mut bad = valid.clone();
    bad.jobs.insert(PersonId(8), bad.jobs[&PersonId(3)].clone());
    assert!(bad.validate(SimTime(0), 365).is_err());
    let mut bad = valid.clone();
    bad.fields
        .get_mut(&SiteId(1))
        .unwrap()
        .contributions
        .insert(PersonId(3), 1);
    assert!(bad.validate(SimTime(0), 365).is_err());
    assert_eq!(valid.jobs[&PersonId(3)].labor_ms, 0);
}
