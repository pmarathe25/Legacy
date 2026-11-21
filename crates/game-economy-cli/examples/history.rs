//! Stream an explicitly annual exploratory interval-host simulation.
use game_economy::core_interval::{CoreHost, CoreHostSnapshot, CoreRunEvent};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[path = "history_support/practice_log.rs"]
mod practice_log;

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
    let mut recorded = Vec::new();
    for event in events {
        if let CoreRunEvent::ElapsedEncounters {
            at_millis,
            meetings,
            words,
            signals,
            recipes,
            writing,
        } = event
        {
            if !meetings.is_empty() {
                recorded.push(json!({"kind":"meetings","at_millis":at_millis,"events":meetings}));
            }
            if !words.is_empty() {
                recorded
                    .push(json!({"kind":"language_events","at_millis":at_millis,"events":words}));
            }
            if !signals.is_empty() {
                recorded
                    .push(json!({"kind":"signal_events","at_millis":at_millis,"events":signals}));
            }
            if !recipes.is_empty() {
                recorded
                    .push(json!({"kind":"recipe_learning","at_millis":at_millis,"events":recipes}));
            }
            if !writing.is_empty() {
                recorded.push(
                    json!({"kind":"written_explanations","at_millis":at_millis,"events":writing}),
                );
            }
        } else if let Some(row) = observation(event, current_tick) {
            recorded.push(row);
        }
    }
    recorded
}

