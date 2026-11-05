use crate::Engine;
use game_parallel::World;

#[test]
fn comparison_hosts_reject_preset_institution_bodies_without_mutation() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/institutions.toml"
    ))
    .unwrap();
    let before = world.clone();
    assert_eq!(
        world.step(),
        Err("preset institutions are not supported by the parallel simulation")
    );
    assert_eq!(world, before);
    let error = match Engine::compile(world) {
        Ok(_) => panic!("preset institutions must be rejected"),
        Err(error) => error,
    };
    assert!(error.contains("preset institutions"));
}
