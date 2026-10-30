use super::support;
use game_core::*;
#[test]
fn partial_harvest_keeps_remaining_yield_and_cultivar_provenance_after_reload() {
    let mut b = support::builder();
    game_cultivars::plugin(&mut b).unwrap();
    let mut h = b.build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "garden".into(),
        })
        .unwrap();
    h.dispatch(game_environment::Configure(game_environment::Climate {
        seasons: vec![game_environment::Season {
            duration: DAY,
            warmth: 20,
            rain: 4,
        }],
    }))
    .unwrap();
    let c = game_agriculture::Crop {
        name: "test grain".into(),
        seed: game_cargo::SEED,
        output: game_cargo::FOOD,
        seed_input: 1,
        yield_units: 100,
        growth: HOUR,
        natural_growth: Some(HOUR),
        sow_labor: HOUR,
        harvest_labor: HOUR,
        minimum_warmth: -10,
        minimum_rain: 0,
        ..Default::default()
    };
    h.dispatch(game_agriculture::InstallCrops(game_agriculture::Crops {
        items: [("grain".into(), c)].into(),
        ..Default::default()
    }))
    .unwrap();
    h.dispatch(game_agriculture::EnableNatural).unwrap();
    let p = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person: p, area })
        .unwrap();
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::SEED,
        quantity: 1,
    })
    .unwrap();
    let plot = h
        .dispatch(game_agriculture::Create(game_agriculture::Plot {
            area,
            steward: p,
            crop: "grain".into(),
            ..Default::default()
        }))
        .unwrap();
    h.dispatch(game_agriculture::Sow {
        person: p,
        plot,
        seed: pack,
    })
    .unwrap();
    support::run(&mut h, SimTime(5 * HOUR.0));
    let game_agriculture::Phase::Ripe { yield_units: total } =
        h.get::<game_agriculture::Farms>().unwrap().plots[&plot].phase
    else {
        panic!("not ripe")
    };
    assert!(total > 10);
    let done = h
        .dispatch(game_agriculture::HarvestSome {
            person: p,
            plot,
            destination: pack,
            quantity: 10,
        })
        .unwrap();
    let saved = h.save().unwrap();
    let mut b = support::builder();
    game_cultivars::plugin(&mut b).unwrap();
    let mut loaded = b.load(&saved, 4).unwrap();
    support::run(&mut h, done);
    support::run(&mut loaded, done);
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    assert_eq!(
        h.get::<game_agriculture::Farms>().unwrap().plots[&plot].phase,
        game_agriculture::Phase::Ripe {
            yield_units: total - 10
        }
    );
    let lots = &h.get::<game_cultivars::Cultivars>().unwrap().stocks[&pack][&game_cargo::FOOD];
    assert_eq!(lots[0].quantity, 10);
    assert_eq!(lots[0].strain.generation, 1);
}
#[test]
fn clearing_a_failed_natural_crop_does_not_try_to_create_zero_seed_provenance() {
    let mut b = support::builder();
    game_cultivars::plugin(&mut b).unwrap();
    let mut h = b.build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "frost".into(),
        })
        .unwrap();
    h.dispatch(game_environment::Configure(game_environment::Climate {
        seasons: vec![game_environment::Season {
            duration: DAY,
            warmth: -100,
            rain: 0,
        }],
    }))
    .unwrap();
    let c = game_agriculture::Crop {
        name: "failed grain".into(),
        seed: game_cargo::SEED,
        output: game_cargo::FOOD,
        seed_input: 1,
        yield_units: 100,
        growth: SimDuration(3 * DAY.0),
        sow_labor: HOUR,
        harvest_labor: HOUR,
        minimum_warmth: 0,
        ..Default::default()
    };
    h.dispatch(game_agriculture::InstallCrops(game_agriculture::Crops {
        items: [("grain".into(), c)].into(),
        ..Default::default()
    }))
    .unwrap();
    h.dispatch(game_agriculture::EnableNatural).unwrap();
    let p = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person: p, area })
        .unwrap();
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p),
            capacity: 100,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::SEED,
        quantity: 1,
    })
    .unwrap();
    let plot = h
        .dispatch(game_agriculture::Create(game_agriculture::Plot {
            area,
            steward: p,
            crop: "grain".into(),
            ..Default::default()
        }))
        .unwrap();
    h.dispatch(game_agriculture::Sow {
        person: p,
        plot,
        seed: inventory,
    })
    .unwrap();
    support::run(&mut h, SimTime(10 * DAY.0));
    assert_eq!(
        h.get::<game_agriculture::Farms>().unwrap().plots[&plot].phase,
        game_agriculture::Phase::Ripe { yield_units: 0 }
    );
    let before = h.get::<game_cultivars::Cultivars>().unwrap().sequence;
    let end = h
        .dispatch(game_agriculture::Harvest {
            person: p,
            plot,
            destination: inventory,
        })
        .unwrap();
    support::run(&mut h, end);
    assert_eq!(
        h.get::<game_agriculture::Farms>().unwrap().plots[&plot].phase,
        game_agriculture::Phase::Fallow
    );
    assert_eq!(
        h.get::<game_cultivars::Cultivars>().unwrap().sequence,
        before
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::FOOD),
        0
    );
    h.save().unwrap();
}
#[test]
fn an_untried_seed_lot_can_be_preferred_to_a_personally_observed_failure() {
    let mut b = support::builder();
    game_cultivars::plugin(&mut b).unwrap();
    let mut h = b.build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "garden".into(),
        })
        .unwrap();
    let p = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person: p, area })
        .unwrap();
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p),
            capacity: 100,
        })
        .unwrap();
    h.dispatch(game_cultivars::Define {
        seed: game_cargo::SEED,
        produce: game_cargo::FOOD,
    })
    .unwrap();
    h.dispatch(game_cultivars::Enable).unwrap();
    for _ in 0..2 {
        h.dispatch(game_cargo::Deposit {
            inventory,
            good: game_cargo::SEED,
            quantity: 1,
        })
        .unwrap();
    }
    let ids: Vec<_> = h.get::<game_cultivars::Cultivars>().unwrap().stocks[&inventory]
        [&game_cargo::SEED]
        .iter()
        .map(|l| l.strain.id)
        .collect();
    h.dispatch(game_cultivars::Observe {
        person: p,
        strain: ids[0],
        yield_units: 0,
        duration: DAY,
    })
    .unwrap();
    assert_eq!(
        h.dispatch(game_cultivars::Select {
            person: p,
            inventory,
            good: game_cargo::SEED
        })
        .unwrap()
        .unwrap()
        .id,
        ids[1]
    );
}