#[rustfmt::skip]
fn observation(event: &CoreRunEvent, current_tick: &mut u64) -> Option<Value> {
    match event {
        CoreRunEvent::Meetings(rows) if !rows.is_empty() => Some(json!({"kind":"meetings","events":rows})),
        CoreRunEvent::WrittenExplanations(rows) if !rows.is_empty() => Some(json!({"kind":"written_explanations","events":rows})),
        CoreRunEvent::Disease(rows) if !rows.is_empty() => Some(json!({"kind":"disease","events":rows})),
        CoreRunEvent::LocalWalking { at_millis, events } => Some(json!({"kind":"local_walking","at_millis":at_millis,"events":events})),
        CoreRunEvent::Confinement { at_millis, events } => Some(json!({"kind":"confinement","at_millis":at_millis,"events":events})),
        CoreRunEvent::SiteChanges { at_millis, changes } => Some(json!({"kind":"site_changes","at_millis":at_millis,"demolished":changes.demolished,"salvaged":changes.salvaged,"released":changes.released})),
        CoreRunEvent::Expeditions(events) => Some(json!({"kind":"expeditions","events":events})),
        CoreRunEvent::Tactical(result) => Some(json!({"kind":"tactical","events":result.events,"strikes":result.strikes,"harm":result.harm,"sites":result.sites,"interrupted_jobs":result.interrupted_jobs,"released":result.released})),
        CoreRunEvent::WoundRecovery { at_millis, people } => Some(json!({"kind":"wound_recovery","at_millis":at_millis,"people":people})),
        CoreRunEvent::Campaign(events) => Some(json!({"kind":"campaign","events":events})),
        CoreRunEvent::Care(visit) => Some(json!({"kind":"care","at_millis":visit.at_millis,"events":visit.events,"transfers":visit.transfers,"incapacitated":visit.incapacitated})),
        CoreRunEvent::WildPlants(cycle) => Some(json!({"kind":"wild_plant_cycles","at_millis":cycle.at_millis,"events":cycle.changes})),
        CoreRunEvent::Fauna(row) => Some(json!({"kind":"elapsed_fauna_events","at_millis":row.at_millis,"births":row.births,"deaths":row.deaths,"migrations":row.migrations})),
        CoreRunEvent::InvocationWarning { at_millis, warning } => Some(json!({"kind":"invocation_warning","at_millis":at_millis,"warning":warning})),
        CoreRunEvent::InvocationCost { at_millis, person, damage, lifespan_loss } => Some(json!({"kind":"invocation_cost","at_millis":at_millis,"person":person,"damage":damage,"lifespan_loss":lifespan_loss})),
        CoreRunEvent::Incapacitations { at_millis, people } => Some(json!({"kind":"incapacitations","at_millis":at_millis,"people":people})),
        CoreRunEvent::FamilyTransitions { at_millis, events } => Some(json!({"kind":"elapsed_family_events","at_millis":at_millis,"events":events})),
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
        CoreRunEvent::Spoilage { at_millis, lots } => Some(json!({"kind":"spoilage","at_millis":at_millis,"lots":lots})),
        CoreRunEvent::StorageTemperature { at_millis, celsius } => Some(json!({"kind":"storage_temperature","at_millis":at_millis,"celsius":celsius})),
        CoreRunEvent::Maturation { at_millis, events } => Some(json!({"kind":"maturation","at_millis":at_millis,"events":events})),
        CoreRunEvent::Work(row) => Some(json!({"kind":"early_work","tick":row.tick,"finished":row.finished,"delivered":row.delivered,"interrupted":row.interrupted})),
        CoreRunEvent::Construction(rows) if !rows.is_empty() => Some(json!({"kind":"construction","events":rows})),
        CoreRunEvent::LocalCarries(rows) if !rows.is_empty() => Some(json!({"kind":"local_carrying","events":rows})),
        CoreRunEvent::CrewStarts { at_millis, crew } => Some(json!({"kind":"crew_starts","at_millis":at_millis,"events":crew})),
        CoreRunEvent::AgreementExpiry { at_millis, events } => Some(json!({"kind":"agreement_expiry","at_millis":at_millis,"events":events})),
        CoreRunEvent::ElapsedCultivation { at_millis, fields } => Some(json!({"kind":"elapsed_cultivation","at_millis":at_millis,"events":fields})),
        CoreRunEvent::FarmReview { at_millis, review } => Some(json!({"kind":"farm_review","at_millis":at_millis,"events":review.work,"requests":review.requests})),
        CoreRunEvent::ElapsedGathering { at_millis, events } => Some(json!({"kind":"gathered_resources","at_millis":at_millis,"events":events})),
        CoreRunEvent::PersonalChoices(rows) if !rows.is_empty() => Some(json!({"kind":"personal_choices","events":rows})),
        CoreRunEvent::PersonalStrikes { events, harm } if !events.is_empty() => Some(json!({"kind":"personal_strikes","tick":*current_tick,"events":events,"harm":harm})),
        CoreRunEvent::MechanicalAssemblies(rows) if !rows.is_empty() => Some(json!({"kind":"mechanical_assemblies","events":rows})),
        CoreRunEvent::MechanicalExperiments(rows) if !rows.is_empty() => Some(json!({"kind":"mechanical_experiments","events":rows})),
        CoreRunEvent::HaulingRequests(rows) if !rows.is_empty() => Some(json!({"kind":"hauling_requests","people":rows})),
        CoreRunEvent::HaulingTransfers(rows) if !rows.is_empty() => Some(json!({"kind":"hauling_transfers","outcomes":rows})),
        CoreRunEvent::ReadPages(rows) if !rows.is_empty() => Some(json!({"kind":"read_pages","items":rows})),
        CoreRunEvent::WrittenPages(rows) if !rows.is_empty() => Some(json!({"kind":"written_pages","items":rows})),
        CoreRunEvent::Boundary(row) => {
            *current_tick = row.tick;
            Some(json!({"kind":"boundary","tick":row.tick,"finished":row.finished,"delivered":row.delivered,"interrupted":row.interrupted,"spoiled":row.spoiled,"physical_actions":row.physical_actions}))
        },
        _ => None,
    }
}

fn animals(host: &CoreHost) -> BTreeSet<u64> {
    host.history_view()
        .animals
        .iter()
        .filter(|animal| animal.health > 0)
        .map(|animal| animal.id)
        .collect()
}

