use super::*;

fn guardian(id: u64, region: u64, health: u32) -> Guardian {
    Guardian {
        id,
        region,
        health,
        confined: false,
        journey: None,
        student: None,
        mentorship: 0,
        released: false,
    }
}
#[test]
fn guardians_choose_weighted_shortest_route_with_stable_ties_and_recover_before_departing() {
    let state = Guardians {
        recovery_per_tick: 1,
        max_health: 10,
        mentorship_ticks: 2,
        guardians: vec![guardian(1, 1, 0), guardian(2, 4, 10)],
    };
    let routes = vec![
        Route {
            source: 1,
            destination: 3,
            ticks: 1,
        },
        Route {
            source: 3,
            destination: 4,
            ticks: 2,
        },
        Route {
            source: 1,
            destination: 2,
            ticks: 2,
        },
        Route {
            source: 2,
            destination: 4,
            ticks: 1,
        },
        Route {
            source: 1,
            destination: 4,
            ticks: 7,
        },
    ];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| state.advance(Some((7, Some(4))), &routes))
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert_eq!(result.guardians[0].health, 1);
    assert_eq!(
        result.guardians[0].journey,
        Some(Journey {
            destination: 2,
            remaining: 2
        })
    );
    assert_eq!(result.guardians[1].mentorship, 1);
}
#[test]
fn traveling_or_confined_guardians_recover_and_wait_without_a_new_route() {
    let mut confined = guardian(1, 1, 0);
    confined.confined = true;
    let mut traveling = guardian(2, 1, 0);
    traveling.journey = Some(Journey {
        destination: 2,
        remaining: 2,
    });
    let state = Guardians {
        recovery_per_tick: 1,
        max_health: 10,
        mentorship_ticks: 2,
        guardians: vec![confined, traveling],
    };
    let result = state.advance(Some((7, Some(3))), &[]);
    assert_eq!(result.guardians[0].health, 1);
    assert!(result.guardians[0].journey.is_none());
    assert_eq!(result.guardians[1].journey.as_ref().unwrap().remaining, 1);
}
#[test]
fn many_guardians_share_routing_and_are_worker_invariant() {
    let state = Guardians {
        recovery_per_tick: 1,
        max_health: 10,
        mentorship_ticks: 2,
        guardians: (1..=50_000).map(|id| guardian(id, 1, 10)).collect(),
    };
    let routes = [
        Route {
            source: 1,
            destination: 2,
            ticks: 3,
        },
        Route {
            source: 2,
            destination: 3,
            ticks: 2,
        },
    ];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| state.advance(Some((7, Some(3))), &routes))
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert!(result.guardians.iter().all(|g| g.journey
        == Some(Journey {
            destination: 2,
            remaining: 3
        })));
}
