use game_core::*;
use game_sim::*;
pub fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_choice::plugin(&mut b).unwrap();
    game_communication::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_testimony::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    b
}
pub fn people(b: Builder) -> (Host, Vec<PersonId>, InventoryId) {
    let mut h = b.build(9, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    let a = h
        .dispatch(game_land::CreateArea {
            name: "Workshop".into(),
        })
        .unwrap();
    let mut people = vec![];
    for intelligence in [4000, 200, 4000] {
        let person = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person, area: a })
            .unwrap();
        h.dispatch(game_traits::Set {
            person,
            profile: [("intelligence".into(), intelligence)].into(),
        })
        .unwrap();
        people.push(person);
    }
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(people[0]),
            capacity: 1000,
        })
        .unwrap();
    (h, people, inventory)
}
pub fn run(h: &mut Host, at: SimTime) {
    for _ in 0..10000 {
        if h.advance(at, 100).unwrap().stop == Stop::Target {
            return;
        }
    }
    panic!("simulation failed to advance");
}
