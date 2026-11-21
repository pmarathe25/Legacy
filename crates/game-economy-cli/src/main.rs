//! Command-line entry point for the elapsed simulation.
mod interval_admin;
mod interval_player;
mod terminal;

use std::ffi::OsString;
use std::path::PathBuf;

enum Mode {
    Campaign,
    Succession,
    Person(u64),
}

fn number(value: Option<OsString>, name: &str) -> Result<u64, String> {
    let value = value.ok_or_else(|| format!("missing {name}"))?;
    value
        .to_str()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| format!("invalid {name}"))
}

fn run() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args_os().skip(1);
    let mut mode = None;
    let mut scenario = None;
    let mut seed = None;
    let mut population = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--player") if mode.is_none() => mode = Some(Mode::Campaign),
            Some("--interval-campaign") if mode.is_none() => mode = Some(Mode::Succession),
            Some("--person" | "--interval-player") if mode.is_none() => {
                mode = Some(Mode::Person(number(args.next(), "controlled person")?));
            }
            Some("--seed") if seed.is_none() => seed = Some(number(args.next(), "seed")?),
            Some("--population") if population.is_none() => {
                population = Some(PathBuf::from(
                    args.next().ok_or("missing population profile")?,
                ));
            }
            _ if scenario.is_none() && !arg.to_string_lossy().starts_with("--") => {
                scenario = Some(PathBuf::from(arg));
            }
            _ => return Err("unexpected argument".into()),
        }
    }
    let is_player = mode.is_some();
    let (default_scenario, person) = match mode {
        None => ("economy.toml", None),
        Some(Mode::Campaign) => ("economy-campaign.toml", None),
        Some(Mode::Succession) => ("economy-succession.toml", None),
        Some(Mode::Person(person)) => ("economy-core-player.toml", Some(person)),
    };
    let scenario =
        scenario.unwrap_or_else(|| root.join("content/scenarios").join(default_scenario));
    let population = population.as_deref();
    if is_player {
        interval_player::run(&root, &scenario, person, seed, population)
    } else {
        interval_admin::run(&root, &scenario, seed, population)
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
