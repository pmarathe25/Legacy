mod cpu;
#[cfg(feature = "gpu")]
mod gpu;
use cpu::*;
use std::{hint::black_box, time::Instant};
fn main() {
    println!(
        "release={} arch={} numeric_candidate_bytes={} column_bytes_per_candidate=49",
        !cfg!(debug_assertions),
        std::env::consts::ARCH,
        std::mem::size_of::<Packed>()
    );
    if let Ok(cpu) = std::fs::read_to_string("/proc/cpuinfo")
        && let Some(line) = cpu.lines().find(|s| s.starts_with("model name"))
    {
        println!("{line}");
    }

    let populations: Vec<usize> = std::env::var("LEGACY_COMPUTE_POPULATIONS")
        .unwrap_or("1000,10000,20000,100000".into())
        .split(',')
        .map(|n| n.parse().unwrap())
        .collect();
    let repeats: usize = std::env::var("LEGACY_COMPUTE_REPEATS")
        .unwrap_or("5".into())
        .parse()
        .unwrap();
    #[cfg(feature = "gpu")]
    let gpu = gpu::Gpu::open();
    for population in populations {
        for choices in [4, 16, 64] {
            let input = Input::fixture(population, choices);
            let expected = input.reference();
            let start = Instant::now();
            let columns = Columns::pack(&input);
            let packing = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            let lanes = Lanes::pack(&input);
            let lane_packing = start.elapsed().as_secs_f64() * 1000.;
            #[cfg(feature = "gpu")]
            let mut prepared = gpu.as_ref().and_then(|gpu| gpu.prepare(&input));
            for repeat in 0..repeats {
                let start = Instant::now();
                let result = black_box(&input).packed();
                assert_eq!(result, expected);
                println!(
                    "population={population} choices={choices} repeat={repeat} backend=packed_aos elapsed_ms={:.3} packing_ms=0",
                    start.elapsed().as_secs_f64() * 1000.
                );
                for workers in [1, 4] {
                    let pool = rayon::ThreadPoolBuilder::new()
                        .num_threads(workers)
                        .build()
                        .unwrap();
                    for vector in [false, true] {
                        for tile in [128, 4096] {
                            let start = Instant::now();
                            let result = black_box(&columns).evaluate(
                                black_box(&input),
                                &pool,
                                vector,
                                tile,
                            );
                            let elapsed = start.elapsed().as_secs_f64() * 1000.;
                            assert_eq!(result, expected);
                            println!(
                                "population={population} choices={choices} repeat={repeat} backend=columns workers={workers} avx2={vector} tile={tile} elapsed_ms={elapsed:.3} packing_ms={packing:.3}"
                            );
                            let start = Instant::now();
                            let result =
                                black_box(&lanes).evaluate(black_box(&input), &pool, vector, tile);
                            let elapsed = start.elapsed().as_secs_f64() * 1000.;
                            assert_eq!(result, expected);
                            println!(
                                "population={population} choices={choices} repeat={repeat} backend=choice_major workers={workers} avx2={vector} tile={tile} elapsed_ms={elapsed:.3} packing_ms={lane_packing:.3}"
                            );
                        }
                    }
                }
                #[cfg(feature = "gpu")]
                if let Some(gpu) = &mut prepared {
                    gpu.evaluate(&input, &lanes, &expected, repeat);
                }
            }
        }
    }
}
