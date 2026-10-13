use game_api::*;
use game_assembly::*;
fn journey(workers: usize) -> Simulation {
    let mut sim = Simulation::kernel_fixture(
        11,
        12,
        Execution {
            workers,
            reverse_jobs: workers > 1,
        },
    )
    .unwrap();
    sim.submit(Submission {
        caller_sequence: 1,
        command: Command::Move {
            destination: AreaId(2),
        },
    })
    .unwrap();
    sim.submit(Submission {
        caller_sequence: 2,
        command: Command::Move {
            destination: AreaId(3),
        },
    })
    .unwrap();
    sim
}
#[test]
fn native_route_has_eight_columns_no_person_shadow_and_is_legal_without_carry_runtime() {
    let sim = journey(1);
    assert!(sim.canonical_state().state.transport.is_none());
    let saved = sim.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(json["version"], 51);
    assert!(json["state"]["state"].get("people").is_none());
    assert!(
        json["state"]["components"]["columns"]["characters/person"][0]["value"]
            .get("route")
            .is_none()
    );
    assert_eq!(
        json["state"]["components"]["columns"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with("transport/"))
            .count(),
        8
    );
    assert_eq!(
        json["state"]["components"]["columns"]["transport/route"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let loaded = Simulation::load(&saved, Execution::default()).unwrap();
    assert_eq!(sim.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn removed_person_field_orphan_and_malformed_native_route_rows_reject_restoration() {
    let sim = journey(1);
    let base: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    for failure in 0..5 {
        let mut json = base.clone();
        match failure {
            0 => {
                json["state"]["components"]["columns"]["characters/person"][0]["value"]["route"] =
                    serde_json::json!([])
            }
            1 => {
                json["state"]["components"]["columns"]["transport/route"][0]["value"]["steps"] =
                    serde_json::json!([])
            }
            2 => {
                json["state"]["components"]["columns"]["transport/route"][0]["value"]["steps"] =
                    serde_json::json!([0])
            }
            3 => {
                json["state"]["components"]["columns"]["transport/route"][0]["value"]["steps"] =
                    serde_json::json!([99])
            }
            _ => {
                let inventory = json["state"]["components"]["columns"]["characters/person"][0]["value"]["inventory"]
                    .as_u64()
                    .unwrap();
                json["state"]["components"]["columns"]["transport/route"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!({"entity":inventory,"value":{"steps":[3]}}));
            }
        }
        assert!(
            Simulation::load(&serde_json::to_vec(&json).unwrap(), Execution::default()).is_err(),
            "failure {failure}"
        );
    }
}
#[test]
fn pending_native_itinerary_continuation_matches_restore_workers_and_irregular_slices() {
    let mut reference = journey(1);
    reference.advance(SimDuration(480_000)).unwrap();
    for workers in [1, 2, 4] {
        let mut sim = journey(workers);
        sim.advance(SimDuration(17_123)).unwrap();
        let bytes = sim.save().unwrap();
        let mut sim = Simulation::load(
            &bytes,
            Execution {
                workers,
                reverse_jobs: true,
            },
        )
        .unwrap();
        for slice in [22_877, 79_999, 1, 80_000, 280_000] {
            sim.advance(SimDuration(slice)).unwrap();
        }
        assert_eq!(reference.save().unwrap(), sim.save().unwrap());
    }
}
