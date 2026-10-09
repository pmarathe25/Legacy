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
        ["cancel"] => submit(sim, Command::Cancel),
        ["eat", source] => submit(sim, Command::Eat { source: InventoryId(source.parse()?) }),
        ["load", source, destination, quantity] => submit(sim, Command::Load { source: InventoryId(source.parse()?), destination: InventoryId(destination.parse()?), quantity: Quantity(quantity.parse()?) }),
        ["work", site] => submit(sim, Command::Work { site: SiteId(site.parse()?) }),
        ["tell", recipient, inventory] => submit(sim, Command::Tell { recipient: PersonId(recipient.parse()?), inventory: InventoryId(inventory.parse()?) }),
        ["save", path] => {
            let path = std::path::Path::new(path);
            let temporary = path.with_extension("save-tmp");
            std::fs::write(&temporary, sim.save()?)?;
            std::fs::rename(temporary, path)?;
            Ok("saved".into())
        }
        ["load", path] => {
            let loaded = Simulation::load(&std::fs::read(path)?, execution.clone())?;
            *sim = loaded;
            Ok("loaded".into())
        }
        ["help"] => Ok("status | run <integer>[ms|s|m|h|d] | move <area-id> | rest | eat <stock-id> | load <source> <destination> <fixed-point quantity> | work <site-id> | tell <person> <known-stock> | cancel | debug (omniscient) | save <path> | load <path> | quit".into()),
        _ => Err("unknown command; use help".into()),
    }
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
