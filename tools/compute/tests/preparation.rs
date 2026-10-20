use game_characters::{Candidate, Decision, choose};
#[path = "../benches/preparation/batch.rs"]
mod batch;
#[path = "../benches/preparation/scoring.rs"]
mod scoring;
use batch::Batch;
fn fixtures(n: usize, seed: u64) -> Vec<Candidate> {
    let mut state = seed;
    (0..n)
        .map(|i| {
            let mut next = || {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                state as i64
            };
            Candidate {
                id: if i % 17 == 0 {
                    u64::MAX
                } else {
                    (i % 11) as u64
                },
                need: next(),
                goal: next(),
                value: next(),
                effort: next(),
                risk: next(),
                survival: i % 2 == 0,
            }
        })
        .collect()
}
#[test]
fn numeric_columns_preserve_saturation_ids_hysteresis_irregular_tails_and_retries() {
    assert!(["avx2", "portable"].contains(&scoring::vector_backend()));
    for seed in 0..48 {
        for n in [
            0, 1, 7, 8, 15, 16, 31, 32, 63, 64, 65, 127, 128, 255, 256, 257, 1023,
        ] {
            let mut rows = fixtures(n, seed);
            let mut batch = Batch::new(rows.iter().copied());
            let mut order: Vec<_> = (0..n).collect();
            for (i, row) in rows.iter_mut().enumerate() {
                let penalty = (i * 7) as i64;
                batch.penalize(i, penalty);
                row.risk = row.risk.saturating_add(penalty);
            }
            for portable in [false, true] {
                if portable {
                    batch.score_portable();
                }
                for _ in 0..3 {
                    let active: Vec<_> = order.iter().map(|i| rows[*i]).collect();
                    for current in [None, Some(0), Some(8), Some(u64::MAX)] {
                        for margin in [i64::MIN, -1, 0, 5, i64::MAX] {
                            assert_eq!(
                                batch.choose(&order, current, margin),
                                choose(&active, current, margin, false)
                            );
                        }
                    }
                    for &i in order.iter().take(3) {
                        let mut restored = rows[i];
                        batch.apply(i, &mut restored);
                        assert_eq!(restored, rows[i]);
                    }
                    if !order.is_empty() {
                        order.remove(order.len() / 2);
                    }
                }
            }
        }
    }
}
