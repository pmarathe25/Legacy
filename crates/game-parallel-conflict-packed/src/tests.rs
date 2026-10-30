use crate::{Damage, Plan};
use game_parallel_conflict::{Conflict, Encounter, Fighter, Order};
use game_parallel_production_model::PersonSnapshot;
use std::collections::BTreeMap;

fn compare(conflict: &Conflict, people: &[PersonSnapshot]) {
    let expected = game_parallel_conflict::prepare(conflict, people, 7);
    let plan = Plan::compile(conflict).unwrap();
    let round = plan.prepare(people, 7);
    assert_eq!(round.expand(), expected.events);
    assert_eq!(round.events().collect::<Vec<_>>(), expected.events);
    assert_eq!(round.event_count(), expected.events.len() as u64);
    let mut totals = BTreeMap::<u64, u32>::new();
    for event in &expected.events {
        let value = totals.entry(event.target).or_default();
        *value = value.saturating_add(event.damage);
    }
    let damage: Vec<_> = totals
        .into_iter()
        .map(|(target, amount)| Damage { target, amount })
        .collect();
    assert_eq!(round.damage(), damage);
}

#[test]
fn sparse_multiple_encounters_keep_exact_attribution_and_defense() {
    let conflict = Conflict {
        damage: 9,
        next_id: 8,
        events: vec![],
        encounters: vec![
            Encounter {
                id: 7,
                region: 2,
                fighters: vec![
                    Fighter {
                        person: 900,
                        side: 2,
                        order: Order::Defend,
                    },
                    Fighter {
                        person: 100,
                        side: 1,
                        order: Order::Attack { target: 900 },
                    },
                    Fighter {
                        person: 700,
                        side: 1,
                        order: Order::Attack { target: 900 },
                    },
                ],
            },
            Encounter {
                id: 2,
                region: 1,
                fighters: vec![
                    Fighter {
                        person: 60,
                        side: 1,
                        order: Order::Attack { target: 20 },
                    },
                    Fighter {
                        person: 20,
                        side: 2,
                        order: Order::Attack { target: 60 },
                    },
                ],
            },
        ],
    };
    let people = vec![
        (20, Some(1), 100, 0),
        (60, Some(1), 100, 0),
        (100, Some(2), 100, 0),
        (700, Some(2), 100, 0),
        (900, Some(2), 100, 0),
    ];
    compare(&conflict, &people);
    let mut unavailable = people;
    unavailable[0].2 = 0;
    unavailable[3].1 = None;
    compare(&conflict, &unavailable);
}

fn concentrated(count: usize) -> (Conflict, Vec<PersonSnapshot>) {
    let mut fighters: Vec<_> = (1..=count as u64)
        .map(|person| Fighter {
            person,
            side: 1,
            order: Order::Attack {
                target: count as u64 + 1,
            },
        })
        .collect();
    fighters.push(Fighter {
        person: count as u64 + 1,
        side: 2,
        order: Order::Retreat,
    });
    let people = (1..=count as u64 + 1)
        .map(|id| (id, Some(1), if id % 13 == 0 { 0 } else { 100 }, 0))
        .collect();
    (
        Conflict {
            damage: u32::MAX,
            next_id: 2,
            encounters: vec![Encounter {
                id: 1,
                region: 1,
                fighters,
            }],
            events: vec![],
        },
        people,
    )
}

#[test]
fn hot_targets_cross_word_chunk_boundaries_without_losing_saturated_receipts() {
    for count in [1, 63, 64, 65, 2047, 2048, 2049, 20_001] {
        let (conflict, people) = concentrated(count);
        compare(&conflict, &people);
    }
}

#[test]
fn variable_strikes_and_later_protection_keep_totals_and_receipts_consistent() {
    let (conflict, people) = concentrated(4097);
    let plan = Plan::compile(&conflict).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut round = plan.prepare(&people, 5);
                round.vary_damage(|attacker, _| (attacker % 5 + 1) as u32);
                round.protect(|_| 2);
                let events = round.expand();
                assert!(
                    events
                        .iter()
                        .all(|e| e.damage == ((e.attacker % 5 + 1) as u32).saturating_sub(2))
                );
                let total: u32 = events.iter().map(|e| e.damage).sum();
                assert_eq!(round.damage()[0].amount, total);
                (round.damage().to_vec(), events)
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn worker_count_does_not_change_compact_rounds() {
    let (conflict, people) = concentrated(100_001);
    let plan = Plan::compile(&conflict).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let round = plan.prepare(&people, 5);
                (round.damage().to_vec(), round.expand())
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn receipts_keep_their_original_generation_after_order_changes() {
    let (mut conflict, people) = concentrated(100);
    let plan = Plan::compile(&conflict).unwrap();
    let round = plan.prepare(&people, 8);
    let expected = round.expand();
    conflict.encounters[0].fighters[0].order = Order::Defend;
    let replacement = Plan::compile(&conflict).unwrap();
    assert_eq!(
        replacement.prepare(&people, 9).event_count() + 1,
        round.event_count()
    );
    drop(plan);
    assert_eq!(round.expand(), expected);
}

#[test]
fn empty_rounds_and_invalid_targets_are_handled() {
    let empty = Conflict {
        damage: 1,
        next_id: 1,
        encounters: vec![],
        events: vec![],
    };
    compare(&empty, &[]);
    let (mut conflict, _) = concentrated(2);
    conflict.encounters[0].fighters[0].order = Order::Attack { target: 999 };
    assert!(Plan::compile(&conflict).is_err());
}

#[test]
fn optional_strike_filters_are_ephemeral_and_filtered_generations_keep_old_receipts() {
    let (mut conflict, _) = concentrated(100);
    conflict.damage = 10;
    let plan = Plan::compile(&conflict).unwrap();
    let first = plan.prepare_with(1, |_, _| true);
    assert_eq!(first.event_count(), 100);
    let blocked = plan.prepare_with_filter(2, |_, _| true, |_, _| false);
    assert_eq!(blocked.event_count(), 0);
    assert_eq!(plan.prepare_with(3, |_, _| true).event_count(), 100);
    let filtered = plan.filtered(
        |id| id != 2,
        |encounter, target| encounter == 1 && target == 101,
    );
    let next = filtered.prepare_with(4, |_, _| true);
    assert_eq!(next.event_count(), 99);
    assert!(
        next.events()
            .all(|event| event.damage == 5 && event.attacker != 2)
    );
    assert!(first.events().all(|event| event.damage == 10));
    let later = filtered.filtered(|id| id != 3, |_, _| false);
    assert!(
        later
            .prepare_with(5, |_, _| true)
            .events()
            .all(|event| event.damage == 5)
    );
    assert_eq!(first.event_count(), 100);
}
