use game_pastimes::{Activity, Interest, Mind, Outcome, Pastimes};
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn independent_leisure_checks_one_local_choice_per_person_across_worker_counts() {
    const PEOPLE: u64 = 4096;
    let run =
        |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut owner = Pastimes {
                        activities: vec![Activity {
                            id: 1,
                            name: "Practice a song".into(),
                            skill: "music".into(),
                            duration_millis: 600,
                            satisfaction_millis: 2000,
                            materials: Vec::new(),
                            tools: Vec::new(),
                        }],
                        minds: (1..=PEOPLE)
                            .map(|person| Mind {
                                person,
                                idle: 0,
                                interests: vec![Interest {
                                    activity: 1,
                                    value: 2,
                                    recent_value: -2,
                                    practice_hour_value: 0,
                                    heard_value: 0,
                                    completions: 0,
                                    last_completion: None,
                                }],
                            })
                            .collect(),
                        active: Vec::new(),
                        training: Default::default(),
                        culture: None,
                    };
                    let inspected = AtomicUsize::new(0);
                    let starts = owner.choose(
                        0,
                        |_| Some(1),
                        |_, _| {
                            inspected.fetch_add(1, Ordering::Relaxed);
                            true
                        },
                    );
                    assert_eq!(inspected.load(Ordering::Relaxed), PEOPLE as usize);
                    owner.start(starts);
                    assert_eq!(owner.next_finish(350), Some(250));
                    let interrupted = owner.settle(350, |row, _| row.person % 8 != 0);
                    assert_eq!(interrupted.len(), PEOPLE as usize / 8);
                    assert!(
                        interrupted
                            .iter()
                            .all(|row| row.outcome == Outcome::Interrupted)
                    );
                    let finished = owner.settle(600, |_, _| true);
                    assert_eq!(finished.len(), 7 * PEOPLE as usize / 8);
                    assert!(
                        finished
                            .iter()
                            .all(|row| row.outcome == Outcome::Completed && row.at_millis == 600)
                    );
                    assert_eq!(owner.next_finish(600), None);
                    assert_eq!(owner.training.records.len(), finished.len());
                    assert!(
                        owner.training.records.iter().all(|row| row.person % 8 != 0
                            && row.completed == 1
                            && row.millis == 600)
                    );
                    assert_eq!(
                        owner.choose(600, |_| Some(1), |_, _| true).len(),
                        PEOPLE as usize / 8
                    );
                    (owner, interrupted, finished)
                })
        };
    assert_eq!(run(1), run(6));
}
