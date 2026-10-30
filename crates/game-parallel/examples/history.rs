//! Independent history runner. No engine mutation or undocumented legacy migration.
mod history_support;
use history_support::{Checkpoint, checkpoint, scenario::Scenario};
use std::path::PathBuf;
fn main() {
    if let Err(error) = execute() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn execute() -> Result<(), String> {
    let mut scenario = None;
    let mut resume = None;
    let mut output = None;
    let mut ticks = None;
    let mut stop = None;
    let mut workers = 4;
    let mut args = std::env::args().skip(1);
    while let Some(key) = args.next() {
        if key == "--help" {
            println!(
                "game-parallel history: --scenario FILE | --resume COMMITTED_DIR; --output NEW_DIR [--workers 1..64] [--ticks N (new scenario only)] [--stop-file FILE]. Calendar comes from scenario/checkpoint; ticks do not imply years. Output includes bounded diagnostic snapshots and actual family receipts; this is not a complete gameplay journal."
            );
            return Ok(());
        }
        let value = args.next().ok_or("missing argument value")?;
        match key.as_str() {
            "--scenario" => scenario = Some(PathBuf::from(value)),
            "--resume" => resume = Some(PathBuf::from(value)),
            "--output" => output = Some(PathBuf::from(value)),
            "--stop-file" => stop = Some(PathBuf::from(value)),
            "--ticks" => ticks = Some(value.parse::<u64>().map_err(|e| e.to_string())?),
            "--workers" => workers = value.parse::<usize>().map_err(|e| e.to_string())?,
            _ => return Err(format!("unknown argument {key}")),
        }
    }
    let output = output.ok_or("--output must name a new directory")?;
    let state = match (scenario, resume) {
        (Some(path), None) => {
            let (scenario, world) = Scenario::load(&path)?;
            Checkpoint::new(scenario, world, ticks)?
        }
        (None, Some(path)) if ticks.is_none() => checkpoint::load(&path)?,
        _ => {
            return Err(
                "choose exactly one scenario or resume; resume retains the saved target".into(),
            );
        }
    };
    let state = history_support::run(state, workers, &output, |_| {
        stop.as_ref().is_some_and(|p| p.exists())
    })?;
    println!(
        "Committed tick {} of {} to {}. Calendar: {:?}. Experimental engine; consult documented feature scope.",
        state.world.tick,
        state.goal_tick,
        output.display(),
        state.calendar.date(state.world.tick)?
    );
    Ok(())
}
