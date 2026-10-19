use game_core::*;
use game_fauna::*;
use game_sim::*;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_environment::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    game_ecology::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b
}
#[test]
fn animals_eat_finite_plants_and_pass_ancestry_through_reproduction_across_workers() {
    let mut h = builder().build(7, 1).unwrap();
    h.advance(SimTime(100 * DAY.0), 10).unwrap();
    let objects = game_objects::standard().unwrap();
    let meat = objects.resolve("raw-meat").unwrap();
    h.dispatch(game_objects::Install(objects)).unwrap();
    h.dispatch(game_environment::Configure(game_environment::Climate {
        seasons: vec![game_environment::Season {
            duration: DAY,
            warmth: 20,
            rain: 10,
        }],
    }))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Meadow".into(),
        })
        .unwrap();
    let patch = h
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area,
            good: game_cargo::FOOD,
            stock: 100_000,
            capacity: 100_000,
            renew: 0,
        }))
        .unwrap();
    h.dispatch(Install(
        [(
            "test-rabbit".into(),
            Species {
                name: "Rabbit".into(),
                body_mass: 500,
                speed: 1000,
                maturity_days: 2,
                lifespan_days: 365,
                gestation_days: 2,
                intake: 1,
                minimum_warmth: 0,
                maximum_warmth: 30,
                meat,
                yield_units: 3,
            },
        )]
        .into(),
    ))
    .unwrap();
    h.dispatch(Populate {
        area,
        species: "test-rabbit".into(),
        quantity: 20,
    })
    .unwrap();
    h.dispatch(Start).unwrap();
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    let target = SimTime(110 * DAY.0);
    for host in [&mut h, &mut loaded] {
        while host.advance(target, 32).unwrap().stop == Stop::Budget {}
        let fauna = host.get::<Fauna>().unwrap();
        assert!(fauna.births > 0);
        assert!(
            fauna
                .animals
                .values()
                .any(|a| a.generation > 0 && a.parents.iter().all(Option::is_some))
        );
        assert!(host.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock < 100_000);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    let mut invalid: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    let rows = invalid["resources"]["fauna.animals"]["animals"]
        .as_object_mut()
        .unwrap();
    rows.values_mut().next().unwrap()["species"] = serde_json::json!("unresolved");
    assert!(builder().load(&invalid.to_string(), 1).is_err());
}
