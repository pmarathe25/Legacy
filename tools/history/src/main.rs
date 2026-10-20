mod distress;
mod journal;
mod plants;
use journal::Ledger;
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
mod snapshots;
use game_core::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
    time::Instant,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut seed = 1;
    let mut days = 360;
    let mut workers = 1;
    let mut output = PathBuf::from("target/history/run");
    let mut content = PathBuf::from("content");
    let mut overlays = vec![];
    let mut reload = 0;
    let mut generations = 0u32;
    let mut sample = 1u64;
    let mut checkpoint = 30u64;
    let mut resume: Option<PathBuf> = None;
    let mut notable = true;
    let mut final_save = true;
    let mut record_distress = false;
    let mut continue_after_extinction = false;
    let mut advance_budget = 1;
    let mut stop_file: Option<PathBuf> = None;
    let args: Vec<_> = std::env::args().skip(1).collect();
    for pair in args.chunks(2) {
        let value = pair.get(1).ok_or("every option needs a value")?;
        match pair[0].as_str() {
            "--seed" => seed = value.parse()?,
            "--days" => days = value.parse()?,
            "--workers" => workers = value.parse()?,
            "--output" => output = value.into(),
            "--content" => content = value.into(),
            "--mod" => overlays.push(PathBuf::from(value)),
            "--reload-day" => reload = value.parse()?,
            "--generations" => generations = value.parse()?,
            "--sample-days" => sample = value.parse()?,
            "--checkpoint-days" => checkpoint = value.parse()?,
            "--resume" => resume = Some(PathBuf::from(value)),
            "--final-save" => final_save = value.parse()?,
            "--distress" => record_distress = value.parse()?,
            "--continue-after-extinction" => continue_after_extinction = value.parse()?,
            "--journal" => {
                notable = match value.as_str() {
                    "full" => false,
                    "notable" => true,
                    _ => return Err("journal must be full or notable".into()),
                }
            }
            "--advance-budget" => advance_budget = value.parse()?,
            "--stop-file" => stop_file = Some(value.into()),
            _ => return Err(format!("unknown option {}", pair[0]).into()),
        }
    }
    if days == 0 || workers == 0 || sample == 0 || checkpoint == 0 || advance_budget == 0 {
        return Err("durations, workers, and advance budget must be positive".into());
    }
    std::fs::create_dir_all(&output)?;
    let mut store = game_content::Store::new(content);
    for overlay in overlays {
        store.overlay(overlay)?;
    }
    let loading = game_diagnostics::span("history load");
    let (mut sim, mut ledger, first_day) = if let Some(folder) = &resume {
        let mut saved = BufReader::new(File::open(folder.join("checkpoint.json"))?);
        let ledger: Ledger = serde_json::from_reader(BufReader::new(File::open(
            folder.join("checkpoint-ledger.json"),
        )?))?;
        let sim = game_assembly::Simulation::load_from(&mut saved, workers)?;
        if sim.debug_host().notices().last().map_or(0, |n| n.sequence) != ledger.sequence {
            return Err("world and journal checkpoint do not match".into());
        }
        seed = sim
            .debug_host()
            .get::<game_openings::Opening>()?
            .generated
            .as_ref()
            .ok_or("resume needs a generated world")?
            .seed;
        let first = sim
            .debug_host()
            .now()
            .0
            .checked_sub(ledger.epoch)
            .ok_or("invalid journal epoch")?
            / DAY.0
            + 1;
        (sim, ledger, first)
    } else {
        let sim = game_assembly::Simulation::uncontrolled(seed, workers, &mut store)?;
        let epoch = sim.debug_host().now().0;
        // Construction is the founding snapshot, not live history. Its bounded
        // notice ring may already have discarded early setup transactions.
        let plants = plants::Plants::founding(sim.debug_host())?;
        let founding_sequence = sim.debug_host().notices().last().map_or(0, |n| n.sequence);
        (
            sim,
            Ledger {
                epoch,
                sequence: founding_sequence,
                founding_sequence,
                plants,
                ..Default::default()
            },
            1,
        )
    };
    drop(loading);
    if first_day > days {
        return Err("checkpoint is later than the requested end day".into());
    }
    ledger.notable = notable;
    let epoch = ledger.epoch;
    let mut journal = BufWriter::new(File::create(output.join("events.jsonl"))?);
    let mut daily = BufWriter::new(File::create(output.join("daily.jsonl"))?);
    let mut distress = if record_distress {
        Some((
            distress::Watch::default(),
            BufWriter::new(File::create(output.join("distress.jsonl"))?),
        ))
    } else {
        None
    };
    let mut completed = first_day - 1;
    let mut checkpointed = None;
    let mut reached = false;
    let mut extinct = false;
    let mut stopped = false;
    let mut last_generation = sim.debug_host().get::<game_heredity::Heredity>()?.maximum;
    let started = Instant::now();
    let mut initial = snapshots::snapshot(sim.debug_host(), &ledger)?;
    if resume.is_none() {
        initial["initial_people"] = serde_json::to_value(snapshots::people(sim.debug_host())?)?;
    }
    if notable {
        initial["initial_plant_samples"]=json!(sim.debug_host().get::<game_botany::Botany>()?.stands.iter().map(|(id,s)|json!({"patch":id,"plants":s.plants.iter().take(4).collect::<Vec<_>>()})).collect::<Vec<_>>());
    } else {
        initial["initial_plants"] =
            serde_json::to_value(&sim.debug_host().get::<game_botany::Botany>()?.stands)?;
        initial["initial_seed_stocks"] =
            serde_json::to_value(&sim.debug_host().get::<game_cultivars::Cultivars>()?.stocks)?;
    }
    write_json(output.join("initial.json"), &initial, true)?;
    drop(initial);
    for day in first_day..=days {
        let target = SimTime(
            epoch
                .checked_add(DAY.0.checked_mul(day).ok_or("duration overflow")?)
                .ok_or("time overflow")?,
        );
        loop {
            let progress = match advance_commits(
                &mut sim,
                target,
                advance_budget,
                &mut ledger,
                &mut journal,
            ) {
                Ok(progress) => progress,
                Err(error) => {
                    journal.flush()?;
                    save_checkpoint(&output, &sim, &ledger)?;
                    write_json(
                        output.join("execution-error.json"),
                        &json!({"error":error.to_string(),"day":sim.now().0.saturating_sub(epoch) as f64 / DAY.0 as f64,"sequence":ledger.sequence}),
                        true,
                    )?;
                    return Err(error);
                }
            };
            match progress {
                game_sim::Stop::Budget => {}
                game_sim::Stop::Target => break,
                game_sim::Stop::Choice(s) => {
                    return Err(format!("unexpected input stop: {s}").into());
                }
            }
        }
        if let Some((watch, output)) = &mut distress {
            watch.collect(sim.debug_host(), epoch, output)?;
            output.flush()?;
        }
        completed = day;
        let ancestry = sim.debug_host().get::<game_heredity::Heredity>()?;
        let max_generation = ancestry.maximum;
        reached = generations > 0 && max_generation >= generations;
        extinct = ancestry.living == 0;
        stopped = stop_file.as_ref().is_some_and(|p| p.exists());
        if max_generation > last_generation {
            eprintln!("seed {seed}: depth {max_generation} first reached on day {day}");
            last_generation = max_generation;
        }
        if day % sample == 0
            || day == days
            || reached
            || extinct && !continue_after_extinction
            || stopped
        {
            let mut row = snapshots::snapshot(sim.debug_host(), &ledger)?;
            row["day"] = json!(day);
            serde_json::to_writer(&mut daily, &row)?;
            writeln!(daily)?;
            daily.flush()?;
            eprintln!(
                "seed {seed}: day {day}, generation {max_generation}, alive {}, births {}, {:.1}s",
                row["alive"],
                row["births"],
                started.elapsed().as_secs_f64()
            );
        }
        journal.flush()?;
        if day % checkpoint == 0
            && !reached
            && (!extinct || continue_after_extinction)
            && !stopped
            && day != days
        {
            save_checkpoint(&output, &sim, &ledger)?;
            checkpointed = Some(day);
        }
        if day == reload {
            let save = sim.save()?;
            sim = game_assembly::Simulation::load(&save, if workers == 1 { 4 } else { 1 })?;
            if sim.save() != Ok(save) {
                return Err("save/load changed the world".into());
            }
        }
        if reached || extinct && !continue_after_extinction || stopped {
            break;
        }
    }
    if checkpointed != Some(completed) {
        save_checkpoint(&output, &sim, &ledger)?;
    }
    if final_save {
        std::fs::copy(
            output.join("checkpoint.json"),
            output.join("final-save.json"),
        )?;
    }
    write_json(
        output.join("people.json"),
        &snapshots::people(sim.debug_host())?,
        true,
    )?;
    write_json(
        output.join("summary.json"),
        &json!({"seed":seed,"days":completed,"requested_generations":generations,"journal":if notable{"notable"}else{"full"},"extinct":extinct,"stopped":stopped,"resumed_from":resume,"reached_generations":reached,"epoch":epoch,"workers":workers,"advance_budget":advance_budget,"reload_day":reload,"elapsed_seconds":started.elapsed().as_secs_f64(),"profile":game_diagnostics::snapshot(),"failures":game_diagnostics::failures(),"final":snapshots::snapshot(sim.debug_host(),&ledger)?}),
        true,
    )?;
    Ok(())
}
// A batch is a responsiveness bound, not an event-retention assumption. A single
// ecological transaction can exceed the recent-notice ring's ordinary capacity.
fn advance_commits(
    sim: &mut game_assembly::Simulation,
    target: SimTime,
    budget: usize,
    ledger: &mut Ledger,
    journal: &mut impl Write,
) -> Result<game_sim::Stop> {
    for _ in 0..budget {
        let advancing = game_diagnostics::span("history advance");
        let p = sim.advance(target, 1)?;
        drop(advancing);
        let collecting = game_diagnostics::span("history collect");
        ledger.collect(sim.debug_host(), journal)?;
        drop(collecting);
        if p.stop != game_sim::Stop::Budget {
            return Ok(p.stop);
        }
    }
    Ok(game_sim::Stop::Budget)
}
fn save_checkpoint(
    output: &std::path::Path,
    sim: &game_assembly::Simulation,
    ledger: &Ledger,
) -> Result<()> {
    let _measurement = game_diagnostics::span("history checkpoint");
    let mut world = BufWriter::new(File::create(output.join("checkpoint-world.tmp"))?);
    sim.save_to(&mut world)?;
    world.flush()?;
    drop(world);
    write_json(output.join("checkpoint-ledger.tmp"), ledger, false)?;
    std::fs::rename(
        output.join("checkpoint-world.tmp"),
        output.join("checkpoint.json"),
    )?;
    std::fs::rename(
        output.join("checkpoint-ledger.tmp"),
        output.join("checkpoint-ledger.json"),
    )?;
    Ok(())
}

