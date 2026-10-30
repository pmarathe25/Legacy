use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "legacy-parallel-history-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(!path.exists());
        Self(path)
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("legacy-parallel-history-")
        );
        if self.0.exists() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn state() -> Checkpoint {
    let world =
        World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap();
    let scenario = Scenario {
        version: 1,
        source: String::new(),
        seed: world.seed,
        ticks: 20,
        calendar: Calendar {
            minutes_per_tick: 60,
            days_per_year: 365,
            epoch_year: 100,
        },
        sample_every: 2,
        checkpoint_every: 4,
        max_samples: 100,
        max_events: 100,
    };
    Checkpoint::new(scenario, world, None).unwrap()
}
fn observations(root: &Path) -> Vec<Sample> {
    let mut entries: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("COMMITTED.json").exists())
        .collect();
    entries.sort();
    entries
        .into_iter()
        .flat_map(|p| {
            std::fs::read_to_string(p.join("samples.jsonl"))
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str::<Sample>(line).unwrap())
                .collect::<Vec<_>>()
        })
        .collect()
}
#[test]
fn exact_resume_and_samples_match_across_worker_counts() {
    let continuous = Temporary::new();
    let first = Temporary::new();
    let resumed = Temporary::new();
    let all = run(state(), 1, &continuous.0, |_| false).unwrap();
    let partial = run(state(), 4, &first.0, |tick| tick >= 7).unwrap();
    assert_eq!(partial.world.tick, 7);
    let loaded = checkpoint::load(&first.0).unwrap();
    assert_eq!(partial, loaded);
    let final_state = run(loaded, 2, &resumed.0, |_| false).unwrap();
    assert_eq!(all, final_state);
    let mut split = observations(&first.0);
    split.extend(observations(&resumed.0));
    assert_eq!(observations(&continuous.0), split);
}
#[test]
fn uncommitted_generations_are_ignored_and_corruption_is_rejected() {
    let dir = Temporary::new();
    run(state(), 1, &dir.0, |tick| tick >= 7).unwrap();
    let pending = dir.0.join(".pending-99999999999999999999");
    std::fs::create_dir(&pending).unwrap();
    std::fs::write(pending.join("state.json"), b"partial").unwrap();
    assert!(
        checkpoint::load(&pending)
            .unwrap_err()
            .contains("unpublished")
    );
    assert_eq!(checkpoint::load(&dir.0).unwrap().world.tick, 7);
    let committed = dir.0.join("checkpoint-00000000000000000007");
    std::fs::write(committed.join("state.json"), b"corrupted").unwrap();
    assert!(checkpoint::load(&dir.0).unwrap_err().contains("integrity"));
}
#[test]
fn samples_are_bounded_stop_is_immediate_and_existing_output_is_preserved() {
    let dir = Temporary::new();
    let mut initial = state();
    initial.max_samples = 2;
    let end = run(initial.clone(), 4, &dir.0, |_| false).unwrap();
    assert_eq!(end.samples_written, 2);
    assert_eq!(observations(&dir.0).len(), 2);
    let before = checkpoint::load(&dir.0).unwrap();
    assert!(run(initial, 1, &dir.0, |_| false).is_err());
    assert_eq!(before, checkpoint::load(&dir.0).unwrap());
    let stopped = Temporary::new();
    let start = state();
    let tick = start.world.tick;
    assert_eq!(
        run(start, 1, &stopped.0, |_| true).unwrap().world.tick,
        tick
    );
    assert_eq!(checkpoint::load(&stopped.0).unwrap().world.tick, tick);
}
#[test]
fn calendar_is_explicit_and_invalid_progress_is_rejected() {
    let mut checkpoint = state();
    assert_eq!(
        checkpoint.calendar.date(24 * 365).unwrap(),
        Date {
            year: 101,
            day_of_year: 1,
            minute_of_day: 0
        }
    );
    checkpoint.goal_tick = 0;
    checkpoint.world.tick = 1;
    assert!(checkpoint.validate().is_err());
    checkpoint.calendar.minutes_per_tick = 0;
    assert!(checkpoint.validate().is_err());
}

fn receipts(root: &Path) -> Vec<game_parallel::FamilyEvent> {
    let mut entries: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("COMMITTED.json").exists())
        .collect();
    entries.sort();
    entries
        .into_iter()
        .flat_map(|p| {
            std::fs::read_to_string(p.join("events.jsonl"))
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn family_receipts_and_complete_owner_state_resume_exactly_with_bounded_events() {
    let mut initial = state();
    initial.world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    initial.goal_tick = 60;
    initial.max_events = 17;
    let all_dir = Temporary::new();
    let first_dir = Temporary::new();
    let rest_dir = Temporary::new();
    let all = run(initial.clone(), 1, &all_dir.0, |_| false).unwrap();
    run(initial, 4, &first_dir.0, |tick| tick >= 7).unwrap();
    let restored = checkpoint::load(&first_dir.0).unwrap();
    let resumed = run(restored, 2, &rest_dir.0, |_| false).unwrap();
    assert_eq!(all, resumed);
    assert_eq!(all.events_written, 17);
    assert!(all.events_omitted > 0);
    assert!(all.world.families.as_ref().unwrap().totals.births > 0);
    let mut split = receipts(&first_dir.0);
    split.extend(receipts(&rest_dir.0));
    assert_eq!(receipts(&all_dir.0), split);
    assert_eq!(split.len(), 17);
}
