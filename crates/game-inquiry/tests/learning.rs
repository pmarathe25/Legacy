mod support;
use game_core::*;
use game_inquiry::*;
use game_knowledge::{Knowledge, known, with_prefix};
use game_reasoning::{Claim, Observation};
#[test]
fn a_capable_discoverer_can_explain_a_result_to_someone_unable_to_discover_it() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, _) = support::people(b);
    for person in [p[0], p[1]] {
        for (heat, value) in [(10, 5), (20, 15)] {
            h.dispatch(Witness {
                observer: person,
                observation: Observation {
                    subject: "iron".into(),
                    context: [("heat".into(), heat), ("water".into(), 1)].into(),
                    outcome: "hardness".into(),
                    value,
                    trial: 1,
                },
            })
            .unwrap();
        }
        h.dispatch(Reflect { person }).unwrap();
    }
    let facts = &h.get::<Knowledge>().unwrap().0[&p[0]];
    let key = with_prefix(facts, "theory:").next().unwrap().0.to_string();
    assert!(
        h.get::<Knowledge>()
            .unwrap()
            .0
            .get(&p[1])
            .is_none_or(|f| with_prefix(f, "theory:").next().is_none())
    );
    assert!(
        !h.dispatch(Explain {
            teacher: p[0],
            listener: p[1],
            key: key.clone(),
            form: Explanation::Detailed
        })
        .unwrap()
    );
    assert!(
        h.dispatch(Explain {
            teacher: p[0],
            listener: p[1],
            key: key.clone(),
            form: Explanation::Practical
        })
        .unwrap()
    );
    support::run(&mut h, SimTime(0));
    assert!(known::<Claim>(&h.get::<Knowledge>().unwrap().0[&p[1]], &key).is_some());
    let facts = &h.get::<Knowledge>().unwrap().0[&p[1]];
    let prediction =
        game_knowledge::forecast(facts, "iron", "hardness", &[("heat".into(), 30)].into()).unwrap();
    assert!(prediction.value > 15);
    assert!(
        game_knowledge::forecast(
            facts,
            "iron",
            "hardness",
            &[("heat".into(), 30), ("water".into(), 0)].into()
        )
        .is_none()
    );
    let saved = h.save().unwrap();
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    assert_eq!(saved, b.load(&saved, 4).unwrap().save().unwrap());
}
#[test]
fn deduction_cannot_launder_received_uncertainty() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, _) = support::people(b);
    let first = Claim {
        subject: "iron".into(),
        factor: "heat".into(),
        outcome: "hardness".into(),
        complexity: 1,
        context: Default::default(),
        effect: 100,
        confidence: 90,
        comparisons: 5,
    };
    let mut second = first.clone();
    second.factor = "hardness".into();
    second.outcome = "durability".into();
    for (claim, confidence) in [(first, 10), (second, 90)] {
        let value = serde_json::to_value(&claim).unwrap();
        h.dispatch(game_knowledge::Conclude {
            person: p[2],
            key: claim.key(),
            value,
            confidence,
        })
        .unwrap();
    }
    h.dispatch(Reflect { person: p[2] }).unwrap();
    let facts = &h.get::<Knowledge>().unwrap().0[&p[2]];
    let derived = with_prefix(facts, "theory:")
        .filter_map(|(_, f)| f.value.decode::<Claim>())
        .find(|c| c.factor == "heat" && c.outcome == "durability")
        .unwrap();
    assert!(derived.confidence <= 10);
}

