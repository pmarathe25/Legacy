mod support;
use game_core::*;
use game_discourse::*;
use game_relationships::{Relationship, SetRelationship, Social};
use game_sim::Event;
use support::*;

#[test]
fn respect_can_outweigh_dislike_while_strong_conviction_can_resist_admiration() {
    let (mut h, p, _) = setup();
    for listener in [p[1], p[2]] {
        h.dispatch(SetRelationship {
            observer: listener,
            other: p[0],
            relationship: Relationship {
                affection: -40,
                trust: 50,
                respect: 100,
                admiration: 100,
                ..Default::default()
            },
        })
        .unwrap();
    }
    profile(&mut h, p[1], &[("openness", 500), ("conviction", 500)]);
    profile(&mut h, p[2], &[("openness", 0), ("conviction", 1000)]);
    stance(&mut h, p[2], -100);
    say(&mut h, p[0], p[1], 80, Approach::Persuade);
    say(&mut h, p[0], p[2], 80, Approach::Persuade);
    assert!(value(&h, p[1]) > 0);
    assert_eq!(value(&h, p[2]), -100);
    assert_eq!(
        h.get::<Social>().unwrap().relationships[&p[1]][&p[0]].affection,
        -40
    );
}

#[test]
fn openness_and_conviction_change_judgment_without_encoding_the_opinion_content() {
    let (mut h, p, _) = setup();
    profile(&mut h, p[1], &[("openness", 0), ("conviction", 0)]);
    profile(&mut h, p[2], &[("openness", 1000), ("conviction", 0)]);
    for listener in [p[1], p[2]] {
        say(&mut h, p[0], listener, 80, Approach::Explain);
    }
    assert_eq!(value(&h, p[1]), 0);
    assert!(value(&h, p[2]) > 0);
    let (mut h, p, _) = setup();
    for listener in [p[1], p[2]] {
        stance(&mut h, listener, -50);
    }
    profile(&mut h, p[1], &[("openness", 1000), ("conviction", 0)]);
    profile(&mut h, p[2], &[("openness", 1000), ("conviction", 1000)]);
    for listener in [p[1], p[2]] {
        say(&mut h, p[0], listener, 80, Approach::Explain);
    }
    assert!(value(&h, p[1]) > -50);
    assert_eq!(value(&h, p[2]), -50);
}

#[test]
fn contradictory_direct_testimony_changes_only_the_hearers_estimate_of_the_reporter() {
    let (mut h, p, _) = setup();
    let (reporter, source, listener, absent) = (p[0], p[1], p[2], p[3]);
    h.dispatch(SetRelationship {
        observer: reporter,
        other: source,
        relationship: Default::default(),
    })
    .unwrap();
    for observer in [listener, absent] {
        h.dispatch(SetRelationship {
            observer,
            other: reporter,
            relationship: Relationship {
                trust: 40,
                ..Default::default()
            },
        })
        .unwrap();
    }
    h.dispatch(SetRelationship {
        observer: listener,
        other: source,
        relationship: Relationship {
            trust: 90,
            respect: 80,
            ..Default::default()
        },
    })
    .unwrap();
    profile(&mut h, listener, &[("openness", 1000)]);
    stance(&mut h, source, -80);
    say(&mut h, reporter, listener, 80, Approach::Cite { source });
    assert!(value(&h, listener) > 0);
    assert_eq!(value(&h, absent), 0);
    assert!(
        h.get::<Social>()
            .unwrap()
            .heard(absent, source, "care")
            .is_none()
    );
    assert_eq!(
        h.get::<Social>()
            .unwrap()
            .heard(listener, source, "care")
            .unwrap()
            .via,
        reporter
    );
    say(&mut h, source, listener, -80, Approach::Explain);
    let social = h.get::<Social>().unwrap();
    assert!(social.relationships[&listener][&reporter].trust < 40);
    assert_eq!(social.relationships[&absent][&reporter].trust, 40);
    assert_eq!(social.heard(listener, source, "care").unwrap().stance, -80);
    let spoken = h.notices().find(|n| n.kind == Spoken::KEY).unwrap();
    assert!(spoken.data.get("lie").is_none() && spoken.data.get("accepted").is_none());
    let save = h.save().unwrap();
    assert_eq!(builder().load(&save, 4).unwrap().save().unwrap(), save);
}

