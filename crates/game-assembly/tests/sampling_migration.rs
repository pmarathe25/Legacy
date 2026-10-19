use game_assembly::Simulation;
#[test]
fn existing_personal_samples_follow_their_owner_when_food_inquiry_is_introduced() {
    let mut sim = Simulation::uncontrolled(3, 1, &mut game_content::Store::standard()).unwrap();
    let person = *sim
        .debug_host()
        .get::<game_physiology::Bodies>()
        .unwrap()
        .people
        .keys()
        .next()
        .unwrap();
    let inventory = sim
        .debug_host()
        .get::<game_cargo::Cargo>()
        .unwrap()
        .carried(person)
        .next()
        .unwrap();
    sim.debug_host_mut()
        .dispatch(game_observations::Inspect { person, inventory })
        .unwrap();
    let expected = sim
        .debug_host()
        .get::<game_observations::Samples>()
        .unwrap()
        .probes
        .clone();
    assert!(!expected.is_empty());
    let mut old: serde_json::Value = serde_json::from_str(&sim.save().unwrap()).unwrap();
    old["plugins"]
        .as_object_mut()
        .unwrap()
        .remove("food-inquiry");
    let samples = old["resources"]
        .as_object_mut()
        .unwrap()
        .remove("food-inquiry.samples")
        .unwrap();
    for field in ["probes", "trials"] {
        old["resources"]["observations.episodes"][field] = samples[field].clone();
    }
    let loaded = Simulation::load(&old.to_string(), 4).unwrap();
    assert_eq!(
        loaded
            .debug_host()
            .get::<game_observations::Samples>()
            .unwrap()
            .probes,
        expected
    );
    let current: serde_json::Value = serde_json::from_str(&loaded.save().unwrap()).unwrap();
    assert!(
        current["resources"]["observations.episodes"]
            .get("probes")
            .is_none()
    );
}
