use game_characters::{Candidate, Decision, choose};
mod preparation {
    pub mod batch;
    pub mod scoring;
}
use preparation::{batch::Batch, scoring::vector_backend};
use std::{hint::black_box, time::Instant};
fn weight(c: &mut Candidate) {
    c.reweight(|t| match t {
        "need" => 100,
        "goal" => 80,
        "value" => 130,
        "effort" => 150,
        _ => 50,
    });
}
fn run(rows: &[Candidate], width: usize, rounds: usize, mode: u8) -> (f64, u64) {
    let start = Instant::now();
    let mut sum = 0u64;
    let order: Vec<_> = (0..width).collect();
    for _ in 0..rounds {
        for group in rows.chunks(width) {
            let (chosen, expected) = if mode == 0 {
                let original = black_box(group).to_vec();
                let mut numeric = original.clone();
                for c in &mut numeric {
                    weight(c);
                }
                let chosen = choose(&numeric, Some(7), 5, false).unwrap();
                let expected = original.iter().find(|c| c.id == chosen.id).unwrap().need;
                (chosen, expected)
            } else {
                let mut batch = Batch::new(black_box(group).iter().map(|c| {
                    let mut c = *c;
                    weight(&mut c);
                    c
                }));
                if mode == 1 {
                    batch.score_portable();
                }
                let chosen = batch.choose(&order, Some(7), 5).unwrap();
                let expected = group.iter().find(|c| c.id == chosen.id).unwrap().need;
                (chosen, expected)
            };
            sum = sum.wrapping_add(chosen.id ^ chosen.score as u64 ^ expected as u64);
        }
    }
    (start.elapsed().as_secs_f64() * 1000., sum)
}
fn main() {
    println!(
        "backend={} candidate_bytes={}",
        vector_backend(),
        std::mem::size_of::<Candidate>()
    );
    let mut state = 1u64;
    for (people, width, rounds) in [
        (1, 16, 300000),
        (1, 64, 100000),
        (1, 256, 25000),
        (4096, 64, 24),
    ] {
        let rows: Vec<_> = (0..people * width)
            .map(|i| {
                let mut next = || {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    (state % 20000) as i64 - 10000
                };
                Candidate {
                    id: (i % width) as u64,
                    need: next(),
                    goal: next(),
                    value: next(),
                    effort: next(),
                    risk: next(),
                    survival: false,
                }
            })
            .collect();
        let mut validation = Batch::new(rows.iter().take(width).copied());
        validation.penalize(0, 0);
        let mut first = rows[0];
        validation.apply(0, &mut first);
        assert_eq!(first, rows[0]);
        assert_eq!(validation.id(0), rows[0].id);
        for pair in 0..3 {
            let mut result = [(0., 0); 3];
            let modes = if pair % 2 == 0 { [0, 1, 2] } else { [2, 1, 0] };
            for mode in modes {
                result[mode as usize] = run(&rows, width, rounds, mode);
            }
            assert!(result.iter().all(|r| r.1 == result[0].1));
            println!(
                "people={people} width={width} pair={pair} old_prepare_ms={:.3} columns_portable_ms={:.3} columns_auto_ms={:.3}",
                result[0].0, result[1].0, result[2].0
            );
        }
    }
}
