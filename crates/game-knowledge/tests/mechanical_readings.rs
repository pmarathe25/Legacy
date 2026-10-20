use game_core::*;
use game_knowledge::*;
use std::sync::Arc;
fn remembered(key: &str, value: serde_json::Value) -> Facts {
    let mut facts = Facts::default();
    facts.insert(
        key.into(),
        Fact {
            value: Arc::new(FactValue::Other(value)),
            source: PersonId(1),
            observed: SimTime(10),
            revision: 20,
            confidence: 70,
        },
    );
    facts
}
#[test]
fn compact_mechanical_evidence_keeps_every_scalar_and_legacy_or_extension_field() {
    let key = "mechanics:device:42:0";
    let value = serde_json::json!({"gain":1693,"input_force":232,"input_travel":619,"load":393,"travel":499});
    let facts = remembered(key, value.clone());
    assert!(matches!(
        facts.get(key).unwrap().value.as_ref(),
        FactValue::Mechanical(_)
    ));
    assert_eq!(
        known::<game_physics::Measurement>(&facts, key).unwrap(),
        game_physics::Measurement {
            gain: 1693,
            travel: 499,
            load: 393
        }
    );
    assert_eq!(
        serde_json::to_value(facts.get(key).unwrap().value).unwrap(),
        value
    );
    let saved = serde_json::to_string(&facts).unwrap();
    let loaded: Facts = serde_json::from_str(&saved).unwrap();
    assert_eq!(serde_json::to_string(&loaded).unwrap(), saved);
    assert_eq!(loaded, facts);
    for value in [
        serde_json::json!({"gain":1000,"travel":50}),
        serde_json::json!({"gain":1693,"input_force":232,"input_travel":619,"load":393,"travel":499,"extension":"private note"}),
    ] {
        let facts = remembered(key, value.clone());
        assert!(matches!(
            facts.get(key).unwrap().value.as_ref(),
            FactValue::Other(_)
        ));
        assert_eq!(
            serde_json::to_value(facts.get(key).unwrap().value).unwrap(),
            value
        );
        assert!(known::<game_physics::Measurement>(&facts, key).is_some());
    }
}
