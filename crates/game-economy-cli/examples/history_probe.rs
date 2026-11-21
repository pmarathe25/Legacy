//! Short, checkpointed diagnostics using the same interval host as history.
use game_economy::core_interval::{CoreHost, CoreHostSnapshot, CoreRunEvent};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

fn write_line(writer: &mut BufWriter<File>, value: &serde_json::Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value).map_err(|error| error.to_string())?;
    writeln!(writer).map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

#[cfg(test)]
fn run(
    root: &Path,
    source: &Path,
    output: &Path,
    ticks: u64,
    interval: u64,
    resume: bool,
) -> Result<(), String> {
    run_steps(root, source, output, ticks, interval, resume, None)
}

fn run_steps(
    root: &Path,
    source: &Path,
    output: &Path,
    ticks: u64,
    interval: u64,
    resume: bool,
    requested_step_millis: Option<u64>,
) -> Result<(), String> {
    let revision = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let modified = std::process::Command::new("git")
        .args(["diff", "--quiet", "HEAD", "--"])
        .current_dir(root)
        .output()
        .ok()
        .and_then(|output| match output.status.code() {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => None,
        });
    let mut host = if resume {
        let file = File::open(source).map_err(|error| error.to_string())?;
        let state: CoreHostSnapshot =
            serde_json::from_reader(file).map_err(|error| error.to_string())?;
        CoreHost::resume(state)?
    } else {
        game_economy_cli::core_scenario(source, &root.join("content"))?
    };
    let initial = host.snapshot();
    let tick_millis = initial.core.physical.tick_millis;
    let step_millis = requested_step_millis.unwrap_or(tick_millis);
    if step_millis == 0 || step_millis > tick_millis {
        return Err(
            "step duration must be positive and no longer than the physical quantum".into(),
        );
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| error.to_string())?;
    let mut writer = BufWriter::new(file);
    let scenario_text = (!resume)
        .then(|| std::fs::read_to_string(source))
        .transpose()
        .map_err(|error| error.to_string())?;
    write_line(
        &mut writer,
        &json!({"kind":"metadata","source":source,"resume":resume,"steps_requested":ticks,"ticks_requested":requested_step_millis.is_none().then_some(ticks),"tick_millis":tick_millis,"step_millis":step_millis,"start_tick":initial.core.physical.custody.tick,"git_revision":revision,"tracked_source_modified":modified,"scenario":scenario_text,"note":"exploratory diagnostic; physical owners may still settle per quantum; counts do not establish causation; executable build provenance is not verified"}),
    )?;
    host.take_opinion_events();
    let mut counts = BTreeMap::<&'static str, u64>::new();
    let mut choice_counts = BTreeMap::<String, u64>::new();
    let initial_view = host.history_view();
    let mut seen_family = (!initial_view.family_events.is_empty()).then_some(initial_view.tick);
    let mut seen_animal_deaths =
        (!initial_view.animal_deaths.is_empty()).then_some(initial_view.tick);
    let mut seen_language = (!initial_view.language_events.is_empty()).then_some(initial_view.tick);
    let mut seen_signal = (!initial_view.signal_events.is_empty()).then_some(initial_view.tick);
    let mut seen_recipe = (!initial_view.recipe_learning.is_empty()).then_some(initial_view.tick);
    for step in 1..=ticks {
        let result = match host.advance_continuous(step_millis, &[]) {
            Ok(result) => result,
            Err(error) => {
                write_line(
                    &mut writer,
                    &json!({"kind":"failure","step":step,"message":error}),
                )?;
                return Err(error);
            }
        };
        if let Some(error) = result.failure {
            write_line(
                &mut writer,
                &json!({"kind":"failure","step":step,"message":error}),
            )?;
            return Err(error);
        }
        if result.elapsed_millis != step_millis || result.boundaries > 1 {
            return Err("unexpected elapsed time or boundary count".into());
        }
        for event in &result.events {
            let (kind, count) = match event {
                CoreRunEvent::Meals(rows) => ("meals", rows.len()),
                CoreRunEvent::Provisions(rows) => ("provision_attempts", rows.len()),
                CoreRunEvent::Starts(rows) => {
                    ("work_started", rows.iter().filter(|row| row.1).count())
                }
                CoreRunEvent::Work(row) => ("work_finished", row.finished.len()),
                CoreRunEvent::Meetings(rows) => ("meetings", rows.len()),
                CoreRunEvent::LocalWalking { at_millis, events } => {
                    write_line(
                        &mut writer,
                        &json!({"kind":"local_walking","step":step,"at_millis":at_millis,"events":events}),
                    )?;
                    ("local_walking", events.len())
                }
                CoreRunEvent::Confinement { at_millis, events } => {
                    write_line(
                        &mut writer,
                        &json!({"kind":"confinement","step":step,"at_millis":at_millis,"events":events}),
                    )?;
                    ("confinement", events.len())
                }
                CoreRunEvent::Disease(rows) => {
                    write_line(
                        &mut writer,
                        &json!({"kind":"disease","step":step,"events":rows}),
                    )?;
                    ("disease_events", rows.len())
                }
                CoreRunEvent::Incapacitations { at_millis, people } => {
                    write_line(
                        &mut writer,
                        &json!({"kind":"incapacitations","step":step,"at_millis":at_millis,"people":people}),
                    )?;
                    ("incapacitations", people.len())
                }
                CoreRunEvent::FamilyTransitions { at_millis, events } => {
                    write_line(
                        &mut writer,
                        &json!({"kind":"elapsed_family_events","step":step,"at_millis":at_millis,"events":events}),
                    )?;
                    ("family_events", events.len())
                }
                CoreRunEvent::PersonalChoices(rows) => {
                    for row in rows {
                        let domain = row.key.map_or(0, |key| key.domain);
                        *choice_counts
                            .entry(format!("{domain}:{:?}", row.outcome))
                            .or_default() += 1;
                    }
                    ("personal_choices", rows.len())
                }
                CoreRunEvent::MechanicalExperiments(rows) => ("experiments", rows.len()),
                CoreRunEvent::Repairs(rows) => ("site_repairs", rows.len()),
                CoreRunEvent::ToolRepairs(rows) => ("tool_repairs", rows.len()),
                CoreRunEvent::Boundary(row) => {
                    *counts.entry("spoilage_events").or_default() += row.spoiled.len() as u64;
                    *counts.entry("physical_actions_accepted").or_default() +=
                        row.physical_actions
                            .iter()
                            .filter(|action| action.accepted)
                            .count() as u64;
                    *counts.entry("physical_actions_denied").or_default() +=
                        row.physical_actions
                            .iter()
                            .filter(|action| !action.accepted)
                            .count() as u64;
                    ("work_finished", row.finished.len())
                }
                _ => continue,
            };
            *counts.entry(kind).or_default() += count as u64;
        }
        let view = host.history_view();
        if !view.animal_deaths.is_empty() && seen_animal_deaths != Some(view.tick) {
            *counts.entry("animal_deaths").or_default() += view.animal_deaths.len() as u64;
            write_line(
                &mut writer,
                &json!({"kind":"animal_deaths","step":step,"events":view.animal_deaths}),
            )?;
            seen_animal_deaths = Some(view.tick);
        }
        let recipe_count = view
            .recipe_learning
            .iter()
            .filter(|event| event.tick == view.tick)
            .count() as u64;
        if recipe_count > 0 && seen_recipe != Some(view.tick) {
            *counts.entry("recipe_learning").or_default() += recipe_count;
            seen_recipe = Some(view.tick);
        }
        let family_count = view
            .family_events
            .iter()
            .filter(|event| event.tick == view.tick)
            .count() as u64;
        if family_count > 0 && seen_family != Some(view.tick) {
            *counts.entry("family_events").or_default() += family_count;
            seen_family = Some(view.tick);
        }
        let language_count = view
            .language_events
            .iter()
            .filter(|event| event.tick == view.tick)
            .count() as u64;
        if language_count > 0 && seen_language != Some(view.tick) {
            *counts.entry("language_events").or_default() += language_count;
            seen_language = Some(view.tick);
        }
        let signal_count = view
            .signal_events
            .iter()
            .filter(|event| event.tick == view.tick)
            .count() as u64;
        if signal_count > 0 && seen_signal != Some(view.tick) {
            *counts.entry("signal_events").or_default() += signal_count;
            seen_signal = Some(view.tick);
        }
        *counts.entry("opinion_changes").or_default() += host.take_opinion_events().len() as u64;
        if step % interval == 0 || step == ticks {
            let state = host.snapshot();
            let living: Vec<_> = state
                .core
                .people
                .iter()
                .filter(|person| person.health > 0)
                .collect();
            let hungry = living
                .iter()
                .filter(|person| person.hunger >= state.core.rules.starvation_threshold)
                .count();
            let max_hunger = living.iter().map(|person| person.hunger).max();
            let mean_hunger = (!living.is_empty()).then(|| {
                living
                    .iter()
                    .map(|person| f64::from(person.hunger))
                    .sum::<f64>()
                    / living.len() as f64
            });
            let mut most_hungry: Vec<_> = living
                .iter()
                .map(|person| (person.hunger, person.id, person.health, person.region))
                .collect();
            most_hungry.sort_unstable_by_key(|row| (std::cmp::Reverse(row.0), row.1));
            most_hungry.truncate(5);
            let animals = state.fauna.as_ref().map_or(0, |fauna| {
                fauna
                    .animals
                    .iter()
                    .filter(|animal| animal.health > 0)
                    .count()
            });
            let food_stocks: Vec<_> = state
                .core
                .physical
                .custody
                .containers
                .iter()
                .filter_map(|container| {
                    let game_lots::Place::Stock { region, good } = container.place else {
                        return None;
                    };
                    let definition = state
                        .core
                        .physical
                        .goods
                        .iter()
                        .find(|row| row.id == good && row.food.is_some())?;
                    let quantity = container
                        .lots
                        .iter()
                        .map(|lot| lot.quantity as u128)
                        .sum::<u128>();
                    (quantity > 0).then(
                        || json!({"region":region,"good":definition.name,"quantity":quantity}),
                    )
                })
                .collect();
            let food_goods: std::collections::HashMap<_, _> = state
                .core
                .physical
                .goods
                .iter()
                .filter(|good| good.food.is_some())
                .map(|good| (good.id, good.name.as_str()))
                .collect();
            let mut food_custody = BTreeMap::<(&str, &str), u128>::new();
            for container in &state.core.physical.custody.containers {
                let kind = match container.place {
                    game_lots::Place::Person(_) => "people",
                    game_lots::Place::Stock { .. } => "regional_stock",
                    game_lots::Place::Inventory(_) => "stores",
                    game_lots::Place::Site(_) => "sites",
                    game_lots::Place::JobInput(_) | game_lots::Place::JobOutput(_) => "work",
                };
                for lot in &container.lots {
                    if let Some(name) = food_goods.get(&lot.good) {
                        *food_custody.entry((kind, *name)).or_default() += u128::from(lot.quantity);
                    }
                }
            }
            let food_custody: Vec<_> = food_custody.into_iter().map(|((place, good), quantity)| json!({"place":place,"good":good,"quantity":quantity})).collect();
            let offset_millis = state
                .core
                .physical
                .work_interval
                .as_ref()
                .map_or(0, |interval| interval.elapsed_millis);
            let simulated_millis =
                state.core.boundary_millis.unwrap_or(
                    u128::from(state.core.physical.custody.tick) * u128::from(tick_millis),
                ) + u128::from(offset_millis);
            let checkpoint = if requested_step_millis.is_some() {
                output.with_extension(format!("time-{simulated_millis}.json"))
            } else {
                output.with_extension(format!("tick-{}.json", state.core.physical.custody.tick))
            };
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&checkpoint)
                .map_err(|error| error.to_string())?;
            let mut checkpoint_writer = BufWriter::new(file);
            serde_json::to_writer(&mut checkpoint_writer, &state)
                .map_err(|error| error.to_string())?;
            checkpoint_writer
                .flush()
                .map_err(|error| error.to_string())?;
            write_line(
                &mut writer,
                &json!({"kind":"sample","tick":state.core.physical.custody.tick,"offset_millis":offset_millis,"simulated_millis":simulated_millis,"living_people":living.len(),"max_hunger":max_hunger,"mean_hunger":mean_hunger,"most_hungry":most_hungry,"at_starvation_threshold":hungry,"living_animals":animals,"food_stocks":food_stocks,"food_custody":food_custody,"family_totals":state.family.as_ref().map(|family| &family.totals),"events":counts,"choices":choice_counts,"checkpoint":checkpoint}),
            )?;
            counts.clear();
            choice_counts.clear();
        }
    }
    write_line(
        &mut writer,
        &json!({"kind":"complete","steps":ticks,"ticks":requested_step_millis.is_none().then_some(ticks),"last_tick":host.history_view().tick}),
    )
}

