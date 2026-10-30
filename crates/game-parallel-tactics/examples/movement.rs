//! cargo run --release -p game-parallel-tactics --example movement -- 1000000
use game_parallel_tactics::*;
use std::time::Instant;
fn main() {
    let count: usize = std::env::args()
        .nth(1)
        .unwrap_or("100000".into())
        .parse()
        .unwrap();
    let base = Tactics {
        maps: vec![
            Map::generate(
                1,
                1,
                Rules {
                    width: count as u32 + 4,
                    height: 100,
                    obstacle_count: 0,
                    max_obstacle_size: 1,
                    movement: 3,
                    attack_range: 5,
                },
            )
            .unwrap(),
        ],
        battles: vec![Battle {
            encounter: 1,
            region: 1,
            people: (0..count)
                .map(|i| Placement {
                    person: i as u64,
                    position: Point { x: i as u32, y: 1 },
                    last_move: None,
                })
                .collect(),
        }],
    };
    for amount in [1, count / 10, count] {
        let mut state = base.clone();
        let begin = Instant::now();
        let mut index = state.index().unwrap();
        let build = begin.elapsed().as_secs_f64() * 1000.;
        let mut sparse = Vec::new();
        let mut cloned = Vec::new();
        for tick in 0..5 {
            let moves: Vec<_> = (0..amount)
                .map(|i| Move {
                    encounter: 1,
                    person: i as u64,
                    destination: Point {
                        x: i as u32,
                        y: 1 + ((tick + 1) % 2) as u32,
                    },
                })
                .collect();
            let begin = Instant::now();
            let next = base.move_batch(&moves, tick).unwrap();
            cloned.push(begin.elapsed().as_secs_f64() * 1000.);
            std::hint::black_box(next);
            let begin = Instant::now();
            index.move_batch(&mut state, &moves, tick).unwrap();
            sparse.push(begin.elapsed().as_secs_f64() * 1000.);
        }
        sparse.sort_by(f64::total_cmp);
        cloned.sort_by(f64::total_cmp);
        println!(
            "people={count} moves={amount} index_ms={build:.3} clone_ms={:.3} sparse_ms={:.3}",
            cloned[2], sparse[2]
        );
        state.battles[0]
            .people
            .retain(|person| person.person % 2 == 0);
        index.membership_changed(
            &state,
            &[1],
            &(1..count as u64).step_by(2).collect::<Vec<_>>(),
        );
        let begin = Instant::now();
        index
            .move_batch(
                &mut state,
                &[Move {
                    encounter: 1,
                    person: 0,
                    destination: Point { x: 0, y: 3 },
                }],
                6,
            )
            .unwrap();
        println!(
            "post_casualty_first_move_ms={:.3}",
            begin.elapsed().as_secs_f64() * 1000.
        );
    }
}