#[test]
fn a_specific_coerced_endorsement_preserves_private_opinion_and_independent_allegiance() {
    let (mut h, p, _) = setup();
    h.dispatch(game_communities::Enable).unwrap();
    stance(&mut h, p[1], -80);
    profile(&mut h, p[1], &[("conviction", 0)]);
    h.dispatch(SetRelationship {
        observer: p[1],
        other: p[0],
        relationship: Relationship {
            fear: 100,
            ..Default::default()
        },
    })
    .unwrap();
    h.dispatch(game_pressure::Press {
        leader: p[0],
        subject: p[1],
        purpose: game_pressure::Purpose::Endorse {
            concept: "care".into(),
            stance: 80,
        },
    })
    .unwrap();
    assert_eq!(value(&h, p[1]), -80);
    assert!(
        h.get::<game_communities::Communities>()
            .unwrap()
            .members
            .is_empty()
    );
    let public = h
        .get::<Social>()
        .unwrap()
        .heard(p[0], p[1], "care")
        .unwrap();
    assert!(public.coerced);
    assert_eq!(public.stance, 80);
}

#[test]
fn received_food_builds_personal_regard_and_requires_recipient_assent() {
    let (mut h, p, _) = setup();
    profile(&mut h, p[1], &[("empathy", 1000)]);
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p[0]),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    let offer = game_physiology::Feed {
        caregiver: p[0],
        patient: p[1],
        inventory,
    };
    let before = h.save().unwrap();
    assert!(h.dispatch(offer.clone()).is_err());
    assert_eq!(h.save().unwrap(), before);
    h.dispatch(game_physiology::Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == game_sim::Stop::Budget {}
    h.dispatch(offer).unwrap();
    let tie = &h.get::<Social>().unwrap().relationships[&p[1]][&p[0]];
    assert!(tie.affection > 0 && tie.respect > 0 && tie.admiration > 0);
    assert!(
        h.get::<Social>()
            .unwrap()
            .relationships
            .get(&p[2])
            .is_none()
    );
}

#[test]
fn assertion_varies_with_personality_but_its_content_comes_from_learned_values() {
    let (mut h, p, _) = setup();
    let traits = [
        ("assertiveness", 1000),
        ("conviction", 0),
        ("foresight", 16000),
        ("aggression", 0),
    ];
    profile(&mut h, p[0], &traits);
    profile(&mut h, p[2], &traits);
    for listener in [p[1], p[3]] {
        profile(&mut h, listener, &[("openness", 1000)]);
    }
    stance(&mut h, p[0], 80);
    stance(&mut h, p[2], -80);
    h.dispatch(Meet(p[0], p[1])).unwrap();
    h.dispatch(Meet(p[2], p[3])).unwrap();
    assert!(value(&h, p[1]) > 0 && value(&h, p[3]) < 0);
    assert_eq!(
        h.get::<game_traits::Traits>().unwrap().people[&p[0]],
        h.get::<game_traits::Traits>().unwrap().people[&p[2]]
    );
    let (mut h, p, _) = setup();
    profile(
        &mut h,
        p[0],
        &[
            ("assertiveness", 0),
            ("conviction", 0),
            ("foresight", 16000),
        ],
    );
    stance(&mut h, p[0], 80);
    h.dispatch(Meet(p[0], p[1])).unwrap();
    assert!(!h.notices().any(|n| n.kind == Spoken::KEY));
}

