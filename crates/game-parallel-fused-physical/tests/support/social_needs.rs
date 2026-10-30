use game_parallel::{Person, SocialParticipant, World};
use game_parallel_fused_physical::Kernel;
#[test]
fn isolation_and_encounters_match_reference_and_reload() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))
    .unwrap();
    let mut fused = Kernel::compile(world.clone()).unwrap();
    for tick in 0..8 {
        world.step().unwrap();
        fused.step().unwrap();
        assert_eq!(fused.export(), world);
        if tick == 3 {
            let saved = serde_json::to_string(&fused.export()).unwrap();
            fused = Kernel::compile(World::from_json(&saved).unwrap()).unwrap();
        }
    }
}
#[test]
fn large_social_needs_are_worker_invariant() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))
    .unwrap();
    world.regions[0].people = (1..=5000)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    world.rules.hunger_per_tick = 0;
    world.social.as_mut().unwrap().participants = (1..=5000)
        .map(|person| SocialParticipant {
            person,
            willing: person % 3 != 0,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                for _ in 0..4 {
                    kernel.step().unwrap();
                }
                kernel.export()
            })
    };
    let serial = run(1);
    let parallel = run(4);
    assert_eq!(serial, parallel);
    parallel.validate().unwrap();
}

#[test]
fn sparse_and_reordered_knowledge_matches_reference_after_reload() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/knowledge.toml")).unwrap();
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
        region.people.reverse();
    }
    for person in &mut world.social.as_mut().unwrap().participants {
        person.person = remap(person.person);
    }
    let knowledge = world.knowledge.as_mut().unwrap();
    for mind in &mut knowledge.minds {
        mind.person = remap(mind.person);
        for record in &mut mind.records {
            record.witness = remap(record.witness);
            record.learned_from = record.learned_from.map(remap);
        }
    }
    knowledge.minds.reverse();
    world.validate().unwrap();
    let mut fused = Kernel::compile(world.clone()).unwrap();
    for tick in 0..6 {
        world.step().unwrap();
        fused.step().unwrap();
        let mut expected = world.clone();
        let mut actual = fused.export();
        for state in [&mut expected, &mut actual] {
            for region in &mut state.regions {
                region.people.sort_unstable_by_key(|person| person.id);
            }
            state
                .knowledge
                .as_mut()
                .unwrap()
                .minds
                .sort_unstable_by_key(|mind| mind.person);
        }
        assert_eq!(actual, expected);
        if tick == 2 {
            let saved = serde_json::to_string(&fused.export()).unwrap();
            fused = Kernel::compile(World::from_json(&saved).unwrap()).unwrap();
        }
    }
}
