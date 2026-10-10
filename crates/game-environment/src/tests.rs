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
    assert_eq!(step.world.weather.temperature, 10);
    assert_eq!(step.world.calendar.day_of_year(SimTime(10)), 0);
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
    let encoded = serde_json::to_vec(&early.world).unwrap();
    let loaded: World = serde_json::from_slice(&encoded).unwrap();
    let split = loaded.advance(SimTime(3), SimTime(10), 19).unwrap();
    assert_eq!(whole.world, split.world);
    assert_eq!(whole.elapsed_day, split.elapsed_day);
    split.world.validate(SimTime(10), 19).unwrap();
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
    changed.world.weather.rain += 1;
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
    assert_eq!(step.world.weather_day, u64::MAX);
    assert_eq!(step.elapsed_day.unwrap().day, u64::MAX);
    assert_eq!(
        step.world
            .boundary(SimTime(u64::MAX), SimTime(u64::MAX))
            .unwrap(),
        SimTime(u64::MAX)
    );
    assert_eq!(step.world.calendar.day_of_year(SimTime(u64::MAX)), 2);
}
