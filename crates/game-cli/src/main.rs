use game_world::{Execution, Simulation};
use std::io::{self, BufRead, IsTerminal, Write};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = "settlement-founder".to_owned();
    let mut seed = 1;
    let mut execution = Execution::default();
    let mut script = None;
    let mut environment_config = None;
    let mut scenario_config = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => scenario = args.next().ok_or("missing scenario")?,
            "--seed" => seed = args.next().ok_or("missing seed")?.parse()?,
            "--workers" => execution.workers = args.next().ok_or("missing workers")?.parse()?,
            "--script" => script = Some(args.next().ok_or("missing script path")?),
            "--config" => {
                scenario_config = Some(args.next().ok_or("missing scenario config path")?)
            }
            "--environment" => {
                environment_config = Some(args.next().ok_or("missing environment config path")?)
            }
            "--help" => {
                println!(
                    "game-cli [--scenario settlement-founder|established-ruler|child-successor|random] [--seed N] [--workers N] [--script PATH] [--config SCENARIO.json] [--environment ENVIRONMENT.json]\nDefault: settlement-founder. Debug fixtures remain selectable by their fixture IDs. Type help for commands."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown option {arg}").into()),
        }
    }
    let sim =
        if game_world::scenarios::PLAYABLE.contains(&scenario.as_str()) || scenario == "random" {
            let mut config: game_world::scenarios::Config = if let Some(path) = scenario_config {
                serde_json::from_slice(&std::fs::read(path)?)?
            } else {
                game_world::scenarios::Config::default()
            };
            if let Some(path) = environment_config {
                config.environment = serde_json::from_slice(&std::fs::read(path)?)?;
            }
            Simulation::playable_scenario(&scenario, seed, execution.clone(), config)?
        } else {
            if scenario_config.is_some() {
                return Err("scenario configuration requires a playable opening".into());
            }
            if let Some(path) = environment_config {
                if scenario != "farming-fixture" {
                    return Err(
                        "environment configuration requires a playable opening or farming-fixture"
                            .into(),
                    );
                }
                let config = serde_json::from_slice(&std::fs::read(path)?)?;
                Simulation::farming_with_config(
                    seed,
                    execution.clone(),
                    game_world::agriculture::FarmingConfig {
                        environment: config,
                        ..Default::default()
                    },
                )?
            } else {
                Simulation::new(&scenario, seed, execution.clone())?
            }
        };
    let selected = sim.canonical_state().scenario.clone();
    let session = game_session::Session::spawn(sim)?;
    let interactive = script.is_none() && io::stdin().is_terminal();
    let reader: Box<dyn BufRead> = if let Some(path) = script {
        Box::new(io::BufReader::new(std::fs::File::open(path)?))
    } else {
        Box::new(io::BufReader::new(io::stdin()))
    };
    println!(
        "{}: {}",
        if selected.debug_fixture {
            "debug fixture"
        } else {
            "opening"
        },
        selected.id
    );
    if interactive {
        print!("> ");
        io::stdout().flush()?;
    }
    for line in reader.lines() {
        let line = line?;
        if line.trim() == "quit" {
            break;
        }
        if !line.trim_start().starts_with('#') {
            match game_cli::execute_session(&session, &line) {
                Ok(game_cli::SessionOutput::Text(output)) => {
                    if !output.is_empty() {
                        println!("{output}");
                    }
                }
                Ok(game_cli::SessionOutput::Advancing(handle)) if interactive => {
                    std::thread::spawn(move || match handle.wait() {
                        Ok(result) => println!(
                            "{}",
                            serde_json::to_string(&result).expect("advance result JSON")
                        ),
                        Err(error) => eprintln!("error: {error}"),
                    });
                }
                Ok(game_cli::SessionOutput::Advancing(handle)) => {
                    println!("{}", serde_json::to_string(&handle.wait()?)?)
                }
                Err(error) if interactive => eprintln!("error: {error}"),
                Err(error) => return Err(error),
            }
        }
        if interactive {
            print!("> ");
            io::stdout().flush()?;
        }
    }
    Ok(())
}