fn run(
    root: &Path,
    source: &Path,
    output: &Path,
    years: u64,
    workers: usize,
    resume: bool,
    requested_step_millis: Option<u64>,
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
    host.detach_player_control()?;
    let initial = host.snapshot();
    let tick_millis = initial.core.physical.tick_millis;
    let ticks_per_year = YEAR_MILLIS
        .is_multiple_of(tick_millis)
        .then(|| YEAR_MILLIS / tick_millis);
    let step_millis = requested_step_millis.unwrap_or(tick_millis);
    if step_millis == 0 {
        return Err("history advance size must be positive".into());
    }
    if years == 0 {
        return Err("years must be positive".into());
    }
    let year_duration = u128::from(YEAR_MILLIS);
    let start_time = host.core().simulated_millis();
    if !start_time.is_multiple_of(year_duration) {
        return Err("history must start at a calendar year boundary".into());
    }
    let start_year = u64::try_from(start_time / year_duration)
        .map_err(|_| "history year overflow")?;
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
        "event times and autonomous decision cadence are defined by the enabled systems",
        "the archive covers modeled discoveries and an incomplete range of scientific, institutional, and selection behaviors",
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
        &json!({"kind":"metadata","schema":2,"years_requested":years,"start_year":start_year,"tick_millis":tick_millis,"step_millis":step_millis,"ticks_per_year":ticks_per_year,"year_millis":YEAR_MILLIS,"calendar":"365-day years","seed":initial.core.seed,"workers":workers,"git_revision":revision(root),"git_revision_scope":"checkout at run time; executable build provenance is not verified","tracked_source_modified":tracked_changes(root),"executable":std::env::current_exe().ok(),"source":source,"source_kind":if resume {"checkpoint"} else {"scenario"},"scenario":scenario_text,"limitations":limitations}),
    )?;
    line(
        &mut writer,
        &json!({"kind":"initial","year":start_year,"at_millis":start_time,"snapshot":initial}),
    )?;
    // The initial snapshot already contains any events restored from a save.
    // Subsequent events are archived with the year in which they occurred.
    host.take_opinion_events();
    let mut observation_tick = initial.core.physical.custody.tick;
    let mut previous_animals = animals(&host);
    let mut practice_log = practice_log::PracticeLog::default();
    practice_log.observe(host.history_view().practices);
    let mut advancing = host.stepper(&[]);
    for year in start_year + 1..=end_year {
        // Bound in-memory history even for hourly or finer simulations.
        let mut observation_batch = Vec::new();
        let mut previous_step_animals = previous_animals.clone();
        let end_time = u128::from(year) * year_duration;
        let mut since_flush = 0;
        while advancing.host().core().simulated_millis() < end_time {
            let opening = advancing.host().core();
            let to_calendar = (end_time - opening.simulated_millis()) as u64;
            let to_physical = tick_millis - opening.elapsed_millis().unwrap_or(0);
            let segment = to_calendar.min(to_physical);
            let settles = segment == to_physical;
            let mut remaining = segment;
            // A resumed interval may already be at its physical deadline.
            // A zero-duration call still publishes that settlement.
            loop {
                let elapsed = remaining.min(step_millis);
                let result = advancing.advance(elapsed).and_then(|result| {
                    if let Some(error) = result.failure {
                        return Err(error);
                    }
                    let expected_boundaries = u64::from(settles && elapsed == remaining);
                    if result.elapsed_millis != elapsed || result.boundaries != expected_boundaries
                    {
                        return Err("unexpected elapsed time or boundary count".into());
                    }
                    Ok(result.events)
                });
                match result {
                    Ok(events) => {
                        observation_batch.extend(observations(&events, &mut observation_tick))
                    }
                    Err(error) => {
                        line(
                            &mut writer,
                            &json!({"kind":"failure","year":year,"message":error}),
                        )?;
                        return Err(format!("year {year}: {error}"));
                    }
                }
                remaining -= elapsed;
                if observation_batch.len() >= 4096 {
                    line(
                        &mut writer,
                        &json!({"kind":"observation_batch","year":year,"observations":observation_batch}),
                    )?;
                    observation_batch.clear();
                }
                if remaining == 0 {
                    break;
                }
            }
            let opinion_events = advancing.take_opinion_events();
            if !opinion_events.is_empty() {
                observation_batch.push(json!({"kind":"opinion_changes", "events":opinion_events}));
            }
            let history = advancing.host().history_view();
            let practice_events = practice_log.observe(history.practices);
            if !practice_events.is_empty() {
                observation_batch.push(json!({
                    "kind":"personal_practices",
                    "at_millis":advancing.host().core().simulated_millis(),
                    "events":practice_events
                }));
            }
            if settles && !history.animal_deaths.is_empty() {
                observation_batch
                    .push(json!({"kind":"animal_deaths","events":history.animal_deaths}));
            }
            if settles && !history.elapsed_social {
                let recipe_learning: Vec<_> = history
                    .recipe_learning
                    .iter()
                    .filter(|event| event.tick == history.tick)
                    .collect();
                if !recipe_learning.is_empty() {
                    observation_batch
                        .push(json!({"kind":"recipe_learning", "events":recipe_learning}));
                }
                if !history.language_events.is_empty() {
                    observation_batch
                        .push(json!({"kind":"language_events", "events":history.language_events}));
                }
                if !history.signal_events.is_empty() {
                    observation_batch
                        .push(json!({"kind":"signal_events", "events":history.signal_events}));
                }
            }
            if settles {
                let learning: Vec<_> = history
                    .signal_learning
                    .iter()
                    .filter(|event| event.tick == history.signal_learning_time)
                    .collect();
                if !learning.is_empty() {
                    observation_batch.push(json!({"kind":"signal_learning", "events":learning}));
                }
            }
            if tick_millis < YEAR_MILLIS {
                if settles && !history.family_events.is_empty() {
                    observation_batch
                        .push(json!({"kind":"family_events", "events":history.family_events}));
                }
                {
                    let current: BTreeSet<_> = history
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
                        let newborns: Vec<_> = history
                            .animals
                            .iter()
                            .filter(|animal| appeared.binary_search(&animal.id).is_ok())
                            .collect();
                        observation_batch.push(json!({"kind":"animal_population","tick":history.tick,"appeared":newborns,"inactive":inactive}));
                    }
                    previous_step_animals = current;
                }
            }
            since_flush += 1;
            if since_flush == 24 || advancing.host().core().simulated_millis() == end_time {
                if !observation_batch.is_empty() {
                    line(
                        &mut writer,
                        &json!({"kind":"observation_batch","year":year,"observations":observation_batch}),
                    )?;
                    observation_batch.clear();
                }
                since_flush = 0;
            }
        }
        let snapshot = advancing.host().snapshot();
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
            &json!({"kind":"year","year":year,"at_millis":end_time,"living_people":living,"living_animals":animal_count,"animal_ids_appeared":appeared,"animal_ids_no_longer_alive":inactive,"observations":[],"snapshot":snapshot}),
        )?;
        previous_animals = current_animals;
        if tick_millis <= YEAR_MILLIS / 8_760 || year % 50 == 0 || year == end_year {
            let checkpoint = output.with_extension(format!("year-{year}.json"));
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&checkpoint)
                .map_err(|e| e.to_string())?;
            let mut checkpoint_writer = BufWriter::new(file);
            serde_json::to_writer(&mut checkpoint_writer, &snapshot)
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
    let first = args.next().ok_or(
        "usage: history [--resume] SCENARIO_OR_CHECKPOINT OUTPUT [YEARS] [WORKERS] [STEP_MILLIS]",
    )?;
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
    let step_millis = args
        .next()
        .map(|arg| {
            arg.to_string_lossy()
                .parse::<u64>()
                .map_err(|_| "invalid history step duration")
        })
        .transpose()?;
    if years == 0 || workers == 0 || step_millis == Some(0) || args.next().is_some() {
        return Err("years, workers, and advance size must be positive".into());
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|e| e.to_string())?
        .install(|| run(&root, &source, &output, years, workers, resume, step_millis))
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_economy::core_interval::CoreHostSnapshot;

    fn records(path: &Path) -> Vec<Value> {
        let mut pending = Vec::new();
        let mut result = Vec::new();
        for line in fs::read_to_string(path).unwrap().lines() {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["kind"] == "observation_batch" {
                pending.append(row["observations"].as_array_mut().unwrap());
            } else {
                if row["kind"] == "year" {
                    let events = row["observations"].as_array_mut().unwrap();
                    pending.append(events);
                    *events = std::mem::take(&mut pending);
                }
                result.push(row);
            }
        }
        assert!(pending.is_empty());
        result
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
        run(&root, &scenario, &output, 1, 1, false, None).unwrap();
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
    fn written_explanations_are_recorded_from_actual_meetings() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut host = game_economy_cli::core_scenario(
            &root.join("content/scenarios/economy-core-written-invention.toml"),
            &root.join("content"),
        )
        .unwrap();
        let run = host.advance_continuous(5_400_001, &[]).unwrap();
        assert_eq!(run.failure, None);
        let mut tick = 0;
        let records = observations(&run.events, &mut tick);
        let explanation = records
            .iter()
            .find(|row| row["kind"] == "written_explanations")
            .unwrap();
        assert_eq!(explanation["events"][0]["speaker"], 1);
        assert_eq!(explanation["events"][0]["listener"], 2);
        assert!(explanation["events"][0]["page"].as_u64().unwrap() > 0);
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
            fs::read_to_string(original).unwrap().replace(
                "tick_millis = 2628000000",
                "tick_millis = 2628000000\nsocial_period_millis = 1314000000",
            )
        );
        fs::write(&scenario, text).unwrap();
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| run(&root, &scenario, &output, 2, 1, false, None))
            .unwrap();
        let rows = records(&output);
        let dated: Vec<_> = rows
            .iter()
            .flat_map(|row| row["observations"].as_array().into_iter().flatten())
            .filter(|event| event["kind"] == "language_events")
            .map(|event| event["at_millis"].as_u64().unwrap())
            .collect();
        assert!(dated.len() > 12);
        assert!(dated.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(dated.iter().any(|at| !at.is_multiple_of(2_628_000_000)));
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
            .install(|| run(&root, &scenario, &output, 1, 1, false, None))
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
            .install(|| run(&root, &scenario, &output, 2, 1, false, None))
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
            .install(|| run(&root, &scenario, &output, 1, 1, false, None))
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
            .install(|| run(&root, &scenario, &output, 1, 1, false, None))
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
                .install(|| run(&root, &scenario, path, 2, workers, false, None))
                .unwrap();
        }
        let serial = records(&one);
        let parallel = records(&four);
        let batches: Vec<Value> = fs::read_to_string(&one)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .filter(|row: &Value| row["kind"] == "observation_batch")
            .collect();
        assert_eq!(batches.len(), 2);
        assert!(
            batches
                .iter()
                .all(|row| !row["observations"].as_array().unwrap().is_empty())
        );
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
            host.snapshot().family.unwrap().members[0].pregnancy
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
            host.snapshot().family.unwrap().members[0].pregnancy
        };
        assert!(run_with_fear(0).is_none());
        assert_eq!(run_with_fear(1).unwrap().father, 2);
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
                .install(|| {
                    run(
                        &root,
                        &scenario,
                        path,
                        years,
                        workers,
                        false,
                        (workers == 6).then_some(YEAR_MILLIS / 3 + 17),
                    )
                })
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
                    None,
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
    fn annual_checkpoints_can_split_a_physical_interval() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let template = root.join("content/scenarios/economy-history-pilot.toml");
        let quantum = YEAR_MILLIS * 3 / 4 + 17;
        let token = format!("legacy-split-calendar-{}", std::process::id());
        let path = |suffix: &str| std::env::temp_dir().join(format!("{token}-{suffix}"));
        let scenario = path("scenario.toml");
        let source = fs::read_to_string(&template).unwrap().replace(
            &format!("tick_millis = {YEAR_MILLIS}"),
            &format!("tick_millis = {quantum}"),
        );
        fs::write(&scenario, source).unwrap();
        let first = path("first.ndjson");
        let full = path("full.ndjson");
        let continued = path("continued.ndjson");
        let run_with_workers = |source: &Path, target: &Path, years, workers, resume, step| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| run(&root, source, target, years, workers, resume, step))
                .unwrap();
        };
        run_with_workers(&scenario, &first, 1, 1, false, None);
        run_with_workers(&scenario, &full, 2, 1, false, None);
        let checkpoint = first.with_extension("year-1.json");
        let checkpoint_state: CoreHostSnapshot =
            serde_json::from_reader(File::open(&checkpoint).unwrap()).unwrap();
        assert_eq!(checkpoint_state.core.boundary_millis, Some(u128::from(quantum)));
        assert_eq!(
            checkpoint_state.core.physical.work_interval.unwrap().elapsed_millis,
            YEAR_MILLIS - quantum
        );
        run_with_workers(
            &checkpoint,
            &continued,
            1,
            6,
            true,
            Some(YEAR_MILLIS / 9 + 37),
        );
        let full_rows = records(&full);
        let resumed_rows = records(&continued);
        assert_eq!(resumed_rows[0]["start_year"], 1);
        assert_eq!(resumed_rows[0]["year_millis"], YEAR_MILLIS);
        assert_eq!(resumed_rows[0]["ticks_per_year"], Value::Null);
        assert_eq!(resumed_rows[1]["at_millis"], YEAR_MILLIS);
        let last_year = |rows: Vec<Value>| {
            rows.into_iter()
                .find(|row| row["kind"] == "year" && row["year"] == 2)
                .unwrap()
        };
        let finished = last_year(full_rows);
        assert_eq!(finished["at_millis"], YEAR_MILLIS * 2);
        assert_eq!(finished, last_year(resumed_rows));
        for path in [
            scenario, checkpoint, full.with_extension("year-2.json"),
            continued.with_extension("year-2.json"), first, full, continued,
        ] {
            fs::remove_file(path).unwrap();
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
            .install(|| run(&root, &scenario, &output, 25, 6, false, None))
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
