use std::io::Write;
use std::process::{Command, Stdio};
#[test]
fn addressed_person_can_answer_a_work_request_in_the_interface() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/work_requests.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "request-work 1 2 1 1 5\nrespond-work 1 2 start\nwork-requests 2\nrun 2\ninventory 1\nquit\n").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("work_request=1 response=started"));
    assert!(output.contains("outcome=Started { job: 100 }"));
    assert!(output.contains("good=2 quantity=2 capacity=4"));
}

#[test]
fn fear_recovery_fixture_changes_later_request_behavior() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/fear_recovery.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "request-work 1 2 1 1 5\nrun 1\nwork-requests 2\nrun 2\nrequest-work 1 2 1 1 8\nrun 1\nwork-requests 2\nopinions 2\nquit\n").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("outcome=Started"), "{output}");
    assert!(output.contains("outcome=Refused"), "{output}");
    assert!(
        output.contains("observer=2 target=1 trust=-4 fear=0"),
        "{output}"
    );
}

#[test]
fn local_food_fixture_uses_personal_food_before_regional_food() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/local_food.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "run 1\nregion 1\nquit\n").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("region=1 food=0"), "{output}");
    assert!(output.contains("person=1 health=100 hunger=5"), "{output}");
    assert!(output.contains("person=2 health=100 hunger=0"), "{output}");
}
#[test]
fn addressed_work_request_runs_through_production_in_the_interface() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/work_requests.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "request-work 1 2 1 1 5\nwork-requests 2\nrun 1\nwork-requests 2\nrun 2\ninventory 1\nquit\n").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("work_request=1 requester=1 worker=2"));
    assert!(output.contains("outcome=Started { job: 100 }"));
    assert!(output.contains("good=2 quantity=2 capacity=4"));
}
#[test]
fn harm_changes_personal_fear_and_later_response_in_the_interface() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/cooperation_harm.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "fight 1 1 1 2\ncombat-order 1 2 retreat\nrun 1\nopinions 2\nrun 1\ngoods 1\nquit\n"
    )
    .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("observer=2 target=1 trust=0 fear=4"));
    assert!(output.contains("person=1 good=1 quantity=2"));
}
#[test]
fn people_can_withhold_promised_goods_through_the_interface() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/agreements_bulk.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "agreement-policy 1 ignore\nrun 4\nagreements 1\ngoods 1\nquit\n"
    )
    .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("person=1 honor=false bulk=false"));
    assert_eq!(output.matches("status=Defaulted").count(), 2);
    assert!(output.contains("person=1 good=1 quantity=5"));
}
#[test]
fn tactical_commands_filter_range_and_move_into_combat() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/tactics.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "tactical-fight 1 1 1 2 1 1 10 1\nrun 1\ncombat\ntactical-move 1 1 9 1\nrun 1\ncombat\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("tick=1 encounter=1 attacker="));
    assert!(text.contains("tick=2 encounter=1 attacker=1 target=2 damage=10"));
}
#[test]
fn compiled_host_runs_production_delivery_and_save_reload() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let path = format!("target/block-cli-test-{}.json", std::process::id());
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/workshop.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(),"work 1 1 1 1 1\nrun 2\npickup-good 1 2 2\nmove 1 2\nrun 1\nsave {path}\nrun 1\nload {path}\nrun 1\ndeposit-good 1 2 2\ninventory 2\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(root.join(&path)).unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    assert!(output.contains("good=2 quantity=2 capacity=20"));
    assert!(output.contains("loaded"));
}
#[test]
fn compiled_host_runs_personal_food_and_autonomous_trade() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
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

#[test]
fn compiled_host_shows_guardian_search_and_readiness() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/guardians.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "ruler\nrun 10\nguardians\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    assert!(output.contains("ruler=Some(1)"));
    assert!(output.contains("guardian=1 region=1"));
    assert!(output.contains("ready=true"));
    assert!(output.contains("guardian=2 region=2 health=100 confined=true"));
}

#[test]
fn time_acceleration_stops_at_an_invocation_warning() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/invocation.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "run 7\ninvoke 1\nrun 20\ninvocation\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    assert!(output.contains("invocation warning:"), "{output}");
    assert!(output.contains("Strain"));
}

