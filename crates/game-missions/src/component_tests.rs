use crate::transition_tests::*;
use crate::*;
use game_core::*;
use game_ecs::{Batch, Schema, StableId, Store};
use std::collections::{BTreeMap, BTreeSet};
fn fixture() -> crate::fixture::Fixture {
    let mut w = state();
    start(&mut w, proposal(), &facts());
    w
}
#[test]
fn one_native_snapshot_contains_records_and_runtime_contains_only_cursors() {
    let w = fixture();
    let runtime = serde_json::to_value(&w.runtime).unwrap();
    assert_eq!(runtime.as_object().unwrap().len(), 2);
    let saved = serde_json::to_value(&w).unwrap();
    let columns = &saved["components"]["columns"];
    assert_eq!(columns["missions/mission"].as_array().unwrap().len(), 1);
    assert_eq!(columns["missions/recruitment"].as_array().unwrap().len(), 2);
    let replay: crate::fixture::Fixture = serde_json::from_value(saved).unwrap();
    assert_eq!(replay, w);
    replay.validate_storage(&replay.store, SimTime(0)).unwrap();
}
#[test]
fn identity_mismatch_and_orphan_recruitment_are_rejected_without_partial_replacement() {
    let mut w = fixture();
    let row = mission(&w.store, ID).unwrap().clone();
    let before = w.clone();
    assert!(insert_mission(&mut w.store, MissionId(88), row).is_err());
    assert_eq!(w, before);
    // Intrinsic schema accepts a positive ID; composed owner validation checks its canonical key.
    let mut snapshot = w.store.snapshot().unwrap();
    let mut json = serde_json::to_value(&snapshot).unwrap();
    json["columns"]["missions/mission"][0]["value"]["id"] = serde_json::json!(88);
    snapshot = serde_json::from_value(json).unwrap();
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let forged = Store::restore(schema, snapshot).unwrap();
    assert!(w.validate_storage(&forged, SimTime(0)).is_err());
    remove_mission(&mut w.store, ID).unwrap();
    assert!(w.validate_storage(&w.store, SimTime(0)).is_err());
}
#[test]
fn failed_combined_admission_keeps_offered_records_and_runtime_unchanged() {
    let mut w = fixture();
    let change = w
        .prepare_command(
            &w.store,
            A,
            &MissionAction::Accept { recruitment: OFFER },
            &facts(),
            &MissionIds::default(),
        )
        .unwrap();
    let mut batch = Batch::new();
    let _finish = change.stage_into(&w.runtime, &w.store, &mut batch).unwrap();
    batch.remove::<Mission>(StableId(0));
    let before = w.clone();
    assert!(w.store.apply_batch(batch).is_err());
    assert_eq!(w, before);
}
#[test]
fn native_membership_index_reads_live_status_and_preserves_other_personal_views() {
    let mut w = fixture();
    let index = MembershipIndex::build(&w.store);
    assert_eq!(index.membership(&w.store, A), None);
    accept(&mut w, A, OFFER, &facts());
    assert_eq!(index.membership(&w.store, A), Some(ID));
    let f = ObservationFacts {
        now: SimTime(0),
        visible_locations: BTreeMap::from([(A, Location::AtArea(HOME))]),
        heard: BTreeSet::from([A]),
    };
    let prepared = w.prepare_observation(&w.store, A, ID, &f).unwrap();
    prepared.commit(&mut w.store).unwrap();
    let mut view = knowledge(&w.store, A).unwrap().missions[&ID].clone();
    view.status = MissionStatus::Completed;
    knowledge_mut(&mut w.store, A)
        .unwrap()
        .missions
        .insert(ID, view);
    assert_eq!(index.membership(&w.store, A), None);
}

#[test]
fn personal_observation_keeps_a_later_unrelated_mission_view_and_pending_talk_survives_save() {
    let mut w = fixture();
    let f = ObservationFacts {
        now: SimTime(0),
        visible_locations: BTreeMap::from([
            (A, Location::AtArea(HOME)),
            (B, Location::AtArea(HOME)),
        ]),
        heard: BTreeSet::from([A, B]),
    };
    w.prepare_observation(&w.store, A, ID, &f)
        .unwrap()
        .commit(&mut w.store)
        .unwrap();
    let pending = w.prepare_observation(&w.store, A, ID, &f).unwrap();
    let other = MissionId(30);
    let mut proposal = proposal();
    proposal.leader = B;
    proposal.recruits.clear();
    proposal.supplies.clear();
    command(
        &mut w,
        SP,
        MissionAction::Propose { proposal },
        &facts(),
        MissionIds {
            mission: Some(other),
            recruitments: BTreeMap::from([(B, RecruitmentId(31))]),
            backing: None,
        },
    );
    w.prepare_observation(&w.store, A, other, &f)
        .unwrap()
        .commit(&mut w.store)
        .unwrap();
    let unrelated = knowledge(&w.store, A).unwrap().missions[&other].clone();
    pending.commit(&mut w.store).unwrap();
    assert_eq!(knowledge(&w.store, A).unwrap().missions[&other], unrelated);
    let talk = PendingTalk {
        speaker: A,
        listener: B,
        area: HOME,
        completes: SimTime(10),
        conversation: Conversation::Objective {
            mission: ID,
            leader: A,
            objective: MissionObjective::Visit { destination: AWAY },
        },
    };
    insert_talk(&mut w.store, A, talk.clone()).unwrap();
    let replay: crate::fixture::Fixture =
        serde_json::from_slice(&serde_json::to_vec(&w).unwrap()).unwrap();
    assert_eq!(crate::talk(&replay.store, A), Some(&talk));
    assert_eq!(w, replay);
}

#[test]
fn derived_roster_index_rebuilds_after_native_restore_and_never_authorizes_removed_records() {
    let mut w = fixture();
    let mut index = MembershipIndex::default();
    index.register_created(&w.store, [ID]);
    assert_eq!(index.membership(&w.store, A), None);
    accept(&mut w, A, OFFER, &facts());
    assert_eq!(index.membership(&w.store, A), Some(ID));
    let replay: crate::fixture::Fixture =
        serde_json::from_slice(&serde_json::to_vec(&w).unwrap()).unwrap();
    let rebuilt = MembershipIndex::build(&replay.store);
    assert_eq!(
        index.membership(&w.store, A),
        rebuilt.membership(&replay.store, A)
    );
    mission_mut(&mut w.store, ID)
        .unwrap()
        .members
        .get_mut(&A)
        .unwrap()
        .status = CompanionStatus::Withdrawn;
    assert_eq!(index.membership(&w.store, A), None);
    mission_mut(&mut w.store, ID)
        .unwrap()
        .members
        .get_mut(&A)
        .unwrap()
        .status = CompanionStatus::Accepted;
    remove_mission(&mut w.store, ID).unwrap();
    assert_eq!(index.membership(&w.store, A), None);
}