#[test]
fn ordinary_reports_cannot_bypass_comprehension_and_zero_intellect_cannot_be_scaffolded() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, _) = support::people(b);
    let claim = Claim {
        subject: "river".into(),
        factor: "rain".into(),
        outcome: "flooding".into(),
        complexity: 4,
        context: [("upstream-snow".into(), 1)].into(),
        effect: 80,
        confidence: 60,
        comparisons: 3,
    };
    let key = claim.key();
    h.dispatch(game_knowledge::Conclude {
        person: p[0],
        key: key.clone(),
        value: serde_json::to_value(&claim).unwrap(),
        confidence: 60,
    })
    .unwrap();
    h.dispatch(game_testimony::Report {
        sender: p[0],
        recipient: p[1],
        key: key.clone(),
        delay: SimDuration(0),
    })
    .unwrap();
    support::run(&mut h, SimTime(0));
    assert!(
        h.get::<Knowledge>()
            .unwrap()
            .0
            .get(&p[1])
            .is_none_or(|f| with_prefix(f, "theory:").next().is_none())
    );
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("intelligence".into(), 0)].into(),
    })
    .unwrap();
    assert!(
        !h.dispatch(Explain {
            teacher: p[0],
            listener: p[1],
            key: key.clone(),
            form: Explanation::Demonstrate,
        })
        .unwrap()
    );
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("intelligence".into(), 500)].into(),
    })
    .unwrap();
    assert!(
        h.dispatch(Explain {
            teacher: p[0],
            listener: p[1],
            key: key.clone(),
            form: Explanation::Practical,
        })
        .unwrap()
    );
    support::run(&mut h, SimTime(0));
    let fact = h.get::<Knowledge>().unwrap().0[&p[1]]
        .get(key.as_str())
        .unwrap();
    assert!(fact.confidence <= 60);
    assert_eq!(fact.value.decode::<Claim>().unwrap().context, claim.context);
}

#[test]
fn recovery_of_reasoning_capacity_reopens_unchanged_evidence() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, _) = support::people(b);
    for (heat, value) in [(10, 5), (20, 15)] {
        h.dispatch(Witness {
            observer: p[1],
            observation: Observation {
                subject: "iron".into(),
                context: [("heat".into(), heat), ("water".into(), 1)].into(),
                outcome: "hardness".into(),
                value,
                trial: 1,
            },
        })
        .unwrap();
    }
    h.dispatch(Reflect { person: p[1] }).unwrap();
    assert!(
        h.dispatch(game_choice::Collect { person: p[1] })
            .unwrap()
            .is_empty()
    );
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("intelligence".into(), 4000)].into(),
    })
    .unwrap();
    let offers = h.dispatch(game_choice::Collect { person: p[1] }).unwrap();
    assert!(offers.iter().any(|o| o.key == "inquiry:reflect"));
    h.dispatch(Reflect { person: p[1] }).unwrap();
    assert!(
        with_prefix(&h.get::<Knowledge>().unwrap().0[&p[1]], "theory:")
            .next()
            .is_some()
    );
}

#[test]
fn a_deaf_listener_can_understand_a_grounded_visual_explanation() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, inventory) = support::people(b);
    h.dispatch(game_communication::Enable).unwrap();
    h.dispatch(game_traits::Set {
        person: p[0],
        profile: [("intelligence".into(), 4000), ("vocalization".into(), 0)].into(),
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("intelligence".into(), 500), ("hearing".into(), 0)].into(),
    })
    .unwrap();
    let claim = Claim {
        subject: "wood".into(),
        factor: "heat".into(),
        outcome: "hardness".into(),
        complexity: 2,
        context: [("water".into(), 1)].into(),
        effect: 100,
        confidence: 40,
        comparisons: 1,
    };
    let key = claim.key();
    h.dispatch(game_knowledge::Conclude {
        person: p[0],
        key: key.clone(),
        value: serde_json::to_value(&claim).unwrap(),
        confidence: 40,
    })
    .unwrap();
    assert!(
        h.dispatch(Explain {
            teacher: p[0],
            listener: p[1],
            key: key.clone(),
            form: Explanation::Demonstrate
        })
        .is_err()
    );
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::TIMBER,
        quantity: 1,
    })
    .unwrap();
    for concept in ["good:2".into(), format!("person:{}", p[0].0)] {
        h.dispatch(game_communication::WordFor {
            person: p[0],
            concept,
        })
        .unwrap();
    }
    assert!(
        h.dispatch(Explain {
            teacher: p[0],
            listener: p[1],
            key: key.clone(),
            form: Explanation::Demonstrate
        })
        .unwrap()
    );
    support::run(&mut h, SimTime(0));
    assert!(known::<Claim>(&h.get::<Knowledge>().unwrap().0[&p[1]], &key).is_some());
    assert!(
        !h.get::<game_communication::Communication>()
            .unwrap()
            .lexicons
            .contains_key(&p[1])
    );
}

