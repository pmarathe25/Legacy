use game_body::{Body, Kind, Rates, Rules, advance_rates, hunger_threshold};
use game_core::*;
fn body() -> Body {
    Body {
        kind: Kind::Mortal,
        born: SimTime(0),
        alive: true,
        hunger: 0,
        fatigue: 80,
        fatigue_fraction: 0,
        hunger_fraction: 0,
        recovery_fraction: 0,
        metabolic_rates: Default::default(),
        health: 100,
        maximum_health: None,
        resting: false,
        last: SimTime(0),
        life_limit: Some(SimTime(1000 * DAY.0)),
    }
}
#[test]
fn fractional_rates_and_starvation_crossings_survive_timestep_partitioning() {
    let rules = Rules {
        deprivation: 5,
        ..Default::default()
    };
    let rates = Rates {
        energy: 333,
        fatigue: 777,
        recovery: 455,
    };
    let mut split = body();
    for tick in 1..=40 {
        split = advance_rates(&split, &rules, SimTime(tick * MINUTE.0), rates);
    }
    let bulk = advance_rates(&body(), &rules, SimTime(40 * MINUTE.0), rates);
    assert_eq!(bulk, split);
    assert_eq!(bulk.health, 75);
    let loaded: Body = serde_json::from_str(&serde_json::to_string(&bulk).unwrap()).unwrap();
    assert_eq!(
        advance_rates(&loaded, &rules, SimTime(41 * MINUTE.0), rates),
        advance_rates(&split, &rules, SimTime(41 * MINUTE.0), rates)
    );
    let safe = advance_rates(
        &body(),
        &rules,
        SimTime(40 * MINUTE.0),
        Rates { energy: 0, ..rates },
    );
    assert_eq!(safe.health, 100);
    assert_eq!(safe.hunger, 0);
}
#[test]
fn recovery_retains_fractions_and_forecasts_use_expressed_energy_demand() {
    let rules = Rules::default();
    let rates = Rates {
        energy: 333,
        fatigue: 777,
        recovery: 455,
    };
    let mut resting = body();
    resting.resting = true;
    let mut split = resting;
    for tick in 1..=40 {
        split = advance_rates(&split, &rules, SimTime(tick * MINUTE.0), rates);
    }
    assert_eq!(
        split,
        advance_rates(&resting, &rules, SimTime(40 * MINUTE.0), rates)
    );
    let active = advance_rates(&body(), &rules, SimTime(7 * MINUTE.0), rates);
    let at = hunger_threshold(&active, &rules, 10).unwrap();
    assert!(advance_rates(&active, &rules, SimTime(at.0 - MINUTE.0), rates).hunger < 10);
    assert!(advance_rates(&active, &rules, at, rates).hunger >= 10);
}
