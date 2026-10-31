use std::{
    fs,
    path::{Path, PathBuf},
};
fn sources(path: &Path) -> Vec<PathBuf> {
    let mut files = vec![];
    for entry in fs::read_dir(path).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            files.extend(sources(&p));
        } else if p.extension().is_some_and(|e| e == "rs") {
            files.push(p);
        }
    }
    files
}
#[test]
fn crate_size_and_dependency_direction_are_enforced() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let applications = [
        "game-api",
        "game-assembly",
        "game-openings",
        "game-session",
        "game-cli",
    ];
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let manifest = fs::read_to_string(path.join("Cargo.toml")).unwrap();
        let files = sources(&path.join("src"));
        let lines: usize = files
            .iter()
            .map(|p| fs::read_to_string(p).unwrap().lines().count())
            .sum();
        let budget = match name {
            "game-sim" => 1250,
            "game-parallel-economy" => 6000,
            "game-parallel-economy-cli" => 1800,
            _ => 1200,
        };
        assert!(lines <= budget, "{name}: {lines} exceeds {budget}");
        if !applications.contains(&name) {
            for app in applications {
                assert!(
                    !manifest.contains(&format!("{app}.")),
                    "{name} depends on {app}"
                );
            }
        }
        if name == "game-sim" || name == "game-core" || name == "game-storage" {
            for line in manifest.lines().filter(|l| l.starts_with("game-")) {
                assert!(
                    [
                        "game-core.workspace = true",
                        "game-storage.workspace = true",
                        "game-packed.workspace = true",
                        "game-snapshot.workspace = true",
                        "game-contracts.workspace = true",
                    ]
                    .contains(&line),
                    "kernel depends on a feature: {line}"
                );
            }
        }
        for file in files {
            let source = fs::read_to_string(&file).unwrap();
            assert!(
                !source.contains("KernelKind") && !source.contains("NativeKernel"),
                "duplicate execution path in {}",
                file.display()
            );
            assert!(
                !source.contains(".edit::<game_"),
                "cross-owner state mutation in {}",
                file.display()
            );
            if name == "game-assembly" {
                let file_name = file.file_name().unwrap().to_str().unwrap();
                let allowed = ["plugins.rs", "binding.rs", "lib.rs"];
                assert!(
                    allowed.contains(&file_name),
                    "assembly gained gameplay module {file_name}"
                );
                if file_name == "lib.rs" {
                    for feature in [
                        "game_physiology::",
                        "game_cargo::",
                        "game_characters::",
                        "game_conflict::",
                    ] {
                        assert!(!source.contains(feature), "feature logic outside binding");
                    }
                }
            }
        }
    }
}
