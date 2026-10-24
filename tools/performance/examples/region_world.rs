//! Spatial opportunity probe; area labels and historical links are not dependency proofs.
#[cfg(feature = "mimalloc")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
use game_core::*;
use game_sim::{Host, Stop};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
fn snapshot(host: &Host) -> std::result::Result<Value, Box<dyn std::error::Error>> {
    let positions = host.get::<game_transport::Positions>()?;
    let living: BTreeSet<_> = host
        .get::<game_physiology::Bodies>()?
        .people
        .iter()
        .filter(|(_, b)| b.alive)
        .map(|(p, _)| *p)
        .collect();
    let mut population = BTreeMap::<AreaId, usize>::new();
    let mut travelling = 0;
    let mut stores = [0u64; 2];
    for &p in &living {
        let Ok(area) = game_transport::area(positions, p) else {
            travelling += 1;
            continue;
        };
        *population.entry(area).or_default() += 1;
        if let Some(facts) = host.get::<game_knowledge::Knowledge>()?.0.get(&p) {
            game_food_memory::with_stores(p, facts, host.get().unwrap(), |known| {
                for store in known.all() {
                    if let Some(location) = &store.location {
                        let remote = location.carrier != Some(p) && location.area != area;
                        stores[usize::from(remote)] += 1;
                    }
                }
            });
        }
    }
    let mut relationships = [0u64; 3];
    for (p, peers) in &host.get::<game_relationships::Social>()?.relationships {
        if !living.contains(p) {
            continue;
        }
        for (q, _) in peers {
            if !living.contains(q) {
                continue;
            }
            let index = match (
                game_transport::area(positions, *p),
                game_transport::area(positions, *q),
            ) {
                (Ok(a), Ok(b)) => usize::from(a != b),
                _ => 2,
            };
            relationships[index] += 1;
        }
    }
    Ok(
        json!({"living":living.len(),"at_area":population,"travelling_or_unplaced":travelling,
        "remembered_stores_local_remote":stores,
        "historical_directed_relationships_local_remote_unlocated":relationships}),
    )
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("LEGACY_PROFILE").is_none() {
        return Err("set LEGACY_PROFILE=1".into());
    }
    let mut args = std::env::args().skip(1);
    let source = args
        .next()
        .ok_or("usage: region_world SAVE DAYS OUTPUT [MAX_BOUNDARIES]")?;
    let days: u64 = args.next().ok_or("missing days")?.parse()?;
    let output = std::path::PathBuf::from(args.next().ok_or("missing output")?);
    let limit: u64 = args.next().as_deref().unwrap_or("1000000").parse()?;
    std::fs::create_dir(&output)?;
    let mut sim = game_assembly::Simulation::load_from(&mut std::fs::File::open(&source)?, 1)?;
    let target = sim.now().after(SimDuration(
        days.checked_mul(DAY.0).ok_or("duration overflow")?,
    ))?;
    let initial = snapshot(sim.debug_host())?;
    let mut regions = BTreeMap::<AreaId, (u64, u64)>::new();
    let mut cohorts = BTreeMap::<(SimTime, u8), BTreeMap<AreaId, u64>>::new();
    let mut queued_regions = BTreeMap::<usize, u64>::new();
    let mut requests = BTreeMap::<String, (u64, u64)>::new();
    let mut full = 0;
    let mut attributed = 0;
    let mut boundaries = 0;
    let mut complete = false;
    let mut queued_other_region = 0;
    let mut located_resume_boundaries = 0;
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
        let positions = host.get::<game_transport::Positions>()?;
        let area = first
            .request
            .request::<game_characters::Resume>()
            .and_then(|r| game_transport::area(positions, r.person).ok());
        let ready: BTreeSet<_> = host
            .scheduled()
            .take_while(|j| (j.at, j.priority) == (at, priority))
            .filter_map(|j| j.request.request::<game_characters::Resume>())
            .filter_map(|r| game_transport::area(positions, r.person).ok())
            .collect();
        if let Some(here) = area {
            located_resume_boundaries += 1;
            *queued_regions.entry(ready.len()).or_default() += 1;
            if ready.iter().any(|a| *a != here) {
                queued_other_region += 1;
            }
        }
        let started = std::time::Instant::now();
        assert_eq!(sim.advance(target, 1)?.stop, Stop::Budget);
        let ns = u64::try_from(started.elapsed().as_nanos())?;
        let timings = game_diagnostics::snapshot();
        let total = timings.get("full character review").map_or(0, |m| m.calls);
        let reviews = total - full;
        full = total;
        if let Some(area) = area {
            let entry = regions.entry(area).or_default();
            entry.0 += reviews;
            entry.1 += ns;
            *cohorts
                .entry((at, priority))
                .or_default()
                .entry(area)
                .or_default() += reviews;
            attributed += reviews;
        }
        let entry = requests.entry(key).or_default();
        entry.0 += 1;
        entry.1 += ns;
        boundaries += 1;
    }
    let mut busy_regions = BTreeMap::<usize, u64>::new();
    for regions in cohorts.values() {
        let count = regions.values().filter(|n| **n > 0).count();
        if count > 0 {
            *busy_regions.entry(count).or_default() += 1;
        }
    }
    let largest = regions
        .values()
        .map(|(reviews, _)| *reviews)
        .max()
        .unwrap_or(0);
    let report = json!({"source":source,"days":days,"workers":1,"complete":complete,
        "boundaries":boundaries,"full_reviews":full,"reviews_attributed_to_resume_area":attributed,
        "unattributed_reviews":full-attributed,"regions_reviews_instrumented_ns":regions,
        "located_resume_boundaries":located_resume_boundaries,
        "resume_boundaries_with_another_ready_area":queued_other_region,
        "queued_resume_area_count_histogram":queued_regions,"executed_phase_area_count_histogram":busy_regions,
        "largest_area_review_share":if attributed>0 {largest as f64/attributed as f64}else{0.0},
        "requests_boundaries_instrumented_ns":requests,"initial":initial,"final":snapshot(sim.debug_host())?,
        "limitations":["Areas are candidate region shards, not demonstrated independent components.",
            "Attribution covers explicit Resume roots; nested reviews and travel can be unlocated.",
            "Historical links and remembered remote stores are potential dependencies, not observed accesses.",
            "Same-time or queued work can conflict; row reads, writes, global IDs and effects need validation.",
            "Instrumented durations are not speed comparisons or a parallel engine benchmark."]});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    sim.save_to(&mut std::fs::File::create(output.join("checkpoint.json"))?)?;
    println!(
        "{}",
        json!({"boundaries":boundaries,"full_reviews":full,"complete":complete})
    );
    Ok(())
}
