//! Limited interactive access to the interval host through a controlled person.
use game_economy::core_interval::{CoreHost, CoreHostSnapshot};
use game_economy_player::interval::Player;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

fn command(host: &mut CoreHost, person: u64, words: &[&str]) -> Result<bool, String> {
    match words {
        [] => {}
        ["help"] => println!(
            "look | wait MILLISECONDS | move REGION | treat GOOD | save PATH | load PATH | quit"
        ),
        ["quit"] | ["exit"] => return Ok(false),
        ["look"] => {
            let view = Player::new(host, person)?.view()?;
            println!(
                "tick={} offset={} person={} region={:?} health={} hunger={}",
                view.tick,
                view.offset_millis,
                view.person.id,
                view.person.region,
                view.person.health,
                view.person.hunger
            );
            for other in &view.nearby {
                println!("nearby={} health={}", other.id, other.health);
            }
            for lot in &view.possessions {
                println!("carried_good={} quantity={}", lot.good, lot.quantity);
            }
            if let Some(wound) = view.wound {
                println!("wound_severity={}", wound.severity);
            }
            if let Some(journey) = view.journey {
                println!(
                    "travel_destination={} remaining={}",
                    journey.destination, journey.remaining
                );
            }
        }
        ["wait", millis] => {
            let millis = millis.parse::<u64>().map_err(|_| "invalid milliseconds")?;
            let result = Player::new(host, person)?.advance(millis)?;
            println!(
                "elapsed={} boundaries={}",
                result.elapsed_millis, result.boundaries
            );
            if let Some(error) = result.failure {
                return Err(error);
            }
        }
        ["move", region] => {
            let region = region.parse::<u64>().map_err(|_| "invalid region")?;
            println!("departed={}", Player::new(host, person)?.depart(region)?);
        }
        ["treat", good] => {
            let good = good.parse::<u32>().map_err(|_| "invalid good")?;
            println!("consumed={}", Player::new(host, person)?.treat_wound(good)?);
        }
        ["save", path @ ..] if !path.is_empty() => {
            host.controlled_person(person)?;
            let path = path.join(" ");
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path.trim_matches('"'))
                .map_err(|error| error.to_string())?;
            serde_json::to_writer(file, &host.snapshot()).map_err(|error| error.to_string())?;
            println!("saved");
        }
        ["load", path @ ..] if !path.is_empty() => {
            let path = path.join(" ");
            let file = File::open(path.trim_matches('"')).map_err(|error| error.to_string())?;
            let snapshot: CoreHostSnapshot =
                serde_json::from_reader(file).map_err(|error| error.to_string())?;
            let loaded = CoreHost::resume(snapshot)?;
            loaded.controlled_person(person)?;
            *host = loaded;
            println!("loaded");
        }
        _ => return Err("unknown interval player command; enter help".into()),
    }
    Ok(true)
}

pub fn run(root: &Path, scenario: &Path, person: u64) -> Result<(), String> {
    let mut host = game_economy_cli::core_scenario(scenario, &root.join("content"))?;
    host.controlled_person(person)?;
    println!("Interval player; enter help for commands.");
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let mut lines = stdin.lock().lines();
    loop {
        if interactive {
            print!("> ");
            io::stdout().flush().map_err(|error| error.to_string())?;
        }
        let Some(line) = lines.next() else { break };
        let line = line.map_err(|error| error.to_string())?;
        let words: Vec<_> = line.split_whitespace().collect();
        match command(&mut host, person, &words) {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => eprintln!("{error}"),
        }
    }
    Ok(())
}
