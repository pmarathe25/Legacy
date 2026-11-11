//! Stream an explicitly annual exploratory interval-host simulation.
use game_economy::core_interval::{CoreHost, CoreRunEvent};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

const YEAR_MILLIS: u64 = 365 * 24 * 60 * 60 * 1000;

fn line<T: Serialize>(writer: &mut BufWriter<File>, value: &T) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value).map_err(|e| e.to_string())?;
    writer.write_all(b"\n").map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())
}

fn revision(root: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn observations(events: &[CoreRunEvent]) -> Vec<Value> {
    events.iter().filter_map(|event| match event {
        CoreRunEvent::Meetings(rows) if !rows.is_empty() => Some(json!({"kind":"meetings","events":rows})),
        CoreRunEvent::Starts(rows) if !rows.is_empty() => Some(json!({"kind":"work_starts","outcomes":rows})),
        CoreRunEvent::Work(row) => Some(json!({"kind":"early_work","tick":row.tick,"finished":row.finished,"interrupted":row.interrupted})),
        CoreRunEvent::Construction(rows) if !rows.is_empty() => Some(json!({"kind":"construction","events":rows})),
        CoreRunEvent::LocalCarries(rows) if !rows.is_empty() => Some(json!({"kind":"local_carrying","events":rows})),
        CoreRunEvent::HaulingRequests(rows) if !rows.is_empty() => Some(json!({"kind":"hauling_requests","people":rows})),
        CoreRunEvent::HaulingTransfers(rows) if !rows.is_empty() => Some(json!({"kind":"hauling_transfers","outcomes":rows})),
        CoreRunEvent::WrittenPages(rows) if !rows.is_empty() => Some(json!({"kind":"written_pages","items":rows})),
        CoreRunEvent::Boundary(row) => Some(json!({"kind":"boundary","tick":row.tick,"finished":row.finished,"delivered":row.delivered,"interrupted":row.interrupted})),
        _ => None,
    }).collect()
}

fn animals(host: &CoreHost) -> BTreeSet<u64> {
    host.snapshot().fauna.map_or_else(BTreeSet::new, |fauna| {
        fauna
            .animals
            .iter()
            .filter(|animal| animal.health > 0)
            .map(|animal| animal.id)
            .collect()
    })
}

fn run(
    root: &Path,
    scenario: &Path,
    output: &Path,
    years: u64,
    workers: usize,
) -> Result<(), String> {
    let scenario_text = fs::read_to_string(scenario).map_err(|e| e.to_string())?;
    let mut host = game_economy_cli::core_scenario(scenario, &root.join("content"))?;
    let initial = host.snapshot();
    if initial.core.physical.tick_millis != YEAR_MILLIS {
        return Err("the annual history runner requires a 365-day tick in the scenario".into());
    }
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);
    line(
        &mut writer,
        &json!({"kind":"metadata","schema":1,"years_requested":years,"tick_millis":YEAR_MILLIS,"calendar":"365-day years; one simulation tick per year","seed":initial.core.seed,"workers":workers,"git_revision":revision(root),"scenario":scenario_text,"limitations":["exploratory annual pilot, not a calibrated civilization","hunger and animal food scarcity are disabled in this scenario","production and decisions are quantized to years","scientific invention, political and organizational history, artificial selection, and several other features are not implemented","newborn social participation is not automatically inherited","animal parentage is not recorded by the current fauna owner","observations include only selected event types; snapshots are retained annually"]}),
    )?;
    line(
        &mut writer,
        &json!({"kind":"initial","year":0,"snapshot":initial}),
    )?;
    let mut previous_animals = animals(&host);
    for year in 1..=years {
        let result = host.advance_continuous(YEAR_MILLIS, &[])?;
        if let Some(error) = &result.failure {
            line(
                &mut writer,
                &json!({"kind":"failure","year":year,"message":error}),
            )?;
            return Err(format!("year {year}: {error}"));
        }
        if result.elapsed_millis != YEAR_MILLIS || result.boundaries != 1 {
            line(
                &mut writer,
                &json!({"kind":"failure","year":year,"message":"unexpected elapsed time or boundary count"}),
            )?;
            return Err(format!(
                "year {year}: unexpected elapsed time or boundary count"
            ));
        }
        let snapshot = host.snapshot();
        if snapshot.core.physical.custody.tick != year {
            return Err(format!("year {year}: unexpected simulation tick"));
        }
        let current_animals: BTreeSet<_> =
            snapshot.fauna.as_ref().map_or_else(BTreeSet::new, |fauna| {
                fauna
                    .animals
                    .iter()
                    .filter(|animal| animal.health > 0)
                    .map(|animal| animal.id)
                    .collect()
            });
        let appeared: Vec<_> = current_animals
            .difference(&previous_animals)
            .copied()
            .collect();
        let inactive: Vec<_> = previous_animals
            .difference(&current_animals)
            .copied()
            .collect();
        let living = snapshot
            .core
            .people
            .iter()
            .filter(|person| person.health > 0)
            .count();
        let animal_count = current_animals.len();
        line(
            &mut writer,
            &json!({"kind":"year","year":year,"living_people":living,"living_animals":animal_count,"animal_ids_appeared":appeared,"animal_ids_no_longer_alive":inactive,"observations":observations(&result.events),"snapshot":snapshot}),
        )?;
        previous_animals = current_animals;
        if year % 50 == 0 || year == years {
            let checkpoint = output.with_extension(format!("year-{year}.json"));
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&checkpoint)
                .map_err(|e| e.to_string())?;
            let mut checkpoint_writer = BufWriter::new(file);
            serde_json::to_writer(&mut checkpoint_writer, &host.snapshot())
                .map_err(|e| e.to_string())?;
            checkpoint_writer.flush().map_err(|e| e.to_string())?;
            println!(
                "year {year}/{years}: {living} living people, {animal_count} living animals; checkpoint {}",
                checkpoint.display()
            );
        }
    }
    line(&mut writer, &json!({"kind":"complete","years":years}))?;
    Ok(())
}

fn main() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args_os().skip(1);
    let scenario = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("content/scenarios/economy-history-pilot.toml"));
    let output = args
        .next()
        .map(PathBuf::from)
        .ok_or("usage: history SCENARIO OUTPUT [YEARS] [WORKERS]")?;
    let years = args
        .next()
        .map(|arg| {
            arg.to_string_lossy()
                .parse::<u64>()
                .map_err(|e| e.to_string())
        })
        .transpose()?
        .unwrap_or(300);
    let workers = args
        .next()
        .map(|arg| {
            arg.to_string_lossy()
                .parse::<usize>()
                .map_err(|e| e.to_string())
        })
        .transpose()?
        .unwrap_or(1);
    if years == 0 || workers == 0 || args.next().is_some() {
        return Err("years and workers must be positive, with no extra arguments".into());
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|e| e.to_string())?
        .install(|| run(&root, &scenario, &output, years, workers))
}
