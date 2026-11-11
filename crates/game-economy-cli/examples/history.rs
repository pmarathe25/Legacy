//! Stream an explicitly annual exploratory interval-host simulation.
use game_economy::core_interval::{CoreHost, CoreHostSnapshot, CoreRunEvent};
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
    source: &Path,
    output: &Path,
    years: u64,
    workers: usize,
    resume: bool,
) -> Result<(), String> {
    let (mut host, scenario_text) = if resume {
        let file = File::open(source).map_err(|e| e.to_string())?;
        let snapshot: CoreHostSnapshot =
            serde_json::from_reader(file).map_err(|e| e.to_string())?;
        (CoreHost::resume(snapshot)?, None)
    } else {
        let text = fs::read_to_string(source).map_err(|e| e.to_string())?;
        (
            game_economy_cli::core_scenario(source, &root.join("content"))?,
            Some(text),
        )
    };
    let initial = host.snapshot();
    if initial.core.physical.tick_millis != YEAR_MILLIS {
        return Err("the annual history runner requires a 365-day tick in the scenario".into());
    }
    if years == 0 {
        return Err("years must be positive".into());
    }
    let start_year = initial.core.physical.custody.tick;
    let end_year = start_year
        .checked_add(years)
        .ok_or("history year overflow")?;
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);
    let mut limitations = vec![
        "one tick per year; production and decisions are quantized to years",
        "scientific invention, political and organizational history, artificial selection, and several other features are not implemented",
        "animal parentage is not recorded by the current fauna owner",
        "observations include only selected event types; snapshots are retained annually",
    ];
    if !resume
        && source
            .file_name()
            .is_some_and(|name| name == "economy-history-pilot.toml")
    {
        limitations.extend([
            "exploratory annual pilot, not a calibrated civilization",
            "hunger and animal food scarcity are disabled in this scenario",
            "newborn social participation is not automatically inherited",
        ]);
    }
    if resume {
        limitations.push("consult the source history metadata for scenario-specific limitations");
    }
    line(
        &mut writer,
        &json!({"kind":"metadata","schema":1,"years_requested":years,"start_year":start_year,"tick_millis":YEAR_MILLIS,"calendar":"365-day years; one simulation tick per year","seed":initial.core.seed,"workers":workers,"git_revision":revision(root),"source":source,"source_kind":if resume {"checkpoint"} else {"scenario"},"scenario":scenario_text,"limitations":limitations}),
    )?;
    line(
        &mut writer,
        &json!({"kind":"initial","year":start_year,"snapshot":initial}),
    )?;
    let mut previous_animals = animals(&host);
    for year in start_year + 1..=end_year {
        let result = match host.advance_continuous(YEAR_MILLIS, &[]) {
            Ok(result) => result,
            Err(error) => {
                line(
                    &mut writer,
                    &json!({"kind":"failure","year":year,"message":error}),
                )?;
                return Err(format!("year {year}: {error}"));
            }
        };
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
            line(
                &mut writer,
                &json!({"kind":"failure","year":year,"message":"unexpected simulation tick"}),
            )?;
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
        if year % 50 == 0 || year == end_year {
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
                "year {year}/{end_year}: {living} living people, {animal_count} living animals; checkpoint {}",
                checkpoint.display()
            );
        }
    }
    line(
        &mut writer,
        &json!({"kind":"complete","years":years,"last_year":end_year}),
    )?;
    Ok(())
}

fn main() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args_os().skip(1);
    let first = args
        .next()
        .ok_or("usage: history [--resume] SCENARIO_OR_CHECKPOINT OUTPUT [YEARS] [WORKERS]")?;
    let resume = first == "--resume";
    let source = if resume {
        PathBuf::from(args.next().ok_or("missing checkpoint path")?)
    } else {
        PathBuf::from(first)
    };
    let output = args
        .next()
        .map(PathBuf::from)
        .ok_or("missing history output path")?;
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
        .install(|| run(&root, &source, &output, years, workers, resume))
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_economy::core_interval::CoreHostSnapshot;

    fn records(path: &Path) -> Vec<Value> {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn annual_history_is_deterministic_and_checkpoint_resumes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scenario = root.join("content/scenarios/economy-history-pilot.toml");
        let token = format!(
            "legacy-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let one = std::env::temp_dir().join(format!("{token}-one.ndjson"));
        let six = std::env::temp_dir().join(format!("{token}-six.ndjson"));
        let four = std::env::temp_dir().join(format!("{token}-four.ndjson"));
        let restart = std::env::temp_dir().join(format!("{token}-restart.ndjson"));
        for (path, years, workers) in [(&one, 3, 1), (&six, 3, 6), (&four, 4, 1)] {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| run(&root, &scenario, path, years, workers, false))
                .unwrap();
        }
        let first = records(&one);
        let parallel = records(&six);
        let continued = records(&four);
        assert_eq!(first.len(), 6);
        assert_eq!(first[0]["tick_millis"], YEAR_MILLIS);
        assert_eq!(first.last().unwrap()["kind"], "complete");
        assert_eq!(&first[1..], &parallel[1..]);
        assert_eq!(first[4]["year"], 3);
        assert!(
            first[4]["snapshot"]["family"]["totals"]["births"]
                .as_u64()
                .unwrap()
                > 0
        );
        let checkpoint: CoreHostSnapshot =
            serde_json::from_reader(File::open(one.with_extension("year-3.json")).unwrap())
                .unwrap();
        let mut resumed = CoreHost::resume(checkpoint).unwrap();
        let result = resumed.advance_continuous(YEAR_MILLIS, &[]).unwrap();
        assert_eq!(result.failure, None);
        assert_eq!(
            serde_json::to_value(resumed.snapshot()).unwrap(),
            continued[5]["snapshot"]
        );
        rayon::ThreadPoolBuilder::new()
            .num_threads(6)
            .build()
            .unwrap()
            .install(|| {
                run(
                    &root,
                    &one.with_extension("year-3.json"),
                    &restart,
                    1,
                    6,
                    true,
                )
            })
            .unwrap();
        let restarted = records(&restart);
        assert_eq!(restarted[0]["start_year"], 3);
        assert_eq!(restarted[2]["snapshot"], continued[5]["snapshot"]);
        for (path, year) in [(&one, 3), (&six, 3), (&four, 4), (&restart, 4)] {
            fs::remove_file(path).unwrap();
            fs::remove_file(path.with_extension(format!("year-{year}.json"))).unwrap();
        }
    }
}