#[test]
fn unrelated_single_experiences_do_not_make_reflection_compete_with_everyday_work() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, _) = support::people(b);
    for outcome in ["mood", "health", "alertness"] {
        h.dispatch(Witness {
            observer: p[0],
            observation: Observation {
                subject: "meal".into(),
                context: [("dose".into(), 1)].into(),
                outcome: outcome.into(),
                value: 0,
                trial: 0,
            },
        })
        .unwrap();
    }
    assert!(
        h.dispatch(game_choice::Collect { person: p[0] })
            .unwrap()
            .is_empty()
    );
    h.dispatch(Witness {
        observer: p[0],
        observation: Observation {
            subject: "meal".into(),
            context: [("dose".into(), 2)].into(),
            outcome: "mood".into(),
            value: 5,
            trial: 0,
        },
    })
    .unwrap();
    assert!(
        h.dispatch(game_choice::Collect { person: p[0] })
            .unwrap()
            .iter()
            .any(|o| o.key == "inquiry:reflect")
    );
}

#[test]
fn too_little_reasoning_capacity_cannot_infer_a_mechanism_even_from_a_simple_contrast() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, _) = support::people(b);
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("intelligence".into(), 0)].into(),
    })
    .unwrap();
    for (heat, value) in [(10, 5), (20, 15)] {
        h.dispatch(Witness {
            observer: p[1],
            observation: Observation {
                subject: "iron".into(),
                context: [("heat".into(), heat)].into(),
                outcome: "hardness".into(),
                value,
                trial: 1,
            },
        })
        .unwrap();
    }
    assert_eq!(h.dispatch(Reflect { person: p[1] }).unwrap(), 0);
    assert!(
        with_prefix(&h.get::<Knowledge>().unwrap().0[&p[1]], "theory:")
            .next()
            .is_none()
    );
}

#[test]
fn simplified_explanations_use_the_same_capacity_limit_at_offer_and_delivery() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, _) = support::people(b);
    let claim = Claim {
        subject: "flow".into(),
        factor: "rain".into(),
        outcome: "flooding".into(),
        complexity: 12,
        context: [("upstream-snow".into(), 1)].into(),
        effect: 100,
        confidence: 60,
        comparisons: 4,
    };
    let key = claim.key();
    h.dispatch(game_knowledge::Conclude {
        person: p[0],
        key: key.clone(),
        value: serde_json::to_value(claim).unwrap(),
        confidence: 60,
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("intelligence".into(), 500)].into(),
    })
    .unwrap();
    assert!(
        !h.dispatch(Explain {
            teacher: p[0],
            listener: p[1],
            key: key.clone(),
            form: Explanation::Practical
        })
        .unwrap()
    );
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("intelligence".into(), 700)].into(),
    })
    .unwrap();
    assert!(
        h.dispatch(Explain {
            teacher: p[0],
            listener: p[1],
            key: key.clone(),
            form: Explanation::Practical
        })
        .unwrap()
    );
    support::run(&mut h, SimTime(0));
    assert!(known::<Claim>(&h.get::<Knowledge>().unwrap().0[&p[1]], &key).is_some());
}

#[test]
fn changed_evidence_can_revise_a_deduction_without_needing_higher_confidence() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, _) = support::people(b);
    let first = Claim {
        subject: "iron".into(),
        factor: "heat".into(),
        outcome: "hardness".into(),
        complexity: 1,
        context: Default::default(),
        effect: 100,
        confidence: 80,
        comparisons: 4,
    };
    let mut second = first.clone();
    second.factor = "hardness".into();
    second.outcome = "durability".into();
    for c in [first, second.clone()] {
        h.dispatch(game_knowledge::Conclude {
            person: p[0],
            key: c.key(),
            value: serde_json::to_value(c).unwrap(),
            confidence: 80,
        })
        .unwrap();
    }
    h.dispatch(Reflect { person: p[0] }).unwrap();
    let derived = with_prefix(&h.get::<Knowledge>().unwrap().0[&p[0]], "theory:")
        .filter_map(|(_, f)| f.value.decode::<Claim>())
        .find(|c| c.factor == "heat" && c.outcome == "durability")
        .unwrap();
    second.effect = 200;
    h.dispatch(game_knowledge::Conclude {
        person: p[0],
        key: second.key(),
        value: serde_json::to_value(second).unwrap(),
        confidence: 80,
    })
    .unwrap();
    h.dispatch(Reflect { person: p[0] }).unwrap();
    let revised = known::<Claim>(&h.get::<Knowledge>().unwrap().0[&p[0]], &derived.key()).unwrap();
    assert_eq!(revised.effect, 200);
    assert_eq!(revised.confidence, derived.confidence);
}