#[test]
fn a_pending_specific_demand_retains_its_purpose_after_reload() {
    let (mut h, p, _) = setup();
    profile(
        &mut h,
        p[0],
        &[("aggression", 1000), ("ambition", 1000), ("patience", 0)],
    );
    profile(
        &mut h,
        p[1],
        &[("risk-tolerance", 1000), ("aggression", 1000)],
    );
    stance(&mut h, p[1], -80);
    let purpose = game_pressure::Purpose::Endorse {
        concept: "care".into(),
        stance: 80,
    };
    h.dispatch(game_pressure::Press {
        leader: p[0],
        subject: p[1],
        purpose: purpose.clone(),
    })
    .unwrap();
    assert_eq!(
        h.get::<game_pressure::Pressure>().unwrap().active[&p[1]].purpose,
        purpose
    );
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    h.dispatch(game_relationships::SufferForce {
        observer: p[1],
        actor: p[0],
        harm: 100,
    })
    .unwrap();
    loaded
        .dispatch(game_relationships::SufferForce {
            observer: p[1],
            actor: p[0],
            harm: 100,
        })
        .unwrap();
    for host in [&mut h, &mut loaded] {
        let at = host.now().after(SimDuration(3 * MINUTE.0)).unwrap();
        while host.advance(at, 128).unwrap().stop == game_sim::Stop::Budget {}
        assert_eq!(value(host, p[1]), -80);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn caregiver_learning_does_not_read_the_recipients_private_food_assessment() {
    let mut observations = vec![];
    for remembered_health in [-10, 10] {
        let (mut h, p, _) = setup_with_food_effect(remembered_health);
        h.dispatch(game_nutrition::Enable).unwrap();
        // The nutritional owner retains the patient's own estimate from a past meal.
        h.dispatch(game_nutrition::Meal {
            person: p[1],
            good: game_cargo::FOOD,
            quantity: 10,
        })
        .unwrap();
        h.dispatch(ObservedFeeding {
            caregiver: p[0],
            patient: p[1],
        })
        .unwrap();
        observations.push((
            value(&h, p[0]),
            h.get::<Social>().unwrap().relationships[&p[1]][&p[0]].affection,
        ));
    }
    assert_eq!(observations[0].0, observations[1].0);
    assert_eq!(observations[0].0, 1);
    assert!(observations[0].1 < observations[1].1);
}

#[test]
fn direct_pressure_cannot_manufacture_speech_without_comprehension_and_expression() {
    for (role, absent) in [
        (1, "vocalization"),
        (1, "hearing"),
        (1, "interpretation"),
        (0, "hearing"),
        (0, "interpretation"),
    ] {
        let (mut h, p, _) = setup();
        h.dispatch(game_communication::Enable).unwrap();
        profile(&mut h, p[1], &[("conviction", 0)]);
        profile(&mut h, p[role], &[(absent, 0), ("conviction", 0)]);
        h.dispatch(SetRelationship {
            observer: p[1],
            other: p[0],
            relationship: Relationship {
                fear: 100,
                ..Default::default()
            },
        })
        .unwrap();
        h.dispatch(game_pressure::Press {
            leader: p[0],
            subject: p[1],
            purpose: game_pressure::Purpose::Endorse {
                concept: "care".into(),
                stance: 80,
            },
        })
        .unwrap();
        assert!(
            h.get::<Social>()
                .unwrap()
                .heard(p[0], p[1], "care")
                .is_none()
        );
        assert!(!h.notices().any(|n| n.kind == Spoken::KEY));
        assert_eq!(value(&h, p[1]), 0);
    }
}

#[test]
fn extreme_rigidity_can_reject_admired_information_while_low_rigidity_revises() {
    let (mut h, p, _) = setup();
    for (listener, rigidity) in [(p[1], 0), (p[2], 1000)] {
        profile(
            &mut h,
            listener,
            &[
                ("rigidity", rigidity),
                ("openness", 1000),
                ("conviction", 1000),
            ],
        );
        stance(&mut h, listener, -20);
        h.dispatch(SetRelationship {
            observer: listener,
            other: p[0],
            relationship: Relationship {
                trust: 100,
                respect: 100,
                admiration: 100,
                ..Default::default()
            },
        })
        .unwrap();
        say(&mut h, p[0], listener, 80, Approach::Persuade);
    }
    assert!(value(&h, p[1]) > -20);
    assert_eq!(value(&h, p[2]), -20);
}