fn main() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args_os().skip(1);
    let first = args.next().ok_or(
        "usage: history_probe [--resume] SOURCE OUTPUT STEPS [CHECKPOINT_INTERVAL] [WORKERS] [STEP_MILLIS]",
    )?;
    let resume = first == "--resume";
    let source = if resume {
        PathBuf::from(args.next().ok_or("missing checkpoint")?)
    } else {
        PathBuf::from(first)
    };
    let output = PathBuf::from(args.next().ok_or("missing output")?);
    let parse = |value: std::ffi::OsString| {
        value
            .to_string_lossy()
            .parse::<u64>()
            .map_err(|error| error.to_string())
    };
    let ticks = parse(args.next().ok_or("missing ticks")?)?;
    let interval = args.next().map(parse).transpose()?.unwrap_or(24);
    let workers = args.next().map(parse).transpose()?.unwrap_or(1);
    let step_millis = args.next().map(parse).transpose()?;
    if ticks == 0 || interval == 0 || workers == 0 || args.next().is_some() {
        return Err("ticks, interval and workers must be positive, with no extra arguments".into());
    }
    let workers = usize::try_from(workers).map_err(|_| "worker count is too large")?;
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|error| error.to_string())?
        .install(|| {
            run_steps(
                &root,
                &source,
                &output,
                ticks,
                interval,
                resume,
                step_millis,
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minute_observation_preserves_world_and_counts_across_resume() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = root.join("content/scenarios/economy-history-integrated-hourly.toml");
        let token = format!(
            "legacy-minute-probe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let hourly = std::env::temp_dir().join(format!("{token}-hourly.ndjson"));
        let minute = std::env::temp_dir().join(format!("{token}-minute.ndjson"));
        let resumed = std::env::temp_dir().join(format!("{token}-resumed.ndjson"));
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| run_steps(&root, &source, &hourly, 2, 2, false, None))
            .unwrap();
        rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap()
            .install(|| run_steps(&root, &source, &minute, 120, 30, false, Some(60_000)))
            .unwrap();
        let start = minute.with_extension("time-1800000.json");
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| run_steps(&root, &start, &resumed, 90, 90, true, Some(60_000)))
            .unwrap();
        let read = |path: PathBuf| -> CoreHostSnapshot {
            serde_json::from_reader(File::open(path).unwrap()).unwrap()
        };
        let expected = read(hourly.with_extension("tick-2.json"));
        assert_eq!(read(minute.with_extension("time-7200000.json")), expected);
        assert_eq!(read(resumed.with_extension("time-7200000.json")), expected);
        let counts = |path: &Path, field: &str| -> u64 {
            std::fs::read_to_string(path)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                .filter(|row| row["kind"] == "sample")
                .map(|row| row["events"][field].as_u64().unwrap_or(0))
                .sum()
        };
        for field in [
            "meetings",
            "language_events",
            "signal_events",
            "recipe_learning",
            "work_started",
            "work_finished",
        ] {
            assert_eq!(counts(&hourly, field), counts(&minute, field), "{field}");
        }
        for path in [
            hourly.clone(),
            minute.clone(),
            resumed.clone(),
            hourly.with_extension("tick-2.json"),
            minute.with_extension("time-1800000.json"),
            minute.with_extension("time-3600000.json"),
            minute.with_extension("time-5400000.json"),
            minute.with_extension("time-7200000.json"),
            resumed.with_extension("time-7200000.json"),
        ] {
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn probe_counts_work_completed_inside_an_hour() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = root.join("content/scenarios/economy-history-integrated-hourly.toml");
        let token = format!(
            "legacy-work-probe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let output = std::env::temp_dir().join(format!("{token}.ndjson"));
        run(&root, &source, &output, 96, 96, false).unwrap();
        let sample: serde_json::Value = std::fs::read_to_string(&output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .find(|row| row["kind"] == "sample")
            .unwrap();
        assert!(sample["events"]["work_finished"].as_u64().unwrap() > 3);
        assert!(sample["events"]["recipe_learning"].as_u64().unwrap() > 0);
        std::fs::remove_file(output.with_extension("tick-96.json")).unwrap();
        std::fs::remove_file(output).unwrap();
    }

    #[test]
    fn short_probe_checkpoints_and_resumes_without_changing_world_state() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = root.join("content/scenarios/economy-history-pilot.toml");
        let token = format!(
            "legacy-probe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let first = std::env::temp_dir().join(format!("{token}-first.ndjson"));
        let resumed = std::env::temp_dir().join(format!("{token}-resumed.ndjson"));
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| run(&root, &source, &first, 2, 1, false))
            .unwrap();
        let checkpoint = first.with_extension("tick-1.json");
        rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap()
            .install(|| run(&root, &checkpoint, &resumed, 1, 1, true))
            .unwrap();
        let original: CoreHostSnapshot =
            serde_json::from_reader(File::open(first.with_extension("tick-2.json")).unwrap())
                .unwrap();
        let replayed: CoreHostSnapshot =
            serde_json::from_reader(File::open(resumed.with_extension("tick-2.json")).unwrap())
                .unwrap();
        assert_eq!(original, replayed);
        let rows: Vec<serde_json::Value> = std::fs::read_to_string(&first)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.iter().filter(|row| row["kind"] == "sample").count(), 2);
        assert_eq!(rows.last().unwrap()["kind"], "complete");
        for path in [
            first.clone(),
            resumed.clone(),
            checkpoint,
            first.with_extension("tick-2.json"),
            resumed.with_extension("tick-2.json"),
        ] {
            std::fs::remove_file(path).unwrap();
        }
    }
}
