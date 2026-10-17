use game_core::*;
use std::sync::Arc;
#[test]
fn immutable_food_projection_does_not_leak_across_evidence_or_rule_profiles() {
    let objects = game_objects::standard().unwrap();
    let grain = objects.resolve("grain").unwrap();
    let known = Arc::new(game_knowledge::FactValue::Stock([(grain, 10)].into()));
    assert!(game_provisioning::remembered_meal(&objects, &known));
    let empty = Arc::new(game_knowledge::FactValue::Stock([(grain, 0)].into()));
    assert!(!game_provisioning::remembered_meal(&objects, &empty));
    let mut wire = serde_json::to_value(&objects).unwrap();
    wire["items"]["grain"]["food"] = serde_json::Value::Null;
    let inedible: game_objects::Objects = serde_json::from_value(wire).unwrap();
    assert!(!game_provisioning::remembered_meal(&inedible, &known));
    assert!(game_provisioning::remembered_meal(&objects, &known));
    let unknown = Arc::new(game_knowledge::FactValue::Stock(
        [(GoodId(u64::MAX), 10)].into(),
    ));
    assert!(!game_provisioning::remembered_meal(&objects, &unknown));
}
