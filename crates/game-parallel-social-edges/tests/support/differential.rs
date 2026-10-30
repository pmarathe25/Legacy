use crate::Runtime;
use game_parallel_social::{Contact, Participant, Relationship, Rules, Social};
fn rules(retention: Option<u64>, interval: u64) -> Rules {
    Rules {
        interval,
        min_health: 1,
        max_hunger: 10,
        familiarity_gain: 3,
        familiarity_max: 20,
        relationship_retention: retention,
    }
}
fn reference(count: u64, retention: Option<u64>, interval: u64) -> (Social, Vec<Contact>) {
    let people: Vec<_> = (1..=count)
        .map(|person| Participant {
            person: person * 19,
            willing: true,
        })
        .collect();
    let contacts = people
        .iter()
        .map(|p| Contact {
            person: p.person,
            region: Some(1),
            health: 100,
            hunger: 0,
            busy: false,
        })
        .collect();
    (
        Social {
            rules: rules(retention, interval),
            participants: people,
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        },
        contacts,
    )
}

#[test]
fn concentrated_graph_matches_reference_with_retention_and_reload() {
    for workers in [1, 4] {
        for retention in [None, Some(0), Some(3)] {
            for interval in [1, 3] {
                rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let (mut expected,mut contacts)=reference(2050,retention,interval);
            let mut runtime=Runtime::compile_validated(&expected.relationships,expected.rules.clone(),0).unwrap();
            for tick in 1..=15 {
                for (i,c) in contacts.iter_mut().enumerate() {c.busy=(i+tick as usize).is_multiple_of(7);}
                let next=expected.prepare(&contacts,42,tick).unwrap();
                        let selection = expected.select_validated(&contacts,42,tick, |_, _| true).unwrap();
                        assert_eq!(selection.events,next.events);
                        assert_eq!(selection.needs,next.needs);
                let draft=runtime.prepare_validated(&selection.events,tick).unwrap();
                assert_eq!(runtime.export(),expected.relationships);
                runtime.commit(draft).unwrap();
                expected=next;
                assert_eq!(runtime.export(),expected.relationships,"tick={tick} workers={workers} retention={retention:?} interval={interval}");
                assert!(runtime.expiry_len()<=runtime.live_len());
                if tick==8 {runtime=Runtime::compile_validated(&runtime.export(),expected.rules.clone(),tick).unwrap();}
            }
        });
            }
        }
    }
}
#[test]
fn expiry_precedes_renewal_and_overflow_never_publishes() {
    let (mut owner, contacts) = reference(2, None, 1);
    owner.relationships = vec![Relationship {
        a: 19,
        b: 38,
        familiarity: 5,
        encounters: u64::MAX,
        last_met: 0,
    }];
    let event = game_parallel_social::Encounter {
        a: 19,
        b: 38,
        region: 1,
        tick: 1,
        pressured: None,
    };
    let mut runtime =
        Runtime::compile_validated(&owner.relationships, owner.rules.clone(), 0).unwrap();
    assert!(
        runtime
            .prepare_validated(std::slice::from_ref(&event), 1)
            .is_err()
    );
    assert_eq!(runtime.export(), owner.relationships);
    owner.rules.relationship_retention = Some(0);
    let mut runtime =
        Runtime::compile_validated(&owner.relationships, owner.rules.clone(), 0).unwrap();
    let expected = owner.prepare(&contacts, 1, 1).unwrap();
    let draft = runtime.prepare_validated(&expected.events, 1).unwrap();
    runtime.commit(draft).unwrap();
    assert_eq!(runtime.export(), expected.relationships);
    assert_eq!(runtime.get(19, 38).unwrap().encounters, 1);
    assert_eq!(runtime.expiry_len(), 1);
}
#[test]
fn authored_order_discarded_generations_and_reused_slots_stay_independent() {
    let rows = vec![
        Relationship {
            a: 3,
            b: 4,
            familiarity: 1,
            encounters: 1,
            last_met: 0,
        },
        Relationship {
            a: 1,
            b: 2,
            familiarity: 1,
            encounters: 1,
            last_met: 0,
        },
    ];
    let mut runtime = Runtime::compile_validated(&rows, rules(Some(1), 3), 0).unwrap();
    let mut other = Runtime::compile_validated(&rows, rules(Some(1), 3), 0).unwrap();
    let foreign = other.prepare_validated(&[], 1).unwrap();
    assert!(runtime.commit(foreign).is_err());
    let old = runtime.prepare_validated(&[], 1).unwrap();
    let current = runtime.prepare_validated(&[], 1).unwrap();
    assert!(runtime.commit(old).is_err());
    assert_eq!(runtime.export(), rows);
    runtime.commit(current).unwrap();
    assert_eq!(runtime.export(), rows);
    let draft = runtime.prepare_validated(&[], 2).unwrap();
    drop(draft);
    assert_eq!(runtime.export(), rows);
    let draft = runtime.prepare_validated(&[], 2).unwrap();
    runtime.commit(draft).unwrap();
    assert!(runtime.export().is_empty());
    let slots = runtime.allocated_slots();
    let events = vec![
        game_parallel_social::Encounter {
            tick: 3,
            region: 1,
            a: 1,
            b: 2,
            pressured: None,
        },
        game_parallel_social::Encounter {
            tick: 3,
            region: 1,
            a: 3,
            b: 4,
            pressured: None,
        },
    ];
    let draft = runtime.prepare_validated(&events, 3).unwrap();
    runtime.commit(draft).unwrap();
    assert_eq!(runtime.allocated_slots(), slots);
    assert_eq!(
        runtime
            .export()
            .iter()
            .map(|r| (r.a, r.b))
            .collect::<Vec<_>>(),
        vec![(1, 2), (3, 4)]
    );
}
#[test]
fn selection_stages_needs_without_publishing_or_hiding_edge_failure() {
    let (mut owner, contacts) = reference(2, None, 1);
    owner.needs = Some(game_parallel_social::Needs {
        rules: game_parallel_social::NeedRules {
            capacity: 100,
            isolation_gain: 3,
            encounter_relief: 5,
        },
        people: vec![
            game_parallel_social::Need {
                person: 38,
                isolation: 9,
            },
            game_parallel_social::Need {
                person: 19,
                isolation: 1,
            },
        ],
    });
    let before = owner.clone();
    let selection = owner
        .select_validated(&contacts, 42, 1, |_, _| true)
        .unwrap();
    let full = owner.prepare(&contacts, 42, 1).unwrap();
    assert_eq!(selection.events, full.events);
    assert_eq!(selection.needs, full.needs);
    assert_eq!(owner, before);
    owner.relationships = vec![Relationship {
        a: 19,
        b: 38,
        last_met: 0,
        encounters: u64::MAX,
        familiarity: 1,
    }];
    assert!(
        owner
            .select_validated(&contacts, 42, 1, |_, _| true)
            .is_ok()
    );
    assert!(owner.prepare(&contacts, 42, 1).is_err());
    owner.rules.interval = 3;
    let selection = owner
        .select_validated(&contacts, 42, 1, |_, _| true)
        .unwrap();
    assert!(selection.events.is_empty());
    assert_eq!(
        selection.needs,
        owner.prepare(&contacts, 42, 1).unwrap().needs
    );
}
#[test]
fn expiry_overflow_keeps_edges_until_the_last_representable_tick() {
    let row = Relationship {
        a: 1,
        b: 2,
        familiarity: 1,
        encounters: 1,
        last_met: u64::MAX - 2,
    };
    let mut runtime =
        Runtime::compile_validated(std::slice::from_ref(&row), rules(Some(3), 1), u64::MAX - 2)
            .unwrap();
    assert_eq!(runtime.expiry_len(), 0);
    for tick in [u64::MAX - 1, u64::MAX] {
        let draft = runtime.prepare_validated(&[], tick).unwrap();
        runtime.commit(draft).unwrap();
        assert_eq!(runtime.export(), vec![row.clone()]);
    }
    assert!(runtime.prepare_validated(&[], 0).is_err());
}
#[test]
fn indexed_busy_membership_matches_reference_across_chunks_and_unsorted_inputs() {
    let (social, mut contacts) = reference(10_000, Some(10), 1);
    let mut busy: Vec<_> = contacts
        .iter()
        .enumerate()
        .filter_map(|(index, row)| (index % 3 == 0).then_some(row.person))
        .collect();
    busy.reverse();
    busy.extend_from_within(..100);
    for contact in &mut contacts {
        contact.busy = busy.contains(&contact.person);
    }
    let expected = social
        .select_validated(&contacts, 42, 1, |_, _| true)
        .unwrap()
        .events;
    let people: Vec<_> = contacts
        .iter()
        .map(|c| (c.person, c.region, c.health, c.hunger))
        .collect();
    for workers in [1, 4] {
        for reverse in [false, true] {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut people = people.clone();
                    if reverse {
                        people.reverse();
                    }
                    let mut runtime =
                        Runtime::compile_validated(&[], social.rules.clone(), 0).unwrap();
                    let actual = runtime
                        .prepare_indexed_with_extra(
                            &social,
                            &people,
                            &busy,
                            (42, 1),
                            &[],
                            |_, _| true,
                        )
                        .unwrap();
                    assert_eq!(actual.events, expected);
                });
        }
    }
}
/// Focused membership/selection workload, excluded from routine tests.
#[test]
#[ignore = "performance measurement"]
fn busy_selection_benchmark() {
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(1);
    let count: u64 = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(200_000);
    let (social, contacts) = reference(count, None, 1);
    let people: Vec<_> = contacts
        .iter()
        .map(|row| (row.person, row.region, row.health, row.hunger))
        .collect();
    let busy: Vec<_> = people
        .iter()
        .enumerate()
        .filter_map(|(i, row)| (i % 2 == 0).then_some(row.0))
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    pool.install(|| {
        let mut runtime = Runtime::compile_validated(&[], social.rules.clone(), 0).unwrap();
        let start = std::time::Instant::now();
        for _ in 0..5 {
            let selected = runtime
                .prepare_indexed_with_extra(&social, &people, &busy, (42, 1), &[], |_, _| true)
                .unwrap();
            assert_eq!(selected.events.len(), count as usize / 4);
            std::hint::black_box(selected);
        }
        println!(
            "workers={workers} people={count} preparations=5 seconds={:.6}",
            start.elapsed().as_secs_f64()
        );
    });
}
