use game_api::Command;
use game_cli::*;
use game_core::*;
use std::{
    io::Write,
    process::{Command as Process, Stdio},
};
#[test]
fn scripts_use_the_same_api_as_a_direct_client() {
    let mut direct = game_assembly::Simulation::new("kernel-fixture", 1, 1).unwrap();
    for (index, command) in [
        Command::ObserveSurroundings,
        Command::Observe {
            inventory: InventoryId(19),
        },
        Command::Tell {
            recipient: PersonId(7),
            key: "roads:1".into(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        direct.submit(index as u64 + 1, command).unwrap();
    }
    direct.advance(SimTime(0), 100).unwrap();
    direct
        .submit(
            4,
            Command::Move {
                destination: AreaId(2),
            },
        )
        .unwrap();
    direct.advance(SimTime(120_000), 100).unwrap();
    direct.submit(5, Command::ObserveSurroundings).unwrap();
    let expected = serde_json::to_value(direct.observe().unwrap()).unwrap();
    let mut child = Process::new(env!("CARGO_BIN_EXE_game-cli"))
        .args(["--scenario", "kernel-fixture", "--seed", "1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"status\nlook\nobserve 19\ntell 7 roads:1\nrun 0\nmove 2\nrun 120\nlook\nstatus\nquit\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let actual: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(actual, expected);
}
#[test]
fn typed_json_exposes_commands_without_a_second_resolver() {
    match parse("command {\"Combat\":{\"battle\":1,\"action\":\"Wait\"}}").unwrap() {
        Input::Command(Command::Combat { battle, .. }) => assert_eq!(battle, 1),
        _ => panic!("wrong command"),
    };
    assert!(parse("run 18446744073709551615").is_err());
    assert!(parse("move -1").is_err());
}
#[test]
fn the_existing_executable_discovers_added_scenario_files() {
    let root = std::env::temp_dir().join(format!("legacy-cli-mod-{}", std::process::id()));
    std::fs::create_dir_all(root.join("scenarios")).unwrap();
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let source = std::fs::read_to_string(
        base.join("../tools/fixtures/content/scenarios/kernel-fixture.toml"),
    )
    .unwrap()
    .replace("id = \"kernel-fixture\"", "id = \"new-start\"")
    .replace("Kernel fixture", "New start");
    std::fs::write(root.join("scenarios/new-start.toml"), source).unwrap();
    let output = Process::new(env!("CARGO_BIN_EXE_game-cli"))
        .args([
            "--content",
            base.to_str().unwrap(),
            "--mod",
            root.to_str().unwrap(),
            "--list",
        ])
        .output()
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("new-start: New start")
    );
}

#[test]
fn everyday_actions_resolve_the_same_keys_as_typed_commands() {
    for (text, expected) in [
        (
            "craft weave-basket 19",
            Command::Craft {
                recipe: "weave-basket".into(),
                inventory: InventoryId(19),
            },
        ),
        (
            "plant rye 19",
            Command::Plant {
                crop: "rye".into(),
                seed: InventoryId(19),
            },
        ),
        (
            "pursue whittle 19",
            Command::Pursue {
                activity: "whittle".into(),
                inventory: InventoryId(19),
            },
        ),
    ] {
        match parse(text).unwrap() {
            Input::Command(actual) => assert_eq!(actual, expected),
            _ => panic!("wrong input"),
        }
    }
}
