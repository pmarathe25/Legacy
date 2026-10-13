use serde_json::Value;
use std::{collections::BTreeSet, process::Command};

fn workspace_packages() -> Vec<Value> {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run Cargo metadata");
    assert!(
        output.status.success(),
        "Cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<Value>(&output.stdout).expect("parse Cargo metadata")["packages"]
        .as_array()
        .expect("metadata packages")
        .clone()
}

fn name(package: &Value) -> &str {
    package["name"].as_str().expect("package name")
}

fn dependencies(package: &Value) -> impl Iterator<Item = (&str, Option<&str>)> {
    package["dependencies"]
        .as_array()
        .expect("package dependencies")
        .iter()
        .map(|dependency| {
            (
                dependency["name"].as_str().expect("dependency name"),
                dependency["kind"].as_str(),
            )
        })
}

fn production_dependencies(package: &Value) -> impl Iterator<Item = &str> {
    dependencies(package)
        .filter(|(_, kind)| *kind != Some("dev"))
        .map(|(dependency, _)| dependency)
}

#[test]
fn feature_owners_never_depend_on_the_playable_assembly() {
    let packages = workspace_packages();
    for package in &packages {
        let package_name = name(package);
        // The CLI is the executable composition root. Other application and
        // domain crates must not import the playable assembly, in any target.
        if package_name == "game-cli" || package_name == "game-assembly" {
            continue;
        }
        for (dependency, _) in dependencies(package) {
            assert_ne!(
                dependency, "game-assembly",
                "{package_name} must not depend on game-assembly, including in dev dependencies"
            );
        }
    }
}

#[test]
fn generic_host_has_only_generic_workspace_production_dependencies() {
    let packages = workspace_packages();
    let host = packages
        .iter()
        .find(|package| name(package) == "game-world")
        .expect("game-world workspace package");
    let workspace_dependencies = production_dependencies(host)
        .filter(|dependency| dependency.starts_with("game-"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        workspace_dependencies,
        BTreeSet::from(["game-core", "game-ecs", "game-sim"]),
        "game-world may depend only on generic workspace infrastructure"
    );
    for dependency in production_dependencies(host) {
        assert!(
            !dependency.starts_with("game-")
                || ["game-core", "game-ecs", "game-sim"].contains(&dependency),
            "game-world cannot import feature crate {dependency}"
        );
    }
}

#[test]
fn ecs_engine_is_a_production_dependency_only_of_game_ecs() {
    let packages = workspace_packages();
    let mut engine_dependencies = BTreeSet::new();
    for package in &packages {
        for dependency in production_dependencies(package) {
            if ["bevy_ecs", "legion", "hecs"].contains(&dependency) {
                engine_dependencies.insert((name(package), dependency));
            }
        }
    }
    assert_eq!(
        engine_dependencies,
        BTreeSet::from([("game-ecs", "bevy_ecs")]),
        "the selected ECS engine must remain a production dependency private to game-ecs"
    );
}
