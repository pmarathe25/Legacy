use game_parallel::{Move, World};
use game_parallel_fused_physical::Kernel;
#[test]
fn initial_family_order_and_superseded_scratch_survive_split_travel_and_reload() {
    let mut initial =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    initial.families.as_mut().unwrap().members.reverse();
    initial.families.as_mut().unwrap().events = vec![game_parallel::FamilyEvent {
        tick: 999,
        kind: "authored-extension".into(),
        people: vec![8, 3, 8, 1],
        region: Some(777),
    }];
    let mut kernel = Kernel::compile(initial.clone()).unwrap();
    assert_eq!(kernel.export(), initial);
    let first = kernel.prepare().unwrap();
    let second = kernel.prepare().unwrap();
    assert!(kernel.commit(first).is_err());
    assert_eq!(kernel.export(), initial);
    kernel.commit(second).unwrap();
    kernel
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    let mut reload = Kernel::compile(
        World::from_json(&serde_json::to_string(&kernel.export()).unwrap()).unwrap(),
    )
    .unwrap();
    for _ in 0..12 {
        kernel.step().unwrap();
        reload.step().unwrap();
        assert_eq!(kernel.export(), reload.export());
        kernel.export().validate().unwrap();
    }
}
