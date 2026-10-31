//! Small interactive entry point for the composed economic simulation.
use game_parallel_economy::{Departure, Economy, PromiseDecision, PromiseProposal};
use game_parallel_lots::Place;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

fn command(world: &mut Economy, line: &str) -> Result<bool, String> {
    let words: Vec<_> = line.split_whitespace().collect();
    if game_parallel_economy_cli_commands::handle(world, &words)? {
        return Ok(true);
    }
    match words.as_slice() {
        [] => {}
        ["help"] => println!(
            "status | people | knowledge PERSON | past-knowledge LIFE | meet PERSON TARGET | eat PERSON GOOD | start-work PERSON RECIPE UNITS | continue-work PERSON | ruler | invoke LIFE... | dismiss-invocation | accept-warning REVISION | guardian-release ID | health | skills PERSON | catalog | fields | claims | claim PERSON REGION GOOD | unclaim PERSON REGION GOOD | take-claimed PERSON on|off | claim-output PERSON on|off | goods PERSON | stock REGION | work PERSON on|off | haul PERSON on|off | carry-local PERSON on|off | farm PERSON on|off | social PERSON on|off | yield PERSON on|off | treat PERSON on|off | go PERSON REGION | expeditions | expedition PERSON REGION VISIT | meet-expedition PERSON ID TARGET | invite-expedition PERSON ID TARGET | cancel-expedition PERSON ID | promises | promise DEBTOR CREDITOR GOOD QUANTITY DUE | decide ID PERSON accept|decline | honor PERSON on|off | step [TICKS] | save PATH | load PATH | quit"
        ),
        ["quit"] | ["exit"] => return Ok(false),
        ["status"] => {
            let state = world.snapshot();
            println!(
                "tick={} people={} living={} active_jobs={}",
                world.tick(),
                state.people.len(),
                state.people.iter().filter(|p| p.health > 0).count(),
                state.physical.work.len()
            );
        }
        ["people"] => {
            for person in world.snapshot().people {
                println!(
                    "person={} region={:?} health={} hunger={} willing_to_work={}",
                    person.id, person.region, person.health, person.hunger, person.willing_to_work
                );
            }
        }
        ["catalog"] => {
            let physical = world.snapshot().physical;
            for good in physical.goods {
                println!("good={} name={}", good.id, good.name);
            }
            for recipe in physical.recipes {
                println!(
                    "recipe={} name={} ticks={} inputs={:?} outputs={:?}",
                    recipe.id, recipe.name, recipe.ticks, recipe.inputs, recipe.outputs
                );
            }
        }
        ["fields"] => {
            let state = world.snapshot();
            for field in &state
                .agriculture
                .as_ref()
                .ok_or("cultivation is disabled")?
                .fields
            {
                println!(
                    "field={} region={} crop={} phase={:?} remaining={}",
                    field.id, field.region, field.crop, field.phase, field.remaining
                );
            }
        }
        ["promises"] => {
            let state = world.snapshot();
            for promise in &state
                .agreements
                .as_ref()
                .ok_or("promises are disabled")?
                .contracts
            {
                println!(
                    "id={} debtor={} creditor={} good={} quantity={} delivered={} due={} status={:?}",
                    promise.id,
                    promise.debtor,
                    promise.creditor,
                    promise.good,
                    promise.quantity,
                    promise.delivered,
                    promise.due,
                    promise.status
                );
            }
        }
        ["promise", debtor, creditor, good, quantity, due] => {
            let ids = world.propose_promises(&[PromiseProposal {
                debtor: debtor.parse().map_err(|_| "invalid debtor")?,
                creditor: creditor.parse().map_err(|_| "invalid creditor")?,
                good: good.parse().map_err(|_| "invalid good")?,
                quantity: quantity.parse().map_err(|_| "invalid quantity")?,
                due: due.parse().map_err(|_| "invalid due tick")?,
            }])?;
            println!("promise={}", ids[0]);
        }
        ["decide", id, person, choice] => world.decide_promises(&[PromiseDecision {
            contract: id.parse().map_err(|_| "invalid promise")?,
            actor: person.parse().map_err(|_| "invalid person")?,
            accept: match *choice {
                "accept" => true,
                "decline" => false,
                _ => return Err("expected accept or decline".into()),
            },
        }])?,
        ["honor", person, choice] => world.set_promise_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["goods", person] => {
            let person = person.parse().map_err(|_| "invalid person")?;
            let state = world.snapshot();
            let container = state
                .physical
                .custody
                .containers
                .iter()
                .find(|row| row.place == Place::Person(person))
                .ok_or("unknown person")?;
            for lot in &container.lots {
                println!(
                    "good={} quantity={} expires_at={:?}",
                    lot.good, lot.quantity, lot.expires_at
                );
            }
        }
        ["stock", region] => {
            let region: u64 = region.parse().map_err(|_| "invalid region")?;
            let state = world.snapshot();
            for container in
                state.physical.custody.containers.iter().filter(
                    |row| matches!(row.place, Place::Stock { region: id, .. } if id == region),
                )
            {
                for lot in &container.lots {
                    let name = state
                        .physical
                        .goods
                        .iter()
                        .find(|good| good.id == lot.good)
                        .map_or("?", |good| good.name.as_str());
                    println!(
                        "good={} name={} quantity={} expires_at={:?}",
                        lot.good, name, lot.quantity, lot.expires_at
                    );
                }
            }
        }
        ["work", person, choice] => {
            let person = person.parse().map_err(|_| "invalid person")?;
            let willing = match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            };
            world.set_work_choice(person, willing)?;
        }
        ["carry-local", person, choice] => world.set_local_carrying_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["farm", person, choice] => world.set_farming_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["social", person, choice] => world.set_social_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["yield", person, choice] => world.set_social_pressure(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["treat", person, choice] => world.set_treatment_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["go", person, destination] => world.depart(&[Departure {
            person: person.parse().map_err(|_| "invalid person")?,
            destination: destination.parse().map_err(|_| "invalid destination")?,
        }])?,
        ["step"] => advance(world, 1)?,
        ["step", count] => advance(world, count.parse().map_err(|_| "invalid tick count")?)?,
        ["save", path] => game_parallel_economy_cli::save(world, Path::new(path))?,
        ["load", path] => *world = game_parallel_economy_cli::load(Path::new(path))?,
        _ => return Err("unknown command or invalid arguments; enter help".into()),
    }
    Ok(true)
}
fn advance(world: &mut Economy, count: u64) -> Result<(), String> {
    for _ in 0..count {
        let event = world.advance()?;
        println!(
            "tick={} started={} finished={} delivered={} spoiled={}",
            event.tick,
            event.started.len(),
            event.finished.len(),
            event.delivered.len(),
            event.spoiled.len()
        );
    }
    Ok(())
}
fn run() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args_os().skip(1);
    let mut player_mode = false;
    let mut scenario = None;
    let mut seed = None;
    let mut population = None;
    while let Some(arg) = args.next() {
        if arg == "--player" && !player_mode {
            player_mode = true;
        } else if arg == "--seed" && seed.is_none() {
            seed = Some(
                args.next()
                    .ok_or("missing seed")?
                    .to_str()
                    .ok_or("invalid seed")?
                    .parse::<u64>()
                    .map_err(|_| "invalid seed")?,
            );
        } else if arg == "--population" && population.is_none() {
            population = Some(PathBuf::from(
                args.next().ok_or("missing population profile")?,
            ));
        } else if scenario.is_none() && !arg.to_string_lossy().starts_with("--") {
            scenario = Some(PathBuf::from(arg));
        } else {
            return Err("unexpected argument".into());
        }
    }
    let scenario = scenario.unwrap_or_else(|| {
        root.join(if player_mode {
            "content/parallel/economy-campaign.toml"
        } else {
            "content/parallel/economy.toml"
        })
    });
    let mut world = game_parallel_economy_cli::scenario_with_options(
        &scenario,
        &root.join("content"),
        seed,
        population.as_deref(),
    )?;
    println!("Economic simulation; enter help for commands.");
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
        let result = if player_mode {
            let words: Vec<_> = line.split_whitespace().collect();
            match words.as_slice() {
                ["save", path] => {
                    game_parallel_economy_cli::save(&world, Path::new(path)).map(|()| true)
                }
                ["load", path] => game_parallel_economy_cli::load(Path::new(path)).map(|loaded| {
                    world = loaded;
                    true
                }),
                _ => game_parallel_economy_cli_commands::player::handle(&mut world, &words),
            }
        } else {
            command(&mut world, &line)
        };
        match result {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => eprintln!("{error}"),
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
