use game_parallel::{Knowledge, KnowledgeRecord, Mind, World};
use game_parallel_fused_physical::Kernel;
#[test]
fn persistent_knowledge_is_private_when_later_lifecycle_preparation_fails() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/enrollment.toml")).unwrap();
    world.families.as_mut().unwrap().totals.partnerships = u64::MAX;
    let mut kernel = Kernel::compile(world.clone()).unwrap();
    assert_eq!(kernel.prepare().err(), Some("family counter overflow"));
    assert_eq!(kernel.export(), world);
    assert_eq!(kernel.prepare().err(), Some("family counter overflow"));
    assert_eq!(kernel.export(), world);
}
#[test]
fn borrowed_past_life_memories_survive_publication_and_reload() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/invocation.toml")).unwrap();
    world.knowledge = Some(Knowledge {
        max_records: 4,
        minds: vec![Mind {
            person: 1,
            records: vec![KnowledgeRecord {
                region: 2,
                food: 77,
                observed_at: 0,
                witness: 1,
                learned_from: None,
            }],
        }],
    });
    let mut kernel = Kernel::compile(world).unwrap();
    let mut ready = false;
    for _ in 0..100 {
        kernel.step().unwrap();
        let snapshot = kernel.export();
        let lineage = snapshot.lineage.as_ref().unwrap();
        if lineage.current.is_some() && lineage.history.iter().any(|life| life.person == 1) {
            ready = true;
            break;
        }
    }
    assert!(ready);
    kernel.invoke_lives(&[1]).unwrap();
    let expected: Vec<_> = kernel.export().knowledge.unwrap().minds[0]
        .records
        .iter()
        .cloned()
        .map(|record| (1, record))
        .collect();
    assert!(!expected.is_empty());
    assert_eq!(kernel.borrowed_memories().unwrap(), expected);
    kernel.step().unwrap();
    assert_eq!(kernel.borrowed_memories().unwrap(), expected);
    let saved = kernel.export();
    let mut reload =
        Kernel::compile(World::from_json(&serde_json::to_string(&saved).unwrap()).unwrap())
            .unwrap();
    assert_eq!(reload.borrowed_memories().unwrap(), expected);
    assert_eq!(reload.export(), saved);
}
