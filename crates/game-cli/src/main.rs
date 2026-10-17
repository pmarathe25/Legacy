use game_assembly::Simulation;
use game_cli::{Input, parse};
use game_core::*;
use std::io::{self, BufRead};
#[cfg(feature = "mimalloc")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut scenario = None;
    let mut content = None;
    let mut mods = vec![];
    let mut list = false;
    let mut seed = 1;
    let mut workers = 1;
    let mut script = None;
    let mut i = 0;
    while i < args.len() {
        let value = || {
            args.get(i + 1)
                .ok_or_else(|| Error::Rejected("missing option value".into()))
        };
        match args[i].as_str() {
            "--scenario" => scenario = Some(value()?.clone()),
            "--content" => content = Some(value()?.clone()),
            "--mod" => mods.push(value()?.clone()),
            "--seed" => {
                seed = value()?
                    .parse()
                    .map_err(|_| Error::Rejected("invalid seed".into()))?
            }
            "--workers" => {
                workers = value()?
                    .parse()
                    .map_err(|_| Error::Rejected("invalid workers".into()))?
            }
            "--script" => script = Some(value()?.clone()),
            "--list" => {
                list = true;
                i += 1;
                continue;
            }
            _ => return Err(Error::Rejected("unknown option".into())),
        }
        i += 2;
    }
    let mut store = content.map_or_else(
        || {
            if list || scenario.as_deref().is_some_and(|s| s != "generated") {
                game_content::Store::fixtures()
            } else {
                game_content::Store::standard()
            }
        },
        game_content::Store::new,
    );
    for directory in mods {
        store.overlay(directory)?;
    }
    if list {
        for scenario in game_assembly::scenarios_with_content(&mut store)? {
            println!(
                "{}: {} ({}, version {})",
                scenario.id,
                scenario.label,
                if scenario.fixture {
                    "fixture"
                } else {
                    "scenario"
                },
                scenario.version
            );
        }
        return Ok(());
    }
    let scenario = match scenario {
        Some(id) => id,
        None => game_assembly::default_scenario(&mut store)?,
    };
    let mut sim = Simulation::with_content(&scenario, seed, workers, &mut store)?;
    let mut sequence = 1;
    let input: Box<dyn BufRead> = match script {
        Some(path) => Box::new(io::BufReader::new(
            std::fs::File::open(path).map_err(|e| Error::Rejected(e.to_string()))?,
        )),
        None => Box::new(io::BufReader::new(io::stdin())),
    };
    for line in input.lines() {
        let line = line.map_err(|e| Error::Rejected(e.to_string()))?;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let result = (|| -> Result<Option<serde_json::Value>> {
            match parse(&line)? {
                Input::Quit => Ok(None),
                Input::Status => Ok(Some(serde_json::to_value(sim.observe()?).unwrap())),
                Input::Debug => Ok(Some(serde_json::from_str(&sim.save()?).unwrap())),
                Input::Run(duration) => {
                    let target = sim.now().after(duration)?;
                    loop {
                        let p = sim.advance(target, 256)?;
                        if p.stop != game_sim::Stop::Budget {
                            return Ok(Some(serde_json::to_value(p).unwrap()));
                        }
                    }
                }
                Input::Command(command) => {
                    let response = sim.submit(sequence, command);
                    sequence += 1;
                    Ok(Some(serde_json::to_value(response?).unwrap()))
                }
                Input::Save(path) => {
                    std::fs::write(path, sim.save()?)
                        .map_err(|e| Error::Rejected(e.to_string()))?;
                    Ok(Some(serde_json::json!({"saved":true})))
                }
                Input::Load(path) => {
                    let source = std::fs::read_to_string(path)
                        .map_err(|e| Error::Rejected(e.to_string()))?;
                    sim = Simulation::load(&source, workers)?;
                    sequence = sim.debug_host().next_sequence();
                    Ok(Some(serde_json::json!({"loaded":true})))
                }
            }
        })();
        match result {
            Ok(Some(value)) => println!("{value}"),
            Ok(None) => break,
            Err(error) => eprintln!("{error}"),
        }
    }
    Ok(())
}
