//! Attribute retained heap and peak RSS to checkpoint loading, advancement, and saving.
use game_core::*;
use game_performance::metrics::{Sample, rss};
use game_sim::Stop;
use std::{fs::File, io::BufReader};
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: memory_world SAVE [DAYS]")?;
    let days: u64 = args.next().as_deref().unwrap_or("0").parse()?;
    game_performance::metrics::metadata();
    #[cfg(feature = "allocation-profile")]
    assert!(game_diagnostics::install_allocation_counter(
        game_performance::metrics::allocation_count
    ));
    let sample = Sample::new();
    let mut reader = BufReader::new(File::open(path)?);
    let mut sim = game_assembly::Simulation::load_from(&mut reader, 1)?;
    drop(reader);
    sample.report("loaded");
    rss();
    #[cfg(feature = "allocation-stacks")]
    if std::env::var_os("LEGACY_ALLOCATION_STACKS").is_some() {
        game_performance::metrics::stacks::start();
    }
    let sample = Sample::new();
    if days > 0 {
        let target = sim.now().after(SimDuration(
            days.checked_mul(DAY.0).ok_or("duration overflow")?,
        ))?;
        loop {
            match sim.advance(target, 128)?.stop {
                Stop::Target => break,
                Stop::Budget => {}
                stop => return Err(format!("unexpected stop {stop:?}").into()),
            }
        }
        println!(
            "advance_requested={} advance_reached={}",
            target.0,
            sim.now().0
        );
    }
    sample.report("advanced");
    rss();
    #[cfg(feature = "allocation-stacks")]
    if let Ok(path) = std::env::var("LEGACY_ALLOCATION_STACKS") {
        let stacks = game_performance::metrics::stacks::finish();
        std::fs::write(path, serde_json::to_vec(&stacks)?)?;
    }
    let sample = Sample::new();
    sim.save_to(&mut std::io::sink())?;
    sample.report("saved_to_sink");
    rss();
    let sample = Sample::new();
    drop(sim);
    sample.report("released");
    rss();
    let sample = Sample::new();
    game_food_memory::retain_people(std::iter::empty());
    sample.report("cleared_supply_projections");
    rss();
    let spans = game_diagnostics::snapshot();
    if !spans.is_empty() {
        println!("{}", serde_json::to_string(&spans)?);
    }
    Ok(())
}
