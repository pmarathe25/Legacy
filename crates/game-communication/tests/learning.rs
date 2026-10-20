use game_communication::Notice;
use game_communication::*;
use game_core::*;
use game_sim::*;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b
}
fn setup() -> (Host, Vec<PersonId>, AreaId) {
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
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
        people.push(p);
    }
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(people[0]),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(Enable).unwrap();
    (h, people, area)
}
fn convey(
    h: &mut Host,
    a: PersonId,
    b: PersonId,
    key: &str,
    demonstrate: bool,
    explanation: Vec<String>,
) -> Reception {
    h.dispatch(Convey {
        speaker: a,
        listener: b,
        concept: key.into(),
        demonstrate,
        explanation,
    })
    .unwrap()
}
fn set(h: &mut Host, p: PersonId, traits: &[(&str, i32)]) {
    h.dispatch(game_traits::Set {
        person: p,
        profile: traits.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
    })
    .unwrap();
}
#[test]
fn demonstrated_sounds_proliferate_through_people_without_a_universal_dictionary() {
    let (mut h, p, _) = setup();
    let invented = h
        .dispatch(WordFor {
            person: p[0],
            concept: "good:1".into(),
        })
        .unwrap();
    let foreign = h
        .dispatch(WordFor {
            person: p[3],
            concept: "good:1".into(),
        })
        .unwrap();
    assert_ne!(invented, foreign);
    assert!(
        convey(&mut h, p[0], p[1], "good:1", false, vec![])
            .interpretation
            .is_none()
    );
    assert_eq!(
        convey(&mut h, p[0], p[1], "good:1", true, vec![])
            .interpretation
            .as_deref(),
        Some("good:1")
    );
    assert_eq!(
        convey(&mut h, p[1], p[2], "good:1", false, vec![]).interpretation,
        None
    );
    // The second learner needs grounding too; their speaker shows a visible person instead.
    let person = format!("person:{}", p[0].0);
    convey(&mut h, p[0], p[1], &person, true, vec![]);
    let learned = convey(&mut h, p[1], p[2], &person, true, vec![]);
    assert_eq!(learned.interpretation, Some(person.clone()));
    assert_eq!(
        decode(h.get().unwrap(), p[2], &learned.sounds).meaning,
        Some(person)
    );
    assert!(
        decode(h.get().unwrap(), p[3], &learned.sounds)
            .meaning
            .is_none()
    );
    let used: std::collections::BTreeSet<_> = h.get::<Communication>().unwrap().lexicons[&p[2]]
        .values()
        .flatten()
        .flat_map(|w| w.sounds.iter())
        .collect();
    assert!(used.len() < PHONEMES.len());
    let save = h.save().unwrap();
    let loaded = builder().load(&save, 4).unwrap();
    assert_eq!(loaded.save().unwrap(), save);
}
#[test]
fn explanations_require_shared_words_and_deaf_people_can_understand_demonstrations() {
    let (mut h, p, _) = setup();
    let person = format!("person:{}", p[0].0);
    assert!(
        convey(&mut h, p[0], p[1], "idea:care", false, vec![])
            .interpretation
            .is_none()
    );
    for key in [&person, "good:1"] {
        convey(&mut h, p[0], p[1], key, true, vec![]);
    }
    assert_eq!(
        convey(
            &mut h,
            p[0],
            p[1],
            "idea:care",
            false,
            vec![person, "good:1".into()]
        )
        .interpretation
        .as_deref(),
        Some("idea:care")
    );
    set(&mut h, p[2], &[("hearing", 0)]);
    let shown = convey(&mut h, p[0], p[2], "good:1", true, vec![]);
    assert!(shown.demonstrated);
    assert_eq!(shown.interpretation.as_deref(), Some("good:1"));
    assert!(
        h.get::<Communication>()
            .unwrap()
            .lexicons
            .get(&p[2])
            .is_none()
    );
    set(&mut h, p[0], &[("vocalization", 0), ("gesturing", 0)]);
    assert!(
        convey(&mut h, p[0], p[3], "good:1", true, vec![])
            .interpretation
            .is_none()
    );
}
#[test]
fn infant_cues_and_anticipation_depend_on_personal_capabilities_and_local_experience() {
    let (mut h, p, area) = setup();
    let baby = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: h.now(),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person: baby, area })
        .unwrap();
    h.dispatch(game_physiology::Express { person: baby })
        .unwrap();
    set(&mut h, p[1], &[("interpretation", 0), ("intelligence", 0)]);
    h.dispatch(game_physiology::Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    let strong = h
        .dispatch(Notice {
            observer: p[0],
            person: baby,
        })
        .unwrap();
    let weak = h
        .dispatch(Notice {
            observer: p[1],
            person: baby,
        })
        .unwrap();
    assert_eq!(strong.cue, "crying");
    assert_eq!(strong.guessed_need.as_deref(), Some("food"));
    assert!(weak.guessed_need.is_none());
    set(&mut h, baby, &[("vocalization", 0), ("gesturing", 0)]);
    let inventory = h
        .get::<game_cargo::Cargo>()
        .unwrap()
        .carried(p[0])
        .next()
        .unwrap();
    h.dispatch(game_physiology::Serve {
        caregiver: p[0],
        patient: baby,
        inventory,
        good: game_cargo::FOOD,
        quantity: 1,
    })
    .unwrap();
    let at = h.now().after(MINUTE).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    let anticipated = h
        .dispatch(Notice {
            observer: p[0],
            person: baby,
        })
        .unwrap();
    assert_eq!(anticipated.cue, "quiet");
    assert_eq!(anticipated.guessed_need.as_deref(), Some("food"));
    assert!(
        h.dispatch(Notice {
            observer: p[2],
            person: baby
        })
        .unwrap()
        .guessed_need
        .is_none()
    );
}
#[test]
fn absent_listeners_gain_no_language_and_invalid_phonemes_fail_loading() {
    let (mut h, p, _) = setup();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Elsewhere".into(),
        })
        .unwrap();
    let from = game_transport::area(h.get().unwrap(), p[1]).unwrap();
    h.dispatch(game_land::Link {
        from,
        to: area,
        duration: MINUTE,
    })
    .unwrap();
    h.dispatch(game_transport::Move {
        person: p[1],
        destination: area,
    })
    .unwrap();
    let at = h.now().after(MINUTE).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    let save = h.save().unwrap();
    assert!(
        h.dispatch(Convey {
            speaker: p[0],
            listener: p[1],
            concept: "good:1".into(),
            demonstrate: true,
            explanation: vec![]
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), save);
    h.dispatch(WordFor {
        person: p[0],
        concept: "good:1".into(),
    })
    .unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    corrupt["resources"]["communication.experience"]["lexicons"][p[0].0.to_string()]["good:1"][0]
        ["sounds"] = serde_json::json!([255]);
    assert!(builder().load(&corrupt.to_string(), 1).is_err());
}

