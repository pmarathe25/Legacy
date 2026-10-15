use game_api::*;
#[test]
fn commands_are_renderer_independent_serializable_values() {
    let command = Command::Move {
        destination: AreaId(2),
    };
    let json = serde_json::to_value(&command).unwrap();
    assert_eq!(json, serde_json::json!({"Move":{"destination":2}}));
    assert_eq!(serde_json::from_value::<Command>(json).unwrap(), command);
    assert!(serde_json::from_str::<Command>("{\"Unknown\":{}}").is_err());
}
