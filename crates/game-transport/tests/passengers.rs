mod support;
use game_core::*;
use game_sim::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Stop(PersonId);
impl Request for Stop {
    const KEY: &'static str = "test.stop-carrier";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = support::builder();
    game_signals::plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.request::<Stop>(|tx, r| tx.cancel_action(r.0)).unwrap();
    b.subscribe::<game_transport::Departed>("test.carry-passenger", |tx, e| {
        let p: Vec<_> = tx
            .get::<game_physiology::Bodies>()?
            .people
            .keys()
            .copied()
            .collect();
        if p.first() == Some(&e.person) {
            tx.call(game_transport::Carry {
                carrier: e.person,
                passenger: p[1],
                destination: e.destination,
            })?;
        }
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn a_passenger_can_rest_and_signal_and_cancellation_preserves_their_own_action() {
    let mut h = builder().build(3, 1).unwrap();
    let home = h
        .dispatch(game_land::CreateArea {
            name: "Home".into(),
        })
        .unwrap();
    let away = h
        .dispatch(game_land::CreateArea {
            name: "Away".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: home,
        to: away,
        duration: SimDuration(2 * MINUTE.0),
    })
    .unwrap();
    let mut p = vec![];
    for _ in 0..2 {
        let person = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person, area: home })
            .unwrap();
        p.push(person);
    }
    h.dispatch(game_transport::Move {
        person: p[0],
        destination: away,
    })
    .unwrap();
    assert!(!h.get::<Actions>().unwrap().0.contains_key(&p[1]));
    h.dispatch(game_physiology::Rest { person: p[1] }).unwrap();
    h.dispatch(game_signals::Express { person: p[1] }).unwrap();
    assert!(
        h.get::<game_signals::Signals>()
            .unwrap()
            .expressed
            .contains_key(&p[1])
    );
    support::run(&mut h, SimTime(MINUTE.0));
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    for host in [&mut h, &mut loaded] {
        host.dispatch(Stop(p[0])).unwrap();
        assert_eq!(host.get::<Actions>().unwrap().0[&p[1]].kind, "rest");
        assert_eq!(
            host.get::<game_transport::Positions>().unwrap().0[&p[0]],
            host.get::<game_transport::Positions>().unwrap().0[&p[1]]
        );
        assert!(matches!(
            host.get::<game_transport::Positions>().unwrap().0[&p[1]],
            game_transport::Location::Road { .. }
        ));
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