#[test]
fn compiled_cli_transfers_stock_title_with_recipient_consent() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/property.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "property-offer 1 2 1 1\nproperty-decide 1 2 accept\nproperties\npickup-good 2 1 1\ngoods 2\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    assert!(output.contains("region=1 good=1 owner=2"));
    assert!(output.contains("person=2 good=1 quantity=1"));
}

#[test]
fn combat_commands_run_a_round_and_show_attributed_damage() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/conflict.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "fight 1 1 1 2\ncombat-order 1 2 defend\nrun 1\ncombat\nquit\n"
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
    assert!(stdout.contains("encounter=1"));
    assert!(stdout.contains("attacker=1 target=2 damage=50"));
}

#[test]
fn treatment_commands_consume_medicine_and_show_healing() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/care.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "treat 1 1 2\nrun 2\ncare\ninventory 1\nquit\n"
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
    assert!(stdout.contains("healed=30"));
    assert!(stdout.contains("good=1 quantity=6"));
}

#[test]
fn exposure_command_reports_finite_shelter() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/exposure.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "exposure\nrun 1\nregion 1\nquit\n"
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
    assert!(stdout.contains("region=1 damage=10 shelter=1"));
    assert!(stdout.contains("person=2 health=90"));
}

#[test]
fn player_mode_limits_observation_and_cannot_command_other_people() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "content/parallel/guardians.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "observe\nregion 2\nmove 2 2\nrun 1\nspectate-guardian 1\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stdout.contains("ruler=1"));
    assert!(stdout.contains("route destination=2 ticks=5"));
    assert!(stdout.contains("GuardianView"));
    assert!(!stdout.contains("person=5"));
    assert!(stderr.contains("command unavailable in player mode"));
    assert!(stderr.contains("player can only command the current ruler"));
}

#[test]
fn disease_inspection_is_available_only_to_the_administrator() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/disease.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "disease\nrun 2\ndisease\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("person=1 strain=1 infected_at=0"));
    assert!(stdout.contains("event=Infected"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "content/parallel/guardians.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "disease\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("command unavailable in player mode")
    );
}

#[test]
fn agreement_commands_move_goods_and_report_default_in_admin_mode() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/agreements.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "agreement-propose 1 2 1 4 3\nagreement-decide 1 2 accept\nagreement-deliver 1 1 2\nagreements 1\nrun 4\nagreements 1\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("delivered=2 quantity=4 due=3 status=Active"),
        "{stdout}"
    );
    assert!(
        stdout.contains("delivered=2 quantity=4 due=3 status=Defaulted"),
        "{stdout}"
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn directed_opinions_inspection_is_limited_in_player_mode() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/distrust.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "opinions 2\nrun 1\nagreements 2\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("observer=2 target=1 trust=-2"));
    assert!(stdout.contains("status=Proposed"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "content/parallel/guardians.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "opinions 2\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("player can only command the current ruler")
    );
}
#[test]
fn work_goals_can_be_set_and_inspected_in_the_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/goals.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "goal-set 2 1 9 1 enable\ngoals 2\nrun 1\nquit\n"
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
            .contains("person=2 recipe=1 priority=9 progress=0 target=1 enabled=true")
    );
}
#[test]
fn caregiving_preferences_are_playable_in_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/caregiving.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "care-policy 1 disable\ncare-policy 2 disable\nrun 1\ncaregiving 9\nquit\n"
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
            .contains("care child=9 caregiver=None")
    );
}
#[test]
fn aspirations_can_be_edited_and_inspected_from_the_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/dreams.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "dream-set 1 1 8 2 5 enable\ndreams 1\nrun 1\ngoals 1\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("person=1 recipe=1 priority=8 batch=2"));
    assert!(text.contains("person=1 recipe=1 priority=8 progress=0 target=2"));
}
#[test]
fn personal_expedition_can_be_started_and_cancelled_in_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/mission_start.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "mission-start 1 2 2\nrun 1\nmissions 1\nmission-cancel 1 1\nmissions 1\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("phase=Outbound"));
    assert!(text.contains("phase=Failed"));
}

