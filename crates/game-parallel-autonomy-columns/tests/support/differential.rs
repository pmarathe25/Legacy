use crate::{Runtime, SituationView};
use game_parallel_autonomy::{Actor, Autonomy, Decision, Path, Rules, Situation};
use game_parallel_knowledge::{Mind, Record};
use std::collections::HashSet;
struct View(Vec<Situation>);
impl SituationView for View {
    fn read(&self, id: u64) -> Situation {
        self.0[self.0.binary_search_by_key(&id, |s| s.person).unwrap()]
    }
}
fn rules() -> Rules {
    Rules {
        review_interval: 2,
        min_health: 10,
        min_hunger: 3,
        min_destination_food: 5,
        min_food_gain: 3,
        max_observation_age: 3,
    }
}
fn decisions(rows: &[Decision]) -> Vec<(u64, u64, u64)> {
    rows.iter()
        .map(|d| (d.person, d.source, d.destination))
        .collect()
}
fn actor(person: u64) -> Actor {
    Actor {
        person,
        enabled: true,
        next_review: 0,
        travel_caution: 0,
        hunger_urgency: 0,
    }
}
#[test]
fn persistent_reviews_match_reference_dispositions_routes_outcomes_and_reload() {
    for workers in [1, 4] {
        for sparse in [false, true] {
            for routed in [false, true] {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .build()
                    .unwrap()
                    .install(|| {
                        let mut actors: Vec<_> = (0..2050u64)
                            .map(|i| {
                                let mut a = actor(if sparse { 10 + i * 17 } else { i + 1 });
                                a.enabled = !i.is_multiple_of(11);
                                a.travel_caution = (i % 5) as u32 * 2;
                                a.hunger_urgency = (i % 3) as u32;
                                a.next_review = i % 3;
                                a
                            })
                            .collect();
                        let minds: Vec<_> = actors
                            .iter()
                            .map(|a| Mind {
                                person: a.person,
                                records: [1, 2, 3]
                                    .into_iter()
                                    .map(|region| Record {
                                        region,
                                        food: if region == 1 { 0 } else { 20 },
                                        observed_at: 0,
                                        witness: a.person,
                                        learned_from: None,
                                    })
                                    .collect(),
                            })
                            .collect();
                        let mut view = View(
                            actors
                                .iter()
                                .map(|a| Situation {
                                    person: a.person,
                                    region: Some(1),
                                    health: 100,
                                    hunger: 4,
                                    busy: false,
                                })
                                .collect(),
                        );
                        actors.reverse();
                        let mut expected = Autonomy {
                            rules: rules(),
                            actors,
                            events: vec![],
                        };
                        let paths = if routed {
                            vec![
                                Path {
                                    source: 1,
                                    destination: 3,
                                    ticks: 2,
                                },
                                Path {
                                    source: 1,
                                    destination: 2,
                                    ticks: 2,
                                },
                            ]
                        } else {
                            vec![]
                        };
                        let regions: HashSet<_> = [1, 2, 3].into_iter().collect();
                        let mut runtime = Runtime::compile_validated(&expected, &paths, 0).unwrap();
                        for now in 0..8 {
                            for (i, s) in view.0.iter_mut().enumerate() {
                                s.busy = (i + now as usize).is_multiple_of(13);
                                s.health = if i.is_multiple_of(7) { 0 } else { 100 };
                                s.region = (!i.is_multiple_of(17)).then_some(1);
                                s.hunger = (i % 6) as u32;
                            }
                            let reference = expected
                                .prepare(&view.0, &paths, &minds, &regions, now)
                                .unwrap();
                            let draft = runtime
                                .prepare(
                                    &view,
                                    |id| {
                                        minds
                                            .binary_search_by_key(&id, |m| m.person)
                                            .ok()
                                            .map(|i| minds[i].records.as_slice())
                                    },
                                    now,
                                )
                                .unwrap();
                            assert_eq!(
                                decisions(&draft.decisions),
                                decisions(&reference.decisions)
                            );
                            assert_eq!(runtime.export(), expected);
                            let available: Vec<_> = (0..draft.decisions.len())
                                .map(|i| !i.is_multiple_of(3))
                                .collect();
                            let outcome = if now % 2 == 0 {
                                Ok(())
                            } else {
                                Err("staged admission rejected".into())
                            };
                            runtime.commit(draft, &available, &outcome).unwrap();
                            expected = game_parallel_autonomy::adapter::finish(
                                reference,
                                &available,
                                &outcome,
                                now + 1,
                            );
                            assert_eq!(runtime.export(), expected);
                            if now == 3 {
                                runtime =
                                    Runtime::compile_validated(&runtime.export(), &paths, now + 1)
                                        .unwrap();
                            }
                        }
                    });
            }
        }
    }
}
#[test]
fn review_overflow_and_stale_foreign_drafts_never_publish() {
    let initial = Autonomy {
        rules: rules(),
        actors: vec![actor(1)],
        events: vec![],
    };
    let view = View(vec![Situation {
        person: 1,
        region: Some(1),
        health: 100,
        hunger: 0,
        busy: false,
    }]);
    let mut runtime = Runtime::compile_validated(&initial, &[], 0).unwrap();
    let first = runtime.prepare(&view, |_| None, 0).unwrap();
    let second = runtime.prepare(&view, |_| None, 0).unwrap();
    assert!(runtime.commit(first, &[], &Ok(())).is_err());
    assert_eq!(runtime.export(), initial);
    runtime.commit(second, &[], &Ok(())).unwrap();
    assert_eq!(runtime.export().actors[0].next_review, 2);
    let mut foreign = Runtime::compile_validated(&initial, &[], 1).unwrap();
    let draft = foreign.prepare(&view, |_| None, 1).unwrap();
    assert!(runtime.commit(draft, &[], &Ok(())).is_err());
    let before = runtime.export();
    runtime
        .enroll(&[Actor {
            next_review: 20,
            ..actor(9)
        }])
        .unwrap();
    assert_eq!(&runtime.export().actors[..1], &before.actors);
    assert_eq!(runtime.export().actors[1].next_review, 20);
    let mut overflow = Runtime::compile_validated(&initial, &[], u64::MAX - 1).unwrap();
    assert_eq!(
        overflow.prepare(&view, |_| None, u64::MAX - 1).err(),
        Some("autonomy review overflow")
    );
    assert_eq!(overflow.export(), initial);
}
