use game_assembly::Simulation;
#[test]
fn older_character_checkpoint_defaults_only_the_transient_interaction_resource() {
    let current = Simulation::new("kernel-fixture", 29, 1)
        .unwrap()
        .save()
        .unwrap();
    let mut old: serde_json::Value = serde_json::from_str(&current).unwrap();
    assert_eq!(
        old["resources"]
            .as_object_mut()
            .unwrap()
            .remove("characters.local-interaction-phase"),
        Some(serde_json::json!({}))
    );
    let source = old.to_string();
    for workers in [1, 4] {
        assert_eq!(
            Simulation::load(&source, workers).unwrap().save().unwrap(),
            current
        );
        assert_eq!(
            Simulation::load_from(&mut std::io::Cursor::new(source.as_bytes()), workers)
                .unwrap()
                .save()
                .unwrap(),
            current
        );
    }
    // The compatibility exception must not become a general missing-state default.
    old["resources"]
        .as_object_mut()
        .unwrap()
        .remove("world.actions");
    let corrupt = old.to_string();
    assert!(Simulation::load(&corrupt, 1).is_err());
    assert!(Simulation::load_from(&mut std::io::Cursor::new(corrupt.as_bytes()), 1).is_err());
}
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

#[test]
fn streaming_migrates_removed_memory_resources_but_rejects_unknown_current_state() {
    let sim = Simulation::new("kernel-fixture", 3, 1).unwrap();
    let mut saved: serde_json::Value = serde_json::from_str(&sim.save().unwrap()).unwrap();
    saved["resources"]["unrecognized.owner"] = serde_json::json!({});
    let invalid = saved.to_string();
    assert!(Simulation::load_from(&mut std::io::Cursor::new(invalid.as_bytes()), 4).is_err());
    saved["resources"]
        .as_object_mut()
        .unwrap()
        .remove("unrecognized.owner");
    saved["plugins"].as_object_mut().unwrap().remove("memory");
    saved["resources"]
        .as_object_mut()
        .unwrap()
        .remove("knowledge.perception");
    saved["resources"]["knowledge.facts"]
        .as_object_mut()
        .unwrap()
        .remove("cognition");
    saved["resources"]["recollection.episodes"] = serde_json::json!({});
    let old = saved.to_string();
    let expected = Simulation::load(&old, 1).unwrap().save().unwrap();
    let streamed = Simulation::load_from(&mut std::io::Cursor::new(old.as_bytes()), 4).unwrap();
    assert_eq!(streamed.save().unwrap(), expected);
    let mut reloaded = Simulation::load(&expected, 1).unwrap();
    while reloaded
        .advance(game_core::SimTime(3 * game_core::DAY.0), 100)
        .unwrap()
        .stop
        == game_sim::Stop::Budget
    {}
    assert!(Simulation::load(&reloaded.save().unwrap(), 4).is_ok());
}
