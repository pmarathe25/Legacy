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
