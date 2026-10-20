#![allow(dead_code)]
use game_core::*;
use game_sim::*;
use game_syntax::*;
pub fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_communication::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b
}
pub fn setup_with(b: Builder, fixed: bool) -> (Host, Vec<PersonId>, AreaId) {
    let mut h = b.build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let mut catalog = game_traits::load(&mut game_content::Store::standard()).unwrap();
    if fixed {
        let keys: Vec<_> = catalog.definitions.keys().cloned().collect();
        for key in keys {
            catalog.definitions.get_mut(&key).unwrap().founder_spread = 0;
        }
        catalog.stamps.clear();
    }
    h.dispatch(game_traits::Install(catalog)).unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    h.advance(SimTime(30 * 365 * DAY.0), 1).unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Clearing".into(),
        })
        .unwrap();
    let mut people = vec![];
    for _ in 0..4 {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        h.dispatch(game_traits::Set {
            person: p,
            profile: [
                ("intelligence".into(), 6000),
                ("attention".into(), 4000),
                ("learning".into(), 4000),
            ]
            .into(),
        })
        .unwrap();
        people.push(p);
    }
    h.dispatch(game_communication::Enable).unwrap();
    (h, people, area)
}
pub fn scene(who: PersonId, area: AreaId) -> Scene {
    [
        ("@predicate".into(), "relation:at".into()),
        ("actor".into(), format!("person:{}", who.0)),
        ("place".into(), format!("place:{}", area.0)),
    ]
    .into()
}

pub fn setup() -> (Host, Vec<PersonId>, AreaId) {
    setup_with(builder(), false)
}
