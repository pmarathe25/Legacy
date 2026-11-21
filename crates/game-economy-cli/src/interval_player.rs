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

fn command(host: &mut CoreHost, person: u64, words: &[&str]) -> Result<bool, String> {
    match words {
        [] => {}
        ["help"] => println!(
            "look | wait MILLISECONDS | move REGION | walk stock|store ID | pause-walk | resume-walk | trip REGION VISIT_MILLISECONDS [food] | trip go|survey|back|cancel | confine PERSON SITE | guard SITE | release PERSON | escape | build BLUEPRINT | demolish SITE | salvage SITE GOOD QUANTITY | eat GOOD | treat GOOD | work RECIPE UNITS | stop JOB | stop-assist | rest on|off | word place|person|object ID SOUNDS | form SOUNDS MARKS | inscribe SOUNDS | read PAGE | recall PAGE | save PATH | load PATH | quit (SOUNDS and MARKS are comma-separated numbers)"
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