#[test]
fn pain_does_not_erase_food_evidence_and_forecasts_survive_loading() {
    let (mut h, p, _) = setup();
    h.dispatch(game_physiology::Start).unwrap();
    h.dispatch(game_physiology::Rest { person: p[1] }).unwrap();
    let at = h.now().after(MINUTE).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    let hungry = h
        .dispatch(Notice {
            observer: p[0],
            person: p[1],
        })
        .unwrap();
    assert_eq!(hungry.guessed_need.as_deref(), Some("food"));
    h.dispatch(game_physiology::Damage {
        person: p[1],
        amount: 50,
    })
    .unwrap();
    let hurt = h
        .dispatch(Notice {
            observer: p[0],
            person: p[1],
        })
        .unwrap();
    assert_eq!(hurt.guessed_need.as_deref(), Some("pain"));
    assert_eq!(hurt.food, hungry.food);
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    assert_eq!(
        loaded
            .dispatch(Notice {
                observer: p[0],
                person: p[1]
            })
            .unwrap(),
        hurt
    );
}

#[test]
fn observing_an_incapacitated_survivor_does_not_require_them_to_act() {
    let (mut h, people, area) = setup();
    let survivor = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Guardian,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place {
        person: survivor,
        area,
    })
    .unwrap();
    h.dispatch(game_physiology::Damage {
        person: survivor,
        amount: u64::MAX,
    })
    .unwrap();
    let body = h.get::<game_physiology::Bodies>().unwrap().people[&survivor];
    assert!(body.alive);
    assert_eq!(body.health, 0);
    h.dispatch(Notice {
        observer: people[0],
        person: survivor,
    })
    .unwrap();
    assert!(
        h.dispatch(Notice {
            observer: survivor,
            person: people[0]
        })
        .is_err()
    );
    let saved = h.save().unwrap();
    assert_eq!(builder().load(&saved, 4).unwrap().save().unwrap(), saved);
}
