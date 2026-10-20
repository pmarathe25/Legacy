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
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_testimony::plugin(&mut b).unwrap();
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
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
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
    h.dispatch(game_knowledge::ObserveSurroundings { person: p[1] })
        .unwrap();
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("risk-tolerance".into(), 1000), ("aggression".into(), 1000)].into(),
    })
    .unwrap();
    h.dispatch(game_relationships::Initialize {
        person: p[1],
        beliefs: Default::default(),
        values: [("independence".into(), 100)].into(),
    })
    .unwrap();
    h.dispatch(game_relationships::SetRelationship {
        observer: p[1],
        other: p[0],
        relationship: game_relationships::Relationship {
            grievance: 100,
            ..Default::default()
        },
    })
    .unwrap();
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

fn independent(h: &mut Host, person: PersonId) {
    h.dispatch(game_traits::Set {
        person,
        profile: [("risk-tolerance".into(), 500), ("aggression".into(), 0)].into(),
    })
    .unwrap();
    h.dispatch(game_relationships::Initialize {
        person,
        beliefs: Default::default(),
        values: [("independence".into(), 100)].into(),
    })
    .unwrap();
}

#[test]
fn an_independent_character_can_escape_after_injury_using_a_known_road() {
    let (mut h, p, away) = setup();
    independent(&mut h, p[1]);
    h.dispatch(game_knowledge::ObserveSurroundings { person: p[1] })
        .unwrap();
    h.dispatch(DemandSubmission {
        leader: p[0],
        subject: p[1],
    })
    .unwrap();
    assert_eq!(h.get::<Actions>().unwrap().0[&p[1]].kind, "combat");
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        while host.advance(SimTime(MINUTE.0), 128).unwrap().stop == Stop::Budget {}
        assert!(host.get::<Pressure>().unwrap().active.is_empty());
        assert!(
            matches!(host.get::<game_transport::Positions>().unwrap().0[&p[1]], game_transport::Location::Transit { to, .. } if to == away)
        );
        assert!(host.get::<game_physiology::Bodies>().unwrap().people[&p[1]].health < 100);
        assert!(!host.notices().any(|n| n.kind == Submitted::KEY));
        assert!(host.notices().any(|n| n.kind == Fled::KEY));
        run(host);
        assert_eq!(
            game_transport::area(host.get().unwrap(), p[1]).unwrap(),
            away
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn a_vulnerable_person_can_flee_before_being_struck_but_cannot_use_an_unknown_or_closed_road() {
    for road in ["known", "unknown", "closed"] {
        let (mut h, p, away) = setup();
        independent(&mut h, p[1]);
        h.dispatch(game_traits::Set {
            person: p[0],
            profile: [("size".into(), 3000)].into(),
        })
        .unwrap();
        h.dispatch(game_traits::Set {
            person: p[1],
            profile: [
                ("size".into(), 300),
                ("foresight".into(), 4000),
                ("aggression".into(), 0),
            ]
            .into(),
        })
        .unwrap();
        if road != "unknown" {
            h.dispatch(game_knowledge::ObserveSurroundings { person: p[1] })
                .unwrap();
        }
        if road == "closed" {
            let from = game_transport::area(h.get().unwrap(), p[1]).unwrap();
            h.dispatch(game_land::CloseRoad { from, to: away }).unwrap();
        }
        h.dispatch(DemandSubmission {
            leader: p[0],
            subject: p[1],
        })
        .unwrap();
        assert!(!h.notices().any(|n| n.kind == Submitted::KEY));
        assert_eq!(
            h.get::<game_physiology::Bodies>().unwrap().people[&p[1]].health,
            100
        );
        if road == "known" {
            assert_eq!(h.get::<Actions>().unwrap().0[&p[1]].kind, "travel");
            assert!(h.notices().any(|n| n.kind == Fled::KEY));
        } else {
            assert_eq!(h.get::<Actions>().unwrap().0[&p[1]].kind, "combat");
            assert_eq!(h.get::<Pressure>().unwrap().active.len(), 2);
            assert!(!h.notices().any(|n| n.kind == Fled::KEY));
        }
        h.save().unwrap();
    }
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

#[test]
fn anticipated_visible_force_can_prompt_submission_before_an_injury() {
    let (mut h, p, _) = setup();
    h.dispatch(game_traits::Set {
        person: p[0],
        profile: [("size".into(), 3000)].into(),
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("size".into(), 300), ("foresight".into(), 4000)].into(),
    })
    .unwrap();
    h.dispatch(DemandSubmission {
        leader: p[0],
        subject: p[1],
    })
    .unwrap();
    assert!(h.get::<Pressure>().unwrap().active.is_empty());
    assert_eq!(
        h.get::<game_physiology::Bodies>().unwrap().people[&p[1]].health,
        100
    );
    assert!(h.notices().any(|n| n.kind == Submitted::KEY));
    assert!(!h.notices().any(|n| n.kind == "conflict.struck"));
}
#[test]
fn a_refused_demand_does_not_force_a_gentle_leader_to_attack() {
    let (mut h, p, _) = setup();
    h.dispatch(game_traits::Set {
        person: p[0],
        profile: [
            ("aggression".into(), 0),
            ("ambition".into(), 0),
            ("patience".into(), 1000),
        ]
        .into(),
    })
    .unwrap();
    h.dispatch(DemandSubmission {
        leader: p[0],
        subject: p[1],
    })
    .unwrap();
    assert!(h.get::<Pressure>().unwrap().active.is_empty());
    assert!(h.notices().any(|n| n.kind == BackedDown::KEY));
    assert!(!h.notices().any(|n| n.kind == Submitted::KEY));
    let save = h.save().unwrap();
    assert_eq!(builder().load(&save, 4).unwrap().save().unwrap(), save);
}

#[test]
fn expected_friendship_can_favor_seeking_help_and_the_friend_decides_whether_to_join() {
    let (mut h, p, _) = setup();
    let area = game_transport::area(h.get().unwrap(), p[1]).unwrap();
    let friend = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place {
        person: friend,
        area,
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: p[0],
        profile: [
            ("aggression".into(), 1000),
            ("ambition".into(), 1000),
            ("patience".into(), 0),
        ]
        .into(),
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("risk-tolerance".into(), 1000)].into(),
    })
    .unwrap();
    h.dispatch(game_relationships::SetRelationship {
        observer: p[1],
        other: p[0],
        relationship: game_relationships::Relationship {
            fear: 100,
            ..Default::default()
        },
    })
    .unwrap();
    for (observer, other) in [(p[1], friend), (friend, p[1])] {
        h.dispatch(game_relationships::SetRelationship {
            observer,
            other,
            relationship: game_relationships::Relationship {
                trust: 100,
                affection: 100,
                ..Default::default()
            },
        })
        .unwrap();
    }
    assert!(
        h.dispatch(Aid {
            person: friend,
            subject: p[1],
            aggressor: p[0]
        })
        .unwrap()
    );
    h.dispatch(DemandSubmission {
        leader: p[0],
        subject: p[1],
    })
    .unwrap();
    let battle = h.get::<Pressure>().unwrap().active[&p[1]].battle;
    assert_eq!(
        h.get::<game_conflict::Battles>().unwrap().0[&battle]
            .fighters
            .len(),
        3
    );
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    for host in [&mut h, &mut loaded] {
        host.dispatch(CancelPerson(p[1])).unwrap();
        run(host);
        assert!(host.get::<Actions>().unwrap().0.is_empty());
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn a_newborn_with_exceptional_genes_cannot_threaten_or_enlist_as_a_fighter() {
    let (mut h, p, _) = setup();
    h.dispatch(game_vitality::Register {
        person: p[0],
        born: h.now(),
        span: Some(SimDuration(80 * 365 * DAY.0)),
        base_health: 100,
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: p[0],
        profile: [
            ("agility".into(), 16000),
            ("intelligence".into(), 16000),
            ("strength".into(), 16000),
        ]
        .into(),
    })
    .unwrap();
    let saved = h.save().unwrap();
    assert!(
        h.dispatch(DemandSubmission {
            leader: p[0],
            subject: p[1]
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), saved);
    let third = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: h.now(),
        })
        .unwrap();
    let area = game_transport::area(h.get().unwrap(), p[0]).unwrap();
    h.dispatch(game_transport::Place {
        person: third,
        area,
    })
    .unwrap();
    assert!(
        !h.dispatch(Aid {
            person: p[0],
            subject: p[1],
            aggressor: third
        })
        .unwrap()
    );
}

#[test]
fn learned_peace_or_violence_can_outweigh_the_same_inherited_aggression() {
    for (aggression, peace, violence, fights) in
        [(1000, 100, 0, false), (1000, 0, 0, true), (0, 0, 100, true)]
    {
        let (mut h, p, _) = setup();
        h.dispatch(game_traits::Set {
            person: p[0],
            profile: [
                ("aggression".into(), aggression),
                ("ambition".into(), 1000),
                ("patience".into(), 0),
            ]
            .into(),
        })
        .unwrap();
        h.dispatch(game_traits::Set {
            person: p[1],
            profile: [("risk-tolerance".into(), 1000), ("aggression".into(), 1000)].into(),
        })
        .unwrap();
        h.dispatch(game_relationships::Initialize {
            person: p[0],
            beliefs: Default::default(),
            values: [("peace".into(), peace), ("violence".into(), violence)].into(),
        })
        .unwrap();
        h.dispatch(DemandSubmission {
            leader: p[0],
            subject: p[1],
        })
        .unwrap();
        assert_eq!(!h.get::<Pressure>().unwrap().active.is_empty(), fights);
    }
}