#[test]
fn a_simpler_held_explanation_can_unlock_new_deductions() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, _) = support::people(b);
    h.dispatch(game_traits::Set {
        person: p[1],
        profile: [("intelligence".into(), 1000)].into(),
    })
    .unwrap();
    let first = Claim {
        subject: "iron".into(),
        factor: "heat".into(),
        outcome: "hardness".into(),
        complexity: 3,
        context: Default::default(),
        effect: 100,
        confidence: 80,
        comparisons: 4,
    };
    let mut second = first.clone();
    second.factor = "hardness".into();
    second.outcome = "durability".into();
    second.complexity = 2;
    for c in [first, second.clone()] {
        h.dispatch(game_knowledge::Conclude {
            person: p[1],
            key: c.key(),
            value: serde_json::to_value(c).unwrap(),
            confidence: 80,
        })
        .unwrap();
    }
    h.dispatch(Reflect { person: p[1] }).unwrap();
    assert!(
        h.dispatch(game_choice::Collect { person: p[1] })
            .unwrap()
            .is_empty()
    );
    second.complexity = 1;
    h.dispatch(game_knowledge::Conclude {
        person: p[1],
        key: second.key(),
        value: serde_json::to_value(second).unwrap(),
        confidence: 80,
    })
    .unwrap();
    assert!(
        h.dispatch(game_choice::Collect { person: p[1] })
            .unwrap()
            .iter()
            .any(|o| o.key == "inquiry:reflect")
    );
    h.dispatch(Reflect { person: p[1] }).unwrap();
    assert!(
        with_prefix(&h.get::<Knowledge>().unwrap().0[&p[1]], "theory:")
            .filter_map(|(_, f)| f.value.decode::<Claim>())
            .any(|c| c.factor == "heat" && c.outcome == "durability")
    );
}

#[test]
fn retained_empirical_episodes_survive_new_witnesses_and_invalid_episode_rollback() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, _) = support::people(b);
    let episode = |n| Observation {
        context: [("heat".into(), n)].into(),
        subject: "iron".into(),
        outcome: "hardness".into(),
        trial: n as u64,
        value: n,
    };
    h.dispatch(Witness {
        observer: p[0],
        observation: episode(1),
    })
    .unwrap();
    let earlier = h.get::<Inquiry>().unwrap().clone();
    h.dispatch(Witness {
        observer: p[0],
        observation: episode(2),
    })
    .unwrap();
    assert_eq!(earlier.people[&p[0]].observations.len(), 1);
    assert_eq!(earlier.people[&p[0]].observations[0].value, 1);
    let saved = h.save().unwrap();
    let mut invalid = episode(3);
    invalid.context.insert("".into(), 0);
    assert!(
        h.dispatch(Witness {
            observer: p[0],
            observation: invalid
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), saved);
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let loaded = b.load(&saved, 1).unwrap();
    assert_eq!(loaded.save().unwrap(), saved);
    assert_eq!(
        loaded.get::<Inquiry>().unwrap(),
        h.get::<Inquiry>().unwrap()
    );
}

#[test]
fn rigidity_can_reject_a_comprehensible_contradiction_without_changing_inference_capacity() {
    let mut b = support::builder();
    plugin(&mut b).unwrap();
    let (mut h, p, _) = support::people(b);
    let mut claim = Claim {
        subject: "food".into(),
        factor: "heat".into(),
        outcome: "spoilage".into(),
        complexity: 1,
        context: Default::default(),
        effect: 100,
        confidence: 80,
        comparisons: 8,
    };
    for (person, rigidity) in [(p[0], 0), (p[1], 1000)] {
        h.dispatch(game_traits::Set {
            person,
            profile: [("rigidity".into(), rigidity), ("intelligence".into(), 4000)].into(),
        })
        .unwrap();
        h.dispatch(game_knowledge::Conclude {
            person,
            key: claim.key(),
            value: serde_json::to_value(&claim).unwrap(),
            confidence: 80,
        })
        .unwrap();
    }
    claim.effect = -100;
    for (person, expected) in [(p[0], true), (p[1], false)] {
        assert_eq!(
            h.dispatch(game_testimony::Comprehend {
                person,
                fact: game_knowledge::Fact {
                    value: std::sync::Arc::new(game_knowledge::FactValue::Other(
                        serde_json::to_value(&claim).unwrap()
                    )),
                    source: p[2],
                    observed: h.now(),
                    revision: h.revision(),
                    confidence: 80
                },
                support: 0
            })
            .unwrap(),
            expected
        );
    }
}
