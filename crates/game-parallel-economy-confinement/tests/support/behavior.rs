use super::*;
use game_parallel_work_sites::Site;

fn shared_barrier(count: u64) -> (Confinement, Vec<Person>, Facilities) {
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
    let owner = Confinement {
        guard_policies: vec![],
        cell_kinds: vec![],
        default_effort: 1,
        cells: vec![Cell {
            site: 1,
            resistance: count + 1,
        }],
        holds: (1..=count).map(|person| Hold { person, site: 1 }).collect(),
        escape: (1..=count)
            .map(|person| EscapePreference {
                person,
                willing: true,
                effort: 1,
            })
            .collect(),
        capture_responses: vec![],
        capture_policies: vec![],
        events: vec![],
    };
    let sites = Facilities {
        next_id: 2,
        sites: vec![Site {
            id: 1,
            region: 1,
            kind: "cell".into(),
            capacity: count,
        }],
        requirements: vec![],
        bindings: vec![],
    };
    (owner, people, sites)
}

fn shared_target(count: u64) -> (Confinement, Vec<Person>, Facilities) {
    let (mut owner, mut people, mut sites) = shared_barrier(count);
    let target = count + 1;
    people.push(Person {
        id: target,
        region: Some(1),
        health: 100,
        hunger: 0,
        immortal: None,
        willing_to_work: false,
        seek_meals: false,
        controlled: false,
    });
    sites.sites[0].capacity = 1;
    owner.holds.clear();
    owner.escape.clear();
    owner.capture_responses = vec![CaptureResponse {
        person: target,
        yield_willingly: true,
        pressure_threshold: None,
    }];
    owner.capture_policies = (1..=count)
        .map(|person| CapturePolicy {
            person,
            target,
            site: 1,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
        })
        .collect();
    (owner, people, sites)
}

fn shared_capacity(count: u64) -> (Confinement, Vec<Person>, Facilities) {
    let (mut owner, mut people, sites) = shared_barrier(count);
    people.extend((count + 1..=count * 2).map(|id| Person {
        id,
        region: Some(1),
        health: 100,
        hunger: 0,
        immortal: None,
        willing_to_work: false,
        seek_meals: false,
        controlled: false,
    }));
    owner.holds.clear();
    owner.escape.clear();
    owner.capture_responses = (count + 1..=count * 2)
        .map(|person| CaptureResponse {
            person,
            yield_willingly: true,
            pressure_threshold: None,
        })
        .collect();
    owner.capture_policies = (1..=count)
        .map(|person| CapturePolicy {
            person,
            target: person + count,
            site: 1,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
        })
        .collect();
    (owner, people, sites)
}

fn shared_guarding(count: u64) -> (Confinement, Vec<Person>, Facilities) {
    let (mut owner, people, sites) = shared_capacity(count);
    owner.capture_responses.clear();
    owner.capture_policies.clear();
    owner.holds = (count + 1..=count * 2)
        .map(|person| Hold { person, site: 1 })
        .collect();
    owner.escape = (count + 1..=count * 2)
        .map(|person| EscapePreference {
            person,
            willing: true,
            effort: 1,
        })
        .collect();
    owner.guard_policies = (1..=count)
        .map(|person| GuardPolicy {
            person,
            site: 1,
            willing: true,
            pressured_by: None,
            pressure_threshold: None,
        })
        .collect();
    (owner, people, sites)
}

#[test]
fn concurrent_guarding_and_escape_use_actual_personal_effort() {
    let (initial, people, sites) = shared_guarding(4096);
    initial.validate(&people, &sites, 0).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let plan = GuardContext {
                    owner: &initial,
                    people: &people,
                    sites: &sites,
                    busy: &[],
                    opinions: None,
                    manual: None,
                    tick: 1,
                }
                .prepare()
                .unwrap();
                assert_eq!(plan.actors.len(), 4096);
                assert_eq!(plan.strengths, vec![(1, 4096)]);
                let (next, attempts) = initial
                    .escape_attempts(&people, &[], 1, None, &plan.strengths)
                    .unwrap();
                assert_eq!(attempts.len(), 4096);
                assert_eq!(next.cells[0].resistance, 4097);
                assert_eq!(next.holds.len(), 4096);
                (next, plan.actors, plan.strengths, plan.events)
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "manual concentrated guarding benchmark"]
fn concentrated_guarding_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    let (initial, people, sites) = shared_guarding(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..5 {
                let now = std::time::Instant::now();
                let plan = GuardContext {
                    owner: &initial,
                    people: &people,
                    sites: &sites,
                    busy: &[],
                    opinions: None,
                    manual: None,
                    tick: 1,
                }
                .prepare()
                .unwrap();
                let (next, _) = initial
                    .escape_attempts(&people, &[], 1, None, &plan.strengths)
                    .unwrap();
                let elapsed = now.elapsed();
                assert_eq!(plan.strengths, vec![(1, count)]);
                assert_eq!(next.cells[0].resistance, count + 1);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort();
            println!(
                "workers={workers} guards={count} prisoners={count} elapsed_ms={:.3}",
                samples[samples.len() / 2].as_secs_f64() * 1000.0
            );
        });
}

