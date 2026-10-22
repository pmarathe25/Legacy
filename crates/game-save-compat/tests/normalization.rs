use game_save_compat::normalize;
fn defaults() -> serde_json::Value {
    let pairs = [
        ("memory", Some("knowledge.perception")),
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

#[test]
fn migration_borrows_opaque_resources_without_changing_numbers_escaping_or_private_data() {
    let mut old = defaults();
    old["plugins"].as_object_mut().unwrap().remove("search");
    for key in ["search.people", "search.goods"] {
        old["resources"].as_object_mut().unwrap().remove(key);
    }
    let mut text = old.to_string();
    // Raw preservation also avoids rounding an opaque extension's large numeric literal.
    text = text.replacen("\"resources\":{", "\"resources\":{\"opaque\":{\"large\":18446744073709551616,\"escaped\":\"\\u0061\",\"private\":[1,2,3]},", 1);
    let migrated = normalize(&text, || Ok(defaults().to_string())).unwrap();
    assert!(migrated.contains(
        r#""opaque":{"large":18446744073709551616,"escaped":"\u0061","private":[1,2,3]}"#
    ));
    assert_eq!(
        normalize(&migrated, || panic!("already normalized")).unwrap(),
        migrated
    );
}

#[test]
fn malformed_legacy_communication_is_rejected_without_panicking() {
    let mut old = defaults();
    old["plugins"].as_object_mut().unwrap().remove("signals");
    old["resources"]
        .as_object_mut()
        .unwrap()
        .remove("signals.experience");
    old["resources"]["communication.experience"] = serde_json::Value::Null;
    assert!(normalize(&old.to_string(), || Ok(defaults().to_string())).is_err());
}
#[test]
fn old_empirical_memories_move_into_personal_evidence_without_inventing_fresh_dates() {
    let mut old = defaults();
    old["plugins"].as_object_mut().unwrap().remove("memory");
    old["resources"]
        .as_object_mut()
        .unwrap()
        .remove("knowledge.perception");
    old["resources"]["recollection.episodes"] = serde_json::json!({"7":["struck:7:8"]});
    old["resources"]["knowledge.facts"] = serde_json::json!({});
    let observation = serde_json::json!({"subject":"iron","outcome":"hardness","context":{"heat":20},"trial":1,"value":9});
    old["resources"]["inquiry.experience"] =
        serde_json::json!({"people":{"7":{"observations":[observation],"count":4,"considered":2}}});
    let saved = normalize(&old.to_string(), || Ok(defaults().to_string()))
        .unwrap()
        .into_owned();
    let v: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert!(v["resources"].get("recollection.episodes").is_none());
    assert!(
        v["resources"]["inquiry.experience"]["people"]["7"]
            .get("observations")
            .is_none()
    );
    let archive: game_snapshot::evidence::Archive<serde_json::Value> =
        serde_json::from_value(v["resources"]["knowledge.facts"]["archive"].clone()).unwrap();
    let facts = archive.restore(|_, v| v).unwrap();
    let f = facts[0]
        .1
        .get("memory:empirical:00000000000000000004")
        .unwrap();
    assert_eq!(f.value.as_ref(), &observation);
    assert_eq!(f.source, game_core::PersonId(7));
    assert_eq!(f.observed, game_core::SimTime(0));
}
