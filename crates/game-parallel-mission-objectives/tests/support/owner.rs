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
