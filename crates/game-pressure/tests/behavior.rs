use game_core::*;
use game_pressure::*;
use game_sim::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CancelPerson(PersonId);
impl Request for CancelPerson {
    const KEY: &'static str = "test.cancel-pressure";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_lineage::plugin(&mut b).unwrap();
    game_conflict::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.request::<CancelPerson>(|tx, r| tx.cancel_action(r.0))
        .unwrap();
    b
}
fn setup() -> (Host, Vec<PersonId>, AreaId) {
    let mut h = builder().build(9, 1).unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Grove".into(),
        })
        .unwrap();
    let away = h
        .dispatch(game_land::CreateArea {
            name: "Ridge".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: area,
        to: away,
        duration: MINUTE,
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
        h.dispatch(game_transport::Place { person, area }).unwrap();
        p.push(person);
    }
    (h, p, away)
}
fn run(h: &mut Host) {
    while h.advance(SimTime(10 * MINUTE.0), 128).unwrap().stop == Stop::Budget {}
}
#[test]
fn interrupted_or_fatal_pressure_releases_both_actions_and_never_fabricates_submission() {
    for (person, kill) in [(0, false), (1, false), (0, true), (1, true)] {
        let (mut h, p, _) = setup();
        h.dispatch(DemandSubmission {
            leader: p[0],
            subject: p[1],
        })
        .unwrap();
        if kill {
            h.dispatch(game_physiology::Damage {
                person: p[person],
                amount: 100,
            })
            .unwrap();
        } else {
            h.dispatch(CancelPerson(p[person])).unwrap();
        }
        assert!(h.get::<Pressure>().unwrap().active.is_empty());
        assert!(h.get::<Actions>().unwrap().0.is_empty());
        let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
        run(&mut h);
        run(&mut loaded);
        assert!(!h.notices().any(|n| n.kind == Submitted::KEY));
        assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    }
}
#[test]
fn a_mortal_can_die_resisting_without_a_forced_submission_or_damage_cap() {
    let (mut h, p, _) = setup();
    h.dispatch(game_physiology::Damage {
        person: p[1],
        amount: 80,
    })
    .unwrap();
    h.dispatch(DemandSubmission {
        leader: p[0],
        subject: p[1],
    })
    .unwrap();
    run(&mut h);
    assert!(!h.get::<game_physiology::Bodies>().unwrap().people[&p[1]].alive);
    assert!(h.get::<Pressure>().unwrap().active.is_empty());
    assert!(h.get::<Actions>().unwrap().0.is_empty());
    assert!(!h.notices().any(|n| n.kind == Submitted::KEY));
}
#[test]
fn a_distant_or_player_controlled_person_requires_a_different_response_path() {
    let (mut h, p, away) = setup();
    h.dispatch(game_transport::Move {
        person: p[1],
        destination: away,
    })
    .unwrap();
    run(&mut h);
    let save = h.save().unwrap();
    assert!(
        h.dispatch(DemandSubmission {
            leader: p[0],
            subject: p[1]
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), save);
    let (mut h, p, _) = setup();
    h.dispatch(game_lineage::Begin { person: p[1] }).unwrap();
    let save = h.save().unwrap();
    assert!(
        h.dispatch(DemandSubmission {
            leader: p[0],
            subject: p[1]
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), save);
}
