use game_history_kernel::{DAY, World};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut people = 1000;
    let mut areas = 8;
    let mut workers = 4;
    let mut days = 30u64;
    let mut seed = 1;
    let mut output = PathBuf::from("settlement-history-output");
    let mut resume = None;
    let mut import = None;
    let mut audit = None;
    while let Some(key) = args.next() {
        if key == "--help" {
            println!(
                "Experimental settlement kernel: --people N --areas N --workers N --days N --seed N --output DIR [--resume FILE | --import-legacy FILE | --audit-legacy FILE]. Days are additional days. This is not the full Legacy simulation."
            );
            return Ok(());
        }
        let value = args.next().ok_or("missing option value")?;
        match key.as_str() {
            "--people" => people = value.parse()?,
            "--areas" => areas = value.parse()?,
            "--workers" => workers = value.parse()?,
            "--days" => days = value.parse()?,
            "--seed" => seed = value.parse()?,
            "--output" => output = value.into(),
            "--resume" => resume = Some(value),
            "--import-legacy" => import = Some(value),
            "--audit-legacy" => audit = Some(value),
            _ => return Err(format!("unknown option {key}").into()),
        }
    }
    if let Some(path) = audit {
        println!(
            "{}",
            serde_json::to_string_pretty(&game_history_import::inspect_legacy(
                &std::fs::read_to_string(path)?
            )?)?
        );
        return Ok(());
    }
    if resume.is_some() && import.is_some() {
        return Err("choose resume or legacy import".into());
    }
    if !(1..=64).contains(&workers) {
        return Err("workers must be 1..64".into());
    }
    let mut world = if let Some(path) = resume {
        World::load(&std::fs::read_to_string(path)?)?
    } else if let Some(path) = import {
        game_history_import::import_legacy(&std::fs::read_to_string(path)?)?
    } else {
        World::fixture(seed, people, areas)?
    };
    let target = world
        .now
        .checked_add(days.checked_mul(DAY).ok_or("day overflow")?)
        .ok_or("time overflow")?;
    std::fs::create_dir(&output)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()?;
    let mut events = BufWriter::new(File::create(output.join("events.jsonl"))?);
    while world.now < target {
        for receipt in world.step(&pool)? {
            serde_json::to_writer(&mut events, &receipt)?;
            writeln!(events)?;
        }
    }
    events.flush()?;
    std::fs::write(output.join("checkpoint.json"), world.save()?)?;
    let summary = serde_json::json!({"model":"experimental-settlement-v1","full_legacy_history":false,"at_minutes":world.now,"requested_target_minutes":target,"workers":workers,"people":world.population().people().len(),"alive":world.population().people().iter().filter(|p|p.alive).count(),"settlements":world.economy().settlements(),"encounters":world.society().encounters(),"events":world.totals});
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;
    Ok(())
}
