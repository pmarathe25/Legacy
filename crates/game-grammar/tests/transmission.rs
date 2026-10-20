use game_grammar::*;
use std::collections::BTreeMap;
fn scene(who: &str, relation: &str, role: &str, value: &str) -> Scene {
    [
        ("@predicate".into(), relation.into()),
        ("actor".into(), who.into()),
        (role.into(), value.into()),
    ]
    .into()
}
fn lexicon() -> BTreeMap<String, Sound> {
    [
        "at", "holds", "A", "B", "C", "clearing", "hill", "grain", "fruit",
    ]
    .iter()
    .enumerate()
    .map(|(i, s)| (s.to_string(), vec![i as u8]))
    .collect()
}
fn train(g: &mut Grammar, utterance: &Utterance, scene: &Scene, capacity: u64) -> bool {
    let lexicon = lexicon();
    g.observe(utterance, scene, capacity, |w| {
        lexicon
            .iter()
            .find(|(_, s)| s.as_slice() == w)
            .map(|(k, _)| k.clone())
    })
}
fn speak(g: &Grammar, scene: &Scene, seed: u64) -> Utterance {
    let lexicon = lexicon();
    g.express(
        scene,
        &Grammar::role_order(scene, seed),
        &BTreeMap::new(),
        |c| lexicon[c].clone(),
    )
    .unwrap()
}
fn understand(g: &Grammar, u: &Utterance) -> Option<Scene> {
    let lexicon = lexicon();
    g.interpret(u, |w| {
        lexicon
            .iter()
            .find(|(_, s)| s.as_slice() == w)
            .map(|(k, _)| k.clone())
    })
}
#[test]
fn repeated_labels_are_not_a_productive_rule_and_some_people_cannot_infer_one() {
    let s = scene("A", "at", "place", "clearing");
    let u = speak(&Grammar::default(), &s, 9);
    let mut incapable = Grammar::default();
    let mut capable = Grammar::default();
    for _ in 0..20 {
        assert!(!train(&mut incapable, &u, &s, 0));
        assert!(train(&mut capable, &u, &s, 1000));
    }
    assert_eq!(incapable.productive_rules(), 0);
    assert_eq!(capable.productive_rules(), 0);
    assert_eq!(understand(&capable, &u), None);
    let contrast = scene("B", "at", "place", "hill");
    let u2 = speak(&Grammar::default(), &contrast, 9);
    assert!(train(&mut capable, &u2, &contrast, 1000));
    assert_eq!(capable.productive_rules(), 1);
    assert_eq!(understand(&capable, &u2), Some(contrast));
}
#[test]
fn conventions_pass_through_three_empty_learners_and_later_support_embedding() {
    let mut first = Grammar::default();
    for n in 0..40 {
        let s = scene(
            if n % 2 == 0 { "A" } else { "B" },
            "at",
            "place",
            if n % 2 == 0 { "clearing" } else { "hill" },
        );
        let u = speak(&Grammar::default(), &s, 11);
        assert!(train(&mut first, &u, &s, 1000));
    }
    assert!(!first.can_embed(5000));
    let mut second = Grammar::default();
    assert_eq!(second.productive_rules(), 0);
    for n in 0..40 {
        let at = scene(if n % 2 == 0 { "A" } else { "B" }, "at", "place", "hill");
        assert!(train(&mut second, &speak(&first, &at, 11), &at, 2000));
        let carries = scene(
            if n % 2 == 0 { "A" } else { "B" },
            "holds",
            "thing",
            if n % 2 == 0 { "grain" } else { "fruit" },
        );
        assert!(train(
            &mut second,
            &speak(&Grammar::default(), &carries, 11),
            &carries,
            2000
        ));
    }
    assert!(second.can_embed(2000));
    assert!(!second.can_embed(1000));
    let mut third = Grammar::default();
    assert!(third.rules.is_empty());
    for n in 0..8 {
        let mut s = scene(
            if n % 2 == 0 { "A" } else { "B" },
            "holds",
            "thing",
            "grain",
        );
        s.insert("actor.@predicate".into(), "at".into());
        s.insert(
            "actor.place".into(),
            if n % 2 == 0 { "hill" } else { "clearing" }.into(),
        );
        let u = speak(&second, &s, 11);
        assert!(train(&mut third, &u, &s, 2000));
        assert!(!train(&mut Grammar::default(), &u, &s, 300));
        assert!(train(&mut Grammar::default(), &u, &s, 1000));
    }
    let mut novel = scene("C", "holds", "thing", "fruit");
    novel.insert("actor.@predicate".into(), "at".into());
    novel.insert("actor.place".into(), "hill".into());
    assert_eq!(understand(&third, &speak(&second, &novel, 11)), Some(novel));
    assert!(third.valid(40));
}
#[test]
fn foreign_order_is_ambiguous_until_particles_are_learned_from_grounded_examples() {
    let mut g = Grammar::default();
    let a = scene("A", "at", "place", "hill");
    let b = scene("B", "at", "place", "clearing");
    let order = vec!["@predicate".into(), "actor".into(), "place".into()];
    let inverse = vec!["@predicate".into(), "place".into(), "actor".into()];
    for _ in 0..8 {
        for s in [&a, &b] {
            g.confirm(s, order.clone(), BTreeMap::new());
            g.confirm(s, inverse.clone(), BTreeMap::new());
        }
    }
    let u = g
        .express(&a, &order, &BTreeMap::new(), |c| lexicon()[c].clone())
        .unwrap();
    assert!(understand(&g, &u).is_none());
    for _ in 0..3 {
        g.failed();
    }
    assert!(g.can_mark_roles(1000));
    assert!(!g.can_mark_roles(100));
    let markers = BTreeMap::from([("actor".into(), vec![38]), ("place".into(), vec![39])]);
    let mut learner = Grammar::default();
    for _ in 0..4 {
        for s in [&a, &b] {
            let u = g
                .express(s, &order, &markers, |c| lexicon()[c].clone())
                .unwrap();
            assert!(train(&mut learner, &u, s, 1000));
        }
    }
    let u = g
        .express(&a, &order, &markers, |c| lexicon()[c].clone())
        .unwrap();
    assert_eq!(understand(&learner, &u), Some(a));
    assert!(learner.rules.iter().any(|r| !r.markers.is_empty()));
}
