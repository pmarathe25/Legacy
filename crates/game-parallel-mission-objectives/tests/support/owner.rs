use super::*;
use game_parallel_missions::Mission;

fn mission(person: u64) -> Mission {
    Mission {
        id: person,
        person,
        party: None,
        party_opt_in: false,
        party_compliant: false,
        origin: 1,
        destination: 2,
        visit_ticks: 1,
        remaining: 0,
        phase: MissionPhase::Outbound,
        objective: Some(MissionObjective::Meet {
            person: person + 100_000,
        }),
        objective_met: false,
        delivery_preference: None,
    }
}
#[test]
fn meeting_receipts_require_active_departure_and_the_correct_place_and_tick() {
    let mut state = Missions {
        next_id: 2,
        pressure_threshold: None,
        entries: vec![mission(1)],
        events: vec![],
    };
    let mut event = Encounter {
        tick: 1,
        region: 2,
        a: 1,
        b: 100_001,
        pressured: None,
    };
    state.entries[0].phase = MissionPhase::Ready;
    assert!(meetings(&mut state, &[event.clone()], 1).is_empty());
    state.entries[0].phase = MissionPhase::Outbound;
    event.region = 1;
    assert!(meetings(&mut state, &[event.clone()], 1).is_empty());
    event.region = 2;
    assert!(meetings(&mut state, &[event.clone()], 2).is_empty());
    assert_eq!(meetings(&mut state, &[event], 1).len(), 1);
    assert!(state.entries[0].objective_met);
}

#[test]
fn deliveries_accumulate_only_for_the_active_matching_objective() {
    let mut entry = mission(1);
    entry.objective = Some(MissionObjective::Deliver {
        person: 7,
        good: 2,
        quantity: 5,
        delivered: 0,
    });
    let mut state = Missions {
        next_id: 2,
        pressure_threshold: None,
        entries: vec![entry],
        events: vec![],
    };
    let gift = Gift {
        region: 2,
        giver: 1,
        recipient: 7,
        good: 2,
        quantity: 2,
    };
    state.entries[0].phase = MissionPhase::Ready;
    assert!(deliveries(&mut state, &[gift], 1).is_empty());
    state.entries[0].phase = MissionPhase::Visiting;
    assert!(
        deliveries(
            &mut state,
            &[
                Gift { region: 1, ..gift },
                Gift {
                    recipient: 8,
                    ..gift
                },
                Gift { good: 3, ..gift }
            ],
            1
        )
        .is_empty()
    );
    assert!(deliveries(&mut state, &[gift], 1).is_empty());
    assert_eq!(
        state.entries[0].objective,
        Some(MissionObjective::Deliver {
            person: 7,
            good: 2,
            quantity: 5,
            delivered: 2
        })
    );
    assert_eq!(deliveries(&mut state, &[gift, gift], 2).len(), 1);
    assert!(state.entries[0].objective_met);
    assert_eq!(
        state.entries[0].objective,
        Some(MissionObjective::Deliver {
            person: 7,
            good: 2,
            quantity: 5,
            delivered: 5
        })
    );
    assert!(deliveries(&mut state, &[gift], 3).is_empty());
}

#[test]
fn concentrated_deliveries_are_deterministic_across_workers() {
    let count = 20_000;
    let state = Missions {
        next_id: count + 1,
        pressure_threshold: None,
        entries: (1..=count)
            .map(|person| {
                let mut entry = mission(person);
                entry.objective = Some(MissionObjective::Deliver {
                    person: 100_001,
                    good: 2,
                    quantity: 3,
                    delivered: 0,
                });
                entry
            })
            .collect(),
        events: vec![],
    };
    let gifts: Vec<_> = (1..=count)
        .flat_map(|giver| {
            [
                Gift {
                    region: 2,
                    giver,
                    recipient: 100_001,
                    good: 2,
                    quantity: 1,
                },
                Gift {
                    region: 2,
                    giver,
                    recipient: 100_001,
                    good: 2,
                    quantity: 2,
                },
            ]
        })
        .rev()
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = state.clone();
                let events = deliveries(&mut state, &gifts, 1);
                assert_eq!(events.len(), count as usize);
                (state, events)
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "manual delivery objective benchmark"]
fn delivery_objectives_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100_000u64);
    let state = Missions {
        next_id: count + 1,
        pressure_threshold: None,
        entries: (1..=count)
            .map(|person| {
                let mut entry = mission(person);
                entry.objective = Some(MissionObjective::Deliver {
                    person: count + 1,
                    good: 2,
                    quantity: 1,
                    delivered: 0,
                });
                entry
            })
            .collect(),
        events: vec![],
    };
    let gifts: Vec<_> = (1..=count)
        .map(|giver| Gift {
            region: 2,
            giver,
            recipient: count + 1,
            good: 2,
            quantity: 1,
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..5 {
                let mut state = state.clone();
                let start = std::time::Instant::now();
                let events = deliveries(&mut state, &gifts, 1);
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(events.len(), count as usize);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} deliveries={count} elapsed={:.6}",
                samples[1]
            );
        });
}
#[test]
fn concentrated_meetings_match_across_worker_counts() {
    let count = 50_000;
    let state = Missions {
        next_id: count + 1,
        pressure_threshold: None,
        entries: (1..=count).map(mission).collect(),
        events: vec![],
    };
    let encounters: Vec<_> = (1..=count)
        .map(|person| Encounter {
            tick: 1,
            region: 2,
            a: person,
            b: person + 100_000,
            pressured: None,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = state.clone();
                let events = meetings(&mut state, &encounters, 1);
                assert_eq!(events.len(), count as usize);
                (state, events)
            })
    };
    assert_eq!(run(1), run(4));
}
