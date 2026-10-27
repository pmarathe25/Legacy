use game_utility::{Candidate, choose_iter};

fn candidate(id: u64, need: i64) -> Candidate {
    Candidate {
        id,
        need,
        goal: 0,
        value: 0,
        effort: 0,
        risk: 0,
        survival: false,
    }
}

#[test]
fn iterator_selection_preserves_ties_and_current_choice() {
    let offers = [candidate(9, 20), candidate(2, 20), candidate(4, 17)];
    assert_eq!(choose_iter(offers.iter(), None, 0, false).unwrap().id, 2);
    assert_eq!(choose_iter(offers.iter(), Some(4), 3, false).unwrap().id, 4);
    assert_eq!(choose_iter(offers.iter(), Some(4), 2, false).unwrap().id, 2);
    assert!(choose_iter([].iter(), None, 0, false).is_none());
}
