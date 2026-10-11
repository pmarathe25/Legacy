use crate::{Execution, Simulation};

#[test]
fn native_project_columns_restore_and_runtime_rejects_entity_map_shadows() {
    let sim = Simulation::project_fixture(73, Execution::default()).unwrap();
    let before = sim.save().unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let runtime = &saved["state"]["state"]["projects"];
    assert_eq!(runtime.as_object().unwrap().len(), 2);
    assert!(runtime.get("rules").is_some());
    assert!(runtime.get("last_event").is_some());
    for column in [
        "project",
        "enterprise",
        "construction",
        "knowledge",
        "requests",
    ] {
        assert!(saved["state"]["components"]["columns"][format!("projects/{column}")].is_array());
    }
    let restored = Simulation::load(&before, Execution::default()).unwrap();
    assert_eq!(restored.canonical_state(), sim.canonical_state());
    assert_eq!(restored.save().unwrap(), before);
    for field in [
        "projects",
        "enterprises",
        "construction",
        "known_projects",
        "known_enterprises",
        "timber_hints",
        "next_search",
    ] {
        assert!(runtime.get(field).is_none());
        let mut forged = saved.clone();
        forged["state"]["state"]["projects"][field] = serde_json::json!({});
        assert!(
            Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err(),
            "project runtime shadow {field}"
        );
    }
}

#[test]
fn project_restore_rejects_missing_runtime_wrong_identity_and_empty_personal_orphans() {
    let mut sim = Simulation::project_fixture(79, Execution::default()).unwrap();
    let sponsor = sim.host.state.people[3].id;
    let proposal = sim
        .project_opportunities(sponsor, None)
        .into_iter()
        .find(|command| {
            matches!(
                command,
                game_api::Command::Project {
                    action: game_api::ProjectAction::Propose { .. }
                }
            )
        })
        .expect("fixture supplies a real locally known workshop proposal");
    sim.debug_actor_command(sponsor, proposal).unwrap();
    let before = sim.save().unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&before).unwrap();
    assert!(
        !saved["state"]["components"]["columns"]["projects/project"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let area = sim.host.state.areas[0].id.0;
    for attack in 0..5 {
        let mut forged = saved.clone();
        let columns = &mut forged["state"]["components"]["columns"];
        match attack {
            0 => forged["state"]["state"]["projects"] = serde_json::Value::Null,
            1 => columns["projects/project"][0]["value"]["id"] = serde_json::json!(999999),
            2 => columns["projects/project"][0]["value"]["sponsor"] = serde_json::json!(area),
            3 => columns["projects/knowledge"].as_array_mut().unwrap().push(serde_json::json!({"entity":area,"value":{"projects":{},"enterprises":{},"timber_hints":[]}})),
            _ => columns["projects/requests"].as_array_mut().unwrap().push(serde_json::json!({"entity":area,"value":{"next_search":0}})),
        }
        assert!(
            Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err(),
            "project reference attack {attack}"
        );
        assert_eq!(sim.save().unwrap(), before);
    }
}

#[test]
fn workshop_sites_use_one_saved_recipe_authority_and_reject_numeric_shadows() {
    let sim = Simulation::survival_fixture(83, Execution::default()).unwrap();
    let before = sim.save().unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let rows = saved["state"]["components"]["columns"]["workshop/site"]
        .as_array()
        .unwrap();
    assert!(!rows.is_empty());
    let recipes = saved["state"]["state"]["workshops"]["catalog"]["recipes"]
        .as_object()
        .unwrap();
    for row in rows {
        let site = &row["value"];
        assert!(recipes.contains_key(site["recipe"].as_str().unwrap()));
        for field in ["input_quantity", "output_quantity", "duration"] {
            assert!(site.get(field).is_none());
        }
    }
    let restored = Simulation::load(&before, Execution::default()).unwrap();
    assert_eq!(restored.canonical_state(), sim.canonical_state());
    for field in ["input_quantity", "output_quantity", "duration"] {
        let mut forged = saved.clone();
        forged["state"]["components"]["columns"]["workshop/site"][0]["value"][field] =
            serde_json::json!(1);
        assert!(
            Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err(),
            "site numeric shadow {field}"
        );
    }
    let mut forged = saved.clone();
    forged["state"]["components"]["columns"]["workshop/site"][0]["value"]["recipe"] =
        serde_json::json!("missing-recipe");
    assert!(Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err());
    let mut forged = saved.clone();
    let selected = rows[0]["value"]["recipe"].as_str().unwrap();
    forged["state"]["state"]["workshops"]["catalog"]["recipes"][selected]["duration"] =
        serde_json::json!(0);
    assert!(Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err());
    assert_eq!(sim.save().unwrap(), before);
}
