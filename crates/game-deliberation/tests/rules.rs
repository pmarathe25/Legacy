use game_core::*;
use game_deliberation::*;
#[test]
fn arbitrary_influence_and_required_support_are_used_literally() {
    let (a, b, c) = (PersonId(1), PersonId(2), PersonId(3));
    let members = [a, b, c].into();
    let rule = DecisionRule {
        influence: [(a, 2), (b, 11), (c, 4)].into(),
        required_support: 6,
    };
    assert!(rule.valid(&members));
    assert!(rule.passed(&members, &[(a, true), (c, true)].into()));
    assert!(rule.passed(&members, &[(b, true)].into()));
    assert!(!rule.passed(&members, &[(c, true), (PersonId(99), true)].into()));
    let absent = [a, c].into();
    assert!(!rule.passed(&absent, &[(b, true), (c, true)].into()));
    assert!(
        !DecisionRule {
            required_support: 0,
            ..rule.clone()
        }
        .valid(&members)
    );
    assert!(
        !DecisionRule {
            influence: [(a, 0)].into(),
            ..rule
        }
        .valid(&members)
    );
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Think {
    person: PersonId,
    members: std::collections::BTreeSet<PersonId>,
}
impl game_sim::Request for Think {
    const KEY: &'static str = "test.think";
    type Response = DecisionRule;
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Evaluate {
    person: PersonId,
    rule: DecisionRule,
}
impl game_sim::Request for Evaluate {
    const KEY: &'static str = "test.evaluate";
    type Response = i64;
}
#[test]
fn losing_supporters_does_not_create_imaginary_influence() {
    let mut b = game_sim::Builder::new();
    game_relationships::plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.request::<Evaluate>(|tx, r| Ok(preference(tx, r.person, &r.rule)))
        .unwrap();
    let mut h = b.build(7, 1).unwrap();
    let p = PersonId(1);
    h.dispatch(game_relationships::LearnValue {
        person: p,
        concept: "independence".into(),
        outcome: 5,
    })
    .unwrap();
    let rule = DecisionRule {
        influence: [(p, 3)].into(),
        required_support: 3,
    };
    let at_limit = h
        .dispatch(Evaluate {
            person: p,
            rule: rule.clone(),
        })
        .unwrap();
    let unreachable = h
        .dispatch(Evaluate {
            person: p,
            rule: DecisionRule {
                required_support: 8,
                ..rule
            },
        })
        .unwrap();
    assert_eq!(at_limit, unreachable);
}
#[test]
fn constructed_influence_follows_personal_values_and_known_relationships() {
    let mut b = game_sim::Builder::new();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.request::<Think>(|tx, r| Ok(construct(tx, r.person, &r.members)))
        .unwrap();
    let mut h = b.build(7, 1).unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    let (a, c) = (PersonId(1), PersonId(2));
    let think = Think {
        person: a,
        members: [a, c].into(),
    };
    let before = h.dispatch(think.clone()).unwrap();
    h.dispatch(game_relationships::LearnValue {
        person: a,
        concept: "shared-decisions".into(),
        outcome: 60,
    })
    .unwrap();
    let shared = h.dispatch(think.clone()).unwrap();
    assert_ne!(before, shared);
    assert!(shared.influence[&a] != shared.influence[&c]);
    // The constructor cannot read another person's private ambition as evidence.
    h.dispatch(game_traits::Set {
        person: c,
        profile: [("ambition".into(), 999)].into(),
    })
    .unwrap();
    assert_eq!(h.dispatch(think.clone()).unwrap(), shared);
    h.dispatch(game_traits::Set {
        person: a,
        profile: [
            ("ambition".into(), 999),
            ("conviction".into(), 900),
            ("patience".into(), 100),
        ]
        .into(),
    })
    .unwrap();
    let changed = h.dispatch(think).unwrap();
    assert_ne!(changed.influence, shared.influence);
    assert_ne!(changed.required_support, shared.required_support);
}
