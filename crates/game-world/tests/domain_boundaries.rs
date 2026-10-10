use std::process::Command;

#[test]
fn domain_kernels_cannot_depend_on_the_coordinator_or_client_facade() {
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
        .unwrap();
    assert!(output.status.success());
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut domains = 0;
    for package in metadata["packages"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap();
        if name == "game-session" {
            for dependency in package["dependencies"].as_array().unwrap() {
                assert_ne!(
                    dependency["name"].as_str().unwrap(),
                    "game-world",
                    "the host must compile without the simulation implementation"
                );
            }
        }
        if name == "game-sim" {
            for dependency in package["dependencies"].as_array().unwrap() {
                if dependency["kind"].as_str() == Some("dev") {
                    continue;
                }
                let dependency = dependency["name"].as_str().unwrap();
                assert!(
                    !dependency.starts_with("game-") || dependency == "game-core",
                    "the engine must be independent of game features: {dependency}"
                );
            }
        }
        if !name.starts_with("game-")
            || [
                "game-api",
                "game-sim",
                "game-world",
                "game-session",
                "game-cli",
            ]
            .contains(&name)
        {
            continue;
        }
        domains += 1;
        for dependency in package["dependencies"].as_array().unwrap() {
            let dependency = dependency["name"].as_str().unwrap();
            assert!(
                !["game-api", "game-world", "game-session", "game-cli"].contains(&dependency),
                "{name} cannot compile independently with dependency {dependency}"
            );
        }
    }
    assert!(
        domains >= 5,
        "expected independently compiled domain kernels"
    );
}
