//! Diagnose actual scheduled boundaries; this instrumentation is not a speed benchmark.
#[cfg(feature = "mimalloc")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "access-trace")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use game_contracts::access_trace;
    use game_core::*;
    use game_sim::Stop;
    use serde_json::json;
    use std::{collections::BTreeMap, io::Write, time::Instant};

    if std::env::var_os("LEGACY_PROFILE").is_none() {
        return Err("set LEGACY_PROFILE=1 to capture reviews inside each boundary".into());
    }
    let mut args = std::env::args().skip(1);
    let source = args
        .next()
        .ok_or("usage: parallelism_world SAVE DAYS OUTPUT [MAX_BOUNDARIES]")?;
    let days: u64 = args.next().ok_or("missing days")?.parse()?;
    let output = std::path::PathBuf::from(args.next().ok_or("missing output directory")?);
    let limit: u64 = args.next().as_deref().unwrap_or("1000000").parse()?;
    std::fs::create_dir(&output)?;
    let mut sim = game_assembly::Simulation::load_from(&mut std::fs::File::open(&source)?, 1)?;
    let target = sim.now().after(SimDuration(
        days.checked_mul(DAY.0).ok_or("duration overflow")?,
    ))?;
    let mut trace = std::io::BufWriter::new(std::fs::File::create(output.join("trace.jsonl"))?);
    let mut jobs: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let mut times: BTreeMap<SimTime, (u64, u64)> = BTreeMap::new();
    let mut widths: BTreeMap<usize, (u64, u64)> = BTreeMap::new();
    let mut resources: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    let mut previous: Option<(SimTime, access_trace::Accesses)> = None;
    let mut adjacent_pairs = 0;
    let mut adjacent_conflicts = 0;
    let mut boundaries = 0;
    let mut complete = false;
    let mut review_calls = 0;
    let mut full_review_calls = 0;
    while boundaries < limit {
        let host = sim.debug_host();
        let Some(first) = host.scheduled().next().filter(|j| j.at <= target) else {
            assert_eq!(sim.advance(target, 1)?.stop, Stop::Target);
            complete = true;
            break;
        };
        let at = first.at;
        let priority = first.priority;
        let key = first.request.key().to_owned();
        let request = serde_json::to_value(&first.request)?;
        let width = host
            .scheduled()
            .take_while(|j| (j.at, j.priority) == (at, priority))
            .count();
        access_trace::begin();
        let start = Instant::now();
        let progress = sim.advance(target, 1)?;
        let ns = u64::try_from(start.elapsed().as_nanos())?;
        let accesses = access_trace::finish();
        if progress.stop != Stop::Budget {
            return Err(format!("unexpected stop: {:?}", progress.stop).into());
        }
        let timings = game_diagnostics::snapshot();
        let calls = timings.get("character review").map_or(0, |m| m.calls);
        let reviews = calls.saturating_sub(review_calls);
        review_calls = calls;
        let calls = timings.get("full character review").map_or(0, |m| m.calls);
        let full_reviews = calls.saturating_sub(full_review_calls);
        full_review_calls = calls;
        if let Some((last_at, last)) = &previous
            && *last_at == at
        {
            adjacent_pairs += 1;
            if !last.writes.is_disjoint(&accesses.reads)
                || !last.writes.is_disjoint(&accesses.writes)
                || !last.reads.is_disjoint(&accesses.writes)
            {
                adjacent_conflicts += 1;
            }
        }
        serde_json::to_writer(
            &mut trace,
            &json!({"at":at,"priority":priority,"request":request,
                "queued_same_phase":width,"nanoseconds":ns,"reviews":reviews,"full_reviews":full_reviews,
                "reads":accesses.reads,"writes":accesses.writes}),
        )?;
        writeln!(trace)?;
        for key in &accesses.reads {
            resources.entry(key).or_default().0 += 1;
        }
        for key in &accesses.writes {
            resources.entry(key).or_default().1 += 1;
        }
        let row = jobs.entry(key).or_insert_with(|| {
            json!({"boundaries":0,"nanoseconds":0,"reviews":0,"full_reviews":0,
                "resource_reads":0,"resource_writes":0})
        });
        for (field, value) in [
            ("boundaries", 1),
            ("nanoseconds", ns),
            ("reviews", reviews),
            ("full_reviews", full_reviews),
            ("resource_reads", accesses.reads.len() as u64),
            ("resource_writes", accesses.writes.len() as u64),
        ] {
            row[field] = json!(row[field].as_u64().unwrap() + value);
        }
        let entry = times.entry(at).or_default();
        entry.0 += 1;
        entry.1 += ns;
        let entry = widths.entry(width).or_default();
        entry.0 += 1;
        entry.1 += ns;
        previous = Some((at, accesses));
        boundaries += 1;
    }
    trace.flush()?;
    let mut cohorts: BTreeMap<u64, (u64, u64)> = BTreeMap::new();
    for (count, ns) in times.values() {
        let entry = cohorts.entry(*count).or_default();
        entry.0 += 1;
        entry.1 += ns;
    }
    let report = json!({"source":source,"workers":1,"complete":complete,
        "target":target,"now":sim.now(),"boundaries":boundaries,
        "same_timestamp_cohorts":cohorts,"queued_same_phase_widths":widths,
        "adjacent_same_time_pairs":adjacent_pairs,
        "adjacent_resource_conflicts":adjacent_conflicts,
        "resources_read_write_boundaries":resources,"requests":jobs,
        "feature_spans":game_diagnostics::snapshot(),
        "limitations":["Instrumented timings are not speed comparisons.",
            "Access sets cover resources, not individual rows; writes include journaling housekeeping.",
            "Queue, identity allocation and notice ordering are additional dependencies.",
            "A root boundary can contain several sequential requests or character reviews.",
            "Equal timestamps do not imply independence; newly scheduled earlier priorities still take precedence."]});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    sim.save_to(&mut std::fs::File::create(output.join("checkpoint.json"))?)?;
    println!(
        "{}",
        serde_json::to_string(
            &json!({"boundaries":boundaries,"complete":complete,"now":sim.now()})
        )?
    );
    Ok(())
}

#[cfg(not(feature = "access-trace"))]
fn main() {
    eprintln!("build this diagnostic with --features access-trace");
    std::process::exit(2);
}