#[test]
fn independent_captures_share_one_cell_without_overbooking() {
    let (initial, people, sites) = shared_capacity(4096);
    initial.validate(&people, &sites, 0).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut owner = initial.clone();
                let outcome = AutonomousContext {
                    owner: &mut owner,
                    people: &people,
                    sites: &sites,
                    busy: &[],
                    controlled_unavailable: &[],
                    opinions: None,
                    seed: 7,
                    tick: 1,
                }
                .prepare();
                assert_eq!(outcome.captured.len(), 4096);
                assert_eq!(outcome.participants.len(), 8192);
                assert_eq!(owner.holds.len(), 4096);
                owner.validate(&people, &sites, 1).unwrap();
                owner
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn limited_capacity_and_overlapping_people_are_resolved_without_duplicates() {
    let (initial, people, mut sites) = shared_capacity(4096);
    sites.sites[0].capacity = 1024;
    let run = |state: Confinement, workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut owner = state;
                let result = AutonomousContext {
                    owner: &mut owner,
                    people: &people,
                    sites: &sites,
                    busy: &[],
                    controlled_unavailable: &[],
                    opinions: None,
                    seed: 7,
                    tick: 1,
                }
                .prepare();
                assert!(result.participants.windows(2).all(|pair| pair[0] < pair[1]));
                assert!(result.captured.len() <= sites.sites[0].capacity as usize);
                owner.validate(&people, &sites, 1).unwrap();
                owner
            })
    };
    let state = run(initial.clone(), 1);
    assert_eq!(state.holds.len(), 1024);
    assert_eq!(state, run(initial.clone(), 4));
    let mut overlapping = initial;
    overlapping.capture_policies.truncate(4);
    for policy in &mut overlapping.capture_policies {
        policy.target = policy.person + 1;
    }
    overlapping.capture_responses = (2..=5)
        .map(|person| CaptureResponse {
            person,
            yield_willingly: true,
            pressure_threshold: None,
        })
        .collect();
    let one = run(overlapping.clone(), 1);
    assert!(!one.events.is_empty());
    assert_eq!(one, run(overlapping, 4));
}

#[test]
#[ignore = "manual concentrated cell capacity benchmark"]
fn concentrated_cell_capture_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    let (initial, people, sites) = shared_capacity(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..5 {
                let mut owner = initial.clone();
                let now = std::time::Instant::now();
                let result = AutonomousContext {
                    owner: &mut owner,
                    people: &people,
                    sites: &sites,
                    busy: &[],
                    controlled_unavailable: &[],
                    opinions: None,
                    seed: 7,
                    tick: 1,
                }
                .prepare();
                let elapsed = now.elapsed();
                assert_eq!(result.captured.len(), count as usize);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort();
            println!(
                "workers={workers} captures={count} elapsed_ms={:.3}",
                samples[samples.len() / 2].as_secs_f64() * 1000.0
            );
        });
}

#[test]
fn many_individuals_competing_for_one_target_choose_one_consistent_winner() {
    let (initial, people, sites) = shared_target(4096);
    initial.validate(&people, &sites, 0).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut owner = initial.clone();
                let outcome = AutonomousContext {
                    owner: &mut owner,
                    people: &people,
                    sites: &sites,
                    busy: &[],
                    controlled_unavailable: &[],
                    opinions: None,
                    seed: 7,
                    tick: 1,
                }
                .prepare();
                assert_eq!(outcome.captured, vec![4097]);
                assert_eq!(outcome.participants.len(), 2);
                assert_eq!(owner.holds.len(), 1);
                let expected = (1..=4096)
                    .min_by_key(|id| {
                        (
                            game_parallel_genetics::random(7 ^ 0x63617074757265, *id, 1),
                            *id,
                        )
                    })
                    .unwrap();
                assert_eq!(owner.events[0].actor, Some(expected));
                owner
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "manual concentrated capture benchmark"]
fn concentrated_capture_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    let (initial, people, sites) = shared_target(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..5 {
                let mut owner = initial.clone();
                let now = std::time::Instant::now();
                let result = AutonomousContext {
                    owner: &mut owner,
                    people: &people,
                    sites: &sites,
                    busy: &[],
                    controlled_unavailable: &[],
                    opinions: None,
                    seed: 7,
                    tick: 1,
                }
                .prepare();
                let elapsed = now.elapsed();
                assert_eq!(result.captured, vec![count + 1]);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort();
            println!(
                "workers={workers} actors={count} elapsed_ms={:.3}",
                samples[samples.len() / 2].as_secs_f64() * 1000.0
            );
        });
}

#[test]
fn simultaneous_attempts_reduce_a_shared_barrier_and_release_everyone() {
    let (initial, people, sites) = shared_barrier(4096);
    initial.validate(&people, &sites, 0).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let (first, actors) = initial.escape_attempts(&people, &[], 1, None, &[]).unwrap();
                assert_eq!(actors.len(), 4096);
                assert_eq!(first.cells[0].resistance, 1);
                assert_eq!(first.holds.len(), 4096);
                let (second, _) = first.escape_attempts(&people, &[], 2, None, &[]).unwrap();
                assert_eq!(second.cells[0].resistance, 0);
                assert!(second.holds.is_empty());
                second.validate(&people, &sites, 2).unwrap();
                second
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "manual concentrated escape benchmark"]
fn concentrated_escape_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    let (initial, people, _) = shared_barrier(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..5 {
                let now = std::time::Instant::now();
                let (next, actors) = initial.escape_attempts(&people, &[], 1, None, &[]).unwrap();
                let elapsed = now.elapsed();
                assert_eq!(actors.len(), count as usize);
                assert_eq!(next.cells[0].resistance, 1);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort();
            println!(
                "workers={workers} people={count} elapsed_ms={:.3}",
                samples[samples.len() / 2].as_secs_f64() * 1000.0
            );
        });
}
