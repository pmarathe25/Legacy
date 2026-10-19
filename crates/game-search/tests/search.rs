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
    game_knowledge::plugin(&mut b).unwrap();
    game_search::plugin(&mut b).unwrap();
    b
}
#[test]
fn searches_visit_only_remembered_roads_and_replay_without_revealing_remote_positions() {
    let mut h = builder().build(1, 1).unwrap();
    let a = h
        .dispatch(game_land::CreateArea { name: "A".into() })
        .unwrap();
    let b = h
        .dispatch(game_land::CreateArea { name: "B".into() })
        .unwrap();
    let hidden = h
        .dispatch(game_land::CreateArea {
            name: "Unknown".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: a,
        to: b,
        duration: MINUTE,
    })
    .unwrap();
    let mut people = vec![];
    for area in [a, hidden] {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        people.push(p);
    }
    h.dispatch(game_knowledge::ObserveSurroundings { person: people[0] })
        .unwrap();
    let r = game_search::Seek {
        person: people[0],
        other: people[1],
        last_known: a,
    };
    assert_eq!(h.dispatch(r.clone()).unwrap(), Some(b));
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        while host.advance(SimTime(MINUTE.0), 128).unwrap().stop == Stop::Budget {}
        assert_eq!(host.dispatch(r.clone()).unwrap(), None);
        assert_eq!(
            game_transport::area(host.get().unwrap(), people[0]).unwrap(),
            b
        );
        assert!(
            !host.get::<game_search::Searches>().unwrap().0[&people[0]][&people[1]]
                .visited
                .contains(&hidden)
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
