//! Streaming memory comparison: workers, elements, iterations.
use rayon::prelude::*;
use std::time::Instant;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let workers: usize = args.first().map_or(Ok(1), |v| v.parse())?;
    let count: usize = args.get(1).map_or(Ok(10_000_000), |v| v.parse())?;
    let iterations: usize = args.get(2).map_or(Ok(20), |v| v.parse())?;
    if workers == 0 || count == 0 || iterations == 0 {
        return Err("invalid arguments".into());
    }
    let a: Vec<u64> = (0..count as u64).collect();
    let b: Vec<u64> = (0..count as u64).map(|i| i.wrapping_mul(3)).collect();
    let mut out = vec![0u64; count];
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let start = Instant::now();
    pool.install(|| {
        for iteration in 0..iterations {
            out.par_iter_mut()
                .zip(a.par_iter())
                .zip(b.par_iter())
                .for_each(|((out, &a), &b)| {
                    *out = a.wrapping_add(b).wrapping_add(iteration as u64);
                });
            std::hint::black_box(&out);
        }
    });
    let elapsed = start.elapsed().as_secs_f64();
    let bytes = count as f64 * iterations as f64 * 24.0;
    println!(
        "workers={workers} elements={count} iterations={iterations} elapsed_seconds={elapsed:.6} effective_gb_per_second={:.2} checksum={}",
        bytes / elapsed / 1e9,
        out[count - 1]
    );
    Ok(())
}
