//! Minimal interactive host for the parallel simulation.
mod production;
use game_parallel::{Move, Shipment, World};
use std::io::{self, BufRead, IsTerminal, Write};

fn number<T: std::str::FromStr>(
    words: &mut std::str::SplitWhitespace<'_>,
    name: &str,
) -> Result<T, String> {
    words
        .next()
        .ok_or_else(|| format!("missing {name}"))?
        .parse()
        .map_err(|_| format!("invalid {name}"))
}
fn end(words: &mut std::str::SplitWhitespace<'_>) -> Result<(), String> {
    if words.next().is_some() {
        Err("unexpected argument".into())
    } else {
        Ok(())
    }
}
fn save_atomic(path: &std::path::Path, data: &[u8]) -> Result<(), String> {
    let name = path.file_name().ok_or("invalid save path")?;
    for attempt in 0..1000u32 {
        let mut temp_name = name.to_os_string();
        temp_name.push(format!(".{}.{}.tmp", std::process::id(), attempt));
        let temp = path.with_file_name(temp_name);
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        };
        let result = file.write_all(data).and_then(|_| file.sync_all());
        drop(file);
        let result = result.and_then(|_| std::fs::rename(&temp, path));
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        return result.map_err(|error| error.to_string());
    }
    Err("unable to reserve temporary save file".into())
}
fn command(world: &mut World, line: &str) -> Result<bool, String> {
    let mut words = line.split_whitespace();
    match words.next() {
        None => {}
        Some("help") => {
            end(&mut words)?;
            println!(
                "status | ruler | region ID | inventory REGION | goods PERSON | pickup-good PERSON GOOD QUANTITY | deposit-good PERSON GOOD QUANTITY | recover-good ACTOR DECEASED GOOD QUANTITY | offer A GOOD_A QUANTITY_A B GOOD_B QUANTITY_B | offers PERSON | accept OFFER PERSON | decline OFFER PERSON | withdraw OFFER PERSON | work ID WORKER REGION RECIPE UNITS | cancel-work ID | assign-work ID WORKER | social PERSON | knowledge PERSON | decisions | run TICKS | move PERSON DESTINATION | take PERSON FOOD | store PERSON FOOD | eat PERSON | recover ACTOR DECEASED FOOD | ship ID SOURCE DESTINATION FOOD | save PATH | load PATH | quit"
            );
        }
        Some("status") => {
            end(&mut words)?;
            let living = world
                .regions
                .iter()
                .flat_map(|r| &r.people)
                .filter(|p| p.health > 0)
                .count()
                + world
                    .travelers
                    .iter()
                    .filter(|t| t.person.health > 0)
                    .count();
            println!(
                "tick={} regions={} living={} travelers={} shipments={}",
                world.tick,
                world.regions.len(),
                living,
                world.travelers.len(),
                world.shipments.len()
            );
        }
        Some("ruler") => {
            end(&mut words)?;
            let lineage = world.lineage.as_ref().ok_or("lineage is disabled")?;
            println!(
                "ruler={:?} started={} waiting_since={:?} past_lives={}",
                lineage.current,
                lineage.started,
                lineage.waiting_since,
                lineage.history.len()
            );
        }
        Some("region") => {
            let id: u64 = number(&mut words, "region")?;
            end(&mut words)?;
            let region = world
                .regions
                .iter()
                .find(|r| r.id == id)
                .ok_or("unknown region")?;
            println!(
                "region={} food={} people={}",
                region.id,
                region.food,
                region.people.len()
            );
            for p in &region.people {
                println!("person={} health={} hunger={}", p.id, p.health, p.hunger);
            }
        }
        Some("social") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let social = world
                .social
                .as_ref()
                .ok_or("social simulation is disabled")?;
            let participant = social
                .participants
                .iter()
                .find(|p| p.person == person)
                .ok_or("unknown social participant")?;
            println!("person={person} willing={}", participant.willing);
            for relation in social
                .relationships
                .iter()
                .filter(|r| r.a == person || r.b == person)
            {
                let other = if relation.a == person {
                    relation.b
                } else {
                    relation.a
                };
                println!(
                    "other={other} familiarity={} encounters={} last_met={}",
                    relation.familiarity, relation.encounters, relation.last_met
                );
            }
        }
        Some("knowledge") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let knowledge = world
                .knowledge
                .as_ref()
                .ok_or("knowledge simulation is disabled")?;
            let mind = knowledge
                .minds
                .iter()
                .find(|m| m.person == person)
                .ok_or("unknown mind")?;
            println!("person={person} known_regions={}", mind.records.len());
            for record in &mind.records {
                println!(
                    "region={} food={} observed_at={} witness={} learned_from={:?}",
                    record.region,
                    record.food,
                    record.observed_at,
                    record.witness,
                    record.learned_from
                );
            }
        }
        Some("decisions") => {
            end(&mut words)?;
            let autonomy = world.autonomy.as_ref().ok_or("autonomy is disabled")?;
            for event in &autonomy.events {
                println!(
                    "tick={} person={} source={} destination={} outcome={:?} reason={:?}",
                    event.tick,
                    event.person,
                    event.source,
                    event.destination,
                    event.outcome,
                    event.reason
                );
            }
        }
        Some("run") => {
            let ticks: u64 = number(&mut words, "ticks")?;
            end(&mut words)?;
            for _ in 0..ticks {
                world
                    .step()
                    .map_err(|e| format!("tick {}: {e}", world.tick))?;
            }
            println!("tick={}", world.tick);
        }
        Some("move") => {
            let person = number(&mut words, "person")?;
            let destination = number(&mut words, "destination")?;
            end(&mut words)?;
            world.move_people(&[Move {
                person,
                destination,
            }])?;
            println!("travel started for person={person}");
        }
        Some("take") => {
            let person = number(&mut words, "person")?;
            let amount = number(&mut words, "food")?;
            end(&mut words)?;
            world.pickup_food(person, amount)?;
            println!("food picked up by person={person}");
        }
        Some("store") => {
            let person = number(&mut words, "person")?;
            let amount = number(&mut words, "food")?;
            end(&mut words)?;
            world.deposit_food(person, amount)?;
            println!("food stored by person={person}");
        }
        Some("eat") => {
            let person = number(&mut words, "person")?;
            end(&mut words)?;
            world.eat_carried_food(person)?;
            println!("person={person} ate carried food");
        }
        Some("recover") => {
            let actor = number(&mut words, "actor")?;
            let deceased = number(&mut words, "deceased")?;
            let amount = number(&mut words, "food")?;
            end(&mut words)?;
            world.recover_food(actor, deceased, amount)?;
            println!("food recovered by person={actor}");
        }
        Some("ship") => {
            let id = number(&mut words, "shipment")?;
            let source = number(&mut words, "source")?;
            let destination = number(&mut words, "destination")?;
            let food = number(&mut words, "food")?;
            end(&mut words)?;
            let ticks_remaining = world
                .routes
                .iter()
                .find(|r| r.source == source && r.destination == destination)
                .ok_or("no route")?
                .ticks;
            world.launch_shipments(&[Shipment {
                id,
                source,
                destination,
                food,
                ticks_remaining,
            }])?;
            println!("shipment={id} launched");
        }
        Some("save") => {
            let path = words.next().ok_or("missing path")?;
            end(&mut words)?;
            world.validate()?;
            let data = serde_json::to_vec_pretty(world).map_err(|e| e.to_string())?;
            save_atomic(std::path::Path::new(path), &data)?;
            println!("saved {path}");
        }
        Some("load") => {
            let path = words.next().ok_or("missing path")?;
            end(&mut words)?;
            let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            *world = World::from_json(&data)?;
            println!("loaded {path}");
        }
        Some("quit") => {
            end(&mut words)?;
            return Ok(false);
        }
        Some(verb) => production::command(world, verb, &mut words)?,
    }
    Ok(true)
}
fn execute() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .unwrap_or_else(|| "content/parallel/example.toml".into());
    if args.next().is_some() {
        return Err("usage: parallel-cli [WORLD.toml]".into());
    }
    let mut world = World::from_toml(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)?;
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let mut input = stdin.lock();
    let mut line = String::new();
    loop {
        if interactive {
            print!("> ");
            io::stdout().flush().map_err(|e| e.to_string())?;
        }
        line.clear();
        if input.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            break;
        }
        match command(&mut world, &line) {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => eprintln!("error: {error}"),
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