#[test]
fn player_forms_a_party_only_with_consented_local_expeditions() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "content/parallel/mission_party.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "mission-party 1 1\nmission-start 1 2 2\nmission-candidates 1\nmission-party 1 2\nmission-consent 1 2 accept\nmission-party 1 2\nrun 1\nmissions 1\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stdout.contains("candidate person=2 mission=1"), "{stdout}");
    assert!(stdout.contains("party=1"), "{stdout}");
    assert!(stdout.contains("phase=Outbound"), "{stdout}");
    assert!(
        stderr.contains("player must participate in the party"),
        "{stderr}"
    );
    assert!(
        stderr.contains("party members must have accepted"),
        "{stderr}"
    );
}

#[test]
fn existing_save_can_be_replaced_and_loaded_again() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let path = format!("target/parallel-overwrite-{}.json", std::process::id());
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/campaign.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "run 1\nsave {path}\nrun 1\nsave {path}\nrun 1\nload {path}\nstatus\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let _ = std::fs::remove_file(root.join(&path));
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.is_empty(), "{stderr}");
    assert!(
        stdout.contains(&format!("loaded {path}\ntick=2")),
        "{stdout}"
    );
}

#[test]
fn seeded_generated_world_is_playable_in_player_mode() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args([
            "--player",
            "--generate",
            "content/parallel/worldgen.toml",
            "--seed",
            "7",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "run 5\nobserve\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert!(stdout.contains("tick=5 ruler=1"), "{stdout}");
    assert!(stdout.contains("season=warm"), "{stdout}");
}

#[test]
fn player_can_converse_but_cannot_command_or_inspect_another_person() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "content/parallel/conversation.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "talk 1 2\nsocial 1\ntalk 2 1\nsocial 2\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stderr}");
    assert!(stdout.contains("conversation=accepted tick=1"), "{stdout}");
    assert!(stdout.contains("a=1 b=2 familiarity="), "{stdout}");
    assert_eq!(
        stderr
            .matches("player can only command the current ruler")
            .count(),
        2,
        "{stderr}"
    );
}

#[test]
fn social_pressure_can_be_changed_and_does_not_grant_trust() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/social_pressure.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "social-pressure 2 disable\ntalk 1 2\nsocial-pressure 2 enable\ntalk 1 2\nopinions 2\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    assert!(stdout.contains("conversation=declined tick=1"), "{stdout}");
    assert!(stdout.contains("conversation=accepted tick=2"), "{stdout}");
    assert!(
        stdout.contains("observer=2 target=1 trust=0 fear=2"),
        "{stdout}"
    );
}

#[test]
fn fear_can_motivate_a_real_agreement_delivery_in_the_interface() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/agreement_pressure.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "agreements 1\nrun 1\nagreements 1\ngoods 1\ngoods 2\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    assert!(
        stdout.contains("honor=false bulk=false fear_honor_threshold=Some(2)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("delivered=2 quantity=2 due=3 status=Fulfilled"),
        "{stdout}"
    );
    assert!(stdout.contains("good=1 quantity=3"), "{stdout}");
    assert!(stdout.contains("good=1 quantity=2"), "{stdout}");
}

#[test]
fn imported_recipe_catalog_exposes_authored_craft_categories() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--generate", "content/parallel/worldgen.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "catalog-recipes\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    let recipe = stdout
        .lines()
        .find(|line| line.contains("name=press-fiberboard"))
        .unwrap();
    assert!(recipe.contains("skill=Some(\"carpentry\")"), "{recipe}");
}

#[test]
fn player_can_use_edible_goods_and_cannot_change_another_persons_pressure() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "content/parallel/social_pressure.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "eat-good 1 1\nsocial-pressure 2 disable\ntalk 1 2\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("production is disabled"), "{stderr}");
    assert!(
        stderr.contains("player can only command the current ruler"),
        "{stderr}"
    );
    assert!(stdout.contains("conversation=accepted tick=1"), "{stdout}");
}

