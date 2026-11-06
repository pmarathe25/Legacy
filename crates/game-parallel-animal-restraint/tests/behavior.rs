use game_parallel_animal_restraint::{Context, EventKind, Manual, Policy, Restraints};
use game_parallel_economy_person::Person;
use game_parallel_fauna::{Animal, Fauna, SocialResponse, Species, restraint};
use game_parallel_genetics::{Genome, Sex};

fn fixture(count: u64, shared: bool) -> (Vec<Person>, Fauna, Restraints) {
    let people = (1..=count)
        .map(|id| Person {
            id,
            region: Some(1),
            health: 100,
            hunger: 0,
            immortal: None,
            willing_to_work: false,
            seek_meals: false,
            controlled: false,
        })
        .collect();
    let animals = if shared { 1 } else { count };
    let fauna = Fauna {
        last_tick: 0,
        next_id: animals + 1,
        species: vec![Species {
            id: 1,
            name: "animal".into(),
            food_good: 1,
            intake: 1,
            hunger_per_tick: 1,
            relief_per_unit: 1,
            starvation_threshold: 100,
            starvation_damage: 1,
            max_health: 10,
            adult_age: 1,
            fertile_until: 10,
            lifespan: 20,
            gestation: 2,
            conception_one_in: 100,
            breeding_max_hunger: 10,
            newborn_health: 10,
            mutation_per_million: 0,
            mutation_step: 0,
            climate: None,
            hunting: None,
            newborn_social_response: None,
            newborn_handling_response: None,
            newborn_restraint_response: None,
        }],
        animals: (1..=animals)
            .map(|id| Animal {
                id,
                species: 1,
                region: 1,
                age: 2,
                health: 10,
                hunger: 0,
                genome: Genome {
                    sex: Sex::Male,
                    alleles: [[500; 2]; 4],
                },
                pregnancy: None,
                parents: None,
                social_response: Some(SocialResponse {
                    feed_affinity: 0,
                    minimum: -10,
                    maximum: 10,
                    capacity: 4,
                }),
                handling_response: None,
                restraint_response: Some(restraint::Response {
                    yield_willingly: false,
                    yield_affinity_at_least: Some(2),
                    yield_affinity_at_most: None,
                    resistance: 3,
                    escape_willing: true,
                    escape_effort: 1,
                    on_yielded: -1,
                    on_forced: -3,
                    on_resisted: 2,
                    on_released: 1,
                }),
                opinions: vec![],
            })
            .collect(),
    };
    let owner = Restraints {
        default_effort: 2,
        capacity_per_handler: 1,
        holds: vec![],
        events: vec![],
        policies: (1..=count)
            .map(|person| Policy {
                person,
                animal: Some(if shared { 1 } else { person }),
                willing: true,
                affinity_at_least: None,
                affinity_at_most: None,
                effort: 2,
                pressured_by: None,
                pressure_threshold: None,
            })
            .collect(),
    };
    (people, fauna, owner)
}

fn prepare(
    owner: &Restraints,
    fauna: &Fauna,
    people: &[Person],
    tick: u64,
    manual: Option<Manual>,
) -> game_parallel_animal_restraint::Plan {
    Context {
        owner: Some(owner),
        fauna: Some(fauna),
        people,
        opinions: None,
        affinity: None,
        busy: &[],
        blocked: &[],
        max_hunger: 100,
        seed: 7,
        tick,
        manual,
    }
    .prepare()
    .unwrap()
}

#[test]
fn an_animal_can_resist_then_yield_and_escape_without_losing_identity() {
    let (people, mut fauna, owner) = fixture(1, true);
    let first = prepare(&owner, &fauna, &people, 1, None);
    assert_eq!(
        first.state.as_ref().unwrap().events[0].kind,
        EventKind::Resisted
    );
    restraint::apply_reactions(&mut fauna, &first.reactions);
    assert_eq!(fauna.animals[0].opinions[0].affinity, 2);
    let second = prepare(first.state.as_ref().unwrap(), &fauna, &people, 2, None);
    assert_eq!(
        second.state.as_ref().unwrap().events[0].kind,
        EventKind::Yielded
    );
    assert!(second.state.as_ref().unwrap().held(1));
    restraint::apply_reactions(&mut fauna, &second.reactions);
    assert_eq!(fauna.animals[0].opinions[0].affinity, 1);
    let third = prepare(second.state.as_ref().unwrap(), &fauna, &people, 3, None);
    assert_eq!(
        third.state.as_ref().unwrap().events[0].kind,
        EventKind::Struggled
    );
    let fourth = prepare(third.state.as_ref().unwrap(), &fauna, &people, 4, None);
    assert_eq!(
        fourth.state.as_ref().unwrap().events[0].kind,
        EventKind::Escaped
    );
    assert!(!fourth.state.as_ref().unwrap().held(1));
    assert_eq!((fauna.animals[0].id, fauna.animals[0].health), (1, 10));
    let saved: Restraints =
        serde_json::from_str(&serde_json::to_string(fourth.state.as_ref().unwrap()).unwrap())
            .unwrap();
    assert_eq!(&saved, fourth.state.as_ref().unwrap());
}

#[test]
fn force_can_overcome_resistance_and_another_person_can_release() {
    let (mut people, fauna, mut owner) = fixture(2, true);
    owner.policies[0].effort = 4;
    owner.policies[1].willing = false;
    let captured = prepare(&owner, &fauna, &people, 1, None);
    assert_eq!(
        captured.state.as_ref().unwrap().events[0].kind,
        EventKind::Forced
    );
    people[1].controlled = true;
    let released = prepare(
        captured.state.as_ref().unwrap(),
        &fauna,
        &people,
        2,
        Some(Manual::Release {
            person: 2,
            animal: 1,
        }),
    );
    assert_eq!(
        released.state.as_ref().unwrap().events[0].kind,
        EventKind::Released
    );
    assert!(!released.state.as_ref().unwrap().held(1));
    let mut departed = captured.state.unwrap();
    people[0].region = None;
    departed.reconcile(&people, &fauna, &|_| true, 2);
    assert!(!departed.held(1));
    assert!(
        departed
            .events
            .iter()
            .any(|event| event.kind == EventKind::LetGo)
    );
    assert_eq!(fauna.animals[0].region, 1);
}

#[test]
fn concentrated_and_independent_captures_are_deterministic() {
    for shared in [false, true] {
        let count = 4096;
        let (people, mut fauna, mut owner) = fixture(count, shared);
        for animal in &mut fauna.animals {
            animal.restraint_response.as_mut().unwrap().yield_willingly = true;
        }
        owner
            .normalize_and_validate(&people, Some(&fauna), 0)
            .unwrap();
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| prepare(&owner, &fauna, &people, 1, None))
        };
        let one = run(1);
        let four = run(4);
        assert_eq!(one.actors.len(), count as usize);
        assert_eq!(
            one.state.as_ref().unwrap().holds.len(),
            if shared { 1 } else { count as usize }
        );
        assert_eq!(one.state, four.state);
        assert_eq!(one.actors, four.actors);
    }
}
