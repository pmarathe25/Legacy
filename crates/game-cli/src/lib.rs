use game_api::*;
use game_sim::{Execution, Simulation};

/// Text parsing and display only. The same authoritative API serves every client.
pub fn execute(
    sim: &mut Simulation,
    line: &str,
    execution: &Execution,
) -> Result<String, Box<dyn std::error::Error>> {
    let words: Vec<_> = line.split_whitespace().collect();
    if let Some(command) = parse_line_command(line, &words)? {
        return submit(sim, command);
    }
    match words.as_slice() {
        [] => Ok(String::new()),
        ["status"] => Ok(serde_json::to_string(&sim.observe())?),
        ["lineage"] => Ok(serde_json::to_string(&sim.observe().lineage)?),
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
        ["help"] => Ok("status | pause | run <integer>[ms|s|m|h|d] | move <area-id> | rest | eat <stock-id> | carry <source> <quantity> | load <source> <destination> <fixed-point quantity> | farm <field-id> <prepare|plant|tend|irrigate|harvest|fallow> | work <site-id> | tell <person> <known-stock> | learn <teacher> <invocation|craft|travel> | guardian <person> <rest|move area|work site|food source quantity> | signal <food|company> | play <interest> | take <source> <cargo> <quantity> | request-access <stock> | recognize <claim> | contest <claim> | report-incident <person> <incident> | command <JSON Command> | social <JSON action> | enforcement <JSON action> | health <JSON action> | ecology <JSON action> | mission <JSON action> | invocation <JSON action> | project <JSON action> | institution <JSON action> | property <JSON action> | agreement <JSON action> | conceive <partner> | comfort <child> | care <child> <food-stock> | engage <person> | combat | lineage | turn <encounter> <attack|capture|release|breach|retreat|defend|wait|surrender> [target] [x-mm y-mm] | escape | restrain <person> | yield <captor> | withdraw-yield | free <captive> | cancel | debug (omniscient) | save <path> | load <path> | quit".into()),
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

fn parse_line_command(
    line: &str,
    words: &[&str],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    let trimmed = line.trim_start();
    let (verb, body) = trimmed
        .split_once(char::is_whitespace)
        .unwrap_or((trimmed, ""));
    let command = match verb {
        "command" => Some(serde_json::from_str(body)?),
        "social" => Some(Command::Social {
            action: serde_json::from_str(body)?,
        }),
        "enforcement" => Some(Command::Enforcement {
            action: serde_json::from_str(body)?,
        }),
        "health" => Some(Command::Health {
            action: serde_json::from_str(body)?,
        }),
        "ecology" => Some(Command::Ecology {
            action: serde_json::from_str(body)?,
        }),
        "mission" => Some(Command::Mission {
            action: serde_json::from_str(body)?,
        }),
        "invocation" => Some(Command::Invocation {
            action: serde_json::from_str(body)?,
        }),
        "project" => Some(Command::Project {
            action: serde_json::from_str(body)?,
        }),
        "institution" => Some(Command::Institution {
            action: serde_json::from_str(body)?,
        }),
        "property" => Some(Command::Property {
            action: serde_json::from_str(body)?,
        }),
        "agreement" => Some(Command::Agreement {
            action: serde_json::from_str(body)?,
        }),
        _ => None,
    };
    if command.is_some() {
        Ok(command)
    } else {
        parse_game_command(words)
    }
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
        ["escape"] => Command::Escape,
        ["restrain", target] => Command::Restrain {
            target: PersonId(target.parse()?),
        },
        ["yield", captor] => Command::Yield {
            captor: PersonId(captor.parse()?),
        },
        ["withdraw-yield"] => Command::WithdrawYield,
        ["guardian", guardian, "food", source, quantity] => Command::RequestGuardian {
            guardian: PersonId(guardian.parse()?),
            request: GuardianRequest::SupplyFood {
                source: InventoryId(source.parse()?),
                quantity: Quantity(quantity.parse()?),
            },
        },
        ["learn", teacher, topic] => Command::Learn {
            teacher: PersonId(teacher.parse()?),
            topic: match *topic {
                "invocation" => LessonTopic::Invocation,
                "craft" => LessonTopic::Craft,
                "travel" => LessonTopic::Travel,
                _ => return Err("unknown lesson topic".into()),
            },
        },
        ["guardian", guardian, "rest"] => Command::RequestGuardian {
            guardian: PersonId(guardian.parse()?),
            request: GuardianRequest::Rest,
        },
        ["guardian", guardian, "move", destination] => Command::RequestGuardian {
            guardian: PersonId(guardian.parse()?),
            request: GuardianRequest::Move {
                destination: AreaId(destination.parse()?),
            },
        },
        ["guardian", guardian, "work", site] => Command::RequestGuardian {
            guardian: PersonId(guardian.parse()?),
            request: GuardianRequest::Work {
                site: SiteId(site.parse()?),
            },
        },
        ["take", source, destination, quantity] => Command::Property {
            action: PropertyAction::Take {
                source: InventoryId(source.parse()?),
                destination: InventoryId(destination.parse()?),
                quantity: Quantity(quantity.parse()?),
            },
        },
        ["request-access", inventory] => Command::Property {
            action: PropertyAction::RequestAccess {
                inventory: InventoryId(inventory.parse()?),
            },
        },
        ["recognize", claim] => Command::Property {
            action: PropertyAction::Recognize {
                claim: ClaimId(claim.parse()?),
            },
        },
        ["contest", claim] => Command::Property {
            action: PropertyAction::Contest {
                claim: ClaimId(claim.parse()?),
            },
        },
        ["report-incident", recipient, incident] => Command::Property {
            action: PropertyAction::ReportIncident {
                recipient: PersonId(recipient.parse()?),
                incident: IncidentId(incident.parse()?),
            },
        },
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
        ["carry", source, quantity] => Command::Carry {
            source: InventoryId(source.parse()?),
            quantity: Quantity(quantity.parse()?),
        },
        ["load", source, destination, quantity] => Command::Load {
            source: InventoryId(source.parse()?),
            destination: InventoryId(destination.parse()?),
            quantity: Quantity(quantity.parse()?),
        },
        ["farm", field, operation] => Command::Cultivate {
            field: SiteId(field.parse()?),
            operation: match *operation {
                "fallow" => FarmOperation::Fallow,
                "prepare" => FarmOperation::Prepare,
                "plant" => FarmOperation::Plant,
                "tend" => FarmOperation::Tend,
                "irrigate" => FarmOperation::Irrigate,
                "harvest" => FarmOperation::Harvest,
                _ => return Err("unknown farming operation".into()),
            },
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
    if let Some(command) = parse_line_command(line, &words)? {
        return Ok(SessionOutput::Text(serde_json::to_string(
            &session.submit_command(command)?,
        )?));
    }
    let text = match words.as_slice() {
        [] => String::new(),
        ["status"] => serde_json::to_string(&session.observe()?)?,
        ["lineage"] => serde_json::to_string(&session.observe()?.lineage)?,
        ["combat"] => serde_json::to_string(&session.observe()?.combat)?,
        ["pause"] => serde_json::to_string(&session.pause()?)?,
        ["debug"] => format!("DEBUG OMNISCIENT {}",session.debug_json()?),
        ["run",duration] => return Ok(SessionOutput::Advancing(session.run(parse_duration(duration)?)?)),
        ["save",path] => { save_file(std::path::Path::new(path),&session.save()?)?; "saved".into() },
        ["load",path] => { session.load(std::fs::read(path)?)?; "loaded".into() },
        ["help"] => "status | pause | run <integer>[ms|s|m|h|d] | move <area-id> | rest | eat <stock-id> | carry <source> <quantity> | load <source> <destination> <fixed-point quantity> | farm <field-id> <prepare|plant|tend|irrigate|harvest|fallow> | work <site-id> | tell <person> <known-stock> | learn <teacher> <invocation|craft|travel> | guardian <person> <rest|move area|work site|food source quantity> | signal <food|company> | play <interest> | take <source> <cargo> <quantity> | request-access <stock> | recognize <claim> | contest <claim> | report-incident <person> <incident> | command <JSON Command> | social <JSON action> | enforcement <JSON action> | health <JSON action> | ecology <JSON action> | mission <JSON action> | invocation <JSON action> | project <JSON action> | institution <JSON action> | property <JSON action> | agreement <JSON action> | conceive <partner> | comfort <child> | care <child> <food-stock> | engage <person> | combat | lineage | turn <encounter> <attack|capture|release|breach|retreat|defend|wait|surrender> [target] [x-mm y-mm] | escape | restrain <person> | yield <captor> | withdraw-yield | free <captive> | cancel | debug (omniscient) | save <path> | load <path> | quit".into(),
        _ => return Err("unknown command; use help".into()),
    };
    Ok(SessionOutput::Text(text))
}

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn structured_commands_preserve_whitespace_inside_text() {
        let line = r#"institution {"Found":{"origin":null,"charter":{"purpose":"Two  timber\tstores","founders":[1],"jurisdiction":2,"open_membership":false,"procedure":{"proposers":{"members":true,"named":[],"offices":[]},"voters":{"members":true,"named":[],"offices":[]},"minimum_support":1},"rules":[],"offices":[],"contributions":[]}}}"#;
        let parsed = parse_line_command(line, &line.split_whitespace().collect::<Vec<_>>())
            .unwrap()
            .unwrap();
        let Command::Institution {
            action: InstitutionAction::Found { charter, .. },
        } = parsed
        else {
            panic!("wrong command")
        };
        assert_eq!(charter.purpose, "Two  timber\tstores");
    }
}
