use game_api::*;
use game_sim::{Execution, Simulation};

/// Text parsing and display only. The same authoritative API serves every client.
pub fn execute(
    sim: &mut Simulation,
    line: &str,
    execution: &Execution,
) -> Result<String, Box<dyn std::error::Error>> {
    let words: Vec<_> = line.split_whitespace().collect();
    if let Some(command) = parse_game_command(&words)? {
        return submit(sim, command);
    }
    match words.as_slice() {
        [] => Ok(String::new()),
        ["status"] => Ok(serde_json::to_string(&sim.observe())?),
        ["combat"] => Ok(serde_json::to_string(&sim.observe().combat)?),
        ["pause"] => Ok(serde_json::to_string(&sim.observe())?),
        ["debug"] => Ok(format!("DEBUG OMNISCIENT {}", serde_json::to_string(sim.canonical_state())?)),
        ["run", duration] => Ok(serde_json::to_string(&sim.advance(parse_duration(duration)?)?)?),
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
        ["help"] => Ok("status | pause | run <integer>[ms|s|m|h|d] | move <area-id> | rest | eat <stock-id> | load <source> <destination> <fixed-point quantity> | work <site-id> | tell <person> <known-stock> | signal <food|company> | play <interest> | conceive <partner> | comfort <child> | care <child> <food-stock> | engage <person> | combat | turn <encounter> <attack|capture|release|breach|retreat|defend|wait|surrender> [target] [x-mm y-mm] | free <captive> | cancel | debug (omniscient) | save <path> | load <path> | quit".into()),
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

fn parse_combat_turn(
    action: &str,
    args: &[&str],
) -> Result<(Option<Point>, CombatAction), Box<dyn std::error::Error>> {
    let target =
        |s: &str| -> Result<PersonId, Box<dyn std::error::Error>> { Ok(PersonId(s.parse()?)) };
    let (action, used) = match action {
        "attack" => (
            CombatAction::Attack {
                target: target(args.first().ok_or("missing target")?)?,
            },
            1,
        ),
        "capture" => (
            CombatAction::Capture {
                target: target(args.first().ok_or("missing target")?)?,
            },
            1,
        ),
        "release" => (
            CombatAction::Release {
                target: target(args.first().ok_or("missing target")?)?,
            },
            1,
        ),
        "retreat" => (
            CombatAction::Retreat {
                destination: AreaId(args.first().ok_or("missing area")?.parse()?),
            },
            1,
        ),
        "breach" => (
            CombatAction::Breach {
                obstacle: args.first().ok_or("missing obstacle")?.parse()?,
            },
            1,
        ),
        "defend" => (CombatAction::Defend, 0),
        "wait" => (CombatAction::Wait, 0),
        "surrender" => (CombatAction::Surrender, 0),
        _ => return Err("unknown combat action".into()),
    };
    let movement = match &args[used..] {
        [] => None,
        [x, y] => Some(Point {
            x: x.parse()?,
            y: y.parse()?,
        }),
        _ => return Err("movement needs both coordinates".into()),
    };
    Ok((movement, action))
}

fn parse_game_command(words: &[&str]) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    let interest = |name: &str| -> Result<ChildhoodInterest, Box<dyn std::error::Error>> {
        match name {
            "food" => Ok(ChildhoodInterest::Food),
            "company" => Ok(ChildhoodInterest::Company),
            "movement" => Ok(ChildhoodInterest::Movement),
            "craft" => Ok(ChildhoodInterest::Craft),
            _ => Err("unknown childhood interest".into()),
        }
    };
    let command = match words {
        ["move", area] => Command::Move {
            destination: AreaId(area.parse()?),
        },
        ["comfort", child] => Command::Comfort {
            child: PersonId(child.parse()?),
        },
        ["signal", name] => Command::Signal {
            preference: interest(name)?,
        },
        ["play", name] => Command::Play {
            interest: interest(name)?,
        },
        ["rest"] => Command::Rest,
        ["cancel"] => Command::Cancel,
        ["engage", target] => Command::Engage {
            target: PersonId(target.parse()?),
        },
        ["free", target] => Command::FreeCaptive {
            target: PersonId(target.parse()?),
        },
        ["conceive", partner] => Command::Conceive {
            partner: PersonId(partner.parse()?),
        },
        ["care", child, source] => Command::Care {
            child: PersonId(child.parse()?),
            source: InventoryId(source.parse()?),
        },
        ["eat", source] => Command::Eat {
            source: InventoryId(source.parse()?),
        },
        ["load", source, destination, quantity] => Command::Load {
            source: InventoryId(source.parse()?),
            destination: InventoryId(destination.parse()?),
            quantity: Quantity(quantity.parse()?),
        },
        ["work", site] => Command::Work {
            site: SiteId(site.parse()?),
        },
        ["tell", recipient, inventory] => Command::Tell {
            recipient: PersonId(recipient.parse()?),
            inventory: InventoryId(inventory.parse()?),
        },
        ["turn", encounter, action, args @ ..] => {
            let (movement, action) = parse_combat_turn(action, args)?;
            Command::CombatTurn {
                encounter: EncounterId(encounter.parse()?),
                movement,
                action,
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(command))
}

pub enum SessionOutput {
    Text(String),
    Advancing(game_sim::session::RunHandle),
}

pub fn execute_session(
    session: &game_sim::session::Session,
    line: &str,
) -> Result<SessionOutput, Box<dyn std::error::Error>> {
    let words: Vec<_> = line.split_whitespace().collect();
    if let Some(command) = parse_game_command(&words)? {
        return Ok(SessionOutput::Text(serde_json::to_string(
            &session.submit_command(command)?,
        )?));
    }
    let text = match words.as_slice() {
        [] => String::new(),
        ["status"] => serde_json::to_string(&session.observe()?)?,
        ["combat"] => serde_json::to_string(&session.observe()?.combat)?,
        ["pause"] => serde_json::to_string(&session.pause()?)?,
        ["debug"] => format!("DEBUG OMNISCIENT {}",session.debug_json()?),
        ["run",duration] => return Ok(SessionOutput::Advancing(session.run(parse_duration(duration)?)?)),
        ["save",path] => { save_file(std::path::Path::new(path),&session.save()?)?; "saved".into() },
        ["load",path] => { session.load(std::fs::read(path)?)?; "loaded".into() },
        ["help"] => "status | pause | run <integer>[ms|s|m|h|d] | move <area-id> | rest | eat <stock-id> | load <source> <destination> <fixed-point quantity> | work <site-id> | tell <person> <known-stock> | signal <food|company> | play <interest> | conceive <partner> | comfort <child> | care <child> <food-stock> | engage <person> | combat | turn <encounter> <attack|capture|release|breach|retreat|defend|wait|surrender> [target] [x-mm y-mm] | free <captive> | cancel | debug (omniscient) | save <path> | load <path> | quit".into(),
        _ => return Err("unknown command; use help".into()),
    };
    Ok(SessionOutput::Text(text))
}
