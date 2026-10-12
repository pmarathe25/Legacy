use crate::*;
#[test]
fn native_society_normal_flow_has_no_header_rows_and_resumes_exactly() {
    let mut sim = Simulation::institution_fixture(81, Execution::default()).unwrap();
    sim.advance(SimDuration(360_000)).unwrap();
    assert!(
        game_society::institutions(&sim.host.storage)
            .next()
            .is_some()
    );
    assert!(game_society::cases(&sim.host.storage).next().is_some());
    let saved = sim.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    let header = &json["state"]["state"]["institutions"];
    assert_eq!(
        header
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["last_event"]
    );
    for column in [
        "institution",
        "proposal",
        "concept",
        "case",
        "knowledge",
        "requests",
        "exclusions",
        "hearing",
    ] {
        assert!(
            json["state"]["components"]["columns"]
                .get(format!("society/{column}"))
                .is_some()
        );
    }
    let mut resumed = Simulation::load(
        &saved,
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    assert_eq!(sim.canonical_state(), resumed.canonical_state());
    sim.advance(MINUTE).unwrap();
    resumed.advance(MINUTE).unwrap();
    assert_eq!(sim.canonical_state(), resumed.canonical_state());
}
#[test]
fn native_society_restore_rejects_orphans_missing_runtime_and_counter_collisions() {
    let mut sim = Simulation::institution_fixture(81, Execution::default()).unwrap();
    sim.advance(SimDuration(360_000)).unwrap();
    let saved = sim.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    let area = sim.host.state.areas[0].id.0;
    let institution = game_society::institutions(&sim.host.storage)
        .next()
        .unwrap()
        .0
        .0;
    for attack in 0..5 {
        let mut forged = json.clone();
        match attack{
            0=>forged["state"]["state"]["institutions"]=serde_json::Value::Null,
            1=>forged["state"]["components"]["columns"]["society/knowledge"].as_array_mut().unwrap().push(serde_json::json!({"entity":area,"value":{"cases":[],"values":{},"institutions":{}}})),
            2=>forged["state"]["components"]["columns"]["society/requests"].as_array_mut().unwrap().push(serde_json::json!({"entity":area,"value":{"search_after":0}})),
            3=>forged["state"]["components"]["columns"]["society/institution"][0]["entity"]=serde_json::json!(area),
            _=>forged["state"]["next_id"]=serde_json::json!(institution),
        }
        assert!(
            Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err(),
            "native society attack {attack}"
        );
        assert_eq!(sim.save().unwrap(), saved);
    }
}
