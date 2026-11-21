//! Limited interactive access to the interval host through a controlled person.
use game_economy::core_interval::{CoreHost, CoreHostSnapshot};
use game_economy_player::interval::Player;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

fn numbers<T: std::str::FromStr>(text: &str) -> Result<Vec<T>, String> {
    text.split(',')
        .map(|part| part.parse().map_err(|_| "invalid number list".to_owned()))
        .collect()
}

fn save(host: &CoreHost, path: &[&str]) -> Result<(), String> {
    let path = path.join(" ");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.trim_matches('"'))
        .map_err(|error| error.to_string())?;
    serde_json::to_writer(file, &host.snapshot()).map_err(|error| error.to_string())?;
    println!("saved");
    Ok(())
}

fn load(path: &[&str]) -> Result<CoreHost, String> {
    let path = path.join(" ");
    let file = File::open(path.trim_matches('"')).map_err(|error| error.to_string())?;
    let snapshot: CoreHostSnapshot =
        serde_json::from_reader(file).map_err(|error| error.to_string())?;
    CoreHost::resume(snapshot)
}

fn command(host: &mut CoreHost, person: u64, words: &[&str]) -> Result<bool, String> {
    match words {
        [] => {}
        ["help"] => println!(
            "look | wait MILLISECONDS | move REGION | walk stock|store ID | pause-walk | resume-walk | trip REGION VISIT_MILLISECONDS [food] | trip go|survey|back|cancel | fight PERSON | tactic wait|defend|retreat | tactic attack PERSON | tactic move X Y | confine PERSON SITE | guard SITE | release PERSON | escape | build BLUEPRINT | demolish SITE | salvage SITE GOOD QUANTITY | eat GOOD | treat GOOD | work RECIPE UNITS | stop JOB | stop-assist | rest on|off | word place|person|object ID SOUNDS | form SOUNDS MARKS | inscribe SOUNDS | read PAGE | recall PAGE | save PATH | load PATH | quit (SOUNDS and MARKS are comma-separated numbers)"
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
            for site in &view.accessible_sites {
                if let Some(kind) = &site.kind {
                    println!("site={} kind={kind}", site.id);
                }
                for lot in &site.debris {
                    println!(
                        "debris_site={} good={} quantity={}",
                        site.id, lot.good, lot.quantity
                    );
                }
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
            if let Some(location) = view.local_location {
                println!("local_location={location:?}");
            }
            if let Some(site) = view.held_at {
                println!("held_at={site}");
            }
            if let Some(plan) = &view.expedition {
                println!(
                    "trip={} destination={} stage={:?} objective_met={}",
                    plan.id, plan.destination, plan.stage, plan.objective_met
                );
            }
            if let Some(tactics) = &view.tactics {
                println!(
                    "fight={} x={} y={} next_round={}",
                    tactics.engagement,
                    tactics.position.x,
                    tactics.position.y,
                    tactics.next_round_millis
                );
                for (person, position) in &tactics.visible {
                    println!("fighter={person} x={} y={}", position.x, position.y);
                }
            }
            for report in view.route_reports {
                let observed = report.observed_at_millis.map_or_else(
                    || format!("tick {}", report.observed_at),
                    |instant| format!("{instant} simulated ms"),
                );
                println!(
                    "route {} to {} was reported {} at {observed}",
                    report.source,
                    report.destination,
                    if report.open { "open" } else { "closed" }
                );
            }
            if let Some(job) = view.job {
                println!(
                    "job={} recipe={} remaining={}",
                    job.id, job.recipe, job.remaining
                );
            }
            if let Some(fatigue) = view.fatigue {
                println!("fatigue={} resting={}", fatigue.fatigue, fatigue.resting);
            }
            if let Some(language) = view.language {
                for word in language.words {
                    println!("word={:?} sounds={:?}", word.meaning, word.sounds);
                }
                for form in language.associations {
                    println!("form_sounds={:?} marks={:?}", form.sounds, form.marks);
                }
                for memory in language.observations {
                    println!(
                        "remembered_page={} marks={:?} heard={:?}",
                        memory.page, memory.marks, memory.heard
                    );
                }
            }
            for page in view.marked_pages {
                println!("marked_page={} marks={:?}", page.item, page.marks);
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
        ["walk", "stock"] => {
            println!(
                "walk={:?}",
                Player::new(host, person)?.walk(game_economy::LocalEndpoint::Stock)?
            );
        }
        ["walk", "store", store] => {
            let id = store.parse::<u64>().map_err(|_| "invalid store")?;
            println!(
                "walk={:?}",
                Player::new(host, person)?.walk(game_economy::LocalEndpoint::Store { id })?
            );
        }
        ["pause-walk"] | ["resume-walk"] => {
            println!(
                "walk_changed={}",
                Player::new(host, person)?.pause_walk(words[0] == "pause-walk")?
            );
        }
        ["trip", "go" | "survey" | "back" | "cancel"] => {
            let mut player = Player::new(host, person)?;
            let events = match words[1] {
                "go" => player.continue_expedition()?,
                "survey" => player.survey_expedition()?,
                "back" => player.return_from_expedition()?,
                _ => vec![player.abandon_expedition()?],
            };
            for event in events {
                println!(
                    "trip={} event={:?} region={} at_millis={}",
                    event.itinerary, event.kind, event.region, event.at_millis
                );
                if let Some(food) = event.food_observed {
                    println!("food_observed={food}");
                }
            }
        }
        ["trip", region, duration] | ["trip", region, duration, "food"] => {
            let region = region.parse().map_err(|_| "invalid region")?;
            let duration = duration.parse().map_err(|_| "invalid visit duration")?;
            let objective =
                (words.len() == 4).then_some(game_economy::MissionObjective::ObserveFood);
            let event = Player::new(host, person)?.plan_expedition(region, duration, objective)?;
            println!("trip={} planned=true", event.itinerary);
        }
        ["fight", target] => {
            let target = target.parse::<u64>().map_err(|_| "invalid person")?;
            let event = Player::new(host, person)?.confront(target)?;
            println!("fight={} started=true", event.engagement);
        }
        ["tactic", "wait" | "defend" | "retreat"] => {
            use game_economy::core_interval::TacticalChoice;
            let action = match words[1] {
                "wait" => TacticalChoice::Wait,
                "defend" => TacticalChoice::Defend,
                _ => TacticalChoice::Retreat,
            };
            Player::new(host, person)?.choose_tactics(action)?;
        }
        ["tactic", "attack", target] => {
            let target = target.parse::<u64>().map_err(|_| "invalid person")?;
            Player::new(host, person)?.choose_tactics(
                game_economy::core_interval::TacticalChoice::Attack {
                    target,
                    pursue: true,
                },
            )?;
        }
        ["tactic", "move", x, y] => {
            use game_economy::core_interval::{TacticalChoice, TacticalPoint};
            let x = x.parse::<u32>().map_err(|_| "invalid position")?;
            let y = y.parse::<u32>().map_err(|_| "invalid position")?;
            Player::new(host, person)?.choose_tactics(TacticalChoice::Move {
                destination: TacticalPoint { x, y },
            })?;
        }
        ["confine", target, site] => {
            let target = target.parse::<u64>().map_err(|_| "invalid person")?;
            let site = site.parse::<u64>().map_err(|_| "invalid site")?;
            println!(
                "confined={}",
                Player::new(host, person)?.confine(target, site)?
            );
        }
        ["guard", site] => {
            let site = site.parse::<u64>().map_err(|_| "invalid site")?;
            Player::new(host, person)?.guard(site)?;
            println!("guarded={site}");
        }
        ["release", target] => {
            let target = target.parse::<u64>().map_err(|_| "invalid person")?;
            println!("released={}", Player::new(host, person)?.release(target)?);
        }
        ["escape"] => println!("escaped={}", Player::new(host, person)?.escape()?),
        ["build", blueprint] => {
            let blueprint = blueprint.parse::<u32>().map_err(|_| "invalid blueprint")?;
            println!(
                "built_site={:?}",
                Player::new(host, person)?.build(blueprint)?
            );
        }
        ["demolish", site] => {
            let site = site.parse::<u64>().map_err(|_| "invalid site")?;
            println!("demolished={}", Player::new(host, person)?.demolish(site)?);
        }
        ["salvage", site, good, quantity] => {
            let site = site.parse::<u64>().map_err(|_| "invalid site")?;
            let good = good.parse::<u32>().map_err(|_| "invalid good")?;
            let quantity = quantity.parse::<u64>().map_err(|_| "invalid quantity")?;
            println!(
                "salvaged={}",
                Player::new(host, person)?.salvage(site, good, quantity)?
            );
        }
        ["eat", good] => {
            let good = good.parse::<u32>().map_err(|_| "invalid good")?;
            println!("consumed={}", Player::new(host, person)?.eat(good)?);
        }
        ["treat", good] => {
            let good = good.parse::<u32>().map_err(|_| "invalid good")?;
            println!("consumed={}", Player::new(host, person)?.treat_wound(good)?);
        }
        ["work", recipe, units] => {
            let recipe = recipe.parse::<u32>().map_err(|_| "invalid recipe")?;
            let units = units.parse::<u32>().map_err(|_| "invalid units")?;
            if let Some(job) = Player::new(host, person)?.start_work(recipe, units)? {
                println!("job={} started=true", job);
            } else {
                println!("started=false");
            }
        }
        ["stop", job] => {
            let job = job.parse::<u64>().map_err(|_| "invalid job")?;
            Player::new(host, person)?.stop_work(job)?;
            println!("stopped");
        }
        ["stop-assist"] => {
            Player::new(host, person)?.stop_assisting()?;
            println!("stopped assisting");
        }
        ["rest", choice @ ("on" | "off")] => {
            let resting = *choice == "on";
            Player::new(host, person)?.set_rest(resting)?;
            println!("resting={resting}");
        }
        ["form", sounds, marks] => {
            Player::new(host, person)?.choose_written_form(numbers(sounds)?, numbers(marks)?)?;
            println!("form chosen");
        }
        ["word", kind, referent, sounds] => {
            let meaning = match *kind {
                "place" => {
                    game_language::Meaning::Place(referent.parse().map_err(|_| "invalid place")?)
                }
                "person" => {
                    game_language::Meaning::Person(referent.parse().map_err(|_| "invalid person")?)
                }
                "object" => {
                    game_language::Meaning::Object(referent.parse().map_err(|_| "invalid object")?)
                }
                _ => return Err("unknown word referent".into()),
            };
            Player::new(host, person)?.choose_word(meaning, numbers(sounds)?)?;
            println!("word chosen");
        }
        ["inscribe", sounds] => {
            if let Some(item) = Player::new(host, person)?.inscribe(numbers(sounds)?)? {
                println!("inscribed_page={item}");
            } else {
                println!("inscribed=false");
            }
        }
        ["read", item] => {
            let item = item.parse::<u64>().map_err(|_| "invalid page")?;
            let observation = Player::new(host, person)?.observe_marked_page(item)?;
            println!(
                "marks={:?} meanings={:?}",
                observation.marks, observation.meanings
            );
        }
        ["recall", item] => {
            let item = item.parse::<u64>().map_err(|_| "invalid page")?;
            Player::new(host, person)?.adopt_remembered_form(item)?;
            println!("remembered form adopted");
        }
        ["save", path @ ..] if !path.is_empty() => {
            host.controlled_person(person)?;
            save(host, path)?;
        }
        ["load", path @ ..] if !path.is_empty() => {
            let loaded = load(path)?;
            loaded.controlled_person(person)?;
            *host = loaded;
            println!("loaded");
        }
        _ => return Err("unknown interval player command; enter help".into()),
    }
    Ok(true)
}

fn campaign_command(host: &mut CoreHost, words: &[&str]) -> Result<bool, String> {
    let view = host
        .campaign_view()
        .ok_or("this scenario has no interval campaign")?;
    let controlled = || {
        view.controlled
            .ok_or("no incarnation currently available for direct action")
    };
    match words {
        [] => {}
        ["quit" | "exit"] => return Ok(false),
        ["campaign"] | ["look"] if view.controlled.is_none() || words[0] == "campaign" => {
            println!(
                "incarnation={:?} controlled={:?}",
                view.current, view.controlled
            );
            for life in &view.past_lives {
                println!(
                    "past_life={} started={} ended={}",
                    life.person, life.started, life.ended
                );
            }
            for guardian in &view.guardians {
                println!(
                    "guardian={} student={:?} mentorship_millis={} released={}",
                    guardian.person, guardian.student, guardian.observed_millis, guardian.released
                );
            }
            if let Some(warning) = view.warning {
                println!(
                    "invocation_warning={:?} revision={} projected_burden={}",
                    warning.kind, warning.revision, warning.projected_burden
                );
            }
        }
        ["wait", duration] => {
            let duration = duration.parse().map_err(|_| "invalid duration")?;
            let run = host.advance_continuous(duration, &[])?;
            println!(
                "elapsed_millis={} boundaries={}",
                run.elapsed_millis, run.boundaries
            );
            if let Some(error) = run.failure {
                return Err(error);
            }
        }
        ["invoke", lives] => host.activate_past_lives(controlled()?, &numbers(lives)?)?,
        ["recall-life", life] => {
            let life = life.parse().map_err(|_| "invalid past life")?;
            let memory = host.consult_past_life(controlled()?, life)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&memory).map_err(|error| error.to_string())?
            );
        }
        ["dismiss"] => host.dismiss_past_lives(controlled()?)?,
        ["accept-cost", revision] => {
            let revision = revision.parse().map_err(|_| "invalid warning revision")?;
            host.accept_past_life_warning(controlled()?, revision)?;
        }
        ["release-mentor", guardian] => {
            let guardian = guardian.parse().map_err(|_| "invalid guardian")?;
            println!(
                "mentorship_released={}",
                host.ask_guardian_release(controlled()?, guardian)?
                    .is_some()
            );
        }
        ["save", path @ ..] if !path.is_empty() => save(host, path)?,
        ["load", path @ ..] if !path.is_empty() => {
            let loaded = load(path)?;
            loaded.campaign_view().ok_or("save has no campaign")?;
            *host = loaded;
            println!("loaded");
        }
        ["help"] => {
            println!(
                "campaign | invoke PAST_LIVES | recall-life LIFE | dismiss | accept-cost REVISION | release-mentor GUARDIAN"
            );
            if let Some(person) = view.controlled {
                return command(host, person, words);
            }
        }
        _ => return command(host, controlled()?, words),
    }
    Ok(true)
}

pub fn run(root: &Path, scenario: &Path, person: Option<u64>) -> Result<(), String> {
    let mut host = game_economy_cli::core_scenario(scenario, &root.join("content"))?;
    if let Some(person) = person {
        host.controlled_person(person)?;
    } else {
        host.campaign_view()
            .ok_or("this scenario has no interval campaign")?;
    }
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
        let result = if let Some(person) = person {
            command(&mut host, person, &words)
        } else {
            campaign_command(&mut host, &words)
        };
        match result {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => eprintln!("{error}"),
        }
    }
    Ok(())
}
