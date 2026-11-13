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
fn tracked_changes(root: &Path) -> Option<bool> {
    let output = std::process::Command::new("git")
        .args(["diff", "--quiet", "HEAD", "--"])
        .current_dir(root)
        .output()
        .ok()?;
    match output.status.code() {
        Some(0) => Some(false),
        Some(1) => Some(true),
        _ => None,
    }
}

fn observations(events: &[CoreRunEvent], current_tick: &mut u64) -> Vec<Value> {
    events.iter().filter_map(|event| match event {
        CoreRunEvent::Meetings(rows) if !rows.is_empty() => Some(json!({"kind":"meetings","events":rows})),
        CoreRunEvent::Disease(rows) if !rows.is_empty() => Some(json!({"kind":"disease","events":rows})),
        CoreRunEvent::Meals(rows) if !rows.is_empty() => Some(json!({"kind":"meals","events":rows})),
        CoreRunEvent::Treatments(rows) if !rows.is_empty() => Some(json!({"kind":"treatments","events":rows})),
        CoreRunEvent::Provisions(rows) if !rows.is_empty() => Some(json!({"kind":"provisions","events":rows})),
        CoreRunEvent::Repairs(rows) if !rows.is_empty() => Some(json!({"kind":"site_repairs","events":rows})),
        CoreRunEvent::ToolRepairs(rows) if !rows.is_empty() => Some(json!({"kind":"tool_repairs","events":rows})),
        CoreRunEvent::Exchanges(rows) if !rows.is_empty() => Some(json!({"kind":"exchanges","people":rows})),
        CoreRunEvent::Offers(rows) if !rows.is_empty() => Some(json!({"kind":"offers","people":rows})),
        CoreRunEvent::Acceptances(rows) if !rows.is_empty() => Some(json!({"kind":"acceptances","people":rows})),
        CoreRunEvent::Requests(rows) if !rows.is_empty() => Some(json!({"kind":"work_requests","events":rows})),
        CoreRunEvent::Starts(rows) if !rows.is_empty() => Some(json!({"kind":"work_starts","outcomes":rows})),
        CoreRunEvent::Work(row) => Some(json!({"kind":"early_work","tick":row.tick,"finished":row.finished,"interrupted":row.interrupted})),
        CoreRunEvent::Construction(rows) if !rows.is_empty() => Some(json!({"kind":"construction","events":rows})),
        CoreRunEvent::LocalCarries(rows) if !rows.is_empty() => Some(json!({"kind":"local_carrying","events":rows})),
        CoreRunEvent::PersonalChoices(rows) if !rows.is_empty() => Some(json!({"kind":"personal_choices","events":rows})),
        CoreRunEvent::PersonalStrikes { events, harm } if !events.is_empty() => Some(json!({"kind":"personal_strikes","tick":*current_tick,"events":events,"harm":harm})),
        CoreRunEvent::MechanicalExperiments(rows) if !rows.is_empty() => Some(json!({"kind":"mechanical_experiments","events":rows})),
        CoreRunEvent::HaulingRequests(rows) if !rows.is_empty() => Some(json!({"kind":"hauling_requests","people":rows})),
        CoreRunEvent::HaulingTransfers(rows) if !rows.is_empty() => Some(json!({"kind":"hauling_transfers","outcomes":rows})),
        CoreRunEvent::WrittenPages(rows) if !rows.is_empty() => Some(json!({"kind":"written_pages","items":rows})),
        CoreRunEvent::Boundary(row) => {
            *current_tick = row.tick;
            Some(json!({"kind":"boundary","tick":row.tick,"finished":row.finished,"delivered":row.delivered,"interrupted":row.interrupted}))
        },
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
    let tick_millis = initial.core.physical.tick_millis;
    if tick_millis == 0 || !YEAR_MILLIS.is_multiple_of(tick_millis) {
        return Err("the history tick must divide a 365-day year exactly".into());
    }
    let ticks_per_year = YEAR_MILLIS / tick_millis;
    if years == 0 {
        return Err("years must be positive".into());
    }
    let start_tick = initial.core.physical.custody.tick;
    if !start_tick.is_multiple_of(ticks_per_year) {
        return Err("history must start at a year boundary".into());
    }
    let start_year = start_tick / ticks_per_year;
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
        "production and decisions are quantized to the scenario tick",
        "scientific invention, political and organizational history, artificial selection, and several other features are not implemented",
        "observations include only selected event types; snapshots are retained annually",
    ];
    if !resume
        && source
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("economy-history-pilot"))
    {
        limitations.extend([
            "exploratory annual pilot, not a calibrated civilization",
            "hunger and animal food scarcity are disabled in this scenario",
        ]);
        if initial
            .social
            .as_ref()
            .is_none_or(|social| !social.owner.inherit_births)
        {
            limitations.push("newborn social participation is not automatically inherited");
        }
    }
    if resume {
        limitations.push("consult the source history metadata for scenario-specific limitations");
    }
    line(
        &mut writer,
        &json!({"kind":"metadata","schema":1,"years_requested":years,"start_year":start_year,"tick_millis":tick_millis,"ticks_per_year":ticks_per_year,"calendar":"365-day years","seed":initial.core.seed,"workers":workers,"git_revision":revision(root),"git_revision_scope":"checkout at run time; executable build provenance is not verified","tracked_source_modified":tracked_changes(root),"executable":std::env::current_exe().ok(),"source":source,"source_kind":if resume {"checkpoint"} else {"scenario"},"scenario":scenario_text,"limitations":limitations}),
    )?;
    line(
        &mut writer,
        &json!({"kind":"initial","year":start_year,"snapshot":initial}),
    )?;
    // The initial snapshot already contains any events restored from a save.
    // Subsequent events are archived with the year in which they occurred.
    host.take_opinion_events();
    let mut observation_tick = initial.core.physical.custody.tick;
    let mut previous_animals = animals(&host);
    for year in start_year + 1..=end_year {
        let mut yearly_observations = Vec::new();
        let mut previous_step_animals = previous_animals.clone();
        for _ in 0..ticks_per_year {
            let result = match host.advance_continuous(tick_millis, &[]) {
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
            if result.elapsed_millis != tick_millis || result.boundaries != 1 {
                line(
                    &mut writer,
                    &json!({"kind":"failure","year":year,"message":"unexpected elapsed time or boundary count"}),
                )?;
                return Err(format!(
                    "year {year}: unexpected elapsed time or boundary count"
                ));
            }
            yearly_observations.extend(observations(&result.events, &mut observation_tick));
            let opinion_events = host.take_opinion_events();
            if !opinion_events.is_empty() {
                yearly_observations
                    .push(json!({"kind":"opinion_changes", "events":opinion_events}));
            }
            let tick_snapshot = host.snapshot();
            if let Some(language) = tick_snapshot
                .social
                .as_ref()
                .and_then(|social| social.language.as_ref())
                && !language.events.is_empty()
            {
                yearly_observations
                    .push(json!({"kind":"language_events", "events":language.events}));
            }
            if let Some(signals) = tick_snapshot
                .social
                .as_ref()
                .and_then(|social| social.signals.as_ref())
            {
                if !signals.events.is_empty() {
                    yearly_observations
                        .push(json!({"kind":"signal_events", "events":signals.events}));
                }
                let learning: Vec<_> = signals
                    .learning_events
                    .iter()
                    .filter(|event| event.tick == tick_snapshot.core.physical.custody.tick)
                    .collect();
                if !learning.is_empty() {
                    yearly_observations.push(json!({"kind":"signal_learning", "events":learning}));
                }
            }
            if ticks_per_year > 1 {
                if let Some(family) = &tick_snapshot.family
                    && !family.events.is_empty()
                {
                    yearly_observations
                        .push(json!({"kind":"family_events", "events":family.events}));
                }
                if let Some(fauna) = &tick_snapshot.fauna {
                    let current: BTreeSet<_> = fauna
                        .animals
                        .iter()
                        .filter(|animal| animal.health > 0)
                        .map(|animal| animal.id)
                        .collect();
                    let appeared: Vec<_> = current
                        .difference(&previous_step_animals)
                        .copied()
                        .collect();
                    let inactive: Vec<_> = previous_step_animals
                        .difference(&current)
                        .copied()
                        .collect();
                    if !appeared.is_empty() || !inactive.is_empty() {
                        let newborns: Vec<_> = fauna
                            .animals
                            .iter()
                            .filter(|animal| appeared.binary_search(&animal.id).is_ok())
                            .collect();
                        yearly_observations.push(json!({"kind":"animal_population","tick":tick_snapshot.core.physical.custody.tick,"appeared":newborns,"inactive":inactive}));
                    }
                    previous_step_animals = current;
                }
            }
        }
        let snapshot = host.snapshot();
        if snapshot.core.physical.custody.tick
            != year
                .checked_mul(ticks_per_year)
                .ok_or("history tick overflow")?
        {
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
            &json!({"kind":"year","year":year,"living_people":living,"living_animals":animal_count,"animal_ids_appeared":appeared,"animal_ids_no_longer_alive":inactive,"observations":yearly_observations,"snapshot":snapshot}),
        )?;
        previous_animals = current_animals;
        if ticks_per_year >= 8_760 || year % 50 == 0 || year == end_year {
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
    fn personal_strikes_are_preserved_in_history() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let token = format!(
            "legacy-strike-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let scenario = std::env::temp_dir().join(format!("{token}.toml"));
        let output = std::env::temp_dir().join(format!("{token}.ndjson"));
        let text =
            fs::read_to_string(root.join("content/scenarios/economy-core-personal-conflict.toml"))
                .unwrap()
                .replace("tick_millis = 900000", "tick_millis = 31536000000");
        fs::write(&scenario, text).unwrap();
        run(&root, &scenario, &output, 1, 1, false).unwrap();
        let rows = records(&output);
        let strike = rows
            .iter()
            .filter(|row| row["kind"] == "year")
            .flat_map(|row| row["observations"].as_array().unwrap())
            .find(|event| event["kind"] == "personal_strikes")
            .unwrap();
        assert_eq!(strike["events"].as_array().unwrap().len(), 2);
        assert_eq!(strike["harm"][0]["target"], 2);
        assert_eq!(strike["harm"][0]["after"], 1);
        assert_eq!(strike["tick"], 0);
        fs::remove_file(scenario).unwrap();
        fs::remove_file(&output).unwrap();
        fs::remove_file(output.with_extension("year-1.json")).unwrap();
    }

    #[test]
    fn language_observations_are_archived_from_real_meetings() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = root.join("content/scenarios/economy-history-monthly.toml");
        let token = format!(
            "legacy-language-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let scenario = std::env::temp_dir().join(format!("{token}.toml"));
        let output = std::env::temp_dir().join(format!("{token}.ndjson"));
        let text = format!(
            "{}\n[language.rules]\nsounds = [\"a\", \"k\"]\nword_length = 3\ncapacity = 16\n",
            fs::read_to_string(original).unwrap()
        );
        fs::write(&scenario, text).unwrap();
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| run(&root, &scenario, &output, 2, 1, false))
            .unwrap();
        let rows = records(&output);
        assert!(rows.iter().filter(|row| row["kind"] == "year").any(|row| {
            row["observations"].as_array().is_some_and(|events| {
                events
                    .iter()
                    .any(|event| event["kind"] == "language_events")
            })
        }));
        fs::remove_file(scenario).unwrap();
        fs::remove_file(&output).unwrap();
        fs::remove_file(output.with_extension("year-2.json")).unwrap();
    }

    #[test]
    fn directed_signals_are_archived_without_private_intent() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let token = format!(
            "legacy-signal-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let scenario = std::env::temp_dir().join(format!("{token}.toml"));
        let output = std::env::temp_dir().join(format!("{token}.ndjson"));
        let source = fs::read_to_string(root.join("content/scenarios/economy-core-signals.toml"))
            .unwrap()
            .replace("tick_millis = 900000", "tick_millis = 31536000000");
        fs::write(&scenario, source).unwrap();
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| run(&root, &scenario, &output, 1, 1, false))
            .unwrap();
        let rows = records(&output);
        let event = rows
            .iter()
            .filter(|row| row["kind"] == "year")
            .flat_map(|row| row["observations"].as_array().unwrap())
            .find(|event| event["kind"] == "signal_events")
            .unwrap();
        assert_eq!(event["events"][0]["actor"], 1);
        assert_eq!(event["events"][0]["interpreted"], 2);
        assert!(event["events"][0].get("intended").is_none());
        fs::remove_file(scenario).unwrap();
        fs::remove_file(&output).unwrap();
        fs::remove_file(output.with_extension("year-1.json")).unwrap();
    }

    #[test]
    fn subjective_signal_learning_is_archived_without_private_intent() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let token = format!(
            "legacy-signal-learning-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let scenario = std::env::temp_dir().join(format!("{token}.toml"));
        let output = std::env::temp_dir().join(format!("{token}.ndjson"));
        let source =
            fs::read_to_string(root.join("content/scenarios/economy-core-signal-learning.toml"))
                .unwrap()
                .replace("tick_millis = 900000", "tick_millis = 31536000000")
                .replace("good = \"bread\"", "good = \"timber\"")
                .replacen(
                    "seek_meals = false",
                    "seek_meals = false\ngoods = { timber = 1 }",
                    1,
                );
        let source = format!(
            "{source}\n[giving]\n[[giving.policies]]\nperson = 1\nrecipient = 2\ngood = \"timber\"\nquantity = 1\nrecipient_hunger_at_least = 2\nwilling = true\n"
        );
        fs::write(&scenario, source).unwrap();
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| run(&root, &scenario, &output, 2, 1, false))
            .unwrap();
        let rows = records(&output);
        let event = rows
            .iter()
            .filter(|row| row["kind"] == "year")
            .flat_map(|row| row["observations"].as_array().unwrap())
            .find(|event| event["kind"] == "signal_learning")
            .unwrap();
        assert_eq!(event["events"][0]["observer"], 2);
        assert_eq!(event["events"][0]["meaning"], 2);
        assert!(event["events"][0].get("intended").is_none());
        fs::remove_file(scenario).unwrap();
        fs::remove_file(&output).unwrap();
        fs::remove_file(output.with_extension("year-2.json")).unwrap();
    }

    #[test]
    fn personal_choice_outcomes_are_archived() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = root.join("content/scenarios/economy-core-personal-choices.toml");
        let token = format!(
            "legacy-choice-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let scenario = std::env::temp_dir().join(format!("{token}.toml"));
        let output = std::env::temp_dir().join(format!("{token}.ndjson"));
        let text = fs::read_to_string(original)
            .unwrap()
            .replace("tick_millis = 900000", "tick_millis = 31536000000");
        fs::write(&scenario, text).unwrap();
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| run(&root, &scenario, &output, 1, 1, false))
            .unwrap();
        let rows = records(&output);
        assert!(rows.iter().filter(|row| row["kind"] == "year").any(|row| {
            row["observations"].as_array().is_some_and(|events| {
                events
                    .iter()
                    .any(|event| event["kind"] == "personal_choices")
            })
        }));
        fs::remove_file(scenario).unwrap();
        fs::remove_file(&output).unwrap();
        fs::remove_file(output.with_extension("year-1.json")).unwrap();
    }

    #[test]
    fn chosen_mechanical_experiments_are_archived() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let token = format!(
            "legacy-mechanics-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let scenario = std::env::temp_dir().join(format!("{token}.toml"));
        let output = std::env::temp_dir().join(format!("{token}.ndjson"));
        let source = fs::read_to_string(root.join("content/scenarios/economy-core-mechanics.toml"))
            .unwrap()
            .replace("tick_millis = 900000", "tick_millis = 31536000000");
        fs::write(&scenario, source).unwrap();
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| run(&root, &scenario, &output, 1, 1, false))
            .unwrap();
        let rows = records(&output);
        assert!(rows.iter().filter(|row| row["kind"] == "year").any(|row| {
            row["observations"].as_array().is_some_and(|events| {
                events.iter().any(|event| {
                    event["kind"] == "mechanical_experiments" && event["events"][0]["actor"] == 1
                })
            })
        }));
        fs::remove_file(scenario).unwrap();
        fs::remove_file(&output).unwrap();
        fs::remove_file(output.with_extension("year-1.json")).unwrap();
    }

    #[test]
    fn monthly_history_records_intermediate_family_events_and_replays() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scenario = root.join("content/scenarios/economy-history-monthly.toml");
        let token = format!(
            "legacy-monthly-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let one = std::env::temp_dir().join(format!("{token}-one.ndjson"));
        let four = std::env::temp_dir().join(format!("{token}-four.ndjson"));
        for (path, workers) in [(&one, 1), (&four, 4)] {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| run(&root, &scenario, path, 2, workers, false))
                .unwrap();
        }
        let serial = records(&one);
        let parallel = records(&four);
        assert_eq!(serial[0]["ticks_per_year"], 12);
        assert_eq!(
            serial[3]["snapshot"]["core"]["physical"]["custody"]["tick"],
            24
        );
        assert_eq!(&serial[1..], &parallel[1..]);
        assert!(serial[2..4].iter().any(|year| {
            year["observations"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["kind"] == "family_events"))
        }));
        let checkpoint: CoreHostSnapshot =
            serde_json::from_reader(File::open(one.with_extension("year-2.json")).unwrap())
                .unwrap();
        assert_eq!(checkpoint.core.physical.custody.tick, 24);
        for path in [&one, &four] {
            fs::remove_file(path).unwrap();
            fs::remove_file(path.with_extension("year-2.json")).unwrap();
        }
    }

    #[test]
    fn monthly_host_supplies_each_persons_directed_affinity_to_mate_choice() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scenario = root.join("content/scenarios/economy-history-monthly.toml");
        let host = game_economy_cli::core_scenario(&scenario, &root.join("content")).unwrap();
        let mut snapshot = host.snapshot();
        let family = snapshot.family.as_mut().unwrap();
        family.rules.conception_one_in = 1;
        for member in &mut family.members {
            member.reproduction_allowed = member.id == 1 || member.id == 2;
            if member.reproduction_allowed {
                member.genome.alleles[0] = [1000, 1000];
                member.mate_preference = Some(game_families::MatePreference {
                    desire: 1000,
                    genetic_weight: 0,
                    affinity_weight: 1,
                    trust_weight: 0,
                    fear_weight: 0,
                    obligation_weight: 0,
                    attraction_weight: 0,
                    female_attraction: 0,
                    male_attraction: 0,
                    hunger_weight: 0,
                    dependent_weight: 0,
                    minimum: 1,
                });
            }
        }
        let run = |reverse: i32| {
            let mut initial = snapshot.clone();
            initial.opinions.as_mut().unwrap().records = serde_json::from_value(json!([
                {"observer":1,"target":2,"trust":0,"affinity":10},
                {"observer":2,"target":1,"trust":0,"affinity":reverse}
            ]))
            .unwrap();
            let mut host = CoreHost::resume(initial).unwrap();
            let result = host.advance_continuous(2_628_000_000, &[]).unwrap();
            assert_eq!(result.failure, None);
            host.snapshot().family.unwrap().members[0].pregnancy.clone()
        };
        assert!(run(-10).is_none());
        assert_eq!(run(10).unwrap().father, 2);
        let run_with_fear = |weight: i32| {
            let mut initial = snapshot.clone();
            initial.family.as_mut().unwrap().members[0]
                .mate_preference
                .as_mut()
                .unwrap()
                .fear_weight = weight;
            initial.opinions.as_mut().unwrap().records = serde_json::from_value(json!([
                {"observer":1,"target":2,"trust":0,"affinity":-10,"fear":20},
                {"observer":2,"target":1,"trust":0,"affinity":10}
            ]))
            .unwrap();
            let mut host = CoreHost::resume(initial).unwrap();
            assert_eq!(
                host.advance_continuous(2_628_000_000, &[]).unwrap().failure,
                None
            );
            host.snapshot().family.unwrap().members[0].pregnancy.clone()
        };
        assert!(run_with_fear(0).is_none());
        assert_eq!(run_with_fear(1).unwrap().father, 2);
    }

    #[test]
    fn whole_boundary_host_supplies_directed_affinity_to_mate_choice() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scenario = root.join("content/scenarios/economy-history-monthly.toml");
        let world = game_economy_cli::scenario(&scenario, &root.join("content")).unwrap();
        let mut snapshot = world.snapshot();
        let family = snapshot.family.as_mut().unwrap();
        family.rules.conception_one_in = 1;
        for member in &mut family.members {
            member.reproduction_allowed = member.id == 1 || member.id == 2;
            if member.reproduction_allowed {
                member.genome.alleles[0] = [1000, 1000];
                member.mate_preference = Some(game_families::MatePreference {
                    desire: 1000,
                    genetic_weight: 0,
                    affinity_weight: 1,
                    trust_weight: 0,
                    fear_weight: 0,
                    obligation_weight: 0,
                    attraction_weight: 0,
                    female_attraction: 0,
                    male_attraction: 0,
                    hunger_weight: 0,
                    dependent_weight: 0,
                    minimum: 1,
                });
            }
        }
        let run = |reverse: i32| {
            let mut initial = snapshot.clone();
            initial.opinions.as_mut().unwrap().records = serde_json::from_value(json!([
                {"observer":1,"target":2,"trust":0,"affinity":10},
                {"observer":2,"target":1,"trust":0,"affinity":reverse}
            ]))
            .unwrap();
            let mut world = game_economy::Economy::compile(initial).unwrap();
            world.advance().unwrap();
            world.snapshot().family.unwrap().members[0]
                .pregnancy
                .clone()
        };
        assert!(run(-10).is_none());
        assert_eq!(run(10).unwrap().father, 2);
    }

    #[test]
    fn expanded_monthly_world_loads_its_composed_owners() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scenario = root.join("content/scenarios/economy-history-monthly-expanded.toml");
        let host = game_economy_cli::core_scenario(&scenario, &root.join("content")).unwrap();
        let state = host.snapshot();
        assert_eq!(state.core.people.len(), 60);
        assert!(state.family.as_ref().unwrap().rules.subjective_mating);
        let preference = state.family.as_ref().unwrap().members[0]
            .mate_preference
            .unwrap();
        assert_eq!(
            (
                preference.trust_weight,
                preference.fear_weight,
                preference.obligation_weight
            ),
            (2, 3, 2)
        );
        assert_eq!(
            state.dependent_care.as_ref().unwrap().rules.care_interval,
            3
        );
        assert_eq!(state.fauna.as_ref().unwrap().species.len(), 2);
        assert!(
            state
                .fauna
                .as_ref()
                .unwrap()
                .species
                .iter()
                .all(
                    |species| u128::from(species.intake) * u128::from(species.relief_per_unit)
                        > u128::from(species.hunger_per_tick)
                )
        );
        assert_eq!(state.health.as_ref().unwrap().owner.cases.len(), 1);
        assert!(state.fauna.is_some() && state.social.is_some() && state.opinions.is_some());
        assert!(
            state.environment.is_some()
                && state.agriculture.is_some()
                && state.irrigation.is_some()
        );
        assert!(
            state.gathering.is_some() && state.dependent_care.is_some() && state.hunting.is_some()
        );
        assert!(
            state.goals.is_some()
                && state.skills.is_some()
                && state.giving.is_some()
                && state.market.is_some()
        );
        assert!(
            state.construction.is_some()
                && state.hauling.is_some()
                && state.local_transport.is_some()
                && state.writing.is_some()
        );
        assert!(
            state.animal_products.is_some()
                && state.animal_care.is_some()
                && state.animal_handling.is_some()
                && state.animal_restraints.is_some()
        );
        assert!(
            state.fatigue.is_some()
                && state.claims.is_some()
                && state.agreements.is_some()
                && state.sites.is_some()
        );
        assert!(
            state.animal_feelings.is_some()
                && state.dreams.is_some()
                && state.food_search.is_some()
        );
        assert!(state.shared_work.is_some() && state.taking.is_some() && state.tool_wear.is_some());
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
        assert!(
            first[0]["git_revision_scope"]
                .as_str()
                .unwrap()
                .contains("not verified")
        );
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

    #[test]
    fn annual_social_pilot_records_later_generations_meeting() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scenario = root.join("content/scenarios/economy-history-pilot-social.toml");
        let token = format!(
            "legacy-social-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let output = std::env::temp_dir().join(format!("{token}.ndjson"));
        rayon::ThreadPoolBuilder::new()
            .num_threads(6)
            .build()
            .unwrap()
            .install(|| run(&root, &scenario, &output, 25, 6, false))
            .unwrap();
        let rows = records(&output);
        let later_meeting = rows.iter().filter(|row| row["kind"] == "year").any(|row| {
            row["observations"].as_array().unwrap().iter().any(|event| {
                event["kind"] == "meetings"
                    && event["events"].as_array().unwrap().iter().any(|meeting| {
                        meeting["a"].as_u64().unwrap() > 12 || meeting["b"].as_u64().unwrap() > 12
                    })
            })
        });
        assert!(later_meeting);
        fs::remove_file(&output).unwrap();
        fs::remove_file(output.with_extension("year-25.json")).unwrap();
    }
}
