use super::*;

fn config() -> Config {
    Config {
        calendar: Calendar {
            day: SimDuration(10),
            season_days: [1; 4],
            starting_day: 3,
        },
        climate: Climate {
            seasonal_temperature: [10, 20, 30, 40],
            seasonal_rain: [1, 2, 3, 4],
            seasonal_evaporation: [0; 4],
            temperature_variation: 0,
            rainfall_variation: 0,
        },
    }
}
#[test]
fn day_effect_uses_elapsed_weather_before_publishing_next_season() {
    let world = World::new(config(), SimTime(0), 7).unwrap();
    assert_eq!(world.weather.temperature, 40);
    let step = world.advance(SimTime(0), SimTime(10), 7).unwrap();
    assert_eq!(
        step.elapsed_day,
        Some(DayFacts {
            day: 1,
            day_of_year: 3,
            weather: world.weather.clone()
        })
    );
    let closing = world.preview(&step).unwrap();
    assert_eq!(closing.weather.temperature, 10);
    assert_eq!(closing.calendar.day_of_year(SimTime(10)), 0);
    assert_eq!(world.weather_day, 0);
}
#[test]
fn irregular_slices_and_current_save_produce_identical_daily_facts() {
    let mut configured = config();
    configured.climate.temperature_variation = 3;
    configured.climate.rainfall_variation = 3;
    let world = World::new(configured, SimTime(0), 19).unwrap();
    let whole = world.advance(SimTime(0), SimTime(10), 19).unwrap();
    let early = world.advance(SimTime(0), SimTime(3), 19).unwrap();
    assert!(early.elapsed_day.is_none());
    let encoded = serde_json::to_vec(&world.preview(&early).unwrap()).unwrap();
    let loaded: World = serde_json::from_slice(&encoded).unwrap();
    let split = loaded.advance(SimTime(3), SimTime(10), 19).unwrap();
    let resumed = loaded.preview(&split).unwrap();
    assert_eq!(world.preview(&whole).unwrap(), resumed);
    assert_eq!(whole.elapsed_day, split.elapsed_day);
    resumed.validate(SimTime(10), 19).unwrap();
}

#[test]
fn runtime_owns_deadline_observation_and_publication() {
    let mut runtime = World::new(config(), SimTime(7), 19).unwrap();
    let before = runtime.clone();
    assert_eq!(runtime.deadline(SimTime(100)).unwrap(), SimTime(10));
    let output = runtime.prepare(SimTime(10)).unwrap();
    assert_eq!(runtime, before);
    assert_eq!(runtime.observe().at, SimTime(7));
    assert_eq!(runtime.observe().season, 3);
    assert_eq!(output.elapsed_day.as_ref().unwrap().weather.temperature, 40);
    runtime.commit(output).unwrap();
    assert_eq!(runtime.epoch(), SimTime(10));
    assert_eq!(runtime.observe().season, 0);
    assert_eq!(runtime.observe().weather.temperature, 10);
    assert_eq!(runtime.deadline(SimTime(100)).unwrap(), SimTime(20));
}

#[test]
fn stale_and_changed_prepared_effects_cannot_publish() {
    let mut runtime = World::new(config(), SimTime(0), 19).unwrap();
    let first = runtime.prepare(SimTime(10)).unwrap();
    let stale = first.clone();
    runtime.commit(first).unwrap();
    let closing = runtime.clone();
    assert_eq!(runtime.commit(stale), Err(Error::SequenceConflict));
    assert_eq!(runtime, closing);
    let mut changed = runtime.prepare(SimTime(20)).unwrap();
    changed.elapsed_day.as_mut().unwrap().weather.rain += 1;
    assert_eq!(runtime.commit(changed), Err(Error::SequenceConflict));
    assert_eq!(runtime, closing);
    let mut changed = runtime.prepare(SimTime(20)).unwrap();
    changed.daily.as_mut().unwrap().weather.rain += 1;
    assert_eq!(runtime.commit(changed), Err(Error::SequenceConflict));
    assert_eq!(runtime, closing);
}

