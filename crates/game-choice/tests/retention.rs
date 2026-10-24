use game_choice::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;

#[derive(Clone, Serialize, Deserialize)]
struct Act;
impl Request for Act {
    const KEY: &'static str = "test.retention-act";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Begin;
impl Request for Begin {
    const KEY: &'static str = "test.retention-begin";
    type Response = u64;
}
fn proposals(mut seed: u64, ongoing: u64) -> Vec<Proposal> {
    (0..2048)
        .map(|i| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let score = if i % 17 == 0 {
                i64::MAX
            } else {
                (seed % 41) as i64 - 20
            };
            let mut p = Proposal::new(
                format!("offer:{i}"),
                motive(score, -score, score, 0, 0),
                Act,
            )
            .unwrap();
            // Deliberately collide numeric motive IDs, independent of full proposal keys.
            p.motive.id = seed % 8;
            p.continuation = match i % 19 {
                0 => Some(ongoing),
                1 => Some(ongoing + 1),
                _ => None,
            };
            p
        })
        .collect()
}
fn weight(i: usize) -> i32 {
    match i {
        0..=399 => 0,
        400..=999 => 100,
        1000..=1499 => -100,
        _ => 0,
    }
}
fn oracle(stream: &[Proposal], ongoing: u64) -> Vec<Proposal> {
    let mut retained = Vec::new();
    for (i, p) in stream.iter().enumerate() {
        retained.push(p.clone());
        if retained.len() > LIMIT {
            let worst = retained
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| {
                    let mut c = p.motive;
                    c.reweight(|term| {
                        if term == "goal" {
                            (100 + i64::from(weight(i)) * 10).clamp(0, 1000)
                        } else {
                            100
                        }
                    });
                    (
                        p.continuation == Some(ongoing),
                        (c.score(), Reverse(c.id)),
                        &p.key,
                    )
                })
                .unwrap()
                .0;
            retained.swap_remove(worst);
        }
    }
    retained.sort_by(|a, b| a.key.cmp(&b.key));
    retained
}
#[test]
fn heap_matches_scan_with_saturation_ties_continuations_and_changing_weights() {
    for seed in [1, 7, 42, 999] {
        let mut b = Builder::new();
        game_traits::plugin(&mut b).unwrap();
        plugin(&mut b).unwrap();
        b.plugin("test", 1).unwrap();
        b.request::<Act>(|_, _| Ok(())).unwrap();
        b.request::<Begin>(|tx, _| {
            tx.start_action(PersonId(1), "test", Some(SimTime(HOUR.0)), Act)?;
            Ok(tx.get::<Actions>()?.0[&PersonId(1)].id)
        })
        .unwrap();
        b.subscribe::<Review>("offers", move |tx, e| {
            let id = tx.get::<Actions>()?.0[&e.person].id;
            for (i, p) in proposals(seed, id).into_iter().enumerate() {
                if i == 0 || weight(i) != weight(i - 1) {
                    tx.call(game_traits::Set {
                        person: e.person,
                        profile: [("drive".into(), weight(i))].into(),
                    })?;
                }
                submit(tx, e.person, p)?;
            }
            Ok(())
        })
        .unwrap();
        let mut h = b.build(seed, 1).unwrap();
        let mut traits = game_traits::Traits::default();
        traits.definitions.insert(
            "drive".into(),
            game_traits::Trait {
                name: "Drive".into(),
                minimum: -100,
                maximum: 100,
                default: 0,
                founder_spread: 0,
                weights: [("goal".into(), 1000)].into(),
            },
        );
        h.dispatch(game_traits::Install(traits)).unwrap();
        let ongoing = h.dispatch(Begin).unwrap();
        assert_eq!(
            h.dispatch(Collect {
                person: PersonId(1)
            })
            .unwrap(),
            oracle(&proposals(seed, ongoing), ongoing)
        );
    }
}
#[test]
fn a_pruned_key_can_return_with_a_better_proposal() {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.request::<Act>(|_, _| Ok(())).unwrap();
    b.subscribe::<Review>("offers", |tx, e| {
        offer(tx, e.person, "returning", motive(0, -1000, 0, 0, 0), Act)?;
        for i in 0..LIMIT {
            offer(
                tx,
                e.person,
                format!("good:{i}"),
                motive(0, 10, 0, 0, 0),
                Act,
            )?;
        }
        offer(tx, e.person, "returning", motive(0, 1000, 0, 0, 0), Act)?;
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    let retained = h
        .dispatch(Collect {
            person: PersonId(1),
        })
        .unwrap();
    assert_eq!(retained.len(), LIMIT);
    assert_eq!(
        retained
            .iter()
            .find(|p| p.key == "returning")
            .unwrap()
            .motive
            .goal,
        1000
    );
}

#[test]
fn final_order_matches_string_sort_across_small_and_full_collections() {
    for count in [0, 1, 17, 31, 32, 33, 64, 255, 256] {
        let mut b = Builder::new();
        plugin(&mut b).unwrap();
        b.plugin("test", 1).unwrap();
        b.request::<Act>(|_, _| Ok(())).unwrap();
        b.subscribe::<Review>("offers", move |tx, e| {
            for i in 0..count {
                offer(
                    tx,
                    e.person,
                    format!("key:{}:雪", i * 71 % 257),
                    motive(0, 1, 0, 0, 0),
                    Act,
                )?;
            }
            Ok(())
        })
        .unwrap();
        let mut h = b.build(1, 1).unwrap();
        let retained = h
            .dispatch(Collect {
                person: PersonId(1),
            })
            .unwrap();
        let mut expected: Vec<_> = (0..count)
            .map(|i| format!("key:{}:雪", i * 71 % 257))
            .collect();
        expected.sort();
        assert_eq!(
            retained.into_iter().map(|p| p.key).collect::<Vec<_>>(),
            expected
        );
    }
}
