use game_core::*;
use game_sim::*;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_environment::plugin(&mut b).unwrap();
    game_cultivars::plugin(&mut b).unwrap();
    game_crops::plugin(&mut b).unwrap();
    game_ecology::plugin(&mut b).unwrap();
    game_botany::plugin(&mut b).unwrap();
    b
}
fn run(h: &mut Host, at: SimTime) {
    while h.advance(at, 1000).unwrap().stop != Stop::Target {}
}
#[test]
fn wild_seed_banks_reproduce_and_select_cold_tolerance_across_generations() {
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "cold garden".into(),
        })
        .unwrap();
    h.dispatch(game_environment::Configure(game_environment::Climate {
        seasons: vec![game_environment::Season {
            duration: DAY,
            warmth: -4,
            rain: 4,
        }],
    }))
    .unwrap();
    // Short-lived experimental plants make many generations feasible in a bounded test.
    let c = game_crops::Crop {
        name: "test potato".into(),
        seed: GoodId(800),
        output: GoodId(500),
        seed_input: 1,
        yield_units: 100,
        growth: SimDuration(2 * DAY.0),
        sow_labor: HOUR,
        harvest_labor: HOUR,
        minimum_warmth: 0,
        minimum_rain: 1,
        maximum_warmth: Some(30),
        maximum_rain: Some(10),
        lifespan: Some(SimDuration(20 * DAY.0)),
        ..Default::default()
    };
    h.dispatch(game_crops::InstallCrops(game_crops::Crops {
        items: [("potato".into(), c)].into(),
        ..Default::default()
    }))
    .unwrap();
    h.dispatch(game_crops::Natural).unwrap();
    let patch = h
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area,
            capacity: 10000,
            good: GoodId(500),
            renew: 100,
            stock: 100,
        }))
        .unwrap();
    h.dispatch(game_ecology::SetRenewal {
        patch,
        minimum_warmth: 0,
        minimum_rain: 1,
    })
    .unwrap();
    h.dispatch(game_botany::Enable).unwrap();
    let initial: f64 = h.get::<game_botany::Botany>().unwrap().stands[&patch]
        .plants
        .iter()
        .map(|p| f64::from(p.expression.cold))
        .sum::<f64>()
        / 32.;
    h.dispatch(game_ecology::StartRenewal).unwrap();
    run(&mut h, SimTime(365 * DAY.0));
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    run(&mut h, SimTime(3 * 365 * DAY.0));
    run(&mut loaded, SimTime(3 * 365 * DAY.0));
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    let b = h.get::<game_botany::Botany>().unwrap();
    let s = &b.stands[&patch];
    assert!(!s.plants.is_empty());
    assert_eq!(
        s.plants
            .iter()
            .map(|p| p.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        s.plants.len()
    );
    assert!(b.births > 100);
    assert!(b.deaths > 100);
    assert!(s.plants.iter().map(|p| p.strain.generation).max().unwrap() >= 5);
    let mean = s
        .plants
        .iter()
        .map(|p| f64::from(p.expression.cold))
        .sum::<f64>()
        / s.plants.len() as f64;
    assert!(mean > initial, "initial {initial}, final {mean}");
    assert_eq!(
        s.fruit.values().map(|l| l.quantity).sum::<u64>(),
        h.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock
    );
}
