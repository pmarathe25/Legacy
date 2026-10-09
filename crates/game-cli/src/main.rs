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
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => scenario = args.next().ok_or("missing scenario")?,
            "--seed" => seed = args.next().ok_or("missing seed")?.parse()?,
            "--workers" => execution.workers = args.next().ok_or("missing workers")?.parse()?,
            "--script" => script = Some(args.next().ok_or("missing script path")?),
            "--help" => {
                println!(
                    "game-cli --scenario kernel-fixture|survival-fixture|autonomy-fixture|life-fixture|combat-fixture --seed 1 [--workers N] [--script PATH]\nDebug fixtures only; type help for commands"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown option {arg}").into()),
        }
    }
    let mut sim = Simulation::new(&scenario, seed, execution.clone())?;
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
            match game_cli::execute(&mut sim, &line, &execution) {
                Ok(output) if !output.is_empty() => println!("{output}"),
                Ok(_) => {}
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
