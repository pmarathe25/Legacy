use crate::{Execution, Simulation};
use game_api::{Command, PersonId, Role, SimDuration, Submission, TransportAction};

fn offered() -> (Simulation, PersonId, PersonId) {
    let mut sim = Simulation::transport_fixture(51, Execution::default()).unwrap();
    let people = sim
        .host
        .state
        .people
        .iter()
        .map(|person| (person.id, person.role))
        .collect::<Vec<_>>();
    for (actor, role) in people {
        if role != Role::Guardian {
            let mut personality = crate::decisions::Personality::restful();
            personality.empathy = 0;
            sim.debug_set_personality(actor, personality, crate::decisions::Aspiration::Comfort)
                .unwrap();
        }
    }
    let ruler = sim.host.state.ruler;
    let passenger = sim.host.state.people[8].id;
    sim.debug_set_relationship(
        passenger,
        ruler,
        crate::decisions::Relationship {
            trust: 500,
            affection: 500,
        },
    )
    .unwrap();
    sim.debug_injure(passenger, 120000).unwrap();
    let sequence = sim.next_caller_sequence().unwrap();
    sim.submit(Submission {
        caller_sequence: sequence,
        command: Command::Transport {
            action: TransportAction::OfferCarry { passenger },
        },
    })
    .unwrap();
    (sim, ruler, passenger)
}
#[test]
fn transport_uses_seven_native_columns_and_rejects_runtime_entity_map_shadows() {
    let (sim, _, _) = offered();
    let saved = sim.save().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    let runtime = &value["state"]["state"]["transport"];
    assert_eq!(runtime.as_object().unwrap().len(), 2);
    for column in [
        "group", "offers", "lift", "approach", "resume", "paused", "support",
    ] {
        assert!(value["state"]["components"]["columns"][format!("transport/{column}")].is_array());
    }
    assert!(
        !value["state"]["components"]["columns"]["transport/offers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for field in [
        "groups",
        "offers",
        "lifts",
        "approaches",
        "resume_targets",
        "paused_approaches",
        "known_support",
    ] {
        assert!(runtime.get(field).is_none());
        let mut forged = value.clone();
        forged["state"]["state"]["transport"][field] = serde_json::json!({});
        assert!(
            Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err(),
            "transport shadow {field}"
        );
    }
    let restored = Simulation::load(
        &saved,
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    assert_eq!(sim.canonical_state(), restored.canonical_state());
    assert_eq!(saved, restored.save().unwrap());
}
#[test]
fn transport_restore_rejects_missing_runtime_wrong_person_keys_and_empty_orphans() {
    let (sim, _, _) = offered();
    let saved = sim.save().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    let area = sim.host.state.areas[0].id.0;
    for attack in 0..4 {
        let mut forged = value.clone();
        match attack {
            0 => forged["state"]["state"]["transport"] = serde_json::Value::Null,
            1 => {
                forged["state"]["components"]["columns"]["transport/offers"][0]["entity"] =
                    serde_json::json!(area)
            }
            2 => forged["state"]["components"]["columns"]["transport/support"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"entity":area,"value":{"by_passenger":{}}})),
            _ => forged["state"]["components"]["columns"]["transport/resume"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"entity":area,"value":{"area":area}})),
        }
        assert!(
            Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err(),
            "transport native attack {attack}"
        );
        assert_eq!(sim.save().unwrap(), saved);
    }
}
#[test]
fn actual_timed_lift_resumes_from_native_rows_and_becomes_a_shared_carry_group() {
    let (mut whole, ruler, passenger) = offered();
    whole.advance(SimDuration(60000)).unwrap();
    let saved = whole.save().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert!(
        !value["state"]["components"]["columns"]["transport/lift"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut split = Simulation::load(
        &saved,
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    whole.advance(SimDuration(6000)).unwrap();
    for duration in [5999, 1] {
        split.observe();
        split.advance(SimDuration(duration)).unwrap();
    }
    assert_eq!(whole.canonical_state(), split.canonical_state());
    assert_eq!(
        game_spatial::location(&whole.host.storage, game_ecs::StableId(passenger.0)),
        Some(&game_api::Location::Carried { carrier: ruler })
    );
    let carried = whole.save().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&carried).unwrap();
    assert!(
        !value["state"]["components"]["columns"]["transport/group"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        value["state"]["components"]["columns"]["transport/lift"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        whole.canonical_state(),
        Simulation::load(&carried, Execution::default())
            .unwrap()
            .canonical_state()
    );
}
