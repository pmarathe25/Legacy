use game_core::*;
use game_session::*;
use std::time::Duration;
#[test]
fn a_large_advance_can_be_paused_and_saved() {
    let sim = game_assembly::Simulation::new("kernel-fixture", 1, 1).unwrap();
    let s = Session::spawn(sim).unwrap();
    s.input.send(Input::Advance(SimTime(DAY.0 * 365))).unwrap();
    s.input.send(Input::Pause).unwrap();
    loop {
        match s.output.recv_timeout(Duration::from_secs(5)).unwrap() {
            Output::Paused(at) => {
                assert!(at < SimTime(DAY.0 * 365));
                break;
            }
            Output::Progress(result) => {
                result.unwrap();
            }
            _ => {}
        }
    }
    s.input.send(Input::Save).unwrap();
    match s.output.recv_timeout(Duration::from_secs(5)).unwrap() {
        Output::Save(result) => {
            let save = result.unwrap();
            game_assembly::Simulation::load(&save, 1).unwrap();
        }
        _ => panic!("expected save"),
    }
}
