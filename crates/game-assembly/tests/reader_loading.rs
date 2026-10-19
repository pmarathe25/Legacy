use game_assembly::Simulation;
#[test]
fn streaming_and_string_loads_preserve_every_field_and_legacy_migration() {
    let sim = Simulation::new("kernel-fixture", 1, 1).unwrap();
    let saved = sim.save().unwrap();
    let loaded = Simulation::load_from(&mut std::io::Cursor::new(saved.as_bytes()), 4).unwrap();
    assert_eq!(loaded.save().unwrap(), saved);
    let mut old: serde_json::Value = serde_json::from_str(&saved).unwrap();
    old["plugins"]
        .as_object_mut()
        .unwrap()
        .remove("communication");
    old["plugins"].as_object_mut().unwrap().remove("signals");
    old["resources"]
        .as_object_mut()
        .unwrap()
        .remove("communication.experience");
    old["resources"]
        .as_object_mut()
        .unwrap()
        .remove("signals.experience");
    let old = old.to_string();
    let expected = Simulation::load(&old, 1).unwrap().save().unwrap();
    let streamed = Simulation::load_from(&mut std::io::Cursor::new(old.as_bytes()), 4).unwrap();
    assert_eq!(streamed.save().unwrap(), expected);
}
