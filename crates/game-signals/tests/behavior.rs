use game_signals::{Catalog, Contact, Directed, Expression, Interpretation, Mind, Named, Signals};
use std::collections::HashSet;

fn initial() -> Signals {
    Signals {
        catalog: Catalog {
            forms: vec![
                Named {
                    id: 1,
                    name: "cry".into(),
                },
                Named {
                    id: 2,
                    name: "gesture".into(),
                },
            ],
            meanings: vec![
                Named {
                    id: 1,
                    name: "food".into(),
                },
                Named {
                    id: 2,
                    name: "comfort".into(),
                },
            ],
        },
        minds: vec![
            Mind {
                person: 1,
                idle: 0,
                act_on_tie: false,
                attention: 1000,
                expressions: vec![Expression {
                    form: 1,
                    intended: Some(1),
                    recipient: None,
                    enabled: true,
                    hunger_at_least: None,
                    value: 10,
                    hunger_weight: 0,
                }],
                interpretations: vec![],
                remember: false,
                capacity: 0,
                observations: vec![],
            },
            Mind {
                person: 2,
                idle: 0,
                act_on_tie: false,
                attention: 1000,
                expressions: vec![],
                interpretations: vec![Interpretation {
                    form: 1,
                    meaning: 2,
                }],
                remember: true,
                capacity: 2,
                observations: vec![],
            },
        ],
        events: vec![],
    }
}

#[test]
fn an_expression_can_be_misleading_and_interpreted_differently() {
    let state = initial();
    state.validate(&HashSet::from([1, 2]), 0).unwrap();
    let next = state
        .prepare(
            &[Contact {
                a: 1,
                b: 2,
                a_hunger: 0,
                b_hunger: 0,
            }],
            7,
            1,
        )
        .unwrap();
    assert_eq!(next.events.len(), 1);
    assert!(next.events[0].noticed);
    assert_eq!(next.events[0].interpreted, Some(2));
    assert_eq!(next.minds[0].expressions[0].intended, Some(1));
    let memory = &next.minds[1].observations[0];
    assert_eq!(memory.interpreted, Some(2));
    assert!(
        serde_json::to_value(memory)
            .unwrap()
            .get("intended")
            .is_none()
    );
    assert_eq!(
        serde_json::from_str::<Signals>(&serde_json::to_string(&next).unwrap()).unwrap(),
        next
    );
    next.validate(&HashSet::from([1, 2]), 1).unwrap();
    let next = next
        .prepare(
            &[Contact {
                a: 1,
                b: 2,
                a_hunger: 0,
                b_hunger: 0,
            }],
            7,
            2,
        )
        .unwrap();
    let next = next
        .prepare(
            &[Contact {
                a: 1,
                b: 2,
                a_hunger: 0,
                b_hunger: 0,
            }],
            7,
            3,
        )
        .unwrap();
    assert_eq!(
        next.minds[1]
            .observations
            .iter()
            .map(|row| row.tick)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
}

#[test]
fn expression_attention_and_memory_are_independent_personal_choices() {
    let mut state = initial();
    let contact = [Contact {
        a: 1,
        b: 2,
        a_hunger: 0,
        b_hunger: 0,
    }];
    state.minds[0].expressions[0].hunger_at_least = Some(5);
    assert!(state.prepare(&contact, 7, 1).unwrap().events.is_empty());
    state.minds[0].expressions[0].hunger_at_least = None;
    state.minds[1].attention = 0;
    let unnoticed = state.prepare(&contact, 7, 1).unwrap();
    assert!(!unnoticed.events[0].noticed);
    assert!(unnoticed.minds[1].observations.is_empty());
    state.minds[1].attention = 1000;
    state.minds[1].remember = false;
    let forgotten = state.prepare(&contact, 7, 1).unwrap();
    assert!(forgotten.events[0].noticed);
    assert!(forgotten.minds[1].observations.is_empty());
    state.minds[0].expressions[0].enabled = false;
    assert!(state.prepare(&contact, 7, 1).unwrap().events.is_empty());
}

#[test]
fn invalid_or_overlapping_encounters_leave_state_unchanged() {
    let state = initial();
    let before = state.clone();
    let contact = Contact {
        a: 1,
        b: 2,
        a_hunger: 0,
        b_hunger: 0,
    };
    assert!(state.prepare(&[contact, contact], 7, 1).is_err());
    assert_eq!(state, before);
    let mut bad = state;
    bad.minds[0].expressions[0].intended = Some(99);
    assert!(bad.validate(&HashSet::from([1, 2]), 0).is_err());
}

#[test]
fn sparse_person_ids_use_the_same_personal_interpretations() {
    let mut state = initial();
    state.minds[0].person = 10;
    state.minds[1].person = 1_000_000;
    let next = state
        .prepare_with_directed(
            &[],
            &[Directed {
                actor: 10,
                observer: 1_000_000,
                hunger: 0,
            }],
            7,
            1,
        )
        .unwrap();
    // A directed signal requires this person's explicit audience preference.
    assert!(next.events.is_empty());
    state.minds[0].expressions[0].recipient = Some(1_000_000);
    let next = state
        .prepare_with_directed(
            &[],
            &[Directed {
                actor: 10,
                observer: 1_000_000,
                hunger: 0,
            }],
            7,
            1,
        )
        .unwrap();
    assert_eq!(next.events[0].interpreted, Some(2));
    assert_eq!(next.minds[1].observations[0].actor, 10);
}

#[test]
fn disjoint_personal_signal_choices_match_across_workers() {
    let template = initial();
    let mut state = Signals {
        catalog: template.catalog,
        minds: vec![],
        events: vec![],
    };
    for person in 1..=2_000 {
        let mut mind = template.minds[(person % 2 == 0) as usize].clone();
        mind.person = person;
        state.minds.push(mind);
    }
    let contacts: Vec<_> = (0..1_000)
        .map(|index| Contact {
            a: index * 2 + 1,
            b: index * 2 + 2,
            a_hunger: 0,
            b_hunger: 0,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| state.prepare(&contacts, 7, 1).unwrap())
    };
    let one = run(1);
    assert_eq!(one.events.len(), 1_000);
    assert_eq!(one, run(4));
}

#[test]
fn directed_signals_need_no_meeting_and_fan_in_keeps_bounded_personal_memory() {
    let template = initial();
    let mut state = Signals {
        catalog: template.catalog,
        minds: vec![],
        events: vec![],
    };
    for person in 1..=1_000 {
        let mut mind = template.minds[0].clone();
        mind.person = person;
        mind.expressions[0].recipient = Some(1_001);
        state.minds.push(mind);
    }
    let mut observer = template.minds[1].clone();
    observer.person = 1_001;
    state.minds.push(observer);
    let directed: Vec<_> = (1..=1_000)
        .map(|actor| Directed {
            actor,
            observer: 1_001,
            hunger: 0,
        })
        .collect();
    assert_eq!(state.directed_targets().len(), 1_000);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| state.prepare_with_directed(&[], &directed, 7, 1).unwrap())
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert_eq!(result.events.len(), 1_000);
    assert!(
        result
            .events
            .iter()
            .all(|event| event.observer == 1_001 && event.interpreted == Some(2))
    );
    assert_eq!(
        result
            .minds
            .last()
            .unwrap()
            .observations
            .iter()
            .map(|row| row.actor)
            .collect::<Vec<_>>(),
        vec![999, 1_000]
    );
}

