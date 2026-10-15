use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
#[cfg(all(feature = "mimalloc", not(feature = "allocation-profile")))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
#[cfg(feature = "allocation-profile")]
mod allocator {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout};
    #[cfg(feature = "mimalloc")]
    const BASE: mimalloc::MiMalloc = mimalloc::MiMalloc;
    #[cfg(not(feature = "mimalloc"))]
    const BASE: std::alloc::System = std::alloc::System;
    struct Meter;
    // Counts requested bytes; allocator metadata and OS thread stacks are covered
    // separately by process RSS. Only this optional measurement shim uses unsafe.
    unsafe impl GlobalAlloc for Meter {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr = unsafe { BASE.alloc(layout) };
            if !ptr.is_null() {
                ALLOCS.fetch_add(1, Ordering::Relaxed);
                let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
                PEAK.fetch_max(live, Ordering::Relaxed);
            }
            ptr
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            unsafe { BASE.dealloc(ptr, layout) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: Meter = Meter;
}
pub struct Sample {
    start: Instant,
    allocations: usize,
}
impl Default for Sample {
    fn default() -> Self {
        Self::new()
    }
}
impl Sample {
    pub fn new() -> Self {
        PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
        Self {
            start: Instant::now(),
            allocations: ALLOCS.load(Ordering::Relaxed),
        }
    }
    pub fn report(&self, phase: &str) {
        println!(
            "phase={phase} elapsed_ms={:.3} allocation_profile={} allocations={} live_bytes={} peak_bytes={}",
            self.start.elapsed().as_secs_f64() * 1000.,
            cfg!(feature = "allocation-profile"),
            ALLOCS.load(Ordering::Relaxed) - self.allocations,
            LIVE.load(Ordering::Relaxed),
            PEAK.load(Ordering::Relaxed)
        );
    }
}
pub fn metadata() {
    println!(
        "arch={} os={} package={} available_workers={} release={} allocation_profile={} allocator={}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        env!("CARGO_PKG_VERSION"),
        std::thread::available_parallelism().map_or(1, usize::from),
        !cfg!(debug_assertions),
        cfg!(feature = "allocation-profile"),
        if cfg!(feature = "mimalloc") {
            "mimalloc"
        } else {
            "system"
        }
    );
    for (path, prefixes) in [
        ("/proc/cpuinfo", vec!["model name"]),
        ("/proc/meminfo", vec!["MemTotal"]),
        ("/proc/self/status", vec!["VmHWM", "VmRSS"]),
    ] {
        if let Ok(source) = std::fs::read_to_string(path) {
            for prefix in prefixes {
                if let Some(line) = source.lines().find(|line| line.starts_with(prefix)) {
                    println!("{line}");
                }
            }
        }
    }
    if let Ok(output) = std::process::Command::new("rustc").arg("-Vv").output() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
}
pub fn rss() {
    if let Ok(source) = std::fs::read_to_string("/proc/self/status") {
        for line in source
            .lines()
            .filter(|s| s.starts_with("VmRSS") || s.starts_with("VmHWM"))
        {
            println!("{line}");
        }
    }
}
pub fn percentile(sorted: &[f64], pct: usize) -> f64 {
    sorted[(sorted.len() - 1) * pct / 100]
}
