use super::*;
fn check(s: &Social, p: PersonId, q: PersonId) {
    assert_eq!(compatibility(s, p, q), uncached(s, p, q));
    assert_eq!(compatibility(s, p, q), uncached(s, p, q));
}
#[test]
fn incremental_join_tracks_values_beliefs_hears_clones_and_rollback() {
    let (p, q) = (PersonId(1), PersonId(2));
    let mut s = Social::default();
    check(&s, p, q);
    for n in 0..12 {
        let key: Arc<str> = format!("topic:{n:02}").into();
        s.values
            .entry(p)
            .or_default()
            .insert(key.clone(), n * 7 - 35);
        s.heard.entry(p).or_default().entry(q).or_default().insert(
            key,
            Opinion {
                stance: n * 5 - 30,
                via: q,
                at: SimTime(0),
                coerced: n == 3,
            },
        );
    }
    check(&s, p, q);
    let original = s.clone();
    s.values.get_mut(&p).unwrap().insert("topic:00".into(), 90);
    check(&s, p, q);
    check(&original, p, q);
    s.heard
        .get_mut(&p)
        .unwrap()
        .get_mut(&q)
        .unwrap()
        .get_mut("topic:01")
        .unwrap()
        .coerced = true;
    check(&s, p, q);
    s.beliefs.insert(
        p,
        Beliefs {
            sharing: 91,
            authority: -25,
            honesty: 7,
        },
    );
    s.heard.get_mut(&p).unwrap().get_mut(&q).unwrap().insert(
        "authority".into(),
        Opinion {
            stance: 42,
            via: q,
            at: SimTime(0),
            coerced: false,
        },
    );
    check(&s, p, q);
    s.beliefs.get_mut(&p).unwrap().authority = 70;
    check(&s, p, q);
    s.values.remove(&p);
    check(&s, p, q);
    s.beliefs.remove(&p);
    check(&s, p, q);
    check(&original, p, q);
    s = original;
    let opinions = s.heard[&p][&q].clone();
    for other in 3..75 {
        s.heard
            .get_mut(&p)
            .unwrap()
            .insert(PersonId(other), opinions.clone());
    }
    for other in 3..75 {
        check(&s, p, PersonId(other));
    }
    check(&s, p, q);
    let collision = PersonId(p.0 + 1024);
    s.heard.insert(collision, s.heard.get(&p).unwrap().clone());
    s.values
        .insert(collision, s.values.get(&p).unwrap().clone());
    check(&s, collision, q);
    check(&s, p, q);
    std::thread::spawn(move || check(&s, p, q)).join().unwrap();
}
#[test]
fn unrelated_observers_keep_the_view_and_live_emotions_are_not_cached() {
    let (p, q) = (PersonId(8), PersonId(9));
    let mut s = Social::default();
    s.values.entry(p).or_default().insert("sharing".into(), 3);
    s.heard.entry(p).or_default().entry(q).or_default().insert(
        "sharing".into(),
        Opinion {
            stance: -7,
            via: q,
            at: SimTime(0),
            coerced: false,
        },
    );
    check(&s, p, q);
    s.values.entry(q).or_default().insert("sharing".into(), 90);
    check(&s, p, q);
    VIEWS.with(|views| {
        assert!(
            views.borrow()[p.0 as usize % 1024]
                .as_ref()
                .unwrap()
                .values
                .as_ref()
                .unwrap()
                .matches(s.values.get(&p).unwrap())
        )
    });
    s.relationships.entry(p).or_default().insert(
        q,
        game_relationships::Relationship {
            affection: 7,
            admiration: 3,
            fear: 2,
            ..Default::default()
        },
    );
    assert_eq!(emotional(&s, p, q), 9);
    check(&s, p, q);
}

#[test]
fn compatibility_caps_eight_qualifying_opinions_and_ignores_coercion_and_claims() {
    let (p, q) = (PersonId(21), PersonId(22));
    let mut s = Social::default();
    for key in ["claim:ignored".to_owned(), "coerced".to_owned()]
        .into_iter()
        .chain((0..9).map(|n| format!("topic:{n}")))
    {
        s.values
            .entry(p)
            .or_default()
            .insert(key.clone().into(), 10);
        s.heard.entry(p).or_default().entry(q).or_default().insert(
            key.clone().into(),
            Opinion {
                stance: 10,
                via: q,
                at: SimTime(0),
                coerced: key == "coerced",
            },
        );
    }
    assert_eq!(compatibility(&s, p, q), 80);
    s.values.get_mut(&p).unwrap().insert("topic:0".into(), -10);
    assert_eq!(compatibility(&s, p, q), 70);
    s.heard
        .get_mut(&p)
        .unwrap()
        .get_mut(&q)
        .unwrap()
        .get_mut("topic:1")
        .unwrap()
        .stance = -10;
    assert_eq!(compatibility(&s, p, q), 60);
}
