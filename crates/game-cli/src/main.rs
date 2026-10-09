use game_sim::{Execution, Simulation};
use std::io::{self, BufRead, IsTerminal, Write};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = "kernel-fixture".to_owned();
    let mut seed = 1;
    let mut execution = Execution::default();
    let mut script = None;
    let mut environment_config = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => scenario = args.next().ok_or("missing scenario")?,
            "--seed" => seed = args.next().ok_or("missing seed")?.parse()?,
            "--workers" => execution.workers = args.next().ok_or("missing workers")?.parse()?,
            "--script" => script = Some(args.next().ok_or("missing script path")?),
            "--environment" => {
                environment_config = Some(args.next().ok_or("missing environment config path")?)
            }
            "--help" => {
                println!(
                    "game-cli --scenario kernel-fixture|survival-fixture|autonomy-fixture|life-fixture|combat-fixture|childhood-fixture|farming-fixture|agreement-fixture --seed 1 [--workers N] [--script PATH] [--environment CONFIG.json]\nDebug fixtures only; type help for commands"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown option {arg}").into()),
        }
    }
    let sim = if let Some(path) = environment_config {
        if scenario != "farming-fixture" {
            return Err("environment configuration requires farming-fixture".into());
        }
        let config = serde_json::from_slice(&std::fs::read(path)?)?;
        Simulation::farming_with_config(seed, execution.clone(), config)?
    } else {
        Simulation::new(&scenario, seed, execution.clone())?
    };
    let session = game_sim::session::Session::spawn(sim)?;
    let interactive = script.is_none() && io::stdin().is_terminal();
    let reader: Box<dyn BufRead> = if let Some(path) = script {
        Box::new(io::BufReader::new(std::fs::File::open(path)?))
    } else {
        Box::new(io::BufReader::new(io::stdin()))
    };
    println!("debug fixture: {scenario}; areas: 1=village 2=farm 3=forest");
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
