use super::{Checkpoint, Metrics, Sample};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Commit {
    version: u32,
    tick: u64,
    state_bytes: u64,
    state_checksum: u64,
    sample_bytes: u64,
    sample_checksum: u64,
    event_bytes: u64,
    event_checksum: u64,
}
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, b| {
        (hash ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn durable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}
pub fn publish(
    root: &Path,
    state: &Checkpoint,
    samples: &[Sample],
    events: &[game_parallel::FamilyEvent],
    workers: usize,
    status: &str,
) -> Result<PathBuf, String> {
    state.validate()?;
    let tick = state.world.tick;
    let pending = root.join(format!(".pending-{tick:020}"));
    let committed = root.join(format!("checkpoint-{tick:020}"));
    if committed.exists() {
        return Err("checkpoint generation already exists".into());
    }
    std::fs::create_dir(&pending).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(state).map_err(|e| e.to_string())?;
    let mut observations = Vec::new();
    for sample in samples {
        serde_json::to_writer(&mut observations, sample).map_err(|e| e.to_string())?;
        observations.push(b'\n');
    }
    durable(&pending.join("state.json"), &bytes)?;
    durable(&pending.join("samples.jsonl"), &observations)?;
    let mut receipts = Vec::new();
    for event in events {
        serde_json::to_writer(&mut receipts, event).map_err(|e| e.to_string())?;
        receipts.push(b'\n');
    }
    durable(&pending.join("events.jsonl"), &receipts)?;
    let summary = serde_json::json!({"engine":"game-parallel","complete_legacy_engine":false,"status":status,"tick":tick,"calendar":state.calendar,"date":state.calendar.date(tick)?,"goal_tick":state.goal_tick,"seed":state.world.seed,"workers":workers,"metrics":Metrics::observe(&state.world),"families":state.world.families.as_ref().map(|f|&f.totals),"samples_written":state.samples_written,"events_written":state.events_written,"events_omitted":state.events_omitted,"event_semantics":"bounded actual family receipts; samples are separate diagnostic snapshots","limitations":"See examples/HISTORY.md and current engine owner documentation; this runner does not add missing mechanics."});
    durable(
        &pending.join("summary.json"),
        &serde_json::to_vec_pretty(&summary).map_err(|e| e.to_string())?,
    )?;
    let commit = Commit {
        version: 1,
        tick,
        state_bytes: bytes.len() as u64,
        state_checksum: checksum(&bytes),
        sample_bytes: observations.len() as u64,
        sample_checksum: checksum(&observations),
        event_bytes: receipts.len() as u64,
        event_checksum: checksum(&receipts),
    };
    durable(
        &pending.join("COMMITTED.json"),
        &serde_json::to_vec(&commit).map_err(|e| e.to_string())?,
    )?;
    // Publication is one directory rename. Incomplete pending directories are never resumed.
    std::fs::rename(&pending, &committed).map_err(|e| e.to_string())?;
    Ok(committed)
}
pub fn load(path: &Path) -> Result<Checkpoint, String> {
    if path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(".pending-"))
    {
        return Err("unpublished checkpoint generation".into());
    }
    let directory = if path.join("COMMITTED.json").is_file() {
        path.to_path_buf()
    } else {
        let mut generations = Vec::new();
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            let p = entry.map_err(|e| e.to_string())?.path();
            if p.is_dir()
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with("checkpoint-"))
                && p.join("COMMITTED.json").is_file()
            {
                generations.push(p);
            }
        }
        generations.sort();
        generations.pop().ok_or("no committed checkpoints")?
    };
    let commit: Commit = serde_json::from_slice(
        &std::fs::read(directory.join("COMMITTED.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let bytes = std::fs::read(directory.join("state.json")).map_err(|e| e.to_string())?;
    let samples = std::fs::read(directory.join("samples.jsonl")).map_err(|e| e.to_string())?;
    let events = std::fs::read(directory.join("events.jsonl")).map_err(|e| e.to_string())?;
    if commit.version != 1
        || bytes.len() as u64 != commit.state_bytes
        || checksum(&bytes) != commit.state_checksum
        || samples.len() as u64 != commit.sample_bytes
        || checksum(&samples) != commit.sample_checksum
        || events.len() as u64 != commit.event_bytes
        || checksum(&events) != commit.event_checksum
    {
        return Err("checkpoint integrity mismatch".into());
    }
    let state: Checkpoint = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if state.world.tick != commit.tick {
        return Err("checkpoint tick mismatch".into());
    }
    state.validate()?;
    Ok(state)
}