#[test]
fn save_restores_feature_epoch_and_rejects_a_host_clock_or_seed_mismatch() {
    let mut runtime = World::new(config(), SimTime(0), 19).unwrap();
    runtime
        .commit(runtime.prepare(SimTime(7)).unwrap())
        .unwrap();
    let encoded = serde_json::to_vec(&runtime).unwrap();
    let restored: World = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(restored.epoch(), SimTime(7));
    restored.validate(SimTime(7), 19).unwrap();
    assert!(restored.validate(SimTime(8), 19).is_err());
    assert!(restored.validate(SimTime(7), 20).is_err());
    assert!(restored.boundary(SimTime(8), SimTime(10)).is_err());
    let uninterrupted = runtime.prepare(SimTime(10)).unwrap();
    let resumed = restored.prepare(SimTime(10)).unwrap();
    assert_eq!(uninterrupted, resumed);
}

#[test]
fn opening_epoch_reset_validates_before_mutating() {
    let mut runtime = World::new(config(), SimTime(0), 19).unwrap();
    runtime.reset_epoch(SimTime(25), 3).unwrap();
    let configured = World::new(config(), SimTime(25), 3).unwrap();
    assert_eq!(runtime, configured);
    runtime.calendar.day = SimDuration(0);
    let before = runtime.clone();
    assert!(runtime.reset_epoch(SimTime(5), 4).is_err());
    assert_eq!(runtime, before);
}

fn upland() -> LocalClimate {
    LocalClimate {
        temperature_offset: -5,
        rain_multiplier_milli: 2_000,
        evaporation_multiplier_milli: 500,
    }
}

#[test]
fn local_cells_publish_their_own_elapsed_forcing_and_observations() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world
        .register_cell(AreaId(1), LocalClimate::default())
        .unwrap();
    world.register_cell(AreaId(2), upland()).unwrap();
    assert_eq!(world.local_weather(AreaId(1)), Some(&world.weather));
    assert_eq!(world.local_weather(AreaId(2)).unwrap().temperature, 35);
    assert_eq!(world.local_weather(AreaId(2)).unwrap().rain, 8);
    assert!(world.local_weather(AreaId(3)).is_none());
    assert!(world.observe_cell(AreaId(3)).is_none());
    let local_before = world.observe_cell(AreaId(2)).unwrap();
    let step = world.prepare(SimTime(10)).unwrap();
    let elapsed = step.elapsed_local_day.as_ref().unwrap();
    assert_eq!(elapsed.day, 1);
    assert_eq!(elapsed.day_of_year, 3);
    assert_eq!(elapsed.cells[&AreaId(2)], local_before.weather);
    let closing = world.preview(&step).unwrap();
    assert_eq!(closing.local_weather(AreaId(2)).unwrap().temperature, 5);
    assert_eq!(closing.observe_cell(AreaId(2)).unwrap().season, 0);
    assert_eq!(world.observe_cell(AreaId(2)), Some(local_before));
    world.commit(step).unwrap();
    assert_eq!(world.local_weather(AreaId(2)).unwrap().rain, 2);
}

#[test]
fn local_cell_registration_order_slices_and_restores_do_not_change_forcing() {
    let mut first = World::new(config(), SimTime(0), 19).unwrap();
    let mut second = first.clone();
    for (area, policy) in [(AreaId(1), LocalClimate::default()), (AreaId(2), upland())] {
        first.register_cell(area, policy).unwrap();
    }
    for (area, policy) in [(AreaId(2), upland()), (AreaId(1), LocalClimate::default())] {
        second.register_cell(area, policy).unwrap();
    }
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    let whole = first.prepare(SimTime(10)).unwrap();
    second.commit(second.prepare(SimTime(3)).unwrap()).unwrap();
    let restored: World = serde_json::from_slice(&serde_json::to_vec(&second).unwrap()).unwrap();
    let split = restored.prepare(SimTime(10)).unwrap();
    assert_eq!(
        first.preview(&whole).unwrap(),
        restored.preview(&split).unwrap()
    );
    assert_eq!(whole.elapsed_local_day, split.elapsed_local_day);
    assert_eq!(whole.elapsed_day, split.elapsed_day);
}

