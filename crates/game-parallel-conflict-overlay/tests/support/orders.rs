use crate::Plan;
use game_parallel_conflict::{Command, Conflict, Encounter, Fighter, Order};
fn fixture() -> Conflict {
    Conflict {
        damage: 9,
        next_id: 2,
        events: vec![],
        encounters: vec![Encounter {
            id: 1,
            region: 7,
            fighters: [
                (10, 1, Order::Attack { target: 90 }),
                (20, 1, Order::Attack { target: 90 }),
                (90, 2, Order::Attack { target: 10 }),
                (100, 2, Order::Defend),
            ]
            .into_iter()
            .map(|(person, side, order)| Fighter {
                person,
                side,
                order,
            })
            .collect(),
        }],
    }
}
fn command(person: u64, order: Order) -> Command {
    Command {
        encounter: 1,
        person,
        order,
    }
}
fn compare(plan: &Plan, conflict: &Conflict, tick: u64) -> Vec<game_parallel_conflict::Event> {
    let mut people: Vec<_> = conflict
        .encounters
        .iter()
        .flat_map(|fight| {
            fight
                .fighters
                .iter()
                .map(|fighter| (fighter.person, Some(fight.region), 100, 0))
        })
        .collect();
    people.sort_unstable_by_key(|person| person.0);
    let expected = game_parallel_conflict::prepare(conflict, &people, tick);
    let round = plan.prepare_with_filter(tick, |_, _| true, |_, _| true);
    assert_eq!(round.expand(), expected.events);
    assert_eq!(round.events().collect::<Vec<_>>(), expected.events);
    assert_eq!(round.event_count(), expected.events.len() as u64);
    let mut totals = std::collections::BTreeMap::<_, u32>::new();
    for event in &expected.events {
        let value = totals.entry(event.target).or_default();
        *value = value.saturating_add(event.damage);
    }
    assert_eq!(
        round
            .damage()
            .iter()
            .map(|damage| (damage.target, damage.amount))
            .collect::<Vec<_>>(),
        totals.into_iter().collect::<Vec<_>>()
    );
    expected.events
}

#[test]
fn bounded_overlay_requests_rebuild_and_keeps_old_receipts() {
    let mut conflict = Conflict {
        damage: 7,
        next_id: 2,
        events: vec![],
        encounters: vec![Encounter {
            id: 1,
            region: 7,
            fighters: (0..5000)
                .map(|person| Fighter {
                    person,
                    side: (person % 2) as u32,
                    order: Order::Attack { target: person ^ 1 },
                })
                .collect(),
        }],
    };
    let mut plan = Plan::compile(&conflict).unwrap();
    let old = plan.prepare_with_filter(1, |_, _| true, |_, _| true);
    let events = old.expand();
    let commands: Vec<_> = (0..4097)
        .map(|person| command(person, Order::Defend))
        .collect();
    conflict.order_sorted(&commands).unwrap();
    assert!(!plan.orders(&conflict, &commands));
    plan = Plan::compile(&conflict).unwrap();
    compare(&plan, &conflict, 2);
    assert_eq!(old.expand(), events);
}

#[test]
fn multiple_encounter_overlays_preserve_saturated_damage_and_sparse_receipts() {
    let mut conflict = fixture();
    conflict.damage = u32::MAX;
    conflict.next_id = 3;
    conflict.encounters.push(Encounter {
        id: 2,
        region: 9,
        fighters: vec![
            Fighter {
                person: 1000,
                side: 1,
                order: Order::Attack { target: 9000 },
            },
            Fighter {
                person: 9000,
                side: 2,
                order: Order::Defend,
            },
        ],
    });
    let mut plan = Plan::compile(&conflict).unwrap();
    let commands = [
        command(100, Order::Attack { target: 10 }),
        Command {
            encounter: 2,
            person: 9000,
            order: Order::Attack { target: 1000 },
        },
    ];
    conflict.order_sorted(&commands).unwrap();
    assert!(plan.orders(&conflict, &commands));
    compare(&plan, &conflict, 1);
}
#[test]
fn persistent_retargets_defense_changes_and_old_receipts_match_full_rebuild() {
    for workers in [1, 2, 4, 6] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut conflict = fixture();
                let mut plan = Plan::compile(&conflict).unwrap();
                let old = plan.prepare_with_filter(1, |_, _| true, |_, _| true);
                let old_events = old.expand();
                let commands = [
                    command(10, Order::Attack { target: 100 }),
                    command(100, Order::Attack { target: 20 }),
                    command(20, Order::Defend),
                    command(10, Order::Defend),
                    command(10, Order::Attack { target: 90 }),
                    command(20, Order::Attack { target: 100 }),
                ];
                for (i, command) in commands.iter().enumerate() {
                    let discarded = plan.prepare_with_filter(2, |_, _| true, |_, _| true);
                    let prior = discarded.expand();
                    conflict
                        .order_sorted(std::slice::from_ref(command))
                        .unwrap();
                    assert!(plan.orders(&conflict, std::slice::from_ref(command)));
                    compare(&plan, &conflict, i as u64 + 2);
                    compare(&plan, &conflict, i as u64 + 10);
                    assert_eq!(discarded.expand(), prior);
                    assert_eq!(old.expand(), old_events);
                }
            });
    }
}
#[test]
fn casualties_filter_overlay_targets_and_cascade_defense_without_changing_old_receipts() {
    let mut conflict = fixture();
    let mut plan = Plan::compile(&conflict).unwrap();
    let commands = [
        command(10, Order::Attack { target: 100 }),
        command(100, Order::Attack { target: 20 }),
    ];
    conflict.order_sorted(&commands).unwrap();
    assert!(plan.orders(&conflict, &commands));
    let old = plan.prepare_with_filter(1, |_, _| true, |_, _| true);
    let events = old.expand();
    game_parallel_conflict::finish(&mut conflict, |person| person != 100);
    plan = plan.filtered(|person| person != 100, |_, person| person == 10);
    compare(&plan, &conflict, 2);
    assert_eq!(old.expand(), events);
}
