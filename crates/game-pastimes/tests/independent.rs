use game_pastimes::{Activity, Interest, Mind, Outcome, Pastimes};
use std::num::NonZeroU64;
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

#[test]
fn concentrated_audience_reads_one_venue_per_person_and_has_stable_choices() {
    use game_pastimes::{Culture, Event, Listener, ListeningInterest, Presentation, Session};

    const PEOPLE: u64 = 4096;
    fn run(workers: usize, concentrated: bool) -> Vec<game_pastimes::Heard> {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut culture = Culture {
                    presentations: (1..=PEOPLE)
                        .map(|person| Presentation {
                            person,
                            activity: 1,
                        })
                        .collect(),
                    listeners: (PEOPLE + 1..=2 * PEOPLE)
                        .map(|person| Listener {
                            person,
                            idle: 0,
                            minimum_millis: NonZeroU64::new(100),
                            stay_value: 0,
                            interests: vec![ListeningInterest {
                                activity: 1,
                                value: 1,
                                last_heard: None,
                            }],
                        })
                        .collect(),
                    attendance: Vec::new(),
                    music: None,
                };
                let sessions: Vec<_> = (1..=PEOPLE)
                    .map(|person| Session {
                        person,
                        activity: 1,
                        region: 1,
                        started_at: 0,
                        ends_at: 200,
                    })
                    .collect();
                let venue_checks = AtomicUsize::new(0);
                culture.attendance = culture.choose_attendance(
                    50,
                    &sessions,
                    |person| {
                        venue_checks.fetch_add(1, Ordering::Relaxed);
                        Some((
                            1,
                            (
                                1,
                                if concentrated {
                                    1
                                } else {
                                    (person - 1) % PEOPLE + 1
                                },
                            ),
                        ))
                    },
                    |_| true,
                    |_| true,
                    |_| None,
                );
                assert_eq!(venue_checks.load(Ordering::Relaxed), 2 * PEOPLE as usize);
                assert_eq!(culture.attendance.len(), PEOPLE as usize);
                let events: Vec<_> = sessions
                    .iter()
                    .map(|row| Event {
                        person: row.person,
                        activity: row.activity,
                        at_millis: row.ends_at,
                        outcome: Outcome::Completed,
                    })
                    .collect();
                let hearings = culture.finish_attendance(&events);
                assert!(culture.attendance.is_empty());
                assert!(hearings.iter().all(|row| {
                    row.attended_millis == Some(150)
                        && row.performer
                            == if concentrated {
                                1
                            } else {
                                row.listener - PEOPLE
                            }
                }));
                hearings
            })
    }
    for concentrated in [false, true] {
        assert_eq!(run(1, concentrated), run(6, concentrated));
    }
}
