use game_assembly::Simulation;
use game_core::*;
#[test]
fn simulation_advancement_integrates_preparation_without_changing_committed_boundaries() {
    let original = Simulation::new("survival-fixture", 13, 1)
        .unwrap()
        .save()
        .unwrap();
    let mut serial = Simulation::load(&original, 1).unwrap();
    while serial.advance(SimTime(2 * HOUR.0), 128).unwrap().stop == game_sim::Stop::Budget {}
    let expected = serial.save().unwrap();
    for (parallel, width, budget) in [(false, 32, 1), (true, 32, 128), (false, 0, 128)] {
        let mut sim = Simulation::load(&original, 4).unwrap();
        sim.set_preparation_window(width).unwrap();
        sim.set_parallel_listeners(parallel);
        assert_eq!(sim.save().unwrap(), original);
        assert!(sim.set_preparation_window(129).is_err());
        while sim.advance(SimTime(2 * HOUR.0), budget).unwrap().stop == game_sim::Stop::Budget {}
        assert_eq!(sim.save().unwrap(), expected);
        if width > 0 {
            assert!(sim.preparation_statistics().actors > 0);
        }
        let saved = sim.save().unwrap();
        let mut loaded = Simulation::load(&saved, 2).unwrap();
        assert_eq!(loaded.preparation_statistics().actors, 0);
        loaded.set_preparation_window(width).unwrap();
        assert_eq!(loaded.save().unwrap(), saved);
    }
}
