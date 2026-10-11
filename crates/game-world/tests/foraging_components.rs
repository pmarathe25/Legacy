use game_api::Error;
use game_world::{Execution, Simulation};

#[test]
fn future_personal_resource_observations_are_rejected_from_the_actual_component_column() {
    let sim = Simulation::ecology_fixture(21, Execution::default()).unwrap();
    let actor = sim.canonical_state().ruler;
    let before = sim.save().unwrap();
    let mut saved: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let now = sim.canonical_state().time.0;
    let row = saved["state"]["components"]["columns"]["ecology/knowledge"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["entity"].as_u64() == Some(actor.0))
        .unwrap();
    let observation = row["value"]["known_resources"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    observation["observed_at"] = (now + 1).into();
    assert!(matches!(
        Simulation::load(&serde_json::to_vec(&saved).unwrap(), Execution::default()),
        Err(Error::InvalidSave(_))
    ));
    assert_eq!(sim.save().unwrap(), before);
}
