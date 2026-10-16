use game_core::*;
use game_performance::{exercise, fixture};
use game_sim::Stop;
#[test]
fn pending_review_cursors_and_physical_churn_survive_worker_and_chunk_changes() {
    let (mut one, _) = fixture(257, 1, false);
    // Stop in a same-clock continuation after the physiology job and first page.
    assert_eq!(
        one.advance(SimTime(MINUTE.0), 2).unwrap().stop,
        Stop::Budget
    );
    let mut four = game_assembly::Simulation::load(&one.save().unwrap(), 4).unwrap();
    let target = SimTime(120 * MINUTE.0);
    while one.advance(target, 1).unwrap().stop == Stop::Budget {}
    while four.advance(target, 17).unwrap().stop == Stop::Budget {}
    assert_eq!(one.save().unwrap(), four.save().unwrap());
    let follow = exercise::prepare(&mut one, 257);
    let mut loaded = game_assembly::Simulation::load(&one.save().unwrap(), 2).unwrap();
    let target = SimTime(123 * MINUTE.0);
    while one.advance(target, 3).unwrap().stop == Stop::Budget {}
    while loaded.advance(target, 19).unwrap().stop == Stop::Budget {}
    assert_eq!(one.save().unwrap(), loaded.save().unwrap());
    follow.check(&one);
    follow.check(&loaded);
}
#[test]
fn crowded_social_life_preserves_real_survival_and_replay() {
    let (mut one, stores) = fixture(257, 1, false);
    game_performance::enable_social(&mut one);
    let initial = game_performance::stock(&one, &stores);
    let mut four = game_assembly::Simulation::load(&one.save().unwrap(), 4).unwrap();
    let target = SimTime(120 * MINUTE.0);
    while one.advance(target, 3).unwrap().stop == Stop::Budget {}
    while four.advance(target, 19).unwrap().stop == Stop::Budget {}
    assert_eq!(one.save().unwrap(), four.save().unwrap());
    assert!(game_performance::stock(&one, &stores) < initial);
    assert!(
        one.debug_host()
            .get::<game_social_life::SocialLife>()
            .unwrap()
            .people
            .values()
            .any(|p| p.encounters > 0)
    );
    assert!(
        one.debug_host()
            .get::<game_physiology::Bodies>()
            .unwrap()
            .people
            .values()
            .all(|p| p.alive)
    );
}