fn write_json(
    path: impl AsRef<std::path::Path>,
    value: &impl serde::Serialize,
    pretty: bool,
) -> Result<()> {
    let _measurement = game_diagnostics::span("history json export");
    let mut out = BufWriter::new(File::create(path)?);
    if pretty {
        serde_json::to_writer_pretty(&mut out, value)?;
    } else {
        serde_json::to_writer(&mut out, value)?;
    }
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_sim::*;
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    struct Publish {
        fail: bool,
    }
    impl Request for Publish {
        const KEY: &'static str = "test.publish";
        type Response = ();
    }
    #[test]
    fn notable_plant_records_keep_observed_genomes_without_every_fruit_family() {
        // Keep every habitat; this test exercises plant records rather than population scale.
        let root =
            std::env::temp_dir().join(format!("legacy-journal-flora-{}", std::process::id()));
        std::fs::create_dir_all(root.join("worldgen")).unwrap();
        let content = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let settings = std::fs::read_to_string(content.join("worldgen/default.toml"))
            .unwrap()
            .replace("population = 96", "population = 24");
        std::fs::write(root.join("worldgen/default.toml"), settings).unwrap();
        let mut store = game_content::Store::standard();
        store.overlay(&root).unwrap();
        let mut sim = game_assembly::Simulation::uncontrolled(3, 1, &mut store).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        let mut ledger = Ledger {
            epoch: sim.now().0,
            sequence: sim.debug_host().notices().last().map_or(0, |n| n.sequence),
            plants: plants::Plants::founding(sim.debug_host()).unwrap(),
            notable: true,
            ..Default::default()
        };
        let mut out = vec![];
        let target = sim.now().after(SimDuration(30 * DAY.0)).unwrap();
        let mut previous = sim.now();
        let mut stagnant = 0;
        let mut checked = 0;
        while advance_commits(&mut sim, target, 64, &mut ledger, &mut out).unwrap()
            == game_sim::Stop::Budget
        {
            // This checks journal enrichment, not thirty days of unrelated decisions.
            // Keep the original deadline, but finish once the required evidence exists.
            let observed = std::str::from_utf8(&out[checked..])
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .any(|r| {
                    r["kind"] == "botany.changed"
                        && r["data"]["observed_plants"]
                            .as_array()
                            .is_some_and(|p| !p.is_empty())
                });
            checked = out.len();
            if observed
                && ledger
                    .counts
                    .get("cultivars.created")
                    .is_some_and(|n| *n > 0)
            {
                break;
            }
            stagnant = if sim.now() == previous {
                stagnant + 1
            } else {
                0
            };
            if stagnant >= 1000 {
                let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/history/test-failures/notable-plants");
                std::fs::create_dir_all(&root).unwrap();
                std::fs::write(root.join("world.json"), sim.save().unwrap()).unwrap();
                panic!(
                    "history made 64,000 commits without advancing from {:?}; state: {}",
                    sim.now(),
                    root.display()
                );
            }
            previous = sim.now();
        }
        let rows: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|r| serde_json::from_str(r).unwrap())
            .collect();
        assert!(ledger.counts["cultivars.created"] > 0);
        assert!(!rows.iter().any(|r| r["kind"] == "cultivars.created"));
        assert!(
            rows.iter()
                .filter(|r| r["kind"] == "botany.changed")
                .any(|r| r["data"]["observed_plants"]
                    .as_array()
                    .is_some_and(|p| !p.is_empty()))
        );
    }
    #[test]
    fn journal_counts_only_committed_events_and_does_not_repeat_them() {
        let mut b = Builder::new();
        game_objects::plugin(&mut b).unwrap();
        b.plugin("test", 1).unwrap();
        b.request::<Publish>(|tx, r| {
            tx.emit(
                game_workshop::Produced {
                    person: PersonId(1),
                    inventory: InventoryId(2),
                    good: GoodId(4),
                    quantity: 2,
                    skill: "healing".into(),
                },
                vec![],
            )?;
            if r.fail {
                Err(game_core::Error::Rejected("rollback".into()))
            } else {
                Ok(())
            }
        })
        .unwrap();
        let mut h = b.build(1, 1).unwrap();
        h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
            .unwrap();
        let mut ledger = Ledger::default();
        let mut out = vec![];
        assert!(h.dispatch(Publish { fail: true }).is_err());
        ledger.collect(&h, &mut out).unwrap();
        assert!(out.is_empty());
        h.dispatch(Publish { fail: false }).unwrap();
        ledger.collect(&h, &mut out).unwrap();
        ledger.collect(&h, &mut out).unwrap();
        assert_eq!(ledger.counts["workshop.produced"], 1);
        assert_eq!(ledger.crafted["Medicine"], 2);
        assert_eq!(String::from_utf8(out).unwrap().lines().count(), 1);
    }
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    struct CareNotice;
    impl Request for CareNotice {
        const KEY: &'static str = "test.care_notice";
        type Response = ();
    }
    #[test]
    fn condensed_journal_keeps_every_count_and_attaches_the_latest_care_to_deaths() {
        let mut b = Builder::new();
        game_objects::plugin(&mut b).unwrap();
        b.plugin("test", 1).unwrap();
        b.request::<CareNotice>(|tx, _| {
            for quantity in [1, 2] {
                if quantity == 2 {
                    tx.emit(
                        game_physiology::Died {
                            person: PersonId(2),
                            at: tx.now(),
                            body: None,
                            cause: None,
                        },
                        vec![],
                    )?;
                }
                tx.emit(
                    game_physiology::Fed {
                        inventory: None,
                        caregiver: PersonId(1),
                        patient: PersonId(2),
                        good: GoodId(4),
                        quantity,
                    },
                    vec![],
                )?;
            }
            Ok(())
        })
        .unwrap();
        let mut h = b.build(1, 1).unwrap();
        h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
            .unwrap();
        let mut ledger = Ledger {
            notable: true,
            ..Default::default()
        };
        let mut out = vec![];
        h.dispatch(CareNotice).unwrap();
        ledger.collect(&h, &mut out).unwrap();
        let rows: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(ledger.counts["physiology.fed"], 2);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["kind"], "physiology.died");
        assert_eq!(rows[1]["last_feed"]["count"], 2);
        assert_eq!(rows[1]["last_feed"]["quantity"], 2);
        assert_eq!(rows[1]["ingestion_reported_after_death_notice"], true);
        // A checkpoint must retain the condensation and care context.
        let loaded: Ledger = serde_json::from_slice(&serde_json::to_vec(&ledger).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&loaded).unwrap()["feeding"]["2"]["quantity"],
            2
        );
        assert_eq!(loaded.seen, ledger.seen);
        // Repeated care does not produce another routine record in later years,
        // while every death and the current meal context remain in the archive.
        h.advance(SimTime(2 * 365 * DAY.0), 1).unwrap();
        h.dispatch(CareNotice).unwrap();
        let mut later = vec![];
        ledger.collect(&h, &mut later).unwrap();
        let rows: Vec<Value> = String::from_utf8(later)
            .unwrap()
            .lines()
            .map(|r| serde_json::from_str(r).unwrap())
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["kind"], "physiology.died");
        assert_eq!(rows[0]["last_feed"]["count"], 4);
        assert_eq!(ledger.counts["physiology.died"], 2);
    }
}
