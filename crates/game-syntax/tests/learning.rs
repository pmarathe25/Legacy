mod support;
use game_core::*;
use game_syntax::*;
use support::*;
#[test]
fn actual_contact_teaches_constructions_without_copying_a_speakers_private_gloss() {
    let (mut h, p, area) = setup();
    assert!(h.get::<Languages>().unwrap().people.is_empty());
    for n in 0..12 {
        h.dispatch(Describe {
            speaker: p[0],
            listener: p[1],
            scene: scene(p[n % 2], area),
            demonstrate: true,
        })
        .unwrap();
        assert_eq!(
            h.dispatch(Read { reader: p[1] })
                .unwrap()
                .last()
                .unwrap()
                .interpretation,
            Some(scene(p[n % 2], area))
        );
    }
    assert!(h.get::<Languages>().unwrap().people[&p[1]].productive_rules() > 0);
    assert!(!h.get::<Languages>().unwrap().people.contains_key(&p[2]));
    assert!(
        !h.dispatch(Describe {
            speaker: p[1],
            listener: p[2],
            scene: scene(p[0], area),
            demonstrate: false
        })
        .unwrap()
    );
    assert!(
        h.dispatch(Read { reader: p[2] })
            .unwrap()
            .last()
            .unwrap()
            .interpretation
            .is_none()
    );
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        for n in 0..12 {
            host.dispatch(Describe {
                speaker: p[1],
                listener: p[2],
                scene: scene(p[n % 2], area),
                demonstrate: true,
            })
            .unwrap();
        }
        assert!(host.get::<Languages>().unwrap().people[&p[2]].productive_rules() > 0);
        assert!(
            host.dispatch(Read { reader: p[2] })
                .unwrap()
                .iter()
                .any(|r| r.interpretation.is_some())
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn incapacity_and_unobserved_claims_do_not_grant_a_grammar() {
    let (mut h, p, area) = setup();
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("intelligence".into(), 0)].into(),
    })
    .unwrap();
    for n in 0..8 {
        assert!(
            !h.dispatch(Describe {
                speaker: p[0],
                listener: p[1],
                scene: scene(p[n % 2], area),
                demonstrate: true
            })
            .unwrap()
        );
    }
    assert!(h.get::<Languages>().unwrap().people[&p[1]].rules.is_empty());
    let saved = h.save().unwrap();
    assert!(
        h.dispatch(Describe {
            speaker: p[0],
            listener: p[2],
            scene: scene(PersonId(999999), area),
            demonstrate: true
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), saved);
    h.dispatch(game_traits::Set {
        person: p[2],
        profile: [("hearing".into(), 0)].into(),
    })
    .unwrap();
    assert!(
        !h.dispatch(Describe {
            speaker: p[0],
            listener: p[2],
            scene: scene(p[0], area),
            demonstrate: true
        })
        .unwrap()
    );
    assert!(!h.get::<Languages>().unwrap().people.contains_key(&p[2]));
}

#[test]
fn an_observation_owner_can_record_a_new_relation_without_teaching_bystanders_or_incapable_people()
{
    let (mut h, p, _) = setup();
    let memory = Scene::from([
        ("@predicate".into(), "relation:cooling".into()),
        ("object".into(), "good:1".into()),
        ("outcome".into(), "preserved".into()),
    ]);
    assert!(
        h.dispatch(Witness {
            observer: p[0],
            scene: memory.clone()
        })
        .unwrap()
    );
    assert!(h.get::<Languages>().unwrap().observed[&p[0]].contains(&memory));
    assert!(!h.get::<Languages>().unwrap().observed.contains_key(&p[1]));
    assert!(h.get::<Languages>().unwrap().people.is_empty());
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("intelligence".into(), 0)].into(),
    })
    .unwrap();
    assert!(
        !h.dispatch(Witness {
            observer: p[1],
            scene: memory
        })
        .unwrap()
    );
    assert!(!h.get::<Languages>().unwrap().observed.contains_key(&p[1]));
}