#[test]
fn catalog_lists_scenarios_and_seeded_random_selection_is_reproducible() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let listing = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("--list")
        .output()
        .unwrap();
    assert!(listing.status.success());
    let listing = String::from_utf8(listing.stdout).unwrap();
    assert!(listing.contains("campaign - A living settlement"));
    assert!(listing.contains("mission-party - A shared expedition"));
    let run = || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
            .current_dir(root)
            .args(["--player", "--random", "--seed", "17"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(), "observe\nquit\n").unwrap();
        child.wait_with_output().unwrap()
    };
    let first = run();
    let second = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    let text = String::from_utf8(first.stdout).unwrap();
    assert!(text.contains("scenario="));
    assert!(text.contains("seed=17"));
    assert!(text.contains("ruler=1"));
}
#[test]
fn named_scenario_accepts_a_seed_override_and_player_defaults_to_campaign() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    for args in [
        vec!["--player", "--scenario", "campaign", "--seed", "7"],
        vec!["--player"],
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
            .current_dir(root)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(), "observe\nquit\n").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("scenario=campaign"), "{stdout}");
        assert!(stdout.contains("ruler=1"), "{stdout}");
    }
}

#[test]
fn player_can_take_restricted_goods_and_see_the_witnessed_incident() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "content/parallel/theft.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "pickup-good 1 1 2\nsteal-good 1 1 2\nobserve\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.contains("stock owner has not authorized"),
        "{stderr}"
    );
    assert!(stdout.contains("theft owner=2 witnessed=true"), "{stdout}");
    assert!(stdout.contains("carried good=1 quantity=2"), "{stdout}");
    assert!(stdout.contains("theft actor=1 owner=2"), "{stdout}");
}

#[test]
fn player_can_use_claimed_material_for_work_and_see_the_incident() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "content/parallel/theft.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "work-unpermitted 55 2 1 1 1\nwork-unpermitted 55 1 1 1 1\nobserve\nquit\n"
    )
    .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    let errors = String::from_utf8(result.stderr).unwrap();
    assert!(
        errors.contains("player can only command the current ruler"),
        "{errors}"
    );
    assert!(output.contains("job=55 started"), "{output}");
    assert!(output.contains("theft actor=1 owner=2"), "{output}");
}

#[test]
fn pressured_companion_can_join_without_revealing_private_approval_to_the_inviter() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "content/parallel/invitation.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "mission-start 1 2 2\nmission-invite 1 1 3\nmissions 1\nrun 1\nquit\n"
    )
    .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    let errors = String::from_utf8(result.stderr).unwrap();
    assert!(errors.is_empty(), "{errors}");
    assert!(output.contains("invitation=joined mission=2"), "{output}");
    assert!(!output.contains("party_compliant=true"), "{output}");
}

#[test]
fn ruler_can_see_an_inherited_title_in_an_authored_scenario() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "--scenario", "estate"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "run 1\nobserve\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert!(
        stdout.contains("inheritance region=1 good=1 previous_owner=2 heir=1"),
        "{stdout}"
    );
}

#[test]
fn ruler_can_invite_a_willing_local_companion() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "--scenario", "invitation"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "mission-start 1 2 2\nmission-invite 1 1 2\nrun 1\nmissions 1\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.is_empty(), "{stderr}");
    assert!(stdout.contains("invitation=joined mission=2"), "{stdout}");
    assert!(stdout.contains("phase=Outbound"), "{stdout}");
}

#[test]
fn player_can_invoke_a_past_lifes_work_skill() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "--scenario", "invoked-skills"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "observe\ninvoke 2\nobserve\nwork 1 1 1 1 1\nrun 1\nobserve\ndismiss-invocation\nobserve\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(
        stdout
            .matches("invoked_skill life=2 recipe=1 rate=4")
            .count(),
        2,
        "{stdout}"
    );
    assert!(stdout.contains("skill recipe=1 completions=1"), "{stdout}");
}

#[test]
fn player_can_teach_a_skill_from_an_invoked_life() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "--scenario", "teaching"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "teach 1 3 1\ninvoke 2\nteach 1 3 1\nobserve\ndismiss-invocation\nobserve\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("teacher must have a higher skill rate"),
        "{stderr}"
    );
    assert!(stdout.contains("lesson=accepted tick=1"), "{stdout}");
    assert!(
        stdout.contains("lesson teacher=1 student=3 recipe=1 tick=1"),
        "{stdout}"
    );
}

