use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn interactive_session_runs_commands_and_reloads_saved_state() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let save = format!("target/parallel-cli-test-{}.json", std::process::id());
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-cli"))
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "status\nmove 1 2\nship 7 1 2 2\nrun 1\nsave {save}\nrun 1\nload {save}\nstatus\nmove 999 2\nrun 1\nsave {save}\nload {save}\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    let saved = std::fs::read_to_string(root.join(&save)).unwrap();
    std::fs::remove_file(root.join(&save)).unwrap();
    let saved = game_parallel::World::from_json(&saved).unwrap();
    assert_eq!(saved.tick, 2);
    assert!(saved.travelers.is_empty());
    assert!(saved.shipments.is_empty());
    assert!(saved.regions[1].people.iter().any(|p| p.id == 1));
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stdout.contains("travel started for person=1"));
    assert!(stdout.contains("shipment=7 launched"));
    assert!(stdout.contains("tick=1 regions=2 living=3 travelers=1 shipments=1"));
    assert!(stderr.contains("unknown person"));
}

#[test]
fn interactive_session_can_carry_and_consume_food_while_traveling() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-cli"))
        .current_dir(root)
        .arg("content/parallel/cargo.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "take 1 4\nmove 1 2\nrun 1\neat 1\nrun 1\nstore 1 2\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("person=1 ate carried food"));
    assert!(stdout.contains("food stored by person=1"));
}

#[test]
fn interactive_session_can_inspect_social_history() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-cli"))
        .current_dir(root)
        .arg("content/parallel/social.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "run 1\nsocial 1\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("person=1 willing=true"));
    assert!(stdout.contains("other=2 familiarity=2 encounters=1 last_met=1"));
}

#[test]
fn interactive_session_inspects_personal_observations() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-cli"))
        .current_dir(root)
        .arg("content/parallel/knowledge.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "run 1\nknowledge 2\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("person=2 known_regions=2"));
    assert!(stdout.contains("region=2 food=4 observed_at=0 witness=3 learned_from=Some(1)"));
}

#[test]
fn interactive_session_displays_autonomous_departure() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-cli"))
        .current_dir(root)
        .arg("content/parallel/autonomy_departure.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "run 3\ndecisions\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("tick=3 person=2 source=1 destination=2 outcome=Started")
    );
}

#[test]
fn interactive_workshop_reserves_and_delivers_goods() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-cli"))
        .current_dir(root)
        .arg("content/parallel/workshop.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "work 1 1 1 1 1\ninventory 1\nrun 2\ninventory 1\npickup-good 1 2 2\ngoods 1\nmove 1 2\nrun 2\ndeposit-good 1 2 2\ninventory 2\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("job=1 started"));
    assert!(stdout.contains("good=1 quantity=6 capacity=20"));
    assert!(stdout.contains("good=2 quantity=2 capacity=4"));
    assert!(stdout.contains("person=1 good=2 quantity=2 pickup-good"));
    assert!(stdout.contains("good=2 quantity=2 capacity=20"));
}

#[test]
fn offers_require_explicit_recipient_decisions() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-cli"))
        .current_dir(root)
        .arg("content/parallel/workshop.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "pickup-good 1 1 2\noffer 1 1 2 2 0 0\noffers 2\naccept 1 2\ngoods 2\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let out = String::from_utf8(output.stdout).unwrap();
    assert!(out.contains("offer=1 proposer=1 recipient=2"));
    assert!(out.contains("offer=1 actor=2 outcome=Accepted"));
    assert!(out.contains("person=2 good=1 quantity=2"));
}

#[test]
fn opted_in_recipient_automatically_accepts_a_valued_gift() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-cli"))
        .current_dir(root)
        .arg("content/parallel/auto_market.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "pickup-good 1 1 2\noffer 1 1 2 2 0 0\nrun 1\ngoods 2\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("person=2 good=1 quantity=2")
    );
}
