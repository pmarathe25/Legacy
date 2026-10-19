//! Bounded diagnostic replay of a raw saved world, without constructing a history ledger.
use game_core::*;
use game_sim::Stop;
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: resume_world SAVE [DAYS] [OUTPUT]")?;
    let days: u64 = args.next().as_deref().unwrap_or("1").parse()?;
    let output = args.next();
    let mut sim = game_assembly::Simulation::load(&std::fs::read_to_string(path)?, 1)?;
    let start = sim.now();
    let target = start.after(SimDuration(
        days.checked_mul(DAY.0).ok_or("duration overflow")?,
    ))?;
    let mut previous = start;
    let mut stagnant = 0;
    let mut batches = 0;
    loop {
        let result = sim.advance(target, 128)?;
        batches += 1;
        if result.stop == Stop::Target {
            break;
        }
        if result.stop != Stop::Budget {
            return Err(format!("unexpected stop: {:?}", result.stop).into());
        }
        stagnant = if sim.now() == previous {
            stagnant + 1
        } else {
            0
        };
        if stagnant >= 1000 {
            return Err(
                format!("no clock progress after 128,000 commits at {:?}", sim.now()).into(),
            );
        }
        previous = sim.now();
    }
    if let Some(path) = output {
        std::fs::write(path, sim.save()?)?
    }
    println!(
        "advanced {} days in {} batches; now {:?}",
        (sim.now().0 - start.0) / DAY.0,
        batches,
        sim.now()
    );
    let timings = game_diagnostics::snapshot();
    if !timings.is_empty() {
        println!("{}", serde_json::to_string(&timings)?);
    }
    Ok(())
}
