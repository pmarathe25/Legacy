use super::*;
use game_core::*;
use game_ecs::{Batch, Schema, StableId, Store};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum NativeActivity {
    Idle,
    MissionSpeech(SimTime),
    OtherSpeech(SimTime),
}
struct Activities;
impl TalkActivityFacts<NativeActivity> for Activities {
    fn deadline(&self, activity: &NativeActivity) -> Option<SimTime> {
        match activity {
            NativeActivity::MissionSpeech(at) => Some(*at),
            _ => None,
        }
    }
}
fn fixture(activity: NativeActivity, reject_idle: bool) -> (Runtime, Store) {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    if reject_idle {
        schema
            .register::<NativeActivity>("test/activity", |activity| {
                if *activity == NativeActivity::Idle {
                    return Err(Error::InvalidConfiguration(
                        "idle not admitted by foreign schema".into(),
                    ));
                }
                Ok(())
            })
            .unwrap();
    } else {
        schema
            .register::<NativeActivity>("test/activity", |_| Ok(()))
            .unwrap();
    }
    let mut store = Store::with_schema(schema);
    insert_talk(
        &mut store,
        PersonId(1),
        PendingTalk {
            speaker: PersonId(1),
            listener: PersonId(2),
            area: AreaId(1),
            completes: SimTime(10),
            conversation: Conversation::Objective {
                mission: MissionId(1),
                leader: PersonId(1),
                objective: MissionObjective::Visit {
                    destination: AreaId(2),
                },
            },
        },
    )
    .unwrap();
    store.insert_or_spawn(StableId(1), activity).unwrap();
    (Runtime::default(), store)
}
fn scene() -> TalkScene {
    TalkScene {
        speaker_area: Some(AreaId(1)),
        listener_area: Some(AreaId(1)),
        speaker_combat: false,
        listener_combat: false,
        active_deadline: Some(SimTime(10)),
    }
}
#[test]
fn cancellation_and_completion_clear_only_the_matching_native_mission_deadline() {
    for complete in [false, true] {
        for activity in [
            NativeActivity::MissionSpeech(SimTime(10)),
            NativeActivity::MissionSpeech(SimTime(11)),
            NativeActivity::OtherSpeech(SimTime(10)),
        ] {
            let (runtime, mut store) = fixture(activity.clone(), false);
            let mut batch = Batch::new();
            let publication = if complete {
                runtime
                    .prepare_talk_completion(&store, PersonId(1), SimTime(10), scene(), None)
                    .unwrap()
                    .stage_into_with_activities(
                        &runtime,
                        &store,
                        &Activities,
                        NativeActivity::Idle,
                        &mut batch,
                    )
                    .unwrap()
            } else {
                runtime
                    .prepare_cancel_talks(&store, &[PersonId(2)])
                    .stage_into_with_activities(
                        &runtime,
                        &store,
                        &Activities,
                        NativeActivity::Idle,
                        &mut batch,
                    )
                    .unwrap()
            };
            publication.validate(&runtime, &store, &batch).unwrap();
            store.apply_batch(batch).unwrap();
            assert!(talk(&store, PersonId(1)).is_none());
            let expected = if activity == NativeActivity::MissionSpeech(SimTime(10)) {
                NativeActivity::Idle
            } else {
                activity
            };
            assert_eq!(store.get::<NativeActivity>(StableId(1)), Some(&expected));
        }
    }
}
#[test]
fn cleanup_rejects_changed_runtime_or_late_activity_edits_without_publication() {
    let (runtime, store) = fixture(NativeActivity::MissionSpeech(SimTime(10)), false);
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let publication = runtime
        .prepare_cancel_talks(&store, &[PersonId(2)])
        .stage_into_with_activities(
            &runtime,
            &store,
            &Activities,
            NativeActivity::Idle,
            &mut batch,
        )
        .unwrap();
    let mut changed = runtime.clone();
    changed.last_event += 1;
    assert_eq!(
        publication.validate(&changed, &store, &batch),
        Err(Error::Unavailable)
    );
    batch.insert(StableId(1), NativeActivity::OtherSpeech(SimTime(15)));
    assert_eq!(
        publication.validate(&runtime, &store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn foreign_idle_schema_failure_cannot_publish_pending_record_removal() {
    for complete in [false, true] {
        let (runtime, mut store) = fixture(NativeActivity::MissionSpeech(SimTime(10)), true);
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let publication = if complete {
            runtime
                .prepare_talk_completion(&store, PersonId(1), SimTime(10), scene(), None)
                .unwrap()
                .stage_into_with_activities(
                    &runtime,
                    &store,
                    &Activities,
                    NativeActivity::Idle,
                    &mut batch,
                )
                .unwrap()
        } else {
            runtime
                .prepare_cancel_talks(&store, &[PersonId(2)])
                .stage_into_with_activities(
                    &runtime,
                    &store,
                    &Activities,
                    NativeActivity::Idle,
                    &mut batch,
                )
                .unwrap()
        };
        publication.validate(&runtime, &store, &batch).unwrap();
        assert!(store.apply_batch(batch).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
    }
}
