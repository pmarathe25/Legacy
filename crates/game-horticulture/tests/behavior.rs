mod support;
use game_core::*;
use game_horticulture::*;
use support::*;
fn crops(h: &mut game_sim::Host) {
    h.dispatch(game_agriculture::InstallCrops(game_agriculture::Crops {
        items: [(
            "beans".into(),
            game_agriculture::Crop {
                name: "Beans".into(),
                seed: good(h, "bean-seed"),
                seed_input: 8,
                output: good(h, "beans"),
                yield_units: 20,
                growth: MINUTE,
                sow_labor: MINUTE,
                harvest_labor: MINUTE,
                minimum_warmth: 1,
                minimum_rain: 1,
                ..Default::default()
            },
        )]
        .into(),
        stamps: vec![],
        natural: false,
    }))
    .unwrap();
    h.dispatch(game_environment::Configure(
        game_environment::load(&mut game_content::Store::standard(), "river-valley").unwrap(),
    ))
    .unwrap();
}
#[test]
fn planting_emerges_from_seed_and_labor_and_harvest_is_paid_separately() {
    let (mut h, p, i, _) = setup();
    crops(&mut h);
    deposit(&mut h, i, "bean-seed", 16);
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("patience".into(), 1000)].into(),
    })
    .unwrap();
    h.dispatch(Enable).unwrap();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    assert_eq!(amount(&h, i, "bean-seed"), 8);
    assert_eq!(amount(&h, i, "beans"), 0);
    run(&mut h, 2 * MINUTE.0);
    let plot = h.get::<Gardens>().unwrap().plots[&p];
    assert!(matches!(
        h.get::<game_agriculture::Farms>().unwrap().plots[&plot].phase,
        game_agriculture::Phase::Ripe { yield_units: 20 }
    ));
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        run(host, HOUR.0);
        assert!(host.dispatch(Consider { person: p }).unwrap());
        assert_eq!(amount(host, i, "beans"), 0);
        run(host, HOUR.0 + MINUTE.0);
        assert_eq!(amount(host, i, "beans"), 20);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn failed_planting_rolls_back_the_plot_and_an_existing_action() {
    let (mut h, p, i, _) = setup();
    crops(&mut h);
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Plant {
            person: p,
            crop: "beans".into(),
            inventory: i
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
    assert!(h.get::<game_agriculture::Farms>().unwrap().plots.is_empty());
    deposit(&mut h, i, "bean-seed", 8);
    h.dispatch(game_physiology::Rest { person: p }).unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Plant {
            person: p,
            crop: "beans".into(),
            inventory: i
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
}
#[test]
fn autonomous_gardening_respects_disposition_and_player_control() {
    let (mut h, p, i, _) = setup();
    crops(&mut h);
    deposit(&mut h, i, "bean-seed", 16);
    h.dispatch(Enable).unwrap();
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("patience".into(), 0)].into(),
    })
    .unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    run(&mut h, HOUR.0);
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("patience".into(), 1000)].into(),
    })
    .unwrap();
    h.dispatch(game_lineage::Begin { person: p }).unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert!(h.get::<Gardens>().unwrap().plots.is_empty());
}
#[test]
fn cancellation_does_not_refund_seed_or_create_a_harvest() {
    let (mut h, p, i, _) = setup();
    crops(&mut h);
    deposit(&mut h, i, "bean-seed", 8);
    h.dispatch(Plant {
        person: p,
        crop: "beans".into(),
        inventory: i,
    })
    .unwrap();
    h.dispatch(Cancel(p)).unwrap();
    run(&mut h, HOUR.0);
    assert_eq!(amount(&h, i, "bean-seed"), 0);
    assert_eq!(amount(&h, i, "beans"), 0);
    assert_eq!(h.get::<game_agriculture::Farms>().unwrap().lost_seed, 8);
}

#[test]
fn unsuitable_climate_prevents_an_autonomous_sowing_attempt() {
    let (mut h, p, i, area) = setup();
    crops(&mut h);
    deposit(&mut h, i, "bean-seed", 8);
    h.dispatch(Enable).unwrap();
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("patience".into(), 1000)].into(),
    })
    .unwrap();
    h.dispatch(game_environment::ConfigureArea {
        area,
        climate: game_environment::Climate {
            seasons: vec![game_environment::Season {
                duration: DAY,
                warmth: -20,
                rain: 0,
            }],
        },
    })
    .unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert_eq!(amount(&h, i, "bean-seed"), 8);
    assert!(h.get::<Gardens>().unwrap().plots.is_empty());
}

#[test]
fn sufficient_local_crop_stock_pauses_resowing() {
    let (mut h, p, i, _) = setup();
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("foresight".into(), 8000)].into(),
    })
    .unwrap();
    crops(&mut h);
    deposit(&mut h, i, "bean-seed", 16);
    h.dispatch(Plant {
        person: p,
        crop: "beans".into(),
        inventory: i,
    })
    .unwrap();
    run(&mut h, 2 * MINUTE.0);
    let plot = h.get::<Gardens>().unwrap().plots[&p];
    h.dispatch(game_agriculture::Harvest {
        person: p,
        plot,
        destination: i,
    })
    .unwrap();
    run(&mut h, 3 * MINUTE.0);
    h.dispatch(Enable).unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert_eq!(amount(&h, i, "bean-seed"), 8);
    assert_eq!(amount(&h, i, "beans"), 20);
}
#[test]
fn a_failed_crop_can_be_cleared_with_labor_instead_of_trapping_the_gardener() {
    let (mut h, p, i, area) = setup();
    crops(&mut h);
    h.dispatch(game_environment::ConfigureArea {
        area,
        climate: game_environment::Climate {
            seasons: vec![game_environment::Season {
                duration: DAY,
                warmth: -100,
                rain: 0,
            }],
        },
    })
    .unwrap();
    deposit(&mut h, i, "bean-seed", 8);
    let plot = h
        .dispatch(Plant {
            person: p,
            crop: "beans".into(),
            inventory: i,
        })
        .unwrap();
    run(&mut h, 2 * MINUTE.0);
    assert_eq!(
        h.get::<game_agriculture::Farms>().unwrap().plots[&plot].phase,
        game_agriculture::Phase::Ripe { yield_units: 0 }
    );
    h.dispatch(Enable).unwrap();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    run(&mut h, 3 * MINUTE.0);
    assert_eq!(
        h.get::<game_agriculture::Farms>().unwrap().plots[&plot].phase,
        game_agriculture::Phase::Fallow
    );
}
