use game_save_compat::normalize;
fn defaults() -> serde_json::Value {
    let pairs = [
        ("information", Some("information.questions")),
        ("search", Some("search.people")),
        ("mechanics", Some("mechanics.devices")),
        ("invention", None),
        ("communication", Some("communication.experience")),
        ("discourse", None),
        ("food-inquiry", Some("food-inquiry.samples")),
        ("companionship", None),
        ("attachments", None),
        ("digestion", Some("digestion.stomachs")),
        ("fauna", Some("fauna.animals")),
        ("credit", Some("credit.conventions")),
        ("testimony", None),
        ("observations", Some("observations.episodes")),
        ("choice", Some("choice.offers")),
        ("signals", Some("signals.experience")),
        ("comfort", Some("comfort.intentions")),
        ("family-life", Some("family-life.bonds")),
        ("literacy", Some("literacy.records")),
        ("inquiry", Some("inquiry.experience")),
    ];
    let mut value = serde_json::json!({"plugins":{},"resources":{"search.goods":{}}});
    for (plugin, resource) in pairs {
        value["plugins"][plugin] = serde_json::json!(1);
        if let Some(resource) = resource {
            value["resources"][resource] = serde_json::json!({});
        }
    }
    value
}
#[test]
fn current_manifests_preserve_original_bytes_without_constructing_defaults() {
    let text = serde_json::to_string_pretty(&defaults()).unwrap();
    let normalized = normalize(&text, || panic!("current save should not need defaults")).unwrap();
    assert!(matches!(normalized, std::borrow::Cow::Borrowed(_)));
    assert_eq!(normalized, text);
}
#[test]
fn old_signal_payload_moves_without_becoming_public_or_disappearing() {
    let old = serde_json::json!({"plugins":{"communication":1},"resources":{
        "communication.experience":{"enabled":true,"lexicons":{},"perceptions":{"7":{"8":{"cue":"crying","estimated_need":40}}}}
    }});
    let text = old.to_string();
    let normalized = normalize(&text, || Ok(defaults().to_string())).unwrap();
    let value: serde_json::Value = serde_json::from_str(&normalized).unwrap();
    assert_eq!(value["resources"]["signals.experience"]["enabled"], true);
    assert_eq!(
        value["resources"]["signals.experience"]["perceptions"],
        old["resources"]["communication.experience"]["perceptions"]
    );
    assert!(
        value["resources"]["communication.experience"]
            .get("perceptions")
            .is_none()
    );
}

#[test]
fn new_question_and_search_owners_start_empty_without_rewriting_private_knowledge() {
    let mut current = defaults();
    current["resources"]["information.questions"] =
        serde_json::json!({"interests":{},"inboxes":{}});
    let mut old = current.clone();
    for owner in ["information", "search"] {
        old["plugins"].as_object_mut().unwrap().remove(owner);
    }
    for resource in ["information.questions", "search.people", "search.goods"] {
        old["resources"].as_object_mut().unwrap().remove(resource);
    }
    old["resources"]["knowledge.facts"] =
        serde_json::json!({"7":{"secret":{"value":42,"source":7}}});
    let normalized = normalize(&old.to_string(), || Ok(current.to_string()))
        .unwrap()
        .into_owned();
    let normalized: serde_json::Value = serde_json::from_str(&normalized).unwrap();
    assert_eq!(
        normalized["resources"]["knowledge.facts"],
        old["resources"]["knowledge.facts"]
    );
    for resource in ["information.questions", "search.people", "search.goods"] {
        assert_eq!(
            normalized["resources"][resource],
            current["resources"][resource]
        );
    }
}
