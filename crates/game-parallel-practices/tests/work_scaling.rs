use game_parallel_practices::{Case, Decision, Mind, Practices};

fn mind(person: u64) -> Mind {
    Mind {
        person,
        invent: true,
        approve: true,
        willing: true,
        ask_willing: true,
        adjust_after_failure: false,
        pressure_threshold: None,
        shortage_threshold: 5,
        minimum_shortages: 2,
        review_interval: 2,
        proposal_lifetime: 8,
        reserve_portions: 1,
        quantity_weight: 0,
        reserve_weight: 0,
        giving_experience_weight: 0,
        shortage_experience_weight: 0,
        proposal_affinity_weight: 0,
        acceptance_threshold: 0,
        value_learning: None,
        precedent: None,
        work: None,
        reporting: Default::default(),
        report_attitude: Default::default(),
        unanswered_report_belief: None,
        require_believed_ready: false,
        next_review: 0,
        sequence: 0,
        cases: vec![].into(),
        authored: vec![].into(),
        inbox: vec![].into(),
        sent_reports: vec![].into(),
        reports: vec![].into(),
    }
}

type Workload = (
    Practices,
    Vec<(u64, u32)>,
    Vec<game_parallel_practices::work::Witness>,
    Vec<(u64, u64)>,
);
fn paired_work_practices(count: u64) -> Workload {
    use game_parallel_practices::work::{Mind as WorkMind, Witness};
    assert_eq!(count % 2, 0);
    let work = std::sync::Arc::new(WorkMind {
        invent: true,
        approve: true,
        willing: true,
        ask_willing: true,
        review_interval: 2,
        proposal_lifetime: 8,
        reserve: 0,
        pressure_threshold: None,
        next_review: 0,
        sequence: 0,
        authored: None,
        engagement: None,
        gifts: vec![],
        inbox: vec![],
    });
    let mut minds: Vec<_> = (1..=count)
        .map(|person| {
            let mut individual = mind(person);
            individual.work = Some(work.clone());
            if person % 2 == 1 {
                individual.cases = vec![Case::Gave {
                    tick: 1,
                    recipient: person + 1,
                    good: 4,
                    quantity: 1,
                }]
                .into();
            }
            individual
        })
        .collect();
    for individual in minds.iter_mut().filter(|row| row.person % 2 == 0) {
        std::sync::Arc::make_mut(individual.work.as_mut().unwrap()).invent = false;
    }
    let goals = (1..count).step_by(2).map(|person| (person, 7)).collect();
    let witnesses = (1..count)
        .step_by(2)
        .map(|person| Witness {
            observer: person,
            worker: person + 1,
            recipe: 7,
            request: person,
            job: person,
            tick: 1,
        })
        .collect();
    let contacts = (1..count)
        .step_by(2)
        .map(|person| (person, person + 1))
        .collect();
    (Practices { minds }, goals, witnesses, contacts)
}

fn run_work_practices(
    initial: &Practices,
    goals: &[(u64, u32)],
    witnesses: &[game_parallel_practices::work::Witness],
    contacts: &[(u64, u64)],
) -> Practices {
    use game_parallel_practices::work::{Context, advance};
    let mut next = initial.clone();
    advance(
        initial,
        &mut next,
        Context {
            goals,
            witnesses,
            contacts,
            tick: 2,
            can_decide: |_| true,
            pressure: |_, _| 0,
        },
    )
    .unwrap();
    let previous = next;
    let mut next = previous.clone();
    advance(
        &previous,
        &mut next,
        Context {
            goals,
            witnesses,
            contacts: &[],
            tick: 3,
            can_decide: |_| true,
            pressure: |_, _| 0,
        },
    )
    .unwrap();
    next
}

#[test]
fn many_independent_work_proposals_are_worker_invariant() {
    let (initial, goals, witnesses, contacts) = paired_work_practices(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| run_work_practices(&initial, &goals, &witnesses, &contacts))
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert_eq!(
        result
            .minds
            .iter()
            .filter(|person| person
                .work
                .as_ref()
                .unwrap()
                .inbox
                .first()
                .is_some_and(|item| item.decision == Decision::Adopted))
            .count(),
        2048
    );
}

