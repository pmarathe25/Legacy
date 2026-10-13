use game_api::*;
use game_assembly::{Execution, Simulation};
use std::collections::BTreeSet;

fn send(sim: &mut Simulation, action: InvocationAction) {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: Command::Invocation { action },
    })
    .unwrap();
}
fn learned() -> Simulation {
    let mut sim = Simulation::invocation_fixture(9, Execution::default()).unwrap();
    let state = sim.canonical_state();
    let actor = state.state.ruler;
    let life = game_lifecycle::life(state.components, actor).unwrap();
    let rules = &state.state.development.as_ref().unwrap().rules;
    let child = life.born.0 + rules.infancy.0;
    let adult = life.born.0 + rules.adulthood.0;
    let teacher = game_lineage::guardians(state.components)
        .map(|(id, _)| id)
        .next()
        .unwrap();
    sim.advance(SimDuration(child - sim.time().0)).unwrap();
    for _ in 0..2 {
        let mut complete = false;
        for _ in 0..12 {
            if matches!(
                game_spatial::location(
                    sim.canonical_state().components,
                    game_ecs::StableId(teacher.0)
                ),
                Some(Location::AtArea(_))
            ) {
                sim.debug_actor_command(teacher, Command::Rest).unwrap();
            }
            if sim
                .debug_actor_command(
                    actor,
                    Command::Learn {
                        teacher,
                        topic: LessonTopic::Invocation,
                    },
                )
                .is_ok()
                && matches!(
                    sim.canonical_state().components.activity(actor),
                    Some(Activity::Lesson { .. })
                )
            {
                sim.advance(MINUTE).unwrap();
                complete = true;
                break;
            }
            sim.advance(MINUTE).unwrap();
        }
        assert!(
            complete,
            "public lesson command must complete actual guidance"
        );
    }
    if sim.time().0 < adult {
        sim.advance(SimDuration(adult - sim.time().0)).unwrap();
    }
    let area = match game_spatial::location(
        sim.canonical_state().components,
        game_ecs::StableId(actor.0),
    )
    .unwrap()
    {
        Location::AtArea(area) => *area,
        _ => panic!("pupil remains stationary"),
    };
    sim.debug_add_inventory(
        Good::Food,
        game_cargo::InventoryLocation::Area(area),
        Quantity(100_000),
        Quantity(100_000),
        BTreeSet::from([actor]),
    )
    .unwrap();
    sim.debug_actor_command(actor, Command::Rest).unwrap();
    sim.advance(MINUTE).unwrap();
    sim
}
fn pending() -> Simulation {
    let mut sim = learned();
    let life = game_invocation::archives::<Command>(sim.canonical_state().components)
        .next()
        .unwrap()
        .0;
    send(
        &mut sim,
        InvocationAction::Activate {
            lives: BTreeSet::from([life]),
        },
    );
    sim
}
#[test]
fn native_session_and_save51_are_public_authority_without_runtime_shadows() {
    let mut sim = pending();
    let actor = sim.canonical_state().state.ruler;
    let state = sim.canonical_state();
    let row = game_invocation::session(state.components, actor).unwrap();
    assert_eq!(game_invocation::sessions(state.components).count(), 1);
    let warning = row.warning.as_ref().unwrap().id;
    assert_eq!(
        game_invocation::warning(state.components, warning)
            .unwrap()
            .actor,
        actor
    );
    assert!(row.active.is_none());
    let bytes = sim.save().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["version"], 51);
    let control = &value["state"]["state"]["invocation"]["control"];
    assert_eq!(
        control.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["rules"]
    );
    assert_eq!(
        value["state"]["components"]["columns"]["invocation/session"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut restored = Simulation::load(
        &bytes,
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    for _ in 0..8 {
        restored.observe();
        let _ = restored
            .invocation_preview(row.activation.as_ref().unwrap().lives.clone())
            .unwrap();
    }
    assert_eq!(restored.save().unwrap(), bytes);
    for sim in [&mut sim, &mut restored] {
        send(sim, InvocationAction::AcceptWarning { warning });
        let state = sim.canonical_state();
        assert!(
            game_invocation::session(state.components, actor)
                .unwrap()
                .active
                .is_some()
        );
        send(sim, InvocationAction::Dismiss);
        assert!(game_invocation::session(sim.canonical_state().components, actor).is_none());
        assert!(game_invocation::burden(sim.canonical_state().components, actor).is_some());
        assert!(game_invocation::warning(sim.canonical_state().components, warning).is_some());
    }
    assert_eq!(sim.save().unwrap(), restored.save().unwrap());
}
#[test]
fn public_restore_rejects_runtime_shadows_orphans_and_pending_session_aliases() {
    let sim = pending();
    let bytes = sim.save().unwrap();
    let base: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let actor = sim.canonical_state().state.ruler.0;
    for attack in 0..8 {
        let mut value = base.clone();
        match attack {
            0 => value["version"] = serde_json::json!(50),
            1 => {
                value["state"]["state"]["invocation"]["control"]["active"] = serde_json::Value::Null
            }
            2 => {
                value["state"]["state"]["invocation"]["control"]["current_actor"] =
                    serde_json::json!(actor)
            }
            3 => {
                value["state"]["components"]["columns"]["invocation/session"][0]["entity"] =
                    serde_json::json!(999_999)
            }
            4 => {
                value["state"]["components"]["columns"]["invocation/session"][0]["value"]["actor"] =
                    serde_json::json!(999_999)
            }
            5 => {
                value["state"]["components"]["columns"]["invocation/session"][0]["value"]["warning"]
                    ["id"] = serde_json::json!(999_999)
            }
            6 => {
                value["state"]["components"]["columns"]["invocation/session"][0]["value"]["warning"]
                    ["activation"] = serde_json::json!([])
            }
            _ => {
                let row = value["state"]["components"]["columns"]["invocation/session"][0].clone();
                value["state"]["components"]["columns"]["invocation/session"]
                    .as_array_mut()
                    .unwrap()
                    .push(row);
            }
        }
        assert!(
            Simulation::load(&serde_json::to_vec(&value).unwrap(), Execution::default()).is_err(),
            "attack {attack}"
        );
        assert_eq!(sim.save().unwrap(), bytes);
    }
}
