use game_assembly::Simulation;
use game_core::*;
fn advance(sim: &mut Simulation, at: SimTime, budget: usize) {
    while sim.advance(at, budget).unwrap().stop == game_sim::Stop::Budget {}
}
fn replay(minimum_cohort: usize, commit_batch: usize) {
    let mut initial = Simulation::new("survival-fixture", 13, 1).unwrap();
    initial
        .debug_host_mut()
        .dispatch(game_characters::ConfigureAttention(None))
        .unwrap();
    initial
        .debug_host_mut()
        .dispatch(game_choice::ConfigureFrames {
            interval: Some(SimDuration(5 * MINUTE.0)),
        })
        .unwrap();
    initial
        .debug_host_mut()
        .dispatch(game_choice::ConfigureFrameDemand(true))
        .unwrap();
    initial
        .debug_host_mut()
        .dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
            recover_idle: true,
            recheck_conflicts: true,
            batch_decisions: true,
            minimum_cohort,
            commit_batch,
            ..Default::default()
        }))
        .unwrap();
    let saved = initial.save().unwrap();
    advance(&mut initial, SimTime(2 * HOUR.0), 128);
    let expected = initial.save().unwrap();
    assert!(
        initial
            .debug_host()
            .get::<game_characters::Minds>()
            .unwrap()
            .people
            .values()
            .any(|m| m.last.is_some())
    );
    for parallel in [false, true] {
        let mut sim = Simulation::load(&saved, 4).unwrap();
        sim.set_parallel_listeners(parallel);
        advance(&mut sim, SimTime(HOUR.0), 1);
        sim = Simulation::load(&sim.save().unwrap(), 2).unwrap();
        sim.set_parallel_listeners(parallel);
        advance(&mut sim, SimTime(2 * HOUR.0), 17);
        assert_eq!(sim.save().unwrap(), expected);
    }
}
#[test]
fn demand_frames_follow_character_deadlines_and_replay_across_workers_and_reload() {
    for (minimum, commits) in [(0, 0), (8, 0), (0, 64), (12, 64)] {
        replay(minimum, commits);
    }
}
