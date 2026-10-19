use game_companionship::SeekCompany;
use game_core::*;
use game_sim::*;
use game_social_life::*;
use std::{collections::BTreeMap, sync::Arc};
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    game_ecology::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_testimony::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    game_companionship::plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.request::<CancelPerson>(|tx, r| tx.cancel_action(r.0))
        .unwrap();
    b
}
fn setup() -> (Host, AreaId, AreaId, Vec<PersonId>) {
    let mut h = builder().build(17, 1).unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    let home = h
        .dispatch(game_land::CreateArea {
            name: "Green".into(),
        })
        .unwrap();
    let away = h
        .dispatch(game_land::CreateArea {
            name: "Grove".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: home,
        to: away,
        duration: MINUTE,
    })
    .unwrap();
    let mut people = vec![];
    for name in ["Mira", "Oren", "Tala"] {
        let person = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person, area: home })
            .unwrap();
        h.dispatch(Register {
            person,
            name: name.into(),
        })
        .unwrap();
        people.push(person);
    }
    (h, home, away, people)
}
fn run(h: &mut Host, time: SimTime) {
    loop {
        let p = h.advance(time, 128).unwrap();
        if p.stop == Stop::Target {
            break;
        }
        assert_eq!(p.stop, Stop::Budget);
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CancelPerson(PersonId);
impl Request for CancelPerson {
    const KEY: &'static str = "test.cancel-person";
    type Response = ();
}
#[test]
fn completed_company_uses_both_actions_and_transmits_only_remembered_evidence() {
    let (mut h, _, _, p) = setup();
    let facts = [(
        Arc::from("orchard"),
        game_knowledge::Fact {
            value: Arc::new(serde_json::json!("fruit gathered yesterday").into()),
            source: p[0],
            observed: SimTime(0),
            revision: h.revision(),
            confidence: 100,
        },
    )]
    .into_iter()
    .collect();
    h.dispatch(game_knowledge::Initialize {
        person: p[0],
        facts,
    })
    .unwrap();
    h.dispatch(Talk {
        person: p[0],
        other: p[1],
    })
    .unwrap();
    assert_eq!(h.get::<Actions>().unwrap().0.len(), 2);
    assert!(game_knowledge::view(&h, p[1]).unwrap().is_empty());
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Talk {
            person: p[2],
            other: p[1]
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
    run(&mut h, SimTime(5 * MINUTE.0));
    assert!(h.get::<Actions>().unwrap().0.is_empty());
    let learned = game_knowledge::view(&h, p[1]).unwrap();
    let fact = learned.get("orchard").unwrap();
    assert_eq!(fact.source, p[0]);
    assert_eq!(fact.observed, SimTime(0));
    assert_eq!(fact.confidence, 90);
    assert!(game_knowledge::view(&h, p[2]).unwrap().is_empty());
    assert!(
        h.get::<game_relationships::Social>().unwrap().relationships[&p[1]][&p[0]].affection > 0
    );
}
#[test]
fn interruption_or_death_releases_both_people_without_crediting_an_encounter() {
    for (index, kill) in [(0, true), (1, true), (0, false), (1, false)] {
        let (mut h, _, _, p) = setup();
        h.dispatch(Talk {
            person: p[0],
            other: p[1],
        })
        .unwrap();
        let save = h.save().unwrap();
        if kill {
            h.dispatch(game_physiology::Damage {
                person: p[index],
                amount: 100,
            })
            .unwrap();
        } else {
            h.dispatch(CancelPerson(p[index])).unwrap();
        }
        assert!(h.get::<Actions>().unwrap().0.is_empty());
        assert!(h.get::<SocialLife>().unwrap().meetings.is_empty());
        run(&mut h, SimTime(10 * MINUTE.0));
        assert!(
            h.get::<SocialLife>()
                .unwrap()
                .people
                .values()
                .all(|p| p.encounters == 0)
        );
        let mut loaded = builder().load(&save, 2).unwrap();
        run(&mut loaded, SimTime(10 * MINUTE.0));
        assert_eq!(
            loaded.get::<SocialLife>().unwrap().people[&p[0]].encounters,
            1
        );
    }
}
#[test]
fn disposition_and_directed_grievances_change_when_and_with_whom_people_socialize() {
    let (mut h, _, _, p) = setup();
    h.dispatch(game_traits::Set {
        person: p[0],
        profile: BTreeMap::from([("sociability".into(), 1000)]),
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: p[2],
        profile: BTreeMap::from([("sociability".into(), 0)]),
    })
    .unwrap();
    run(&mut h, SimTime(45 * MINUTE.0));
    assert!(
        !h.dispatch(SeekCompany {
            person: p[2],
            protected: None
        })
        .unwrap()
    );
    h.dispatch(game_relationships::SetRelationship {
        observer: p[1],
        other: p[0],
        relationship: game_relationships::Relationship {
            grievance: 80,
            ..Default::default()
        },
    })
    .unwrap();
    assert!(
        h.dispatch(SeekCompany {
            person: p[0],
            protected: None
        })
        .unwrap()
    );
    assert_eq!(h.get::<SocialLife>().unwrap().meetings[&p[0]].other, p[2]);
    run(&mut h, SimTime(50 * MINUTE.0));
    assert_eq!(
        h.get::<SocialLife>().unwrap().people[&p[2]].recent,
        Some(p[0])
    );
}
#[test]
fn refusal_distance_and_failed_travel_preserve_state_and_local_visibility() {
    let (mut h, home, away, p) = setup();
    h.dispatch(game_relationships::SetRelationship {
        observer: p[1],
        other: p[0],
        relationship: game_relationships::Relationship {
            fear: 80,
            ..Default::default()
        },
    })
    .unwrap();
    let save = h.save().unwrap();
    assert!(
        h.dispatch(Talk {
            person: p[0],
            other: p[1]
        })
        .is_err()
    );
    assert_eq!(save, h.save().unwrap());
    h.dispatch(game_transport::Move {
        person: p[2],
        destination: away,
    })
    .unwrap();
    assert_eq!(neighbors(&h, p[0]).unwrap().len(), 1);
    let save = h.save().unwrap();
    assert!(
        h.dispatch(game_transport::Move {
            person: p[0],
            destination: home
        })
        .is_err()
    );
    assert_eq!(save, h.save().unwrap());
    run(&mut h, SimTime(MINUTE.0));
    assert!(
        h.dispatch(Talk {
            person: p[0],
            other: p[2]
        })
        .is_err()
    );
    assert!(neighbors(&h, p[2]).unwrap().is_empty());
    builder().load(&h.save().unwrap(), 2).unwrap();
}
#[test]
fn a_saved_conversation_continues_identically_with_different_workers_and_advances() {
    let (mut h, _, _, p) = setup();
    h.dispatch(Talk {
        person: p[0],
        other: p[1],
    })
    .unwrap();
    run(&mut h, SimTime(MINUTE.0));
    let save = h.save().unwrap();
    let mut loaded = builder().load(&save, 4).unwrap();
    run(&mut h, SimTime(HOUR.0));
    for t in [70_001, 299_999, 300_000, HOUR.0] {
        run(&mut loaded, SimTime(t));
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    let mut corrupt: serde_json::Value = serde_json::from_str(&save).unwrap();
    corrupt["resources"]["social-life.people"]["residents"] = serde_json::json!({});
    assert!(builder().load(&corrupt.to_string(), 1).is_err());
}

#[test]
fn expressed_disagreements_have_asymmetric_and_learned_consequences() {
    let (mut h, _, _, p) = setup();
    for (person, sharing, authority, empathy) in [(p[0], 10, 10, 0), (p[1], -10, -10, 1000)] {
        h.dispatch(game_relationships::DefineBeliefs {
            person,
            beliefs: game_relationships::Beliefs {
                sharing,
                authority,
                honesty: 5,
            },
        })
        .unwrap();
        h.dispatch(game_traits::Set {
            person,
            profile: BTreeMap::from([("empathy".into(), empathy)]),
        })
        .unwrap();
    }
    for (speaker, listener, stance) in [(p[0], p[1], 10), (p[1], p[0], -10)] {
        for concept in ["sharing", "authority"] {
            h.dispatch(game_relationships::HearOpinion {
                listener,
                speaker,
                via: speaker,
                concept: concept.into(),
                stance,
                coerced: false,
            })
            .unwrap();
        }
    }
    h.dispatch(Talk {
        person: p[0],
        other: p[1],
    })
    .unwrap();
    run(&mut h, SimTime(5 * MINUTE.0));
    let social = h.get::<game_relationships::Social>().unwrap();
    assert!(social.relationships[&p[0]][&p[1]].affection < 0);
    assert!(social.relationships[&p[1]][&p[0]].affection > 0);
    assert!(social.values[&p[0]]["fellowship"] < 0);
    assert!(social.values[&p[1]]["fellowship"] > 0);
}
#[test]
fn bounded_search_rotates_past_unwilling_neighbors_and_protects_the_player() {
    let (mut h, home, _, p) = setup();
    // The first neighborhood contains only the protected player and hostile recipients.
    for person in [p[1], p[2]] {
        h.dispatch(game_relationships::SetRelationship {
            observer: person,
            other: p[0],
            relationship: game_relationships::Relationship {
                grievance: 100,
                ..Default::default()
            },
        })
        .unwrap();
    }
    let mut last = p[2];
    for i in 0..20 {
        let person = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person, area: home })
            .unwrap();
        h.dispatch(Register {
            person,
            name: format!("Neighbor {i}"),
        })
        .unwrap();
        if i < 19 {
            h.dispatch(game_relationships::SetRelationship {
                observer: person,
                other: p[0],
                relationship: game_relationships::Relationship {
                    grievance: 100,
                    ..Default::default()
                },
            })
            .unwrap();
        }
        last = person;
    }
    run(&mut h, SimTime(HOUR.0));
    assert!(
        !h.dispatch(SeekCompany {
            person: p[0],
            protected: Some(p[1])
        })
        .unwrap()
    );
    let cursor = h.get::<SocialLife>().unwrap().people[&p[0]].cursor;
    assert!(
        !h.dispatch(SeekCompany {
            person: p[0],
            protected: Some(p[1])
        })
        .unwrap()
    );
    // A declined search has a cooldown and does not move the scan cursor again.
    assert_eq!(h.get::<SocialLife>().unwrap().people[&p[0]].cursor, cursor);
    run(&mut h, SimTime(HOUR.0 + 10 * MINUTE.0));
    assert!(
        h.dispatch(SeekCompany {
            person: p[0],
            protected: Some(p[1])
        })
        .unwrap()
    );
    assert_eq!(h.get::<SocialLife>().unwrap().meetings[&p[0]].other, last);
}

#[test]
fn an_unexpressed_private_belief_cannot_change_another_persons_conversation_response() {
    let mut impressions = vec![];
    for hidden in [-100, 100] {
        let (mut h, _, _, p) = setup();
        h.dispatch(game_relationships::DefineBeliefs {
            person: p[0],
            beliefs: game_relationships::Beliefs {
                sharing: 10,
                authority: 10,
                honesty: 0,
            },
        })
        .unwrap();
        h.dispatch(game_relationships::DefineBeliefs {
            person: p[1],
            beliefs: game_relationships::Beliefs {
                sharing: hidden,
                authority: hidden,
                honesty: 0,
            },
        })
        .unwrap();
        h.dispatch(Talk {
            person: p[0],
            other: p[1],
        })
        .unwrap();
        run(&mut h, SimTime(5 * MINUTE.0));
        let social = h.get::<game_relationships::Social>().unwrap();
        impressions.push((
            social.relationships[&p[0]][&p[1]].clone(),
            social.values[&p[0]]["fellowship"],
        ));
    }
    assert_eq!(impressions[0], impressions[1]);
}