#[test]
fn crowded_work_inbox_remains_bounded_and_worker_invariant() {
    let (initial, goals, mut witnesses, mut contacts) = paired_work_practices(4096);
    for witness in &mut witnesses {
        witness.worker = 2;
    }
    for contact in &mut contacts {
        contact.1 = 2;
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| run_work_practices(&initial, &goals, &witnesses, &contacts))
    };
    let result = run(1);
    assert_eq!(result, run(4));
    let inbox = &result.minds[1].work.as_ref().unwrap().inbox;
    assert_eq!(inbox.len(), game_parallel_practices::MAX_PROPOSALS);
    assert!(inbox.iter().all(|item| item.decision == Decision::Adopted));
    assert_eq!(
        result
            .minds
            .iter()
            .filter(|mind| mind.work.as_ref().unwrap().authored.is_some())
            .count(),
        game_parallel_practices::MAX_PROPOSALS
    );
}

#[test]
fn personal_non_food_gift_memories_are_bounded_and_deterministic() {
    use game_parallel_practices::work::{Gift, GiftObservation, remember_gifts};
    use std::collections::HashSet;
    let (initial, _, _, _) = paired_work_practices(2);
    let observations: Vec<_> = (1..=40)
        .rev()
        .map(|tick| GiftObservation {
            person: 1,
            gift: Gift {
                tick,
                other: 2,
                good: 2,
                quantity: tick,
                given: true,
            },
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = initial.clone();
                remember_gifts(&mut next, &observations);
                next
            })
    };
    let next = run(1);
    assert_eq!(next, run(4));
    let gifts = &next.minds[0].work.as_ref().unwrap().gifts;
    assert_eq!(gifts.len(), game_parallel_practices::MAX_CASES);
    assert_eq!(
        (gifts.first().unwrap().tick, gifts.last().unwrap().tick),
        (25, 40)
    );
    next.validate_with_goods(
        &HashSet::from([1, 2]),
        &HashSet::from([4]),
        &HashSet::from([2, 4]),
        40,
    )
    .unwrap();
    assert!(
        next.validate_with_goods(
            &HashSet::from([1, 2]),
            &HashSet::from([4]),
            &HashSet::from([4]),
            40
        )
        .is_err()
    );
}

#[test]
#[ignore = "performance measurement"]
fn independent_work_practice_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let (initial, goals, mut witnesses, mut contacts) = paired_work_practices(count);
    let concentrated = std::env::var_os("LEGACY_CONCENTRATED").is_some();
    if concentrated {
        for witness in &mut witnesses {
            witness.worker = 2;
        }
        for contact in &mut contacts {
            contact.1 = 2;
        }
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            let mut creates = Vec::new();
            let mut assesses = Vec::new();
            for round in 0..4 {
                let started = std::time::Instant::now();
                let mut next = initial.clone();
                game_parallel_practices::work::advance(&initial, &mut next, game_parallel_practices::work::Context {
                    goals: &goals, witnesses: &witnesses, contacts: &contacts, tick: 2,
                    can_decide: |_| true, pressure: |_, _| 0,
                }).unwrap();
                let created = started.elapsed().as_secs_f64();
                let previous = next;
                let mut result = previous.clone();
                game_parallel_practices::work::advance(&previous, &mut result, game_parallel_practices::work::Context {
                    goals: &goals, witnesses: &witnesses, contacts: &[], tick: 3,
                    can_decide: |_| true, pressure: |_, _| 0,
                }).unwrap();
                if round > 0 {
                    let elapsed = started.elapsed().as_secs_f64();
                    samples.push(elapsed);
                    creates.push(created);
                    assesses.push(elapsed - created);
                }
                assert_eq!(
                    result
                        .minds
                        .iter()
                        .flat_map(|person| person.work.as_ref().unwrap().inbox.iter())
                        .filter(|item| item.decision == Decision::Adopted)
                        .count(),
                    if concentrated { game_parallel_practices::MAX_PROPOSALS.min(count as usize / 2) } else { count as usize / 2 }
                );
            }
            samples.sort_by(f64::total_cmp);
            creates.sort_by(f64::total_cmp);
            assesses.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} concentrated={concentrated} boundaries=2 elapsed={:.6} create={:.6} assess={:.6}",
                samples[1], creates[1], assesses[1]
            );
        });
}