#[test]
#[ignore = "personal signal scaling benchmark"]
fn independent_signals_benchmark() {
    let count: u64 = std::env::var("LEGACY_SIGNAL_PEOPLE")
        .ok()
        .map(|value| value.parse().expect("invalid population"))
        .unwrap_or(50_000);
    assert!(count > 0 && count.is_multiple_of(2));
    let template = initial();
    let mut state = Signals {
        catalog: template.catalog,
        minds: vec![],
        events: vec![],
    };
    for person in 1..=count {
        let mut mind = template.minds[(person % 2 == 0) as usize].clone();
        mind.person = person;
        state.minds.push(mind);
    }
    let contacts: Vec<_> = (0..count / 2)
        .map(|index| Contact {
            a: index * 2 + 1,
            b: index * 2 + 2,
            a_hunger: 0,
            b_hunger: 0,
        })
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
            let result = pool.install(|| state.prepare(&contacts, 7, 1).unwrap());
            times.push(start.elapsed());
            assert_eq!(result.events.len() as u64, count / 2);
            if let Some(expected) = &reference {
                assert_eq!(&result, expected);
            } else {
                reference = Some(result);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} people",
            times[1]
        );
    }
}

#[test]
#[ignore = "concentrated signal scaling benchmark"]
fn concentrated_signals_benchmark() {
    let count: u64 = std::env::var("LEGACY_SIGNAL_PEOPLE")
        .ok()
        .map(|value| value.parse().expect("invalid population"))
        .unwrap_or(50_000);
    assert!(count > 0);
    let template = initial();
    let mut state = Signals {
        catalog: template.catalog,
        minds: vec![],
        events: vec![],
    };
    for person in 1..=count {
        let mut mind = template.minds[0].clone();
        mind.person = person;
        mind.expressions[0].recipient = Some(count + 1);
        state.minds.push(mind);
    }
    let mut observer = template.minds[1].clone();
    observer.person = count + 1;
    state.minds.push(observer);
    let directed: Vec<_> = (1..=count)
        .map(|actor| Directed {
            actor,
            observer: count + 1,
            hunger: 0,
        })
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
            let result =
                pool.install(|| state.prepare_with_directed(&[], &directed, 7, 1).unwrap());
            times.push(start.elapsed());
            assert_eq!(result.events.len() as u64, count);
            assert_eq!(result.minds.last().unwrap().observations.len(), 2);
            if let Some(expected) = &reference {
                assert_eq!(&result, expected);
            } else {
                reference = Some(result);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} directed signals",
            times[1]
        );
    }
}
