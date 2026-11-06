use game_parallel_economy_work::reserve;
use rayon::prelude::*;

#[test]
fn reservations_are_sorted_unique_for_unsorted_overlapping_inputs() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut busy = vec![7, 2, 2, 11, 1];
                reserve(&mut busy, [11, 5, 3, 2, 5]);
                busy
            })
    };
    assert_eq!(run(1), vec![1, 2, 3, 5, 7, 11]);
    assert_eq!(run(1), run(4));
    let run_large = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut busy: Vec<_> = (0..100_000).rev().map(|n| n * 2).collect();
                busy.extend([8, 8, 4096]);
                let actors = (0..100_000).rev().flat_map(|n| [n * 2 + 1, n * 2]);
                reserve(&mut busy, actors);
                busy
            })
    };
    assert_eq!(run_large(1), (0..200_000).collect::<Vec<_>>());
    assert_eq!(run_large(1), run_large(4));
}

#[test]
fn reservations_match_sorting_across_sizes_and_unknown_iterator_lengths() {
    for count in [0, 1, 2047, 2048, 4095, 4096, 4097, 65_535, 131_073] {
        let original: Vec<_> = (0..count).map(|n| (n * 37 + 13) % (count + 7)).collect();
        let additions: Vec<_> = (0..count).map(|n| (n * 53 + 11) % (count + 7)).collect();
        let mut expected = original.clone();
        expected.extend_from_slice(&additions);
        expected.sort_unstable();
        expected.dedup();
        for workers in [1, 4] {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut busy = original.clone();
                    let mut iter = additions.iter().copied();
                    reserve(&mut busy, std::iter::from_fn(|| iter.next()));
                    assert_eq!(busy, expected, "count={count}, workers={workers}");
                });
        }
    }
}

#[test]
#[ignore = "manual reservation benchmark"]
fn concentrated_reservation_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000u64);
    let initial: Vec<_> = (0..count).map(|n| n * 2).collect();
    let additions: Vec<_> = (0..count).map(|n| n * 2 + 1).collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            for case in ["empty", "disjoint", "overlap"] {
                let actors = match case {
                    "empty" => &[][..],
                    "disjoint" => additions.as_slice(),
                    _ => initial.as_slice(),
                };
                for mode in ["sort", "merge"] {
                    let mut samples = Vec::new();
                    for round in 0..8 {
                        let mut busy = initial.clone();
                        let now = std::time::Instant::now();
                        if mode == "sort" {
                            busy.extend(actors.iter().copied());
                            busy.par_sort_unstable();
                            busy.dedup();
                        } else {
                            reserve(&mut busy, actors.iter().copied());
                        }
                        let elapsed = now.elapsed();
                        assert_eq!(
                            busy.len(),
                            if case == "disjoint" {
                                count as usize * 2
                            } else {
                                count as usize
                            }
                        );
                        if round > 0 {
                            samples.push(elapsed);
                        }
                    }
                    samples.sort();
                    println!(
                        "workers={workers} busy={count} case={case} mode={mode} elapsed_ms={:.3}",
                        samples[samples.len() / 2].as_secs_f64() * 1000.0
                    );
                }
            }
        });
}
