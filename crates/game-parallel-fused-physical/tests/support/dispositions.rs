use game_parallel::{KnowledgeRecord, World};
use game_parallel_fused_physical::Kernel;

#[test]
fn disposition_affects_fused_travel_without_revealing_remote_state() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/dispositions.toml"
    ))
    .unwrap();
    let records = &mut world
        .knowledge
        .as_mut()
        .unwrap()
        .minds
        .iter_mut()
        .find(|mind| mind.person == 2)
        .unwrap()
        .records;
    *records = vec![
        KnowledgeRecord {
            region: 1,
            food: 0,
            observed_at: 0,
            witness: 2,
            learned_from: None,
        },
        KnowledgeRecord {
            region: 2,
            food: 5,
            observed_at: 0,
            witness: 3,
            learned_from: Some(1),
        },
    ];
    let mut reference = world.clone();
    let mut fused = Kernel::compile(world).unwrap();
    reference.step().unwrap();
    fused.step().unwrap();
    assert_eq!(fused.export(), reference);
    assert!(reference.travelers.is_empty());
}

#[test]
fn sparse_autonomous_actors_use_personal_reports_consistently_across_reload() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/autonomy.toml")).unwrap();
    let remap = |id| match id {
        1 => 10,
        2 => 100,
        3 => 1000,
        _ => unreachable!(),
    };
    for region in &mut world.regions {
        for person in &mut region.people {
            person.id = remap(person.id);
        }
    }
    for participant in &mut world.social.as_mut().unwrap().participants {
        participant.person = remap(participant.person);
    }
    for actor in &mut world.autonomy.as_mut().unwrap().actors {
        actor.person = remap(actor.person);
    }
    for mind in &mut world.knowledge.as_mut().unwrap().minds {
        mind.person = remap(mind.person);
        for record in &mut mind.records {
            record.witness = remap(record.witness);
            record.learned_from = record.learned_from.map(remap);
        }
    }
    world
        .knowledge
        .as_mut()
        .unwrap()
        .minds
        .iter_mut()
        .find(|mind| mind.person == 100)
        .unwrap()
        .records = vec![
        KnowledgeRecord {
            region: 1,
            food: 0,
            observed_at: 0,
            witness: 100,
            learned_from: None,
        },
        KnowledgeRecord {
            region: 2,
            food: 50,
            observed_at: 0,
            witness: 1000,
            learned_from: Some(10),
        },
    ];
    world.knowledge.as_mut().unwrap().minds.reverse();
    world.validate().unwrap();
    let mut fused = Kernel::compile(world.clone()).unwrap();
    for tick in 0..5 {
        world.step().unwrap();
        fused.step().unwrap();
        let mut expected = world.clone();
        let mut actual = fused.export();
        for state in [&mut expected, &mut actual] {
            for region in &mut state.regions {
                region.people.sort_unstable_by_key(|person| person.id);
            }
        }
        expected
            .knowledge
            .as_mut()
            .unwrap()
            .minds
            .sort_unstable_by_key(|mind| mind.person);
        actual
            .knowledge
            .as_mut()
            .unwrap()
            .minds
            .sort_unstable_by_key(|mind| mind.person);
        assert_eq!(actual, expected);
        if tick == 2 {
            let saved = serde_json::to_string(&fused.export()).unwrap();
            fused = Kernel::compile(World::from_json(&saved).unwrap()).unwrap();
        }
    }
}
