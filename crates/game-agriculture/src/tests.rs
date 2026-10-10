use super::*;

fn rules() -> CropRules {
    // Test fixture, deliberately distinct from the authored setting.
    CropRules {
        planting_first_day: 0,
        planting_last_day: 9,
        prepare_labor: SimDuration(14_400_000),
        plant_labor: SimDuration(7_200_000),
        tend_labor: SimDuration(3_600_000),
        irrigation_labor: SimDuration(3_600_000),
        harvest_labor: SimDuration(14_400_000),
        fallow_labor: SimDuration(3_600_000),
        seed_input: Quantity(1_000),
        irrigation_input: Quantity(1_000),
        irrigation_water: 30,
        base_temperature: 0,
        ripe_degree_days: 20,
        optimal_water: 50,
        failure_stress_days: 12,
        food_yield: Quantity(6_000_000),
        seed_yield: Quantity(2_000),
        soil_depletion: 20,
        soil_recovery: 1,
    }
}
fn world() -> World {
    let rules = rules();
    let definition = CropId("test/cereal".into());
    let mut world = World::new(
        Catalog {
            default_crop: definition.clone(),
            crops: BTreeMap::from([(definition.clone(), rules.clone())]),
        },
        0,
    )
    .unwrap();
    world.fields.insert(
        SiteId(1),
        Field {
            id: SiteId(1),
            area: AreaId(2),
            access: BTreeSet::from([PersonId(3)]),
            custodian: Some(PersonId(3)),
            seed: InventoryId(4),
            tool: InventoryId(5),
            water: InventoryId(6),
            food: InventoryId(7),
        },
    );
    world.crops.insert(
        SiteId(1),
        Crop {
            definition,
            phase: CropPhase::Growing,
            preparation_ms: rules.prepare_labor.0,
            planting_ms: rules.plant_labor.0,
            harvest_ms: 0,
            degree_days_milli: 0,
            stress_days: 0,
            tending: 1_000,
            contributions: BTreeMap::new(),
        },
    );
    world.soils.insert(
        SiteId(1),
        Soil {
            fertility: 900,
            water: 75,
            capacity: 120,
        },
    );
    world
}
fn day(number: u64, rain: u64) -> LocalWeatherDay {
    LocalWeatherDay {
        day: number,
        day_of_year: number - 1,
        cells: BTreeMap::from([(
            AreaId(2),
            WeatherFacts {
                temperature: 10,
                rain,
                evaporation: 2,
            },
        )]),
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
        day_of_year: Some(0),
        temperature_c: Some(10),
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
fn start(world: &World, operation: FarmOperation, facts: BeginFacts) -> Result<BeginEffect, Error> {
    let (field, crop, soil, rules) = world.field_bundle(SiteId(1))?;
    prepare_begin(field, crop, soil, rules, operation, facts)
}
fn finish(
    world: &World,
    job: &Cultivation,
    facts: CompletionFacts,
) -> Result<CropCompletion, Error> {
    let (field, crop, soil, rules) = world.field_bundle(SiteId(1))?;
    prepare_completion(field, crop, soil, job, rules, facts)
}

#[test]
fn growth_and_harvest_describe_component_changes_without_stock_storage() {
    let world = world();
    let before = world.clone();
    let first = world
        .advance(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(world, before);
    assert_eq!(first.crops[&SiteId(1)].phase, CropPhase::Growing);
    let ripe = first
        .advance(SimTime(60_000), SimTime(120_000), Some(&day(2, 10)))
        .unwrap();
    assert_eq!(ripe.crops[&SiteId(1)].phase, CropPhase::Ripe);
    assert_eq!(ripe.fields, world.fields);
    let job = Cultivation {
        field: SiteId(1),
        operation: FarmOperation::Harvest,
        labor_ms: ripe.rules_for(SiteId(1)).unwrap().harvest_labor.0,
        completes: SimTime(120_000),
    };
    let effect = finish(&ripe, &job, closing(job.completes)).unwrap();
    assert_eq!(effect.harvest.unwrap().food, Quantity(5_400_000));
    assert_eq!(effect.harvest.unwrap().seed, Quantity(1_800));
    assert_eq!(effect.crop.phase, CropPhase::Fallow);
    assert_eq!(effect.soil.fertility, 880);
    let blocked = finish(
        &ripe,
        &job,
        CompletionFacts {
            seed_room: Quantity(0),
            ..closing(job.completes)
        },
    )
    .unwrap();
    assert!(blocked.harvest.is_none());
    assert_eq!(blocked.crop.phase, CropPhase::Ripe);
    assert_eq!(
        blocked.crop.harvest_ms,
        ripe.rules_for(SiteId(1)).unwrap().harvest_labor.0 - 1
    );
}

#[test]
fn failure_and_fallow_recovery_consume_supplied_local_weather_only() {
    let mut world = world();
    world
        .catalog
        .crops
        .values_mut()
        .next()
        .unwrap()
        .failure_stress_days = 2;
    world.soils.get_mut(&SiteId(1)).unwrap().water = 0;
    let first = world
        .advance(SimTime(0), SimTime(60_000), Some(&day(1, 0)))
        .unwrap();
    let failed = first
        .advance(SimTime(60_000), SimTime(120_000), Some(&day(2, 0)))
        .unwrap();
    assert_eq!(failed.crops[&SiteId(1)].phase, CropPhase::Failed);
    let job = Cultivation {
        field: SiteId(1),
        operation: FarmOperation::Fallow,
        labor_ms: failed.rules_for(SiteId(1)).unwrap().fallow_labor.0,
        completes: SimTime(120_000),
    };
    let mut effect = finish(&failed, &job, closing(job.completes)).unwrap();
    effect.soil.water = 50;
    let before = effect.soil.fertility;
    let (_, soil) = grow(
        &effect.crop,
        &effect.soil,
        failed.rules_for(SiteId(1)).unwrap(),
        &day(3, 0).cells[&AreaId(2)],
    )
    .unwrap();
    assert_eq!(soil.fertility, before + 1);
}

#[test]
fn planting_consumes_seed_once_and_resuming_requires_remaining_work() {
    let mut world = world();
    world.crops.get_mut(&SiteId(1)).unwrap().phase = CropPhase::Prepared;
    world.crops.get_mut(&SiteId(1)).unwrap().planting_ms = 0;
    assert_eq!(
        start(
            &world,
            FarmOperation::Plant,
            BeginFacts {
                seed_available: Quantity(0),
                ..begin()
            }
        ),
        Err(Error::Unavailable)
    );
    let effect = start(&world, FarmOperation::Plant, begin()).unwrap();
    assert_eq!(
        effect.consumed,
        Some(InputConsumption {
            inventory: InventoryId(4),
            quantity: Quantity(1_000)
        })
    );
    assert_eq!(effect.crop.phase, CropPhase::Planting);
    world.crops.insert(SiteId(1), effect.crop);
    world.jobs.insert(PersonId(3), effect.job);
    let half = world.advance(SimTime(0), SimTime(3_600_000), None).unwrap();
    let resumed = start(
        &half,
        FarmOperation::Plant,
        BeginFacts {
            now: SimTime(3_600_000),
            seed_available: Quantity(0),
            day_of_year: None,
            temperature_c: None,
            ..begin()
        },
    )
    .unwrap();
    assert!(resumed.consumed.is_none());
    assert_eq!(resumed.job.completes, SimTime(7_200_000));
    assert_eq!(resumed.crop.contributions[&PersonId(3)], 3_600_000);
}

#[test]
fn labor_and_restore_are_identical_across_slices() {
    let mut world = world();
    world.crops.get_mut(&SiteId(1)).unwrap().phase = CropPhase::Planting;
    world.crops.get_mut(&SiteId(1)).unwrap().planting_ms = 0;
    world.catalog.crops.values_mut().next().unwrap().plant_labor = SimDuration(120_000);
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
    assert_eq!(whole.crops[&SiteId(1)].contributions[&PersonId(3)], 60_000);
    assert_eq!(whole.crops[&SiteId(1)].phase, CropPhase::Planting);
}

#[test]
fn physical_ineligibility_and_early_completion_are_inert() {
    let mut world = world();
    world.crops.get_mut(&SiteId(1)).unwrap().phase = CropPhase::Ripe;
    let before = world.clone();
    assert_eq!(
        start(
            &world,
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
        finish(&world, &job, closing(SimTime(9))),
        Err(Error::Unavailable)
    );
    assert_eq!(
        finish(
            &world,
            &job,
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
fn duplicate_labor_days_and_overflow_cannot_partially_change_components() {
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
    let mut crop = world.crops[&SiteId(1)].clone();
    crop.degree_days_milli = u64::MAX;
    let before = crop.clone();
    let soil = world.soils[&SiteId(1)].clone();
    assert_eq!(
        grow(
            &crop,
            &soil,
            world.rules_for(SiteId(1)).unwrap(),
            &day(1, 10).cells[&AreaId(2)]
        ),
        Err(Error::Overflow)
    );
    assert_eq!(crop, before);
    assert_eq!(world.soils[&SiteId(1)], soil);
}

#[test]
fn calendar_validation_is_optional_and_has_no_climate_state_dependency() {
    let world = world();
    world.catalog.validate(None).unwrap();
    world.catalog.validate(Some(365)).unwrap();
    assert!(world.catalog.validate(Some(2)).is_err());
    let mut bad = world.catalog.clone();
    bad.crops.values_mut().next().unwrap().prepare_labor = SimDuration(0);
    assert!(bad.validate(None).is_err());
}

#[test]
fn owner_validation_rejects_missing_components_and_corrupt_jobs() {
    let mut valid = world();
    valid.crops.get_mut(&SiteId(1)).unwrap().tending = 0;
    valid.jobs.insert(
        PersonId(3),
        Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Tend,
            labor_ms: 0,
            completes: SimTime(valid.rules_for(SiteId(1)).unwrap().tend_labor.0),
        },
    );
    valid.validate(SimTime(0), Some(365)).unwrap();
    let mut bad = valid.clone();
    bad.crops.get_mut(&SiteId(1)).unwrap().phase = CropPhase::Prepared;
    assert!(bad.validate(SimTime(0), Some(365)).is_err());
    let mut bad = valid.clone();
    bad.jobs.get_mut(&PersonId(3)).unwrap().completes.0 += 1;
    assert!(bad.validate(SimTime(0), Some(365)).is_err());
    let mut bad = valid.clone();
    bad.jobs.insert(PersonId(8), bad.jobs[&PersonId(3)].clone());
    assert!(bad.validate(SimTime(0), None).is_err());
    let mut bad = valid.clone();
    bad.crops
        .get_mut(&SiteId(1))
        .unwrap()
        .contributions
        .insert(PersonId(3), 1);
    assert!(bad.validate(SimTime(0), None).is_err());
    let mut bad = valid.clone();
    bad.soils.clear();
    assert!(bad.validate(SimTime(0), None).is_err());
}

#[test]
fn different_crop_definitions_join_their_own_soil_and_weather_cells() {
    let mut world = world();
    let mut crop = world.crops[&SiteId(1)].clone();
    crop.definition = CropId("test/slow".into());
    let mut rules = rules();
    rules.base_temperature = 9;
    world.catalog.crops.insert(crop.definition.clone(), rules);
    world.crops.insert(SiteId(8), crop);
    let mut field = world.fields[&SiteId(1)].clone();
    field.id = SiteId(8);
    field.area = AreaId(9);
    world.fields.insert(field.id, field);
    world.soils.insert(
        SiteId(8),
        Soil {
            fertility: 400,
            water: 25,
            capacity: 30,
        },
    );
    let mut weather = day(1, 10);
    weather.cells.insert(
        AreaId(9),
        WeatherFacts {
            temperature: 10,
            rain: 0,
            evaporation: 0,
        },
    );
    let next = world
        .advance(SimTime(0), SimTime(60_000), Some(&weather))
        .unwrap();
    assert_eq!(next.crops[&SiteId(1)].degree_days_milli, 10_000);
    assert_eq!(next.crops[&SiteId(8)].degree_days_milli, 500);
    assert_eq!(next.soils[&SiteId(1)].water, 83);
    assert_eq!(next.soils[&SiteId(8)].water, 25);
    assert_eq!(next.fields, world.fields);
}

#[test]
fn unmatched_growth_entities_are_skipped_and_missing_weather_preserves_labor() {
    let mut world = world();
    let mut spare = world.crops[&SiteId(1)].clone();
    spare.tending = 0;
    world.crops.insert(SiteId(8), spare);
    world.jobs.insert(
        PersonId(3),
        Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Tend,
            labor_ms: 0,
            completes: SimTime(60_000),
        },
    );
    let mut weather = day(1, 10);
    weather.cells.clear();
    let next = world
        .advance(SimTime(0), SimTime(1), Some(&weather))
        .unwrap();
    assert_eq!(next.crops[&SiteId(1)].degree_days_milli, 0);
    assert_eq!(next.crops[&SiteId(1)].contributions[&PersonId(3)], 1);
    assert_eq!(next.crops[&SiteId(8)], world.crops[&SiteId(8)]);
    assert_eq!(next.soils, world.soils);
    let mut partial = world.clone();
    partial.soils.clear();
    let next = partial
        .advance(SimTime(0), SimTime(1), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(next.crops[&SiteId(1)].degree_days_milli, 0);
}

#[test]
fn absence_of_calendar_and_weather_only_disables_operations_that_need_them() {
    let mut world = world();
    let facts = BeginFacts {
        day_of_year: None,
        temperature_c: None,
        ..begin()
    };
    assert!(start(&world, FarmOperation::Tend, facts).is_err()); // Already fully tended.
    world.crops.get_mut(&SiteId(1)).unwrap().tending = 0;
    start(&world, FarmOperation::Tend, facts).unwrap();
    world.crops.get_mut(&SiteId(1)).unwrap().phase = CropPhase::Fallow;
    assert_eq!(
        start(&world, FarmOperation::Prepare, facts),
        Err(Error::Unavailable)
    );
    world.crops.get_mut(&SiteId(1)).unwrap().phase = CropPhase::Prepared;
    world.crops.get_mut(&SiteId(1)).unwrap().planting_ms = 0;
    assert_eq!(
        start(&world, FarmOperation::Plant, facts),
        Err(Error::Unavailable)
    );
    world.crops.get_mut(&SiteId(1)).unwrap().phase = CropPhase::Ripe;
    start(&world, FarmOperation::Harvest, facts).unwrap();
}

#[cfg(feature = "content")]
#[test]
fn authored_crop_content_round_trips_and_edits_change_behavior_without_code_changes() {
    let document = Catalog::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/agriculture/crops.toml"
    ))
    .unwrap();
    document.data.validate(Some(365)).unwrap();
    let original = World::from_content(document.clone(), 0).unwrap();
    assert!(original.content.is_some());
    let restored: World = serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    assert_eq!(original, restored);
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/agriculture/crops.toml"
    ))
    .unwrap();
    let edited: game_content::Document<Catalog> =
        game_content::parse(&source.replace("food_yield = 6000000", "food_yield = 250")).unwrap();
    let changed = World::from_content(edited, 0).unwrap();
    assert_ne!(original.content, changed.content);
    let crop = world().crops[&SiteId(1)].clone();
    let soil = Soil {
        fertility: 1_000,
        water: 50,
        capacity: 100,
    };
    assert_eq!(
        yield_amounts(
            &crop,
            &soil,
            &changed.catalog.crops[&changed.catalog.default_crop]
        )
        .0,
        Quantity(250)
    );
    assert_eq!(
        restored.catalog.crops[&restored.catalog.default_crop].food_yield,
        Quantity(6_000_000)
    );
}

#[test]
fn idle_intervals_prepare_no_rows_and_never_run_daily_growth() {
    let world = world();
    let batch = world
        .prepare_advance(SimTime(0), SimTime(1_000), None, |_| {
            panic!("daily growth must be due")
        })
        .unwrap();
    assert_eq!(batch.changed_crop_rows(), 0);
    assert_eq!(batch.changed_job_rows(), 0);
    assert_eq!(batch.growth_rows(), 0);
    let mut next = world.clone();
    next.commit_advance(batch).unwrap();
    assert_eq!(next, world);
}

#[test]
fn sparse_growth_commits_preserve_unrelated_rows_and_metadata() {
    let mut world = world();
    world
        .crops
        .insert(SiteId(8), Crop::new(world.catalog.default_crop.clone()));
    let batch = world
        .prepare_advance_serial(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(batch.changed_crop_rows(), 1);
    assert_eq!(batch.growth_rows(), 1);
    world
        .fields
        .get_mut(&SiteId(1))
        .unwrap()
        .access
        .insert(PersonId(99));
    world.fields.get_mut(&SiteId(1)).unwrap().food = InventoryId(99);
    world.crops.get_mut(&SiteId(8)).unwrap().tending = 500;
    world.catalog.default_crop = CropId("unrelated/default-selection".into());
    let metadata = world.fields.clone();
    world.commit_advance(batch).unwrap();
    assert_eq!(world.crops[&SiteId(1)].degree_days_milli, 10_000);
    assert_eq!(world.crops[&SiteId(8)].tending, 500);
    assert_eq!(world.fields, metadata);
}

#[test]
fn stale_read_rows_reject_the_entire_sparse_commit_before_publication() {
    let world = world();
    let batch = world
        .prepare_advance_serial(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    for mutation in 0..6 {
        let mut changed = world.clone();
        match mutation {
            0 => changed.crops.get_mut(&SiteId(1)).unwrap().tending = 0,
            1 => changed.soils.get_mut(&SiteId(1)).unwrap().water = 0,
            2 => changed.fields.get_mut(&SiteId(1)).unwrap().area = AreaId(99),
            3 => {
                changed
                    .catalog
                    .crops
                    .values_mut()
                    .next()
                    .unwrap()
                    .base_temperature += 1
            }
            4 => changed.day += 1,
            5 => {
                changed.jobs.insert(
                    PersonId(99),
                    Cultivation {
                        field: SiteId(1),
                        operation: FarmOperation::Tend,
                        labor_ms: 0,
                        completes: SimTime(120_000),
                    },
                );
            }
            _ => unreachable!(),
        }
        let before = changed.clone();
        assert!(changed.commit_advance(batch.clone()).is_err());
        assert_eq!(changed, before);
    }
}

#[test]
fn executor_errors_wrong_keys_and_foreign_writes_cannot_prepare_changes() {
    let world = world();
    let before = world.clone();
    let forcing = day(1, 10);
    assert!(
        world
            .prepare_advance(SimTime(0), SimTime(1), Some(&forcing), |_| Err(
                Error::Overflow
            ))
            .is_err()
    );
    assert!(
        world
            .prepare_advance(SimTime(0), SimTime(1), Some(&forcing), |inputs| {
                Ok(vec![(SiteId(99), inputs[0].1.evaluate()?)])
            })
            .is_err()
    );
    assert!(
        world
            .prepare_advance(SimTime(0), SimTime(1), Some(&forcing), |inputs| {
                let mut output = inputs[0].1.evaluate()?;
                output.crop.contributions.insert(PersonId(99), 1);
                Ok(vec![(inputs[0].0, output)])
            })
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn pure_growth_batches_are_identical_under_threaded_evaluation_and_zero_duration() {
    let mut world = world();
    for id in 8..24 {
        let mut field = world.fields[&SiteId(1)].clone();
        field.id = SiteId(id);
        world.fields.insert(field.id, field);
        world
            .crops
            .insert(SiteId(id), world.crops[&SiteId(1)].clone());
        world
            .soils
            .insert(SiteId(id), world.soils[&SiteId(1)].clone());
    }
    world.jobs.insert(
        PersonId(3),
        Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Tend,
            labor_ms: 0,
            completes: SimTime(60_000),
        },
    );
    let forcing = day(1, 10);
    let serial = world
        .prepare_advance_serial(SimTime(0), SimTime(0), Some(&forcing))
        .unwrap();
    let parallel = world
        .prepare_advance(SimTime(0), SimTime(0), Some(&forcing), |inputs| {
            std::thread::scope(|scope| {
                let handles: Vec<_> = inputs
                    .iter()
                    .map(|(id, input)| scope.spawn(move || Ok((*id, input.evaluate()?))))
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().unwrap())
                    .collect()
            })
        })
        .unwrap();
    let mut serial_world = world.clone();
    let mut parallel_world = world.clone();
    serial_world.commit_advance(serial).unwrap();
    parallel_world.commit_advance(parallel).unwrap();
    assert_eq!(serial_world, parallel_world);
    assert_eq!(
        parallel_world.crops[&SiteId(1)].contributions,
        BTreeMap::new()
    );
    assert_eq!(
        world.next_boundary(SimTime(0), SimTime(120_000)),
        SimTime(60_000)
    );
}
