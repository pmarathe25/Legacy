use std::process::Command;
#[test]
fn unattended_history_and_its_chronicle_replay_across_workers_and_loading() {
    let root = std::env::temp_dir().join(format!("legacy-history-replay-{}", std::process::id()));
    let content = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    for (folder, workers, reload) in [("continuous", "1", "0"), ("reloaded", "4", "1")] {
        let result = Command::new(env!("CARGO_BIN_EXE_game-history"))
            .args([
                "--days",
                "2",
                "--seed",
                "3",
                "--workers",
                workers,
                "--advance-budget",
                if folder == "continuous" { "64" } else { "1" },
                "--reload-day",
                reload,
                "--content",
                content.to_str().unwrap(),
                "--output",
                root.join(folder).to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for name in [
        "initial.json",
        "daily.jsonl",
        "events.jsonl",
        "people.json",
        "final-save.json",
    ] {
        assert_eq!(
            std::fs::read(root.join("continuous").join(name)).unwrap(),
            std::fs::read(root.join("reloaded").join(name)).unwrap(),
            "{name}"
        );
    }
    let save: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("continuous/final-save.json")).unwrap())
            .unwrap();
    assert_eq!(
        save["resources"]["lineage.continuity"]["current"],
        serde_json::Value::Null
    );
    assert!(save["resources"]["lineage.continuity"].is_object());
    // Only files made by this test are removed.
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn resuming_a_checkpoint_preserves_the_world_and_the_journal_without_overriding_the_seed() {
    let root = std::env::temp_dir().join(format!("legacy-history-resume-{}", std::process::id()));
    let content = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    for (name, days, resume) in [
        ("continuous", "2", None),
        ("prefix", "2", None),
        ("suffix", "2", Some("prefix")),
    ] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_game-history"));
        c.args([
            "--days",
            days,
            "--checkpoint-days",
            "1",
            "--content",
            content.to_str().unwrap(),
            "--output",
            root.join(name).to_str().unwrap(),
        ]);
        if let Some(folder) = resume {
            c.args([
                "--resume",
                root.join(folder).to_str().unwrap(),
                "--workers",
                "4",
            ]);
        } else {
            c.args(["--seed", "11"]);
        }
        if name == "prefix" {
            let flag = root.join("stop");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(&flag, "stop after the next complete day").unwrap();
            c.args(["--stop-file", flag.to_str().unwrap()]);
        }
        let result = c.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let stopped: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("prefix/summary.json")).unwrap()).unwrap();
    assert_eq!(stopped["stopped"], true);
    assert_eq!(stopped["days"], 1);
    assert_eq!(
        std::fs::read(root.join("continuous/final-save.json")).unwrap(),
        std::fs::read(root.join("suffix/final-save.json")).unwrap()
    );
    let mut combined = std::fs::read(root.join("prefix/events.jsonl")).unwrap();
    combined.extend(std::fs::read(root.join("suffix/events.jsonl")).unwrap());
    assert_eq!(
        combined,
        std::fs::read(root.join("continuous/events.jsonl")).unwrap()
    );
    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("suffix/summary.json")).unwrap()).unwrap();
    assert_eq!(summary["seed"], 11);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn optional_ecological_horizon_preserves_the_mortal_extinction_outcome() {
    let root =
        std::env::temp_dir().join(format!("legacy-history-extinction-{}", std::process::id()));
    let content = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let overlay = root.join("short-lived");
    std::fs::create_dir_all(overlay.join("worldgen")).unwrap();
    std::fs::create_dir_all(overlay.join("physiology")).unwrap();
    let world = std::fs::read_to_string(content.join("worldgen/default.toml"))
        .unwrap()
        .lines()
        .map(|line| {
            if line.starts_with("population =") {
                "population = 4"
            } else if line.starts_with("areas =") {
                "areas = 1"
            } else if line.starts_with("history =") {
                "history = 0"
            } else if line.starts_with("physiology =") {
                "physiology = \"short-test\""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(overlay.join("worldgen/default.toml"), world).unwrap();
    std::fs::write(
        overlay.join("physiology/short-test.toml"),
        r#"id = "physiology/short-test"
version = 1
[data]
lifespan = 86400000
tick = 3600000
hunger_per_tick = 0
fatigue_per_tick = 0
rest_per_tick = 4
deprivation = 120
harm_per_tick = 1
guardian_recovery = 2
max_health = 100
"#,
    )
    .unwrap();
    for (name, keep_running, expected_days) in [("stop", "false", 1), ("continue", "true", 3)] {
        let folder = root.join(name);
        let result = Command::new(env!("CARGO_BIN_EXE_game-history"))
            .args([
                "--days",
                "3",
                "--seed",
                "17",
                "--content",
                content.to_str().unwrap(),
                "--mod",
                overlay.to_str().unwrap(),
                "--continue-after-extinction",
                keep_running,
                "--output",
                folder.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let summary: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.join("summary.json")).unwrap()).unwrap();
        assert_eq!(summary["days"], expected_days);
        assert_eq!(summary["extinct"], true);
        assert_eq!(summary["final"]["mortals"], 0);
        assert_eq!(summary["final"]["births"], 0);
    }
    std::fs::remove_dir_all(root).unwrap();
}
