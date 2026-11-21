use game_mechanics::{Observation, Outcome, inquiry::Study};

fn trial(actor: u64, force: u64, load: u64, time: u128, outcome: Outcome) -> Observation {
    Observation {
        tick: 0,
        at_millis: Some(time),
        actor,
        apparatus: 1,
        output_part: 1002,
        input_force: force,
        input_distance: 100,
        load_force: load,
        outcome,
        available_force: 0,
        travel: 0,
        delivered_work: 0,
        broken_parts: Vec::new(),
    }
}

#[test]
fn inquiry_holds_observed_conditions_and_preserves_counterexamples() {
    let mut observations = vec![
        trial(1, 100, 10, 10, Outcome::Moved),
        trial(1, 100, 60, 20, Outcome::Stalled),
        trial(1, 100, 10, 22, Outcome::Moved),
        trial(1, 101, 500, 23, Outcome::Moved),
        trial(2, 100, 200, 24, Outcome::Moved),
        trial(1, 100, 1000, 25, Outcome::Broken),
        trial(1, 100, 2000, 26, Outcome::Contended),
    ];
    let mut study = Study {
        activity: 3,
        trial_limit: 16,
        capacity: 1,
        comparisons: Vec::new(),
    };
    let estimate = study.reflect(1, &observations, 30)[0];
    assert_eq!(
        (
            estimate.evidence.largest_load_moved,
            estimate.evidence.smallest_load_stalled
        ),
        (10, 60)
    );
    assert_eq!(
        (
            estimate.evidence.distinct_moved_loads,
            estimate.evidence.distinct_stalled_loads
        ),
        (1, 1)
    );
    assert_eq!(
        (
            estimate.evidence.first_evidence_millis,
            estimate.evidence.latest_evidence_millis
        ),
        (10, 22)
    );
    assert!(!estimate.evidence.contradicted());
    assert!(study.reflect(1, &observations, 31).is_empty());
    observations.push(trial(1, 100, 55, 32, Outcome::Moved));
    observations.push(trial(1, 100, 30, 33, Outcome::Stalled));
    let revised = study.reflect(1, &observations, 38)[0];
    assert_eq!(
        (
            revised.evidence.largest_load_moved,
            revised.evidence.smallest_load_stalled
        ),
        (55, 30)
    );
    assert!(revised.evidence.contradicted());
    assert!(study.validate(38));
    assert!(!study.validate(32));
}