#[test]
fn administrative_view_shows_autonomous_teaching_and_player_view_stays_personal() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--scenario", "auto-teaching"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "run 1\nskills 4\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.is_empty(), "{stderr}");
    assert!(
        stdout.contains("person=4 skill_rate recipe=1 rate=2"),
        "{stdout}"
    );
    assert!(
        stdout.contains("lesson teacher=3 student=4 recipe=1 tick=1"),
        "{stdout}"
    );
}

#[test]
fn player_can_ask_a_willing_npc_to_teach() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "--scenario", "auto-teaching"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "learn 1 3 1\nobserve\nskills 3\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stdout.contains("lesson=accepted tick=1"), "{stdout}");
    assert!(stdout.contains("skill_rate recipe=1 rate=2"), "{stdout}");
    assert!(
        stdout.contains("lesson teacher=3 student=1 recipe=1 tick=1"),
        "{stdout}"
    );
    assert!(
        stderr.contains("player can only command the current ruler"),
        "{stderr}"
    );
}

#[test]
fn meeting_expedition_objective_is_playable_in_the_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/mission_meeting.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "mission-start 1 2 2\nmission-meet 1 1 3\nrun 14\nmissions 1\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    assert!(
        stdout.contains("phase=Completed") && stdout.contains("objective_met=true"),
        "{stdout}"
    );
}

#[test]
fn farm_volunteering_can_be_inspected_and_changed_in_the_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/farming_volunteers.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "farm-status 1\nfarm-policy 1 disable\nfarm-status 1\nfarm-policy 2 enable\nfarm-status 2\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    assert!(
        stdout.contains("farm person=1 volunteering=true")
            && stdout.contains("farm person=1 volunteering=false")
            && stdout.contains("farm person=2 volunteering=true"),
        "{stdout}"
    );
}
#[test]
fn automatic_farming_observes_a_persons_persistent_refusal_in_the_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/farming_volunteers_auto.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "farm-policy 1 disable\nrun 2\nfarm-status 1\nfarm-status 2\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    assert!(
        stdout.contains("farm person=1 volunteering=false refused=true"),
        "{stdout}"
    );
    assert!(
        stdout.contains("farm person=2 volunteering=true refused=false"),
        "{stdout}"
    );
}

#[test]
fn player_can_travel_and_meet_their_expedition_target() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "--scenario", "meeting-expedition"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "mission-start 1 2 2\nmission-meet 1 1 5\nrun 6\ntalk 1 5\nrun 12\nmissions 1\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    assert!(
        stdout.contains("phase=Completed") && stdout.contains("objective_met=true"),
        "{stdout}"
    );
}
#[test]
fn player_cannot_change_another_residents_farm_consent() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .args(["--player", "content/parallel/campaign.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "farm-policy 2 disable\nfarm-policy 1 disable\nfarm-status 1\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success());
    assert!(
        stderr.contains("player can only command the current ruler"),
        "{stderr}"
    );
    assert!(
        stdout.contains("farm person=1 volunteering=false refused=true"),
        "{stdout}"
    );
}

#[test]
fn food_seeking_can_be_changed_in_the_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/personal_food.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "food-seeking 2 disable\nrun 1\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    assert!(
        stdout.contains("food-seeking person=2 enabled=false"),
        "{stdout}"
    );
}

#[test]
fn food_requests_can_be_inspected_and_withdrawn_in_the_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/food_aid.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "request-food 2 1 7 2 5\nfood-requests 2\nwithdraw-food 2 7\nfood-requests 2\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    assert!(
        stdout.contains("food-request=7 requester=2 giver=1 amount=2 expires=5"),
        "{stdout}"
    );
    assert!(
        stdout.contains("food-request=7 withdrawn by person=2"),
        "{stdout}"
    );
}

#[test]
fn automatic_food_asking_can_be_changed_in_the_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parallel-block-cli"))
        .current_dir(root)
        .arg("content/parallel/food_requests.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "food-asking 2 disable\nfood-asking 2 enable\nrun 1\nfood-requests 2\nquit\n"
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    assert!(
        stdout.contains("food-asking person=2 enabled=false"),
        "{stdout}"
    );
    assert!(
        stdout.contains("food-asking person=2 enabled=true"),
        "{stdout}"
    );
    assert!(
        stdout.contains("food-request=0 requester=2 giver=1 amount=2 expires=4"),
        "{stdout}"
    );
}
