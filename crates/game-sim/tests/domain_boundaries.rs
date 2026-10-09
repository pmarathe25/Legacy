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
        if !name.starts_with("game-") || ["game-api", "game-sim", "game-cli"].contains(&name) {
            continue;
        }
        domains += 1;
        for dependency in package["dependencies"].as_array().unwrap() {
            let dependency = dependency["name"].as_str().unwrap();
            assert!(
                !["game-api", "game-sim", "game-cli"].contains(&dependency),
                "{name} cannot compile independently with dependency {dependency}"
            );
        }
    }
    assert!(
        domains >= 5,
        "expected independently compiled domain kernels"
    );
}
