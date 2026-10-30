use crate::{ObservationView, Runtime};
use game_parallel_knowledge::{Knowledge, Mind, Observation, Record};
use game_parallel_social::Encounter;
use std::collections::HashSet;
struct View(Vec<Observation>);
impl ObservationView for View {
    fn read(&self, id: u64) -> Observation {
        self.0[self.0.binary_search_by_key(&id, |r| r.person).unwrap()]
    }
}
fn fixture(count: usize, limit: usize, sparse: bool) -> (Knowledge, View, HashSet<u64>) {
    let mut minds: Vec<_> = (0..count)
        .map(|i| {
            let person = if sparse {
                19 + i as u64 * 31
            } else {
                i as u64 + 1
            };
            Mind {
                person,
                records: (1..=limit.min(7))
                    .rev()
                    .map(|region| Record {
                        region: region as u64,
                        food: (i % 5) as u32,
                        observed_at: 0,
                        witness: person,
                        learned_from: None,
                    })
                    .collect(),
            }
        })
        .collect();
    let view = View(
        minds
            .iter()
            .map(|m| Observation {
                person: m.person,
                health: 100,
                local: Some((1, 100)),
            })
            .collect(),
    );
    minds.reverse();
    (
        Knowledge {
            max_records: limit,
            minds,
        },
        view,
        (1..=8).collect(),
    )
}
#[test]
fn persistent_records_match_reference_for_sparse_reordered_minds_and_spilled_capacity() {
    for workers in [1, 4] {
        for sparse in [false, true] {
            for limit in [1, 4, 7] {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .build()
                    .unwrap()
                    .install(|| {
                        let (mut expected, mut view, regions) = fixture(2050, limit, sparse);
                        let mut runtime = Runtime::compile_validated(&expected, 0).unwrap();
                        assert_eq!(runtime.export(), expected);
                        for now in 0..8 {
                            for (i, row) in view.0.iter_mut().enumerate() {
                                row.health = if i % 11 == 0 { 0 } else { 100 };
                                row.local = (i % 13 != 0).then_some((1, (now * 9) as u32));
                            }
                            let encounters: Vec<_> = view
                                .0
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .filter(|p| p.iter().all(|p| p.health > 0 && p.local.is_some()))
                                .map(|p| Encounter {
                                    tick: now + 1,
                                    region: 1,
                                    a: p[0].person,
                                    b: p[1].person,
                                    pressured: None,
                                })
                                .collect();
                            let next = expected
                                .prepare(&view.0, &regions, &encounters, now)
                                .unwrap();
                            let draft = runtime.prepare_validated(&view, &encounters, now).unwrap();
                            assert_eq!(runtime.export(), expected);
                            runtime.commit(draft).unwrap();
                            expected = next;
                            assert_eq!(
                                runtime.export(),
                                expected,
                                "now={now} workers={workers} sparse={sparse} capacity={limit}"
                            );
                            if now == 3 {
                                runtime =
                                    Runtime::compile_validated(&runtime.export(), now + 1).unwrap();
                            }
                        }
                    });
            }
        }
    }
}
#[test]
fn generations_birth_enrollment_and_rejected_updates_are_atomic() {
    let (owner, mut view, regions) = fixture(4, 4, false);
    let mut runtime = Runtime::compile_validated(&owner, 0).unwrap();
    let mut other = Runtime::compile_validated(&owner, 0).unwrap();
    let foreign = other.prepare_validated(&view, &[], 0).unwrap();
    assert!(runtime.commit(foreign).is_err());
    let first = runtime.prepare_validated(&view, &[], 0).unwrap();
    let second = runtime.prepare_validated(&view, &[], 0).unwrap();
    assert!(runtime.commit(first).is_err());
    assert_eq!(runtime.export(), owner);
    assert!(runtime.prepare_validated(&view, &[], 1).is_err());
    runtime.commit(second).unwrap();
    let before = runtime.export();
    assert!(runtime.enroll(&[8, 7]).is_err());
    assert!(runtime.enroll(&[4]).is_err());
    assert_eq!(runtime.export(), before);
    let pending = runtime.prepare_validated(&view, &[], 1).unwrap();
    runtime.enroll(&[9, 10]).unwrap();
    assert!(runtime.commit(pending).is_err());
    let after = runtime.export();
    assert_eq!(&after.minds[..4], &before.minds);
    assert_eq!(after.minds[4].person, 9);
    assert_eq!(runtime.records(10), Some(&[][..]));
    assert!(runtime.records(8).is_none());
    view.0.extend([9, 10].map(|person| Observation {
        person,
        health: 100,
        local: Some((1, 20)),
    }));
    let expected = after.prepare(&view.0, &regions, &[], 1).unwrap();
    let draft = runtime.prepare_validated(&view, &[], 1).unwrap();
    runtime.commit(draft).unwrap();
    assert_eq!(runtime.export(), expected);
}
#[test]
fn reports_use_prior_records_not_the_partners_same_tick_observation() {
    let owner = Knowledge {
        max_records: 4,
        minds: vec![
            Mind {
                person: 1,
                records: vec![],
            },
            Mind {
                person: 2,
                records: vec![],
            },
        ],
    };
    let view = View(vec![
        Observation {
            person: 1,
            health: 100,
            local: Some((1, 10)),
        },
        Observation {
            person: 2,
            health: 100,
            local: Some((2, 20)),
        },
    ]);
    // Trusted adapter boundary: deliberately distinct observations expose accidental read-after-write.
    let encounters = vec![Encounter {
        tick: 1,
        region: 1,
        a: 1,
        b: 2,
        pressured: None,
    }];
    let mut runtime = Runtime::compile_validated(&owner, 0).unwrap();
    let draft = runtime.prepare_validated(&view, &encounters, 0).unwrap();
    runtime.commit(draft).unwrap();
    assert_eq!(runtime.records(1).unwrap().len(), 1);
    assert_eq!(runtime.records(1).unwrap()[0].region, 1);
    assert_eq!(runtime.records(2).unwrap()[0].region, 2);
}

#[test]
fn thousands_of_births_fill_partial_and_new_blocks_deterministically() {
    let (initial, view, _) = fixture(1023, 4, false);
    let births: Vec<_> = (1024..=7000).collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut runtime = Runtime::compile_validated(&initial, 0).unwrap();
                runtime.enroll(&births).unwrap();
                let enrolled = runtime.export();
                assert_eq!(
                    &enrolled.minds[..initial.minds.len()],
                    initial.minds.as_slice()
                );
                assert!(
                    enrolled.minds[initial.minds.len()..]
                        .iter()
                        .zip(&births)
                        .all(|(mind, id)| mind.person == *id && mind.records.is_empty())
                );
                let mut observations = view.0.clone();
                observations.extend(births.iter().map(|&person| Observation {
                    person,
                    health: 100,
                    local: Some((1, 7)),
                }));
                let draft = runtime
                    .prepare_validated(&View(observations), &[], 0)
                    .unwrap();
                runtime.commit(draft).unwrap();
                runtime.export()
            })
    };
    assert_eq!(run(1), run(4));
}
