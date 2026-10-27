use game_sim::{Pending, Request};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Meal(u64);
impl Request for Meal {
    const KEY: &'static str = "test.inspection-meal";
    type Response = ();
}

#[derive(Clone, Serialize, Deserialize)]
struct Different(u64);
impl Request for Different {
    const KEY: &'static str = "test.inspection-different";
    type Response = ();
}

#[test]
fn inspection_handles_bound_and_reloaded_nested_requests_without_invoking_them() {
    let bound = Pending::new(Meal(42)).unwrap();
    let reloaded: Pending = serde_json::from_str(&serde_json::to_string(&bound).unwrap()).unwrap();
    assert!(reloaded.request::<Meal>().is_none());
    assert_eq!(
        game_decision_frames::decode_pending::<Meal>(&bound),
        Some(Meal(42))
    );
    assert_eq!(
        game_decision_frames::decode_pending::<Meal>(&reloaded),
        Some(Meal(42))
    );
    let proposal =
        game_offers::Proposal::new("meal", game_offers::motive(10, 0, 0, 0, 0), Meal(42)).unwrap();
    let restored: game_offers::Proposal =
        serde_json::from_str(&serde_json::to_string(&proposal).unwrap()).unwrap();
    assert!(restored.request.request::<Meal>().is_none());
    assert_eq!(
        game_decision_frames::decode_pending::<Meal>(&restored.request),
        Some(Meal(42))
    );
    assert!(game_decision_frames::decode_pending::<Different>(&reloaded).is_none());
    let malformed: Pending =
        serde_json::from_str(r#"{"key":"test.inspection-meal","payload":"not a number"}"#).unwrap();
    assert!(game_decision_frames::decode_pending::<Meal>(&malformed).is_none());
}
