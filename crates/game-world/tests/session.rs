use game_api::*;
use game_session::Session;
use game_world::{Execution, Simulation};

#[test]
fn host_services_pause_queries_commands_and_save_during_long_advancement() {
    let sim = Simulation::kernel_fixture(
        1,
        10_000,
        Execution {
            workers: 4,
            reverse_jobs: false,
        },
    )
    .unwrap();
    let session = Session::spawn(sim).unwrap();
    let run = session.run(SimDuration(10 * 365 * 86_400_000)).unwrap();
    session
        .submit(Submission {
            caller_sequence: 1,
            command: Command::Move {
                destination: AreaId(2),
            },
        })
        .unwrap();
    let view = session.observe().unwrap();
    assert!(view.time.0 < 10 * 365 * 86_400_000);
    let saved = session.save().unwrap();
    let loaded = Simulation::load(&saved, Execution::default()).unwrap();
    assert!(loaded.time() >= view.time);
    let paused = session.pause().unwrap();
    let result = run.wait().unwrap();
    assert_eq!(result.reason, StopReason::Paused);
    assert_eq!(result.reached, paused.time);
    assert_eq!(session.observe().unwrap().time, paused.time);
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
    let session = Session::spawn(sim).unwrap();
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
