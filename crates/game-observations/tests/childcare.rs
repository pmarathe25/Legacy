mod support;
use game_core::*;
#[test]
fn caregiving_is_practiced_and_a_surviving_recipient_can_learn_a_simplified_lesson() {
    let (mut h, p, pack) = support::people(support::builder());
    h.dispatch(game_signals::Enable).unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    for (i, quantity) in [1, 3, 7].into_iter().enumerate() {
        support::run(&mut h, SimTime((i as u64 + 1) * 35 * MINUTE.0));
        h.dispatch(game_physiology::Serve {
            caregiver: p[0],
            patient: p[1],
            inventory: pack,
            good: game_cargo::FOOD,
            quantity,
        })
        .unwrap();
    }
    assert!(h.get::<game_skills::Skills>().unwrap().levels[&p[0]]["child-care"] > 0);
    assert!(
        game_knowledge::with_prefix(
            &h.get::<game_knowledge::Knowledge>().unwrap().0[&p[1]],
            "memory:empirical:"
        )
        .filter_map(|(_, fact)| fact.value.decode::<game_reasoning::Observation>())
        .any(|o| o.subject == "child-care")
    );
    assert!(h.dispatch(game_inquiry::Reflect { person: p[0] }).unwrap() > 0);
    let key = h.get::<game_knowledge::Knowledge>().unwrap().0[&p[0]]
        .iter()
        .find(|(k, _)| k.starts_with("theory:child-care:"))
        .unwrap()
        .0
        .to_string();
    assert!(
        !h.dispatch(game_inquiry::Explain {
            teacher: p[0],
            listener: p[1],
            key: key.clone(),
            form: game_inquiry::Explanation::Detailed
        })
        .unwrap()
    );
    assert!(
        h.dispatch(game_inquiry::Explain {
            teacher: p[0],
            listener: p[1],
            key: key.clone(),
            form: game_inquiry::Explanation::Practical
        })
        .unwrap()
    );
    support::run(&mut h, SimTime(106 * MINUTE.0));
    assert!(h.get::<game_knowledge::Knowledge>().unwrap().0[&p[1]].contains_key(&key));
    assert!(h.get::<game_skills::Skills>().unwrap().levels[&p[1]]["child-care"] > 0);
    let saved = h.save().unwrap();
    assert_eq!(
        support::builder().load(&saved, 4).unwrap().save().unwrap(),
        saved
    );
}
