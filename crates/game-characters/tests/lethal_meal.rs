mod support;
use game_core::*;
use game_sim::*;
#[test]
fn a_lethal_meal_commits_death_without_resuming_a_removed_mind() {
    let mut b = support::builder();
    game_nutrition::plugin(&mut b).unwrap();
    let mut h = b.build(7, 1).unwrap();
    let mut objects = game_objects::standard().unwrap();
    objects
        .items
        .get_mut("grain")
        .unwrap()
        .food
        .as_mut()
        .unwrap()
        .effects
        .health = -200;
    h.dispatch(game_objects::Install(objects)).unwrap();
    h.dispatch(game_nutrition::Enable).unwrap();
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        tick: MINUTE,
        hunger_per_tick: 40,
        fatigue_per_tick: 0,
        ..Default::default()
    }))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
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
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 20,
    })
    .unwrap();
    h.dispatch(game_property::ClaimInventory {
        claimant: p,
        inventory: pack,
        recognized_by: [p].into(),
    })
    .unwrap();
    h.dispatch(game_property::Permit {
        issuer: p,
        person: p,
        inventory: pack,
        allow: true,
    })
    .unwrap();
    h.dispatch(game_characters::Register {
        person: p,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    h.dispatch(game_characters::Start).unwrap();
    while h.advance(SimTime(2 * MINUTE.0), 1).unwrap().stop != Stop::Target {}
    assert!(!h.get::<game_physiology::Bodies>().unwrap().people[&p].alive);
    assert!(
        !h.get::<game_characters::Minds>()
            .unwrap()
            .people
            .contains_key(&p)
    );
    assert!(!h.get::<Actions>().unwrap().0.contains_key(&p));
    let saved = h.save().unwrap();
    let mut b = support::builder();
    game_nutrition::plugin(&mut b).unwrap();
    assert_eq!(b.load(&saved, 4).unwrap().save().unwrap(), saved);
}
