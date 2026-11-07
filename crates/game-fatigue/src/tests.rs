use crate::{Activity, Fatigue, Person, Rules};
use std::collections::HashSet;
fn owner() -> Fatigue {
    Fatigue {
        rules: Rules {
            capacity: 10,
            baseline_gain: 0,
            work_gain: 3,
            travel_gain: 0,
            idle_recovery: 1,
            rest_recovery: 2,
            rest_at: 6,
            resume_at: 2,
            exhaustion_at: 6,
            exhaustion_damage: 4,
        },
        people: vec![],
    }
}
#[test]
fn work_causes_exhaustion_then_rest_uses_separate_resume_threshold() {
    let activity = [Activity {
        id: 1,
        alive: true,
        working: true,
        traveling: false,
        can_rest: true,
    }];
    let mut state = owner();
    state.validate(&HashSet::from([1])).unwrap();
    state = state.prepare(&activity).unwrap().state;
    assert_eq!(state.people[0].fatigue, 3);
    let next = state.prepare(&activity).unwrap();
    assert_eq!(next.consequences[0].damage, 4);
    state = next.state;
    assert_eq!(state.resting(), vec![1]);
    let resting = [Activity {
        id: 1,
        alive: true,
        working: false,
        traveling: false,
        can_rest: true,
    }];
    state = state.prepare(&resting).unwrap().state;
    assert_eq!(state.people[0].fatigue, 4);
    assert!(state.people[0].resting);
    state = state.prepare(&resting).unwrap().state;
    assert_eq!(state.people[0].fatigue, 2);
    assert!(!state.people[0].resting);
}
#[test]
fn dead_people_preserve_fatigue_without_recovery_or_damage() {
    let mut state = owner();
    state.people.push(Person {
        id: 1,
        fatigue: 9,
        resting: true,
    });
    let next = state
        .prepare(&[Activity {
            id: 1,
            alive: false,
            working: true,
            traveling: false,
            can_rest: true,
        }])
        .unwrap();
    assert_eq!(next.state, state);
    assert!(next.consequences.is_empty());
}
#[test]
fn rejects_duplicate_and_missing_activities() {
    let mut state = owner();
    state.people.push(Person {
        id: 1,
        fatigue: 3,
        resting: false,
    });
    assert!(state.prepare(&[]).is_err());
    let a = Activity {
        id: 1,
        alive: true,
        working: false,
        traveling: false,
        can_rest: true,
    };
    assert!(state.prepare(&[a, a]).is_err());
}

#[test]
fn baseline_fatigue_also_requires_rest_when_idle() {
    let mut state = owner();
    state.rules.baseline_gain = 4;
    state.rules.idle_recovery = 1;
    let activity = [Activity {
        id: 1,
        alive: true,
        working: false,
        traveling: false,
        can_rest: true,
    }];
    state = state.prepare(&activity).unwrap().state;
    state = state.prepare(&activity).unwrap().state;
    assert_eq!(state.people[0].fatigue, 6);
    assert!(state.people[0].resting);
}
#[test]
fn many_people_are_deterministic_across_worker_counts() {
    let state = owner();
    let activity: Vec<_> = (1..=20000)
        .map(|id| Activity {
            id,
            alive: id % 7 != 0,
            working: id % 2 == 0,
            traveling: false,
            can_rest: true,
        })
        .collect();
    let mut expected = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut current = state.clone();
        for _ in 0..8 {
            current = pool.install(|| current.prepare(&activity).unwrap().state);
        }
        if let Some(expected) = &expected {
            assert_eq!(&current, expected);
        } else {
            expected = Some(current);
        }
    }
}

#[test]
fn travel_without_a_rest_opportunity_does_not_recover_fatigue() {
    let mut state = owner();
    state.people.push(Person {
        id: 1,
        fatigue: 6,
        resting: true,
    });
    let activity = [Activity {
        id: 1,
        alive: true,
        working: false,
        traveling: true,
        can_rest: false,
    }];
    let next = state.prepare(&activity).unwrap();
    assert_eq!(next.state.people[0].fatigue, 6);
    assert!(next.state.people[0].resting);
    assert_eq!(next.consequences[0].damage, 4);
}

#[test]
fn travel_exertion_accumulates_until_a_stationary_rest_opportunity() {
    let mut state = owner();
    state.rules.travel_gain = 2;
    let traveling = [Activity {
        id: 1,
        alive: true,
        working: false,
        traveling: true,
        can_rest: false,
    }];
    state = state.prepare(&traveling).unwrap().state;
    state = state.prepare(&traveling).unwrap().state;
    let next = state.prepare(&traveling).unwrap();
    assert_eq!(next.state.people[0].fatigue, 6);
    assert_eq!(next.consequences[0].damage, 4);
    let resting = [Activity {
        id: 1,
        alive: true,
        working: false,
        traveling: false,
        can_rest: true,
    }];
    let next = next.state.prepare(&resting).unwrap();
    assert_eq!(next.state.people[0].fatigue, 4);
}
