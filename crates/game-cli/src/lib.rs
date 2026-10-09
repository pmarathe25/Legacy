use game_api::*;
use game_sim::{Execution, Simulation};

/// Text parsing and display only. The same authoritative API serves every client.
pub fn execute(
    sim: &mut Simulation,
    line: &str,
    execution: &Execution,
) -> Result<String, Box<dyn std::error::Error>> {
    let words: Vec<_> = line.split_whitespace().collect();
    match words.as_slice() {
        [] => Ok(String::new()),
        ["status"] => Ok(serde_json::to_string(&sim.observe())?),
        ["debug"] => Ok(format!("DEBUG OMNISCIENT {}", serde_json::to_string(sim.canonical_state())?)),
        ["run", duration] => Ok(serde_json::to_string(&sim.advance(parse_duration(duration)?)?)?),
        ["move", area] => {
            let receipt = sim.submit(Submission { caller_sequence: sim.next_caller_sequence()?, command: Command::Move { destination: AreaId(area.parse()?) } })?;
            Ok(serde_json::to_string(&receipt)?)
        }
        ["rest"] => submit(sim, Command::Rest),
        ["conceive", partner] => submit(sim, Command::Conceive { partner: PersonId(partner.parse()?) }),
        ["care", child, source] => submit(sim, Command::Care { child: PersonId(child.parse()?), source: InventoryId(source.parse()?) }),
        ["cancel"] => submit(sim, Command::Cancel),
        ["eat", source] => submit(sim, Command::Eat { source: InventoryId(source.parse()?) }),
        ["load", source, destination, quantity] => submit(sim, Command::Load { source: InventoryId(source.parse()?), destination: InventoryId(destination.parse()?), quantity: Quantity(quantity.parse()?) }),
        ["work", site] => submit(sim, Command::Work { site: SiteId(site.parse()?) }),
        ["tell", recipient, inventory] => submit(sim, Command::Tell { recipient: PersonId(recipient.parse()?), inventory: InventoryId(inventory.parse()?) }),
        ["save", path] => {
            let path = std::path::Path::new(path);
            save_file(path, &sim.save()?)?;
            Ok("saved".into())
        }
        ["load", path] => {
            let loaded = Simulation::load(&std::fs::read(path)?, execution.clone())?;
            *sim = loaded;
            Ok("loaded".into())
        }
        ["help"] => Ok("status | run <integer>[ms|s|m|h|d] | move <area-id> | rest | eat <stock-id> | load <source> <destination> <fixed-point quantity> | work <site-id> | tell <person> <known-stock> | conceive <partner> | care <child> <food-stock> | cancel | debug (omniscient) | save <path> | load <path> | quit".into()),
        _ => Err("unknown command; use help".into()),
    }
}

fn save_file(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "save needs a filename")
    })?;
    let temporary = path.with_file_name(format!(
        ".{}.{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn submit(sim: &mut Simulation, command: Command) -> Result<String, Box<dyn std::error::Error>> {
    let receipt = sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence()?,
        command,
    })?;
    Ok(serde_json::to_string(&receipt)?)
}

pub fn parse_duration(value: &str) -> Result<SimDuration, Box<dyn std::error::Error>> {
    let split = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    let amount: u64 = value[..split].parse()?;
    let multiplier = match &value[split..] {
        "" | "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return Err("unknown duration unit".into()),
    };
    Ok(SimDuration(
        amount.checked_mul(multiplier).ok_or(Error::Overflow)?,
    ))
}
