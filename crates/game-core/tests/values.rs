use game_core::*;
#[test]
fn time_overflow_is_checked_and_randomness_is_independent_of_iteration_order() {
    assert_eq!(SimTime(u64::MAX).after(MINUTE), Err(Error::Overflow));
    let forward: Vec<_> = (0..100).map(|id| (id, random(7, id, 4))).collect();
    let mut reverse: Vec<_> = (0..100).rev().map(|id| (id, random(7, id, 4))).collect();
    reverse.reverse();
    assert_eq!(forward, reverse);
}
