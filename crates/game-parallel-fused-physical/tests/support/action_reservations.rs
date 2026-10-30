use game_parallel::{CropPhase, KnowledgeRecord, Person, SocialParticipant, World};
use game_parallel_fused_physical::Kernel;

fn agriculture() -> game_parallel::Agriculture {
    World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture
        .unwrap()
}

fn run(world: &World, workers: usize) -> World {
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut kernel = Kernel::compile(world.clone()).unwrap();
            kernel.step().unwrap();
            let result = kernel.export();
            result.validate().unwrap();
            result
        })
}

#[test]
fn teaching_encounter_reserves_both_people_from_farm_work() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/auto_teaching.toml"
    ))
    .unwrap();
    world.agriculture = Some(agriculture());
    let state = run(&world, 1);
    assert_eq!(state, run(&world, 4));
    let lessons = &state.skills.as_ref().unwrap().lessons;
    assert_eq!((lessons[0].teacher, lessons[0].student), (3, 4));
    assert!(
        state.agriculture.as_ref().unwrap().farms[0]
            .fields
            .iter()
            .all(|field| field.phase == CropPhase::Fallow)
    );
}

fn traveler_world() -> World {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/autonomy.toml")).unwrap();
    world
        .knowledge
        .as_mut()
        .unwrap()
        .minds
        .iter_mut()
        .find(|mind| mind.person == 2)
        .unwrap()
        .records
        .push(KnowledgeRecord {
            region: 2,
            food: 50,
            observed_at: 0,
            witness: 3,
            learned_from: None,
        });
    world
        .knowledge
        .as_mut()
        .unwrap()
        .minds
        .iter_mut()
        .find(|mind| mind.person == 2)
        .unwrap()
        .records
        .push(KnowledgeRecord {
            region: 1,
            food: 0,
            observed_at: 0,
            witness: 2,
            learned_from: None,
        });
    world
}

#[test]
fn social_encounter_reserves_person_from_autonomous_departure() {
    let mut world = traveler_world();
    world.regions[0].people.push(Person {
        id: 4,
        health: 100,
        hunger: 0,
    });
    let social = world.social.as_mut().unwrap();
    social.participants[0].willing = false;
    social.participants.push(SocialParticipant {
        person: 4,
        willing: false,
    });
    let baseline = run(&world, 1);
    assert!(
        baseline
            .travelers
            .iter()
            .any(|traveler| traveler.person.id == 2)
    );
    world
        .social
        .as_mut()
        .unwrap()
        .participants
        .last_mut()
        .unwrap()
        .willing = true;
    let state = run(&world, 1);
    assert_eq!(state, run(&world, 4));
    assert!(
        state
            .social
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| (event.a, event.b) == (2, 4))
    );
    assert!(
        !state
            .travelers
            .iter()
            .any(|traveler| traveler.person.id == 2)
    );
}

#[test]
fn autonomous_departure_reserves_person_from_farm_work() {
    let mut world = traveler_world();
    world.social = None;
    world.regions[0].food = 4;
    world.agriculture = Some(agriculture());
    let state = run(&world, 1);
    assert_eq!(state, run(&world, 4));
    assert!(
        state
            .travelers
            .iter()
            .any(|traveler| traveler.person.id == 2)
    );
    let fields = &state.agriculture.as_ref().unwrap().farms[0].fields;
    assert_eq!(
        fields
            .iter()
            .filter(|field| field.phase != CropPhase::Fallow)
            .count(),
        1
    );
}

#[test]
fn configured_social_interval_leaves_time_for_other_work() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/auto_teaching.toml"
    ))
    .unwrap();
    world.agriculture = Some(agriculture());
    world.social.as_mut().unwrap().rules.interval = 2;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel.step().unwrap();
                let first = kernel.export();
                assert!(
                    first.agriculture.as_ref().unwrap().farms[0]
                        .fields
                        .iter()
                        .all(|field| field.phase == CropPhase::Growing)
                );
                assert!(first.social.as_ref().unwrap().events.is_empty());
                kernel.step().unwrap();
                let second = kernel.export();
                assert_eq!(
                    second.agriculture.as_ref().unwrap().farms[0].fields,
                    first.agriculture.as_ref().unwrap().farms[0].fields
                );
                assert_eq!(second.skills.as_ref().unwrap().lessons.len(), 1);
                second
            })
    };
    assert_eq!(run(1), run(4));
}
