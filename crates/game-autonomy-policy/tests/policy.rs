use game_autonomy_policy::{Policy, Rules};
use game_core::*;
#[test]
fn optional_policy_validates_bounds_and_does_not_change_default_wire_fields() {
    let mut rules = Rules::default();
    assert!(
        serde_json::to_value(&rules)
            .unwrap()
            .get("care_signal_changes_only")
            .is_none()
    );
    let restored: Rules = serde_json::from_value(serde_json::to_value(&rules).unwrap()).unwrap();
    assert!(!restored.care_signal_changes_only);
    assert!(!restored.busy_attention);
    assert!(
        serde_json::to_value(&rules)
            .unwrap()
            .get("busy_attention")
            .is_none()
    );
    assert!(
        serde_json::to_value(&rules)
            .unwrap()
            .get("routine")
            .is_none()
    );
    let policy = Policy {
        review_floor: None,
        preserve_on_care_completion: false,
        minimum: MINUTE,
        maximum: HOUR,
        kinds: ["reason".into()].into(),
    };
    let wire = serde_json::to_value(&policy).unwrap();
    assert!(wire.get("preserve_on_care_completion").is_none());
    let restored: Policy = serde_json::from_value(wire).unwrap();
    assert!(!restored.preserve_on_care_completion);
    rules.routine = Some(policy.clone());
    assert!(rules.validate().is_ok());
    rules.routine.as_mut().unwrap().minimum = SimDuration(0);
    assert!(rules.validate().is_err());
    rules.routine = Some(policy.clone());
    rules.routine.as_mut().unwrap().maximum = SimDuration(1);
    assert!(rules.validate().is_err());
    rules.routine = Some(policy);
    rules.routine.as_mut().unwrap().kinds.clear();
    assert!(rules.validate().is_err());
}

#[test]
fn option_memory_is_optional_positive_and_drops_action_specific_continuations() {
    let mut rules = Rules::default();
    assert!(
        serde_json::to_value(&rules)
            .unwrap()
            .get("option_memory")
            .is_none()
    );
    assert!(
        serde_json::to_value(game_autonomy_policy::Mind::default())
            .unwrap()
            .get("considered")
            .is_none()
    );
    rules.option_memory = Some(SimDuration(0));
    assert!(rules.validate().is_err());
    rules.option_memory = Some(MINUTE);
    assert!(rules.validate().is_ok());
    let mut active = game_choice::Proposal::new(
        "active",
        game_choice::motive(0, 1, 0, 0, 0),
        game_choice::Wait,
    )
    .unwrap();
    active.continuation = Some(17);
    let fresh = game_choice::Proposal::new(
        "fresh",
        game_choice::motive(0, 2, 0, 0, 0),
        game_choice::Wait,
    )
    .unwrap();
    let memory =
        game_autonomy_policy::Consideration::remember(SimTime(0), MINUTE, &[active, fresh.clone()])
            .unwrap();
    assert_eq!(memory.recall(SimTime(MINUTE.0 - 1)), Some(vec![fresh]));
    assert!(memory.recall(SimTime(MINUTE.0)).is_none());
}

#[test]
fn consumed_options_are_not_repeated_and_shared_snapshots_keep_their_own_shortlist() {
    let offer = game_choice::Proposal::new(
        "fresh",
        game_choice::motive(0, 2, 0, 0, 0),
        game_choice::Wait,
    )
    .unwrap();
    let mut memory = std::sync::Arc::new(
        game_autonomy_policy::Consideration::remember(
            SimTime(0),
            MINUTE,
            std::slice::from_ref(&offer),
        )
        .unwrap(),
    );
    let previous = memory.clone();
    std::sync::Arc::make_mut(&mut memory).consume("fresh");
    assert_eq!(memory.recall(SimTime(1)), Some(vec![]));
    assert_eq!(previous.recall(SimTime(1)), Some(vec![offer]));
    let loaded: game_autonomy_policy::Consideration =
        serde_json::from_str(&serde_json::to_string(&memory).unwrap()).unwrap();
    assert_eq!(loaded.recall(SimTime(1)), Some(vec![]));
}

#[test]
fn shortlist_keeps_weighted_priorities_without_reordering_or_reweighting_the_saved_motives() {
    let offers = vec![
        game_choice::Proposal::new("a", game_choice::motive(0, 100, 0, 0, 0), game_choice::Wait)
            .unwrap(),
        game_choice::Proposal::new("b", game_choice::motive(40, 0, 0, 0, 0), game_choice::Wait)
            .unwrap(),
        game_choice::Proposal::new("c", game_choice::motive(0, 200, 0, 0, 0), game_choice::Wait)
            .unwrap(),
    ];
    let memory =
        game_autonomy_policy::Consideration::shortlist(SimTime(0), MINUTE, &offers, 2, |term| {
            if term == "goal" { 10 } else { 100 }
        })
        .unwrap();
    assert_eq!(
        memory.recall(SimTime(0)),
        Some(vec![offers[1].clone(), offers[2].clone()])
    );
    let all = game_autonomy_policy::Consideration::shortlist(
        SimTime(0),
        MINUTE,
        &offers,
        game_choice::LIMIT,
        |_| 100,
    )
    .unwrap();
    assert_eq!(all.recall(SimTime(0)), Some(offers.clone()));
    for limit in [0, game_choice::LIMIT + 1] {
        assert!(
            game_autonomy_policy::Consideration::shortlist(
                SimTime(0),
                MINUTE,
                &offers,
                limit,
                |_| { 100 }
            )
            .is_err()
        );
        assert!(
            Rules {
                option_memory: Some(MINUTE),
                option_limit: limit,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        serde_json::to_value(Rules::default())
            .unwrap()
            .get("option_limit")
            .is_none()
    );
}

#[test]
fn independent_attention_policies_cannot_share_conflicting_clocks() {
    let rules = Rules {
        option_memory: Some(MINUTE),
        routine: Some(Policy {
            review_floor: None,
            preserve_on_care_completion: false,
            minimum: MINUTE,
            maximum: HOUR,
            kinds: ["reason".into()].into(),
        }),
        ..Default::default()
    };
    assert!(rules.validate().is_err());
}
