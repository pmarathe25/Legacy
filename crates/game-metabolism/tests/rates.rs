use game_core::*;
use game_metabolism::{demand, rates};
use game_vitality::{Life, Upbringing};
fn life() -> Life {
    Life {
        born: SimTime(0),
        span: Some(SimDuration(120 * 365 * DAY.0)),
        base_health: 100,
        expressed: true,
        physical: 1000,
        stress: 0,
        satisfaction: 0,
        updated: SimTime(0),
        upbringing: Upbringing::default(),
    }
}
#[test]
fn inherited_metabolic_potential_trades_food_demand_for_recovery() {
    let mut traits = game_traits::load(&mut game_content::Store::standard()).unwrap();
    let p = PersonId(1);
    let at = SimTime(30 * 365 * DAY.0);
    let life = life();
    traits
        .people
        .insert(p, [("metabolism".into(), 500)].into_iter().collect());
    let slow = rates(&traits, &life, p, at, Default::default());
    traits
        .people
        .get_mut(&p)
        .unwrap()
        .insert("metabolism".into(), 2000);
    let fast = rates(&traits, &life, p, at, Default::default());
    assert_eq!(fast.energy, slow.energy * 4);
    assert_eq!(fast.recovery, slow.recovery * 4);
}
#[test]
fn development_aging_condition_and_thermal_load_have_distinct_costs() {
    let mut traits = game_traits::load(&mut game_content::Store::standard()).unwrap();
    let p = PersonId(1);
    let life = life();
    let at = SimTime(30 * 365 * DAY.0);
    let adult = rates(&traits, &life, p, at, Default::default());
    assert!(demand(&traits, &life, p, SimTime(0)) < adult.energy);
    let elder = rates(
        &traits,
        &life,
        p,
        SimTime(110 * 365 * DAY.0),
        Default::default(),
    );
    assert!(elder.fatigue > adult.fatigue);
    assert!(elder.recovery < adult.recovery);
    let stressed = Life {
        stress: 900,
        updated: at,
        ..life
    };
    let impaired = rates(&traits, &stressed, p, at, Default::default());
    assert!(impaired.fatigue > adult.fatigue);
    assert!(impaired.recovery < adult.recovery);
    let climate = game_body::Rates {
        energy: 1500,
        fatigue: 1500,
        recovery: 500,
    };
    let exposed = rates(&traits, &life, p, at, climate);
    assert!(exposed.energy > adult.energy);
    assert!(exposed.fatigue > adult.fatigue);
    assert!(exposed.recovery < adult.recovery);
    traits.people.insert(
        p,
        [("size".into(), 3000), ("constitution".into(), 6000)]
            .into_iter()
            .collect(),
    );
    assert!(demand(&traits, &life, p, at) > adult.energy);
}
