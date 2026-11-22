//! Unattended elapsed worlds and their diagnostic views.
use game_economy::core_interval::CoreHost;
use game_lots::Place;
use serde::Serialize;
use std::path::Path;

fn display(state: &impl Serialize) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(state).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn advance(host: &mut CoreHost, milliseconds: u64) -> Result<(), String> {
    let moved = host.advance_continuous(milliseconds, &[])?;
    println!(
        "elapsed_millis={} now_millis={} boundaries={}",
        moved.elapsed_millis,
        host.core().simulated_millis(),
        moved.boundaries
    );
    moved.failure.map_or(Ok(()), Err)
}

fn command(host: &mut CoreHost, words: &[&str]) -> Result<bool, String> {
    match words {
        [] => {}
        ["help"] => println!(
            "status | people | catalog | goods PERSON | stock REGION | fields | irrigation | tool-condition | claims | promises | expeditions | health | step [MILLISECONDS] | save PATH | load PATH | catalog-additions CONTENT_ROOT | quit"
        ),
        ["quit"] | ["exit"] => return Ok(false),
        ["step"] => advance(host, 60_000)?,
        ["step", millis] => advance(
            host,
            millis
                .parse()
                .map_err(|_| "invalid duration in milliseconds")?,
        )?,
        ["save", path @ ..] if !path.is_empty() => crate::interval_player::save(host, path)?,
        ["catalog-additions", path @ ..] if !path.is_empty() => {
            crate::interval_player::extend(host, path)?;
        }
        ["load", path @ ..] if !path.is_empty() => {
            let mut loaded = crate::interval_player::load(path)?;
            loaded.detach_player_control()?;
            *host = loaded;
            println!("loaded");
        }
        _ => inspect(host, words)?,
    }
    Ok(true)
}

fn inspect(host: &CoreHost, words: &[&str]) -> Result<(), String> {
    let state = host.snapshot();
    let physical = &state.core.physical;
    match words {
        ["status"] => println!(
            "now_millis={} tick={} people={} living={} active_jobs={}",
            host.core().simulated_millis(),
            physical.custody.tick,
            state.core.people.len(),
            state
                .core
                .people
                .iter()
                .filter(|person| person.health > 0)
                .count(),
            physical.work.len()
        ),
        ["people"] => display(&state.core.people)?,
        ["catalog"] => display(&(&physical.goods, &physical.recipes))?,
        ["goods", id] => {
            let person = id.parse().map_err(|_| "invalid person")?;
            let holder = physical
                .custody
                .containers
                .iter()
                .find(|holder| holder.place == Place::Person(person))
                .ok_or("unknown person")?;
            display(&holder.lots)?;
        }
        ["stock", id] => {
            let region: u64 = id.parse().map_err(|_| "invalid region")?;
            let stock: Vec<_> = physical
                .custody
                .containers
                .iter()
                .filter(|row| matches!(row.place, Place::Stock { region: id, .. } if id == region))
                .collect();
            display(&stock)?;
        }
        ["fields"] => display(
            &state
                .agriculture
                .as_ref()
                .ok_or("cultivation is disabled")?
                .fields,
        )?,
        ["irrigation"] => display(&state.irrigation.as_ref().ok_or("irrigation is disabled")?)?,
        ["tool-condition"] => display(
            &state
                .tool_wear
                .as_ref()
                .ok_or("tool wear is disabled")?
                .conditions,
        )?,
        ["claims"] => display(&state.claims.as_ref().ok_or("claims are disabled")?)?,
        ["promises"] => display(
            &state
                .agreements
                .as_ref()
                .ok_or("promises are disabled")?
                .contracts,
        )?,
        ["expeditions"] => display(
            &state
                .expeditions
                .as_ref()
                .ok_or("expeditions are disabled")?
                .itineraries,
        )?,
        ["health"] => display(
            &state
                .health
                .as_ref()
                .ok_or("disease is disabled")?
                .owner
                .cases,
        )?,
        _ => return Err("unknown command; enter help for commands".into()),
    }
    Ok(())
}

pub(super) fn run(
    root: &Path,
    scenario: &Path,
    seed: Option<u64>,
    population: Option<&Path>,
) -> Result<(), String> {
    let mut host = game_economy_cli::core_scenario_with_options(
        scenario,
        &root.join("content"),
        seed,
        population,
    )?;
    host.detach_player_control()?;
    println!("Elapsed simulation; enter help for commands.");
    crate::terminal::run(|words| command(&mut host, words))
}
