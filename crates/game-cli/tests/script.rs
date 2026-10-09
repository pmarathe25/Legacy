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

#[test]
fn newborn_care_is_reachable_through_normal_observations_and_the_text_adapter() {
    let execution = Execution::default();
    let mut sim = Simulation::life_fixture(1, execution.clone()).unwrap();
    game_cli::execute(&mut sim, "run 5m", &execution).unwrap();
    let view: PlayerView =
        serde_json::from_str(&game_cli::execute(&mut sim, "status", &execution).unwrap()).unwrap();
    let child = view
        .nearby_life_stages
        .iter()
        .find(|(_, stage)| *stage == LifeStage::Infant)
        .unwrap()
        .0;
    let source = view
        .available_commands
        .iter()
        .find_map(|c| {
            if let Command::Care {
                child: target,
                source,
            } = c
            {
                (*target == child).then_some(*source)
            } else {
                None
            }
        })
        .unwrap();
    game_cli::execute(
        &mut sim,
        &format!("care {} {}", child.0, source.0),
        &execution,
    )
    .unwrap();
    game_cli::execute(&mut sim, "run 1m", &execution).unwrap();
    let ruler = sim.canonical_state().ruler;
    assert!(sim.canonical_state().events.iter().any(|e| matches!(e.kind, EventKind::Cared { caregiver, child: target, .. } if caregiver == ruler && target == child)));
}

#[test]
fn save_is_atomic_even_when_the_target_has_the_old_temporary_extension() {
    let execution = Execution::default();
    let mut sim = Simulation::new("kernel-fixture", 1, execution.clone()).unwrap();
    let root = std::env::temp_dir().join(format!("legacy-cli-save-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("world.save-tmp");
    std::fs::write(&path, b"old save").unwrap();
    game_cli::execute(&mut sim, &format!("save {}", path.display()), &execution).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(
        sim.canonical_state(),
        Simulation::load(&bytes, execution.clone())
            .unwrap()
            .canonical_state()
    );
    let directory = root.join("not-a-file");
    std::fs::create_dir_all(&directory).unwrap();
    assert!(
        game_cli::execute(
            &mut sim,
            &format!("save {}", directory.display()),
            &execution
        )
        .is_err()
    );
    assert!(directory.is_dir());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn an_encounter_is_playable_through_the_text_client_and_ordinary_views() {
    let execution = Execution::default();
    let mut sim = Simulation::combat_fixture(1, execution.clone()).unwrap();
    let target = sim
        .observe()
        .nearby_people
        .iter()
        .find(|(_, role)| *role == Role::Citizen)
        .unwrap()
        .0;
    game_cli::execute(&mut sim, &format!("engage {}", target.0), &execution).unwrap();
    let view: Option<TacticalView> =
        serde_json::from_str(&game_cli::execute(&mut sim, "combat", &execution).unwrap()).unwrap();
    let encounter = view.unwrap().encounter;
    for _ in 0..3 {
        game_cli::execute(
            &mut sim,
            &format!("turn {} attack {}", encounter.0, target.0),
            &execution,
        )
        .unwrap();
        game_cli::execute(&mut sim, "run 6s", &execution).unwrap();
    }
    game_cli::execute(
        &mut sim,
        &format!("turn {} capture {}", encounter.0, target.0),
        &execution,
    )
    .unwrap();
    game_cli::execute(&mut sim, "run 6s", &execution).unwrap();
    assert_eq!(sim.time(), SimTime(24_000));
    assert!(
        sim.canonical_state()
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .contains_key(&target)
    );
}

#[test]
fn hosted_text_commands_pause_long_runs_and_restore_committed_saves() {
    use game_cli::SessionOutput;
    let execution = Execution {
        workers: 4,
        reverse_jobs: true,
    };
    let sim = Simulation::kernel_fixture(1, 10_000, execution.clone()).unwrap();
    let session = game_sim::session::Session::spawn(sim).unwrap();
    let SessionOutput::Advancing(run) = game_cli::execute_session(&session, "run 3650d").unwrap()
    else {
        panic!("expected an advance handle")
    };
    game_cli::execute_session(&session, "move 2").unwrap();
    let SessionOutput::Text(status) = game_cli::execute_session(&session, "status").unwrap() else {
        panic!("expected a view")
    };
    let view: PlayerView = serde_json::from_str(&status).unwrap();
    assert!(view.time.0 < 3650 * 86_400_000);
    let SessionOutput::Text(paused) = game_cli::execute_session(&session, "pause").unwrap() else {
        panic!("expected a view")
    };
    let paused: PlayerView = serde_json::from_str(&paused).unwrap();
    let result = run.wait().unwrap();
    assert_eq!(result.reason, StopReason::Paused);
    assert_eq!(result.reached, paused.time);
    let bytes = session.save().unwrap();
    let saved = Simulation::load(&bytes, execution).unwrap();
    assert_eq!(saved.time(), paused.time);
    assert!(session.load(b"invalid".to_vec()).is_err());
    assert_eq!(session.observe().unwrap().time, paused.time);
    session
        .submit_command(Command::Move {
            destination: AreaId(3),
        })
        .unwrap();
    session.load(bytes).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&session.debug_json().unwrap()).unwrap(),
        serde_json::to_value(saved.canonical_state()).unwrap()
    );
}

#[test]
fn a_hosted_combat_run_returns_input_requests_instead_of_spinning() {
    let sim = Simulation::combat_fixture(1, Execution::default()).unwrap();
    let target = sim
        .observe()
        .nearby_people
        .iter()
        .find(|(_, role)| *role == Role::Citizen)
        .unwrap()
        .0;
    let session = game_sim::session::Session::spawn(sim).unwrap();
    session.submit_command(Command::Engage { target }).unwrap();
    let encounter = session.observe().unwrap().combat.unwrap().encounter;
    let result = session
        .run(SimDuration(86_400_000))
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(result.reason, StopReason::CombatInput { encounter });
    assert_eq!(result.reached, SimTime(0));
    session
        .submit_command(Command::CombatTurn {
            encounter,
            movement: None,
            action: CombatAction::Defend,
        })
        .unwrap();
    let result = session
        .run(SimDuration(86_400_000))
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(result.reached, SimTime(6_000));
    assert_eq!(result.reason, StopReason::CombatInput { encounter });
}
