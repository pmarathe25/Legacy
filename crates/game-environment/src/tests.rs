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
    assert_eq!(whole, split);
    split.world.validate(SimTime(10), 19).unwrap();
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
