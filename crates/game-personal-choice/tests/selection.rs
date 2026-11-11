use game_personal_choice::{ActionPreference, Key, Person, Preference, Proposal, select};

fn profile(id: u64, construction: i64, carrying: i64) -> Person {
    Person {
        id,
        idle: 0,
        act_on_tie: false,
        actions: vec![],
        preferences: vec![
            Preference {
                domain: 1,
                enabled: true,
                bias: construction,
                weight: 1000,
            },
            Preference {
                domain: 2,
                enabled: true,
                bias: carrying,
                weight: 1000,
            },
        ],
    }
}
fn proposal(actor: u64, domain: u32, action: u64, value: i64) -> Proposal {
    Proposal {
        actor,
        key: Key { domain, action },
        value,
    }
}

#[test]
fn each_person_compares_opportunities_with_their_own_idle_preference() {
    let mut people = vec![profile(1, 5, 0), profile(2, 0, 5), profile(3, 0, 0)];
    people[2].preferences[1].enabled = false;
    let candidates = vec![
        proposal(1, 1, 10, 1),
        proposal(1, 2, 20, 1),
        proposal(2, 1, 10, 1),
        proposal(2, 2, 20, 1),
        proposal(3, 1, 10, 0),
        proposal(3, 2, 20, 100),
    ];
    let result = select(&people, &candidates, 7, 2).unwrap();
    assert_eq!(
        result[0].action,
        Some(Key {
            domain: 1,
            action: 10
        })
    );
    assert_eq!(
        result[1].action,
        Some(Key {
            domain: 2,
            action: 20
        })
    );
    assert_eq!(result[2].action, None);
    assert_eq!(
        serde_json::from_str::<Vec<Person>>(&serde_json::to_string(&people).unwrap()).unwrap(),
        people
    );
}

#[test]
fn ties_and_worker_counts_do_not_depend_on_input_order() {
    let people: Vec<_> = (1..=10_000).map(|id| profile(id, 0, 0)).collect();
    let mut candidates: Vec<_> = people
        .iter()
        .flat_map(|p| [proposal(p.id, 1, 10, 10), proposal(p.id, 2, 20, 10)])
        .collect();
    let run = |workers, candidates: &[Proposal]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| select(&people, candidates, 7, 13).unwrap())
    };
    let original = run(1, &candidates);
    candidates.reverse();
    assert_eq!(original, run(4, &candidates));
}

#[test]
fn malformed_or_duplicate_proposals_fail_without_selecting() {
    let people = [profile(1, i64::MAX, i64::MIN)];
    assert!(select(&people, &[proposal(2, 1, 1, 0)], 1, 1).is_err());
    assert!(select(&people, &[proposal(1, 1, 1, 0), proposal(1, 1, 1, 5)], 1, 1).is_err());
    let excessive: Vec<_> = (1..=65).map(|action| proposal(1, 1, action, 1)).collect();
    assert!(select(&people, &excessive, 1, 1).is_err());
    let result = select(
        &people,
        &[proposal(1, 1, 1, i64::MAX), proposal(1, 2, 1, i64::MIN)],
        1,
        1,
    )
    .unwrap();
    assert_eq!(
        result[0].action,
        Some(Key {
            domain: 1,
            action: 1
        })
    );
}

#[test]
fn people_can_value_the_same_consequence_in_opposite_ways() {
    let mut people = vec![profile(1, 0, 0), profile(2, 0, 0)];
    people[0].preferences[0].weight = 1000;
    people[1].preferences[0].weight = -1000;
    let outcomes = select(
        &people,
        &[proposal(1, 1, 1, 20), proposal(2, 1, 1, 20)],
        1,
        1,
    )
    .unwrap();
    assert_eq!(
        outcomes[0].action,
        Some(Key {
            domain: 1,
            action: 1
        })
    );
    assert_eq!(outcomes[1].action, None);
}

#[test]
fn people_can_prefer_or_reject_specific_opportunities_in_the_same_domain() {
    let mut people = vec![profile(1, 0, 0), profile(2, 0, 0)];
    people[0].actions = vec![
        ActionPreference {
            key: Key {
                domain: 1,
                action: 10,
            },
            enabled: true,
            bias: -20,
        },
        ActionPreference {
            key: Key {
                domain: 1,
                action: 11,
            },
            enabled: true,
            bias: 20,
        },
    ];
    people[1].actions = vec![
        ActionPreference {
            key: Key {
                domain: 1,
                action: 10,
            },
            enabled: true,
            bias: 20,
        },
        ActionPreference {
            key: Key {
                domain: 1,
                action: 11,
            },
            enabled: false,
            bias: 100,
        },
    ];
    let candidates = vec![
        proposal(1, 1, 10, 1),
        proposal(1, 1, 11, 1),
        proposal(2, 1, 10, 1),
        proposal(2, 1, 11, 1),
    ];
    let selected = select(&people, &candidates, 1, 1).unwrap();
    assert_eq!(selected[0].action.unwrap().action, 11);
    assert_eq!(selected[1].action.unwrap().action, 10);
    let saved = serde_json::to_string(&people).unwrap();
    assert_eq!(serde_json::from_str::<Vec<Person>>(&saved).unwrap(), people);
    people[0].actions.reverse();
    assert!(select(&people, &candidates, 1, 1).is_err());
}

#[test]
fn whether_to_act_when_indifferent_is_personal() {
    let mut people = vec![profile(1, 0, 0), profile(2, 0, 0)];
    people[1].act_on_tie = true;
    let selected = select(&people, &[proposal(1, 1, 1, 0), proposal(2, 1, 1, 0)], 1, 1).unwrap();
    assert_eq!(selected[0].action, None);
    assert_eq!(
        selected[1].action,
        Some(Key {
            domain: 1,
            action: 1
        })
    );
}

#[test]
#[ignore = "performance measurement"]
fn parallel_personal_choice_benchmark() {
    let people: Vec<_> = (1..=100_000).map(|id| profile(id, 0, 0)).collect();
    let proposals: Vec<_> = people
        .iter()
        .flat_map(|p| [proposal(p.id, 1, 10, 10), proposal(p.id, 2, 20, 10)])
        .collect();
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let start = std::time::Instant::now();
            let result = pool.install(|| select(&people, &proposals, 7, 13).unwrap());
            times.push(start.elapsed());
            if let Some(expected) = &reference {
                assert_eq!(&result, expected);
            } else {
                reference = Some(result);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {} people",
            times[1],
            people.len()
        );
    }
}