#[test]
fn short_intervals_prepare_no_weather_work_and_leave_cell_storage_in_place() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world
        .register_cell(AreaId(1), LocalClimate::default())
        .unwrap();
    world.register_cell(AreaId(2), upland()).unwrap();
    let cells = world.cells.clone();
    let address = std::ptr::from_ref(&world.cells[&AreaId(2)]);
    for next in 1..10 {
        let step = world.prepare(SimTime(next)).unwrap();
        assert!(step.daily.is_none());
        assert!(step.elapsed_day.is_none());
        assert!(step.elapsed_local_day.is_none());
        world.commit(step).unwrap();
        assert_eq!(world.epoch(), SimTime(next));
        assert_eq!(world.cells, cells);
        assert_eq!(std::ptr::from_ref(&world.cells[&AreaId(2)]), address);
    }
    let step = world.prepare(SimTime(10)).unwrap();
    assert_eq!(step.daily.as_ref().unwrap().cells.len(), 2);
    assert_eq!(step.elapsed_local_day.as_ref().unwrap().cells.len(), 2);
    world.commit(step).unwrap();
    assert_eq!(std::ptr::from_ref(&world.cells[&AreaId(2)]), address);
    assert_ne!(world.cells, cells);
}

#[test]
fn changed_daily_membership_or_a_late_cell_guard_cannot_partially_publish() {
    let mut opening = World::new(config(), SimTime(0), 19).unwrap();
    opening
        .register_cell(AreaId(1), LocalClimate::default())
        .unwrap();
    opening.register_cell(AreaId(2), upland()).unwrap();
    for variation in 0..4 {
        let mut world = opening.clone();
        let step = world.prepare(SimTime(10)).unwrap();
        match variation {
            0 => world
                .register_cell(AreaId(3), LocalClimate::default())
                .unwrap(),
            1 => {
                world.cells.remove(&AreaId(2));
            }
            2 => {
                let existing = world.cells.remove(&AreaId(2)).unwrap();
                world.cells.insert(AreaId(3), existing);
            }
            3 => {
                world.cells.get_mut(&AreaId(2)).unwrap().weather.rain += 1;
            }
            _ => unreachable!(),
        }
        let before = world.clone();
        assert_eq!(world.commit(step), Err(Error::SequenceConflict));
        assert_eq!(world, before);
        assert_eq!(world.epoch(), SimTime(0));
        assert_eq!(
            world.cells[&AreaId(1)].weather,
            opening.cells[&AreaId(1)].weather
        );
    }
}

#[test]
fn short_interval_header_changes_and_added_forcing_are_rejected_before_publication() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    let mut changed = world.prepare(SimTime(1)).unwrap();
    changed.elapsed_local_day = Some(LocalDayFacts {
        day: 1,
        day_of_year: 3,
        cells: BTreeMap::new(),
    });
    let before = world.clone();
    assert_eq!(world.commit(changed), Err(Error::SequenceConflict));
    assert_eq!(world, before);
    let step = world.prepare(SimTime(1)).unwrap();
    world.climate.seasonal_temperature[0] += 1;
    let before = world.clone();
    assert_eq!(world.commit(step), Err(Error::SequenceConflict));
    assert_eq!(world, before);
}

