use game_api::*;
use game_sim::*;

#[test]
fn text_adapter_and_direct_api_produce_identical_logical_results() {
    let execution = Execution::default();
    let mut direct = Simulation::new("kernel-fixture", 1, execution.clone()).unwrap();
    let mut text = Simulation::new("kernel-fixture", 1, execution.clone()).unwrap();
    direct
        .submit(Submission {
            caller_sequence: 1,
            command: Command::Move {
                destination: AreaId(2),
            },
        })
        .unwrap();
    direct.advance(SimDuration(180_003)).unwrap();
    for line in [
        "status",
        "move 2",
        "run 119999ms",
        "debug",
        "run 1ms",
        "status",
        "run 60003ms",
    ] {
        game_cli::execute(&mut text, line, &execution).unwrap();
    }
    assert_eq!(direct.canonical_state(), text.canonical_state());
    let before = text.canonical_state().clone();
    assert!(game_cli::execute(&mut text, "move 999", &execution).is_err());
    assert!(game_cli::execute(&mut text, "run 18446744073709551615d", &execution).is_err());
    assert_eq!(&before, text.canonical_state());
}

#[test]
fn cli_process_accepts_a_noninteractive_script() {
    use std::io::Write;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_game-cli"))
        .args([
            "--scenario",
            "kernel-fixture",
            "--seed",
            "1",
            "--workers",
            "2",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"move 2\nrun 119999ms\nstatus\nrun 1ms\nstatus\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    let views: Vec<PlayerView> = output
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    assert_eq!(views.len(), 2);
    assert!(matches!(
        views[0].location,
        Location::InTransit { to: AreaId(2), .. }
    ));
    assert_eq!(views[1].time, SimTime(120_000));
    assert_eq!(views[1].location, Location::AtArea(AreaId(2)));
}
