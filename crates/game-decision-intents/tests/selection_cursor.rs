use game_choice::{Proposal, Wait, motive};
use game_decision_intents::SelectionCursor;

fn offer(key: &str, id: u64, score: i64, action: Option<u64>) -> Proposal {
    let mut proposal = Proposal::new(key, motive(score, 0, 0, 0, 0), Wait).unwrap();
    proposal.motive.id = id;
    proposal.continuation = action;
    proposal
}

#[test]
fn review_can_resume_after_a_rejected_claim_without_recording_a_failure() {
    let mut cursor = SelectionCursor::new(
        vec![offer("first", 1, 20, None), offer("second", 2, 10, None)],
        None,
        0,
    );
    let (_, first) = cursor.next(None, None).unwrap();
    assert_eq!(first.key, "first");
    // Resuming does not retry or execute the rejected alternative.
    assert_eq!(cursor.next(None, None).unwrap().1.key, "second");
    assert!(cursor.next(None, None).is_none());
}

#[test]
fn continuation_margin_only_applies_to_the_original_action() {
    let offers = vec![
        offer("continue", 1, 10, Some(7)),
        offer("switch", 2, 14, None),
    ];
    let mut same = SelectionCursor::new(offers.clone(), Some(1), 5);
    assert_eq!(same.next(Some(7), Some(7)).unwrap().1.key, "continue");
    let mut changed = SelectionCursor::new(offers.clone(), Some(1), 5);
    assert_eq!(changed.next(Some(8), Some(7)).unwrap().1.key, "switch");
    let mut stopped = SelectionCursor::new(offers, Some(1), 5);
    assert_eq!(stopped.next(None, Some(7)).unwrap().1.key, "switch");
    assert!(stopped.next(None, Some(7)).is_none());
}

#[test]
fn action_ending_between_selections_removes_obsolete_continuations() {
    let mut cursor = SelectionCursor::new(
        vec![
            offer("first", 1, 30, None),
            offer("old-action", 2, 20, Some(7)),
            offer("fallback", 3, 10, None),
        ],
        None,
        0,
    );
    assert_eq!(cursor.next(Some(7), Some(7)).unwrap().1.key, "first");
    assert_eq!(cursor.next(None, Some(7)).unwrap().1.key, "fallback");
    assert!(cursor.next(None, Some(7)).is_none());
}

#[test]
fn updated_margin_and_stable_tie_breaking_apply_at_each_selection() {
    let offers = vec![
        offer("later-id", 2, 10, None),
        offer("earlier-id", 1, 10, None),
    ];
    let mut tie = SelectionCursor::new(offers.clone(), None, 0);
    assert_eq!(tie.next(None, None).unwrap().1.key, "earlier-id");
    let mut cursor = SelectionCursor::new(offers, Some(2), -1);
    cursor.set_margin(0);
    assert_eq!(cursor.next(Some(4), Some(4)).unwrap().1.key, "later-id");
    assert_eq!(cursor.next(Some(4), Some(4)).unwrap().1.key, "earlier-id");
}
