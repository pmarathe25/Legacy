mod support;
use game_agriculture::*;
use game_core::*;
use support::*;

#[test]
fn crops_require_paid_inputs_growth_and_harvest_labor() {
    let mut f = setup();
    f.host
        .dispatch(game_environment::Configure(game_environment::Climate {
            seasons: vec![game_environment::Season {
                duration: DAY,
                warmth: 10,
                rain: 1,
            }],
        }))
        .unwrap();
    let plot = f
        .host
        .dispatch(Create(Plot {
            area: f.home,
            steward: f.ruler,
            phase: Phase::Fallow,
            crop: "grain".into(),
            ..Default::default()
        }))
        .unwrap();
    f.host
        .dispatch(Sow {
            person: f.ruler,
            plot,
            seed: f.store,
        })
        .unwrap();
    assert_eq!(
        f.host.get::<game_cargo::Cargo>().unwrap().0[&f.store].amount(game_cargo::SEED),
        90
    );
    assert!(
        f.host
            .dispatch(Harvest {
                person: f.ruler,
                plot,
                destination: f.store
            })
            .is_err()
    );
    run(&mut f.host, SimTime(HOUR.0 * 2));
    assert_eq!(
        f.host.get::<Farms>().unwrap().plots[&plot].phase,
        Phase::Ripe { yield_units: 50 }
    );
    f.host
        .dispatch(Harvest {
            person: f.ruler,
            plot,
            destination: f.store,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.store), 1000);
    run(&mut f.host, SimTime(HOUR.0 * 3));
    assert_eq!(food(&f.host, f.store), 1050);
    assert_eq!(
        f.host.get::<Farms>().unwrap().plots[&plot].phase,
        Phase::Fallow
    );
}
#[test]
fn canceled_sowing_does_not_refund_consumed_seed() {
    let mut f = setup();
    let plot = f
        .host
        .dispatch(Create(Plot {
            area: f.home,
            steward: f.ruler,
            phase: Phase::Fallow,
            crop: "grain".into(),
            ..Default::default()
        }))
        .unwrap();
    f.host
        .dispatch(Sow {
            person: f.ruler,
            plot,
            seed: f.store,
        })
        .unwrap();
    f.host
        .dispatch(game_physiology::Damage {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    assert_eq!(f.host.get::<Farms>().unwrap().lost_seed, 10);
    assert_eq!(
        f.host.get::<Farms>().unwrap().plots[&plot].phase,
        Phase::Fallow
    );
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}

#[test]
fn regional_climate_changes_real_crop_output() {
    let mut f = setup();
    let climate = |warmth| game_environment::Climate {
        seasons: vec![game_environment::Season {
            duration: DAY,
            warmth,
            rain: 4,
        }],
    };
    f.host
        .dispatch(game_environment::Configure(climate(20)))
        .unwrap();
    f.host
        .dispatch(game_environment::ConfigureArea {
            area: f.away,
            climate: climate(-5),
        })
        .unwrap();
    let local = f
        .host
        .dispatch(Create(Plot {
            area: f.home,
            steward: f.ruler,
            phase: Phase::Fallow,
            crop: "grain".into(),
            ..Default::default()
        }))
        .unwrap();
    let remote = f
        .host
        .dispatch(Create(Plot {
            area: f.away,
            steward: f.ruler,
            phase: Phase::Fallow,
            crop: "grain".into(),
            ..Default::default()
        }))
        .unwrap();
    f.host
        .dispatch(Sow {
            person: f.ruler,
            plot: local,
            seed: f.store,
        })
        .unwrap();
    run(&mut f.host, SimTime(HOUR.0));
    f.host
        .dispatch(game_transport::Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, SimTime(HOUR.0 + 120_000));
    let supplies = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(f.away),
            capacity: 100,
        })
        .unwrap();
    f.host
        .dispatch(game_property::ClaimInventory {
            claimant: f.ruler,
            inventory: supplies,
            recognized_by: [f.ruler].into(),
        })
        .unwrap();
    f.host
        .dispatch(game_cargo::Deposit {
            inventory: supplies,
            good: game_cargo::SEED,
            quantity: 10,
        })
        .unwrap();
    f.host
        .dispatch(Sow {
            person: f.ruler,
            plot: remote,
            seed: supplies,
        })
        .unwrap();
    run(&mut f.host, SimTime(4 * HOUR.0));
    assert_eq!(
        f.host.get::<Farms>().unwrap().plots[&local].phase,
        Phase::Ripe { yield_units: 50 }
    );
    assert_eq!(
        f.host.get::<Farms>().unwrap().plots[&remote].phase,
        Phase::Ripe { yield_units: 0 }
    );
}
