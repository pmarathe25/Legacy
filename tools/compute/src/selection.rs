use crate::cpu::*;
use std::{hint::black_box, time::Instant};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Auto,
    Cpu,
    Gpu,
}
fn select(mode: Mode, cpu: f64, gpu: Option<f64>) -> Result<&'static str, String> {
    match mode {
        Mode::Gpu if gpu.is_none() => Err("requested GPU is unavailable for this batch".into()),
        Mode::Gpu => Ok("gpu"),
        Mode::Auto if gpu.is_some_and(|time| time < cpu * 0.8) => Ok("gpu"),
        _ => Ok("cpu"),
    }
}
fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}
fn measure(expected: &[Answer], f: impl FnOnce() -> Vec<Answer>) -> f64 {
    let start = Instant::now();
    let result = f();
    assert_eq!(
        result, expected,
        "calibration must preserve exact decisions"
    );
    drop(result);
    start.elapsed().as_secs_f64() * 1000.
}
pub fn run(populations: &[usize], repeats: usize) -> Result<(), String> {
    if repeats < 3 {
        return Err("calibration needs at least three repeats".into());
    }
    let mode = match std::env::var("LEGACY_COMPUTE_BACKEND")
        .as_deref()
        .unwrap_or("auto")
    {
        "auto" => Mode::Auto,
        "cpu" => Mode::Cpu,
        "gpu" => Mode::Gpu,
        _ => return Err("LEGACY_COMPUTE_BACKEND must be auto, cpu, or gpu".into()),
    };
    let setting = |key, default| {
        std::env::var(key)
            .map_or(Ok(default), |v| v.parse::<usize>())
            .map_err(|e| format!("invalid {key}: {e}"))
    };
    let choices = setting("LEGACY_COMPUTE_CHOICES", 16)?;
    let available = std::thread::available_parallelism().map_or(1, usize::from);
    let workers = setting("LEGACY_COMPUTE_WORKERS", available.min(4))?;
    if choices == 0 || !(1..=64).contains(&workers) || populations.contains(&0) {
        return Err("calibration requires positive populations/choices and 1..=64 workers".into());
    }
    if populations
        .iter()
        .any(|p| p.checked_mul(choices).is_none_or(|n| n > 8_000_000))
    {
        return Err("calibration is bounded to 8,000,000 candidates per batch".into());
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|e| e.to_string())?;
    #[cfg(feature = "gpu")]
    let gpu = (mode != Mode::Cpu).then(crate::gpu::Gpu::open).flatten();
    for &people in populations {
        let input = Input::fixture(people, choices);
        let expected = input.reference();
        #[cfg(feature = "gpu")]
        let mut prepared = gpu.as_ref().and_then(|gpu| gpu.prepare(&input));
        #[cfg(feature = "gpu")]
        if let Some(gpu) = &mut prepared {
            gpu.round_trip(&input, &Lanes::pack(&input), &expected);
        }
        let mut serial = Vec::new();
        let mut parallel = Vec::new();
        let mut columns = Vec::new();
        #[allow(unused_mut)]
        let mut gpu_samples: Vec<f64> = Vec::new();
        // Warm the worker pool and vector path before collecting comparable costs.
        assert_eq!(input.parallel(&pool), expected);
        assert_eq!(
            Lanes::pack(&input).evaluate(&input, &pool, true, 4096),
            expected
        );
        for repeat in 0..repeats {
            let packed = measure(&expected, || black_box(&input).packed());
            let threaded = measure(&expected, || black_box(&input).parallel(&pool));
            let vector = measure(&expected, || {
                Lanes::pack(black_box(&input)).evaluate(&input, &pool, true, 4096)
            });
            serial.push(packed);
            parallel.push(threaded);
            columns.push(vector);
            #[cfg(feature = "gpu")]
            if let Some(gpu) = &mut prepared {
                let start = Instant::now();
                let lanes = Lanes::pack(black_box(&input));
                gpu.round_trip(&input, &lanes, &expected);
                drop(lanes);
                gpu_samples.push(start.elapsed().as_secs_f64() * 1000.);
            }
            println!(
                "calibration_sample population={people} choices={choices} workers={workers} repeat={repeat} cpu_packed_ms={packed:.3} cpu_parallel_ms={threaded:.3} cpu_columns_with_projection_ms={vector:.3} gpu_with_projection_ms={:?}",
                gpu_samples.last()
            );
        }
        let serial = median(&mut serial);
        let parallel = median(&mut parallel);
        let columns = median(&mut columns);
        let cpu = serial.min(parallel).min(columns);
        let gpu = (!gpu_samples.is_empty()).then(|| median(&mut gpu_samples));
        let backend = select(mode, cpu, gpu)?;
        println!(
            "calibration population={people} choices={choices} workers={workers} backend={backend} automatic={} cpu_best_ms={cpu:.3} gpu_with_projection_ms={gpu:?} required_gpu_gain_percent=20",
            mode == Mode::Auto
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_requires_a_material_gain_and_honors_explicit_preferences() {
        assert_eq!(select(Mode::Auto, 10., Some(7.)).unwrap(), "gpu");
        for gpu in [None, Some(8.), Some(12.)] {
            assert_eq!(select(Mode::Auto, 10., gpu).unwrap(), "cpu");
        }
        assert_eq!(select(Mode::Cpu, 10., Some(1.)).unwrap(), "cpu");
        assert_eq!(select(Mode::Gpu, 10., Some(100.)).unwrap(), "gpu");
        assert!(select(Mode::Gpu, 10., None).is_err());
    }
}
