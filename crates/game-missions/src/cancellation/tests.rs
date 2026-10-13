use super::*;
use crate::transition_tests::*;
fn row(speaker: PersonId, listener: PersonId) -> PendingTalk {
    PendingTalk {
        speaker,
        listener,
        area: HOME,
        completes: SimTime(10),
        conversation: Conversation::Objective {
            mission: ID,
            leader: A,
            objective: MissionObjective::Visit { destination: AWAY },
        },
    }
}
#[test]
fn receiver_and_sender_cancel_one_native_talk_in_first_initiator_order() {
    let mut w = state();
    insert_talk(&mut w.store, A, row(A, B)).unwrap();
    let prepared = w.prepare_cancel_talks(&w.store, &[B, A, B]);
    let groups = prepared
        .effects()
        .map(|(actor, effects)| (actor, effects.to_vec()))
        .collect::<Vec<_>>();
    assert_eq!(
        groups,
        vec![
            (
                B,
                vec![ClearTalk {
                    speaker: A,
                    completes: SimTime(10)
                }]
            ),
            (A, Vec::new())
        ]
    );
    let mut batch = Batch::new();
    let finish = prepared.stage_into(&w.store, &mut batch).unwrap();
    assert!(talk(&w.store, A).is_some());
    w.store.apply_batch(batch).unwrap();
    assert!(talk(&w.store, A).is_none());
    assert_eq!(finish.finish(), groups);
}
#[test]
fn new_related_talk_invalidates_cancellation_before_foreign_batch_publication() {
    let mut w = state();
    insert_talk(&mut w.store, A, row(A, B)).unwrap();
    let prepared = w.prepare_cancel_talks(&w.store, &[B]);
    insert_talk(&mut w.store, SP, row(SP, B)).unwrap();
    let before = w.clone();
    assert!(prepared.stage_into(&w.store, &mut Batch::new()).is_err());
    assert_eq!(before, w);
    let prepared = w.prepare_cancel_talks(&w.store, &[B]);
    let mut batch = Batch::new();
    prepared.stage_into(&w.store, &mut batch).unwrap();
    batch.remove::<PendingTalk>(StableId(0));
    assert!(w.store.apply_batch(batch).is_err());
    assert_eq!(before, w);
}

#[test]
fn cancellation_projection_requires_native_matching_deadline() {
    let effect = ClearTalk {
        speaker: PersonId(7),
        completes: SimTime(10),
    };
    assert_eq!(effect.activity_reset(Some(SimTime(10))), Some(PersonId(7)));
    assert_eq!(effect.activity_reset(Some(SimTime(11))), None);
    assert_eq!(effect.activity_reset(None), None);
}