#[test]
fn invalid_local_policies_and_future_extremes_are_rejected_before_registration() {
    let configured = Config {
        climate: Climate {
            seasonal_rain: [1, 1, 1, 1_000_000],
            ..config().climate
        },
        ..config()
    };
    let mut world = World::new(configured, SimTime(10), 19).unwrap();
    let before = world.clone();
    assert!(world.register_cell(AreaId(1), upland()).is_err());
    assert_eq!(world, before);
    assert!(
        world
            .register_cell(AreaId(0), LocalClimate::default())
            .is_err()
    );
    assert!(
        world
            .register_cell(
                AreaId(1),
                LocalClimate {
                    temperature_offset: 101,
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert_eq!(world, before);
    world
        .register_cell(AreaId(1), LocalClimate::default())
        .unwrap();
    let before = world.clone();
    assert!(
        world
            .register_cell(AreaId(1), LocalClimate::default())
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn local_cell_corruption_and_changed_elapsed_facts_cannot_be_committed() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world.register_cell(AreaId(2), upland()).unwrap();
    let mut corrupt = world.clone();
    corrupt.cells.get_mut(&AreaId(2)).unwrap().weather.rain += 1;
    assert!(corrupt.validate(SimTime(0), 19).is_err());
    let before = world.clone();
    let mut changed = world.prepare(SimTime(10)).unwrap();
    changed
        .elapsed_local_day
        .as_mut()
        .unwrap()
        .cells
        .get_mut(&AreaId(2))
        .unwrap()
        .rain += 1;
    assert_eq!(world.commit(changed), Err(Error::SequenceConflict));
    assert_eq!(world, before);
    let mut changed = world.prepare(SimTime(10)).unwrap();
    changed.daily.as_mut().unwrap().cells.clear();
    assert_eq!(world.commit(changed), Err(Error::SequenceConflict));
    assert_eq!(world, before);
}

#[test]
fn resetting_an_opening_epoch_preserves_local_policies_and_samples_new_weather() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world.register_cell(AreaId(2), upland()).unwrap();
    world.reset_epoch(SimTime(25), 3).unwrap();
    assert_eq!(world.cells[&AreaId(2)].policy, upland());
    assert_eq!(
        world.local_weather(AreaId(2)),
        Some(&upland().apply(&world.weather).unwrap())
    );
    assert_eq!(world.local_weather(AreaId(2)).unwrap().temperature, 15);
    world.validate(SimTime(25), 3).unwrap();
}

#[cfg(feature = "content")]
#[test]
fn runtime_loads_edited_toml_without_changing_an_existing_save() {
    let bundled = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/environment/temperate.toml");
    let source = std::fs::read_to_string(&bundled).unwrap();
    let original = Config::read(&bundled).unwrap();
    assert_eq!(original.data, Config::default());
    let runtime = World::from_content(original, SimTime(0), 19).unwrap();
    let before = serde_json::to_vec(&runtime).unwrap();
    let path = std::env::temp_dir().join(format!(
        "legacy-environment-content-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, source.replace("[12, 20, 10, 0]", "[30, 30, 30, 30]")).unwrap();
    let edited = World::from_content(Config::read(&path).unwrap(), SimTime(0), 19).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_ne!(runtime.content_stamp(), edited.content_stamp());
    assert_ne!(runtime.observe().weather, edited.observe().weather);
    let restored: World = serde_json::from_slice(&before).unwrap();
    assert_eq!(restored, runtime);
    restored.validate(SimTime(0), 19).unwrap();
    assert_eq!(
        restored.prepare(SimTime(86_400_000)),
        runtime.prepare(SimTime(86_400_000))
    );
}

#[cfg(feature = "content")]
#[test]
fn invalid_or_changed_authored_definitions_are_rejected() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/environment/temperate.toml"),
    )
    .unwrap();
    let invalid =
        game_content::parse::<Config>(&source.replace("day = 86400000", "day = 0")).unwrap();
    assert!(World::from_content(invalid, SimTime(0), 19).is_err());
    assert!(
        game_content::parse::<Config>(
            &source.replace("temperature_variation = 3", "temperature_varation = 3")
        )
        .is_err()
    );
    let document = game_content::parse::<Config>(&source).unwrap();
    let mut world = World::from_content(document, SimTime(0), 19).unwrap();
    world.climate.seasonal_temperature[0] += 1;
    world.weather = world.climate.weather(19, 0, 0);
    assert!(world.validate(SimTime(0), 19).is_err());
    let before = world.clone();
    assert!(world.reset_epoch(SimTime(10), 19).is_err());
    assert_eq!(world, before);
}

#[cfg(feature = "content")]
#[test]
fn attaching_content_preserves_local_components_and_epoch_and_rejects_mismatches() {
    let bundled = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/environment/temperate.toml");
    let document = Config::read(&bundled).unwrap();
    let expected_stamp = document.stamp().unwrap();
    let mut runtime = World::new(document.data.clone(), SimTime(7), 19).unwrap();
    runtime.register_cell(AreaId(2), upland()).unwrap();
    let components = runtime.cells.clone();
    runtime.attach_content(document.clone()).unwrap();
    assert_eq!(runtime.epoch(), SimTime(7));
    assert_eq!(runtime.cells, components);
    assert_eq!(runtime.content_stamp(), Some(&expected_stamp));
    let before = runtime.clone();
    let mut changed = document.clone();
    changed.data.climate.seasonal_rain[0] += 1;
    assert!(runtime.attach_content(changed).is_err());
    assert_eq!(runtime, before);
    let mut invalid = document;
    invalid.version = 0;
    assert!(runtime.attach_content(invalid).is_err());
    assert_eq!(runtime, before);
}
#[test]
fn invalid_calendar_is_rejected_before_division_and_weather_sampling() {
    for bad in [
        Config {
            calendar: Calendar {
                day: SimDuration(0),
                ..config().calendar
            },
            ..config()
        },
        Config {
            calendar: Calendar {
                season_days: [0, 1, 1, 1],
                ..config().calendar
            },
            ..config()
        },
        Config {
            calendar: Calendar {
                starting_day: 4,
                ..config().calendar
            },
            ..config()
        },
    ] {
        assert!(World::new(bad, SimTime(0), 0).is_err());
    }
    let bad = Config {
        climate: Climate {
            temperature_variation: 101,
            ..config().climate
        },
        ..config()
    };
    assert!(World::new(bad, SimTime(0), 0).is_err());
}
#[test]
fn skipped_reversed_and_corrupted_intervals_leave_the_snapshot_unchanged() {
    let world = World::new(config(), SimTime(0), 1).unwrap();
    let before = world.clone();
    assert!(world.advance(SimTime(0), SimTime(20), 1).is_err());
    assert!(world.advance(SimTime(10), SimTime(9), 1).is_err());
    assert_eq!(world, before);
    let mut corrupt = world;
    corrupt.weather.rain += 1;
    assert!(corrupt.advance(SimTime(0), SimTime(1), 1).is_err());
}
#[test]
fn extreme_time_and_day_boundary_do_not_overflow() {
    let configured = Config {
        calendar: Calendar {
            day: SimDuration(1),
            ..config().calendar
        },
        ..config()
    };
    let world = World::new(configured, SimTime(u64::MAX - 1), 2).unwrap();
    let step = world
        .advance(SimTime(u64::MAX - 1), SimTime(u64::MAX), 2)
        .unwrap();
    let closing = world.preview(&step).unwrap();
    assert_eq!(closing.weather_day, u64::MAX);
    assert_eq!(step.elapsed_day.unwrap().day, u64::MAX);
    assert_eq!(
        closing
            .boundary(SimTime(u64::MAX), SimTime(u64::MAX))
            .unwrap(),
        SimTime(u64::MAX)
    );
    assert_eq!(closing.calendar.day_of_year(SimTime(u64::MAX)), 2);
}
