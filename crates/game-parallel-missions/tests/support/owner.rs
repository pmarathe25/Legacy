use super::*;
fn owner() -> Missions {
    Missions {
        next_id: 2,
        pressure_threshold: None,
        entries: vec![Mission {
            id: 1,
            person: 1,
            party: None,
            party_opt_in: false,
            party_compliant: false,
            objective: None,
            objective_met: false,
            origin: 1,
            destination: 2,
            visit_ticks: 2,
            remaining: 0,
            phase: Phase::Ready,
        }],
        events: vec![],
    }
}
fn routes() -> Vec<Route> {
    vec![
        Route {
            source: 1,
            destination: 2,
            ticks: 1,
        },
        Route {
            source: 2,
            destination: 1,
            ticks: 1,
        },
    ]
}
#[test]
fn expedition_waits_for_real_arrival_visit_and_return() {
    let mut state = owner();
    let first = state
        .prepare(&[(1, Some(1), 100, 0)], &[], &routes(), 1)
        .unwrap();
    assert_eq!(first.departures.len(), 1);
    state = first.finish(state, &[false], true, 1);
    assert_eq!(state.entries[0].phase, Phase::Ready);
    assert_eq!(state.events[0].kind, EventKind::Unavailable);
    let next = state
        .prepare(&[(1, Some(1), 100, 0)], &[], &routes(), 2)
        .unwrap();
    state = next.finish(state, &[true], true, 2);
    assert_eq!(state.entries[0].phase, Phase::Outbound);
    state = state
        .prepare(&[(1, None, 100, 0)], &[], &routes(), 3)
        .unwrap()
        .finish(state, &[], true, 3);
    assert_eq!(state.entries[0].phase, Phase::Outbound);
    state = state
        .prepare(&[(1, Some(2), 100, 0)], &[], &routes(), 4)
        .unwrap()
        .finish(state, &[], true, 4);
    assert_eq!(
        (state.entries[0].phase, state.entries[0].remaining),
        (Phase::Visiting, 2)
    );
    state = state
        .prepare(&[(1, Some(2), 100, 0)], &[], &routes(), 5)
        .unwrap()
        .finish(state, &[], true, 5);
    state = state
        .prepare(&[(1, Some(2), 100, 0)], &[], &routes(), 6)
        .unwrap()
        .finish(state, &[], true, 6);
    assert_eq!(state.entries[0].phase, Phase::Returning);
    state = state
        .prepare(&[(1, Some(2), 100, 0)], &[], &routes(), 7)
        .unwrap()
        .finish(state, &[true], true, 7);
    assert_eq!(state.events[0].kind, EventKind::Return);
    state = state
        .prepare(&[(1, Some(1), 100, 0)], &[], &routes(), 8)
        .unwrap()
        .finish(state, &[], true, 8);
    assert_eq!(state.entries[0].phase, Phase::Completed);
    assert_eq!(state.events[0].kind, EventKind::Completed);
}
#[test]
fn blocked_and_rejected_departures_do_not_advance_or_create_goods() {
    let state = owner();
    let busy = state
        .prepare(&[(1, Some(1), 100, 0)], &[1], &routes(), 1)
        .unwrap();
    assert!(busy.departures.is_empty());
    let proposed = state
        .prepare(&[(1, Some(1), 100, 0)], &[], &routes(), 1)
        .unwrap();
    let result = proposed.finish(state.clone(), &[true], false, 1);
    assert_eq!(result.entries[0].phase, Phase::Ready);
    assert_eq!(result.events[0].kind, EventKind::Rejected);
    let dead = state
        .prepare(&[(1, Some(1), 0, 0)], &[], &routes(), 1)
        .unwrap()
        .finish(state, &[], true, 1);
    assert_eq!(dead.entries[0].phase, Phase::Failed);
    assert_eq!(dead.events[0].kind, EventKind::Died);
}
#[test]
fn owner_resolves_an_unmet_or_satisfied_objective_when_the_traveler_returns() {
    for met in [false, true] {
        let mut state = owner();
        state.entries[0].phase = Phase::Returning;
        state.entries[0].objective = Some(Objective::Meet { person: 2 });
        state.entries[0].objective_met = met;
        let draft = state
            .prepare(&[(1, Some(1), 100, 0)], &[], &routes(), 1)
            .unwrap();
        let result = draft.finish(state, &[], true, 1);
        assert_eq!(
            result.entries[0].phase,
            if met { Phase::Completed } else { Phase::Failed }
        );
        assert_eq!(
            result.events[0].kind,
            if met {
                EventKind::Completed
            } else {
                EventKind::ObjectiveFailed
            }
        );
    }
}
