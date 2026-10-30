use crate::support;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
struct Seen {
    starts: Vec<usize>,
    collects: u64,
    rested: Vec<PersonId>,
    reject: bool,
}
impl Resource for Seen {
    const KEY: &'static str = "test.activity-seen";
    fn validate(&self) -> Result<()> {
        require(
            !self.reject || self.starts.is_empty(),
            "reject activity frame",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Cue(PersonId);
impl Request for Cue {
    const KEY: &'static str = "test.activity-cue";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Reject;
impl Request for Reject {
    const KEY: &'static str = "test.activity-reject";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = support::builder();
    game_autonomy_frames::register(&mut b).unwrap();
    b.plugin("activity-test", 1).unwrap();
    b.resource::<Seen>().unwrap();
    b.before::<game_physiology::RestActivities>("test.activity-starts", |tx, r| {
        tx.edit::<Seen>()?.starts.push(r.0.len());
        Ok(())
    })
    .unwrap();
    b.before::<game_choice::Collect>("test.activity-collects", |tx, _| {
        tx.edit::<Seen>()?.collects += 1;
        Ok(())
    })
    .unwrap();
    b.subscribe::<game_physiology::Rested>("test.activity-rested", |tx, e| {
        tx.edit::<Seen>()?.rested.push(e.person);
        Ok(())
    })
    .unwrap();
    b.request::<Cue>(|tx, r| tx.notify(game_characters::Alert { person: r.0 }))
        .unwrap();
    b.request::<Reject>(|tx, _| {
        tx.edit::<Seen>()?.reject = true;
        Ok(())
    })
    .unwrap();
    b
}
fn setup(count: usize, fatigue: u64) -> (Host, Vec<PersonId>) {
    setup_with_hunger(count, fatigue, 0)
}
fn setup_with_hunger(count: usize, fatigue: u64, hunger_per_tick: u64) -> (Host, Vec<PersonId>) {
    let mut host = builder().build(17, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    host.dispatch(game_physiology::Configure(game_physiology::Rules {
        hunger_per_tick,
        fatigue_per_tick: 0,
        rest_per_tick: 1,
        ..Default::default()
    }))
    .unwrap();
    host.dispatch(game_characters::Configure(game_characters::Rules {
        meal_hunger: 32,
        ..Default::default()
    }))
    .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "recovery".into(),
        })
        .unwrap();
    let mut people = Vec::new();
    for _ in 0..count {
        let person = host
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        host.dispatch(game_transport::Place { person, area })
            .unwrap();
        host.dispatch(game_physiology::Exert {
            person,
            energy: 0,
            fatigue,
        })
        .unwrap();
        host.dispatch(game_characters::Register {
            person,
            mind: Default::default(),
        })
        .unwrap();
        people.push(person);
    }
    host.dispatch(game_physiology::Start).unwrap();
    host.dispatch(game_characters::Start).unwrap();
    host.dispatch(game_choice::ConfigureFrames {
        interval: Some(SimDuration(15 * MINUTE.0)),
    })
    .unwrap();
    host.dispatch(game_choice::ConfigureFrameDemand(true))
        .unwrap();
    host.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
        activity_epochs: true,
        batch_decisions: true,
        commit_batch: 128,
        urgent_immediate: true,
        ..Default::default()
    }))
    .unwrap();
    (host, people)
}
#[test]
fn persistent_rest_batches_real_frames_and_retains_identity_across_epochs_and_reload() {
    let (initial, people) = setup(64, 500_000);
    let saved = initial.save().unwrap();
    let mut worlds = Vec::new();
    for workers in [1, 4, 12] {
        let mut host = builder().load(&saved, workers).unwrap();
        support::run(&mut host, SimTime(15 * MINUTE.0));
        assert_eq!(host.get::<Seen>().unwrap().starts, vec![64]);
        assert_eq!(host.get::<Seen>().unwrap().collects, 0);
        let ids: Vec<_> = people
            .iter()
            .map(|p| host.get::<Actions>().unwrap().0[p].id)
            .collect();
        support::run(&mut host, SimTime(HOUR.0 + 15 * MINUTE.0));
        // History may reapply an unchanged frame configuration when resuming.
        host.dispatch(game_choice::ConfigureFrames {
            interval: Some(SimDuration(15 * MINUTE.0)),
        })
        .unwrap();
        let mut host = builder().load(&host.save().unwrap(), workers).unwrap();
        support::run(&mut host, SimTime(2 * HOUR.0 + 15 * MINUTE.0));
        assert_eq!(host.get::<Seen>().unwrap().collects, 0);
        assert_eq!(
            people
                .iter()
                .map(|p| host.get::<Actions>().unwrap().0[p].id)
                .collect::<Vec<_>>(),
            ids
        );
        assert!(
            people
                .iter()
                .all(|p| host.get::<game_physiology::Bodies>().unwrap().people[p].resting)
        );
        worlds.push(host.save().unwrap());
    }
    assert!(worlds.windows(2).all(|w| w[0] == w[1]));
}
#[test]
fn rest_epoch_finishes_recovered_people_in_canonical_order() {
    let (mut host, people) = setup(40, 100_000);
    support::run(&mut host, SimTime(2 * HOUR.0 + 15 * MINUTE.0));
    assert_eq!(host.get::<Seen>().unwrap().rested, people);
    assert!(
        people
            .iter()
            .all(|p| !host.get::<game_physiology::Bodies>().unwrap().people[p].resting)
    );
}
#[test]
fn alert_damage_control_and_death_interrupt_persistent_rest_before_next_epoch() {
    for cause in 0..4 {
        let (mut host, people) = setup(2, 500_000);
        support::run(&mut host, SimTime(15 * MINUTE.0));
        let person = people[0];
        match cause {
            0 => host.dispatch(Cue(person)).unwrap(),
            1 => host
                .dispatch(game_physiology::Damage { person, amount: 1 })
                .unwrap(),
            2 => host.dispatch(game_lineage::Begin { person }).unwrap(),
            _ => host
                .dispatch(game_physiology::Damage {
                    person,
                    amount: u64::MAX,
                })
                .unwrap(),
        }
        assert!(!host.get::<Actions>().unwrap().0.contains_key(&person));
        assert!(!host.get::<game_physiology::Bodies>().unwrap().people[&person].resting);
        let replacement = if cause < 3 {
            if cause < 2 {
                assert_eq!(
                    host.get::<game_characters::Minds>().unwrap().people[&person].urgent_frame_at,
                    Some(host.now())
                );
                host.dispatch(game_lineage::Begin { person }).unwrap();
            }
            host.dispatch(game_choice::Undertake {
                person,
                kind: "replacement".into(),
                duration: DAY,
                request: Pending::immediate(game_choice::Wait),
            })
            .unwrap();
            Some(host.get::<Actions>().unwrap().0[&person].id)
        } else {
            None
        };
        let mut host = builder().load(&host.save().unwrap(), 4).unwrap();
        support::run(&mut host, SimTime(HOUR.0 + 15 * MINUTE.0));
        assert!(
            !host.get::<Seen>().unwrap().rested.contains(&person),
            "cancelled epoch must not finish a replacement"
        );
        assert_eq!(
            host.get::<Actions>().unwrap().0.get(&person).map(|a| a.id),
            replacement
        );
    }
}
#[test]
fn rest_activity_start_and_epoch_job_rollback_with_enclosing_frame() {
    let (mut host, _) = setup(40, 500_000);
    host.dispatch(Reject).unwrap();
    loop {
        let before = host.save().unwrap();
        if host.advance(SimTime(15 * MINUTE.0), 1).is_err() {
            assert_eq!(host.save().unwrap(), before);
            break;
        }
        assert!(host.get::<Seen>().unwrap().starts.is_empty());
        assert!(host.now() <= SimTime(15 * MINUTE.0));
    }
}
#[test]
fn disabling_activity_epochs_cancels_active_recovery() {
    let (mut host, people) = setup(4, 500_000);
    support::run(&mut host, SimTime(15 * MINUTE.0));
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.activity_epochs = false;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    assert!(
        people
            .iter()
            .all(|p| !host.get::<Actions>().unwrap().0.contains_key(p))
    );
}

#[test]
fn hunger_forecast_ends_recovery_between_frame_boundaries() {
    let (mut host, people) = setup_with_hunger(2, 500_000, 1);
    support::run(&mut host, SimTime(15 * MINUTE.0));
    assert_eq!(host.get::<Seen>().unwrap().starts, vec![2]);
    assert!(host.scheduled().any(
        |job| job.request.key() == "physiology.rest-epoch" && job.at == SimTime(32 * MINUTE.0)
    ));
    support::run(&mut host, SimTime(32 * MINUTE.0));
    assert_eq!(host.get::<Seen>().unwrap().rested, people);
    assert!(
        people
            .iter()
            .all(|p| !host.get::<game_physiology::Bodies>().unwrap().people[p].resting)
    );
}

#[test]
fn owner_batch_revalidates_control_after_request_hooks_and_rolls_back() {
    let (initial, people) = setup(2, 500_000);
    let mut b = builder();
    b.before::<game_physiology::RestActivities>("test.new-control", |tx, r| {
        tx.call(game_lineage::Begin {
            person: r.0[0].person,
        })
    })
    .unwrap();
    let mut host = b.load(&initial.save().unwrap(), 4).unwrap();
    let before = host.save().unwrap();
    assert!(
        host.dispatch(game_physiology::RestActivities(
            people
                .iter()
                .map(|&person| game_physiology::RestActivity {
                    person,
                    fatigue_above: 10,
                    hunger_below: 32,
                })
                .collect()
        ))
        .is_err()
    );
    assert_eq!(host.save().unwrap(), before);
}

#[test]
fn existing_rest_hooks_keep_the_scalar_owner_path() {
    let (initial, _) = setup(2, 500_000);
    let mut b = builder();
    b.before::<game_physiology::Rest>("test.scalar-rest", |_, _| Ok(()))
        .unwrap();
    let mut host = b.load(&initial.save().unwrap(), 4).unwrap();
    support::run(&mut host, SimTime(15 * MINUTE.0));
    assert!(host.get::<Seen>().unwrap().starts.is_empty());
    assert!(host.get::<Seen>().unwrap().collects > 0);
}

#[test]
fn start_rejects_a_permission_callback_that_clears_the_settled_resting_flag() {
    let (initial, people) = setup(2, 500_000);
    let mut b = builder();
    b.before::<game_physiology::CheckRestActivities>("test.clear-started-rest", |tx, r| {
        if r.starting && tx.get::<game_physiology::Bodies>()?.people[&r.people[0]].resting {
            tx.call(game_soma::SetResting {
                person: r.people[0],
                resting: false,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let mut host = b.load(&initial.save().unwrap(), 4).unwrap();
    let before = host.save().unwrap();
    assert!(
        host.dispatch(game_physiology::RestActivities(
            people
                .iter()
                .map(|&person| game_physiology::RestActivity {
                    person,
                    fatigue_above: 10,
                    hunger_below: 32,
                })
                .collect()
        ))
        .is_err()
    );
    assert_eq!(host.save().unwrap(), before);
}

#[test]
fn continuation_reads_physical_state_after_the_permission_callback() {
    let (mut initial, people) = setup(2, 500_000);
    support::run(&mut initial, SimTime(15 * MINUTE.0));
    let target = people[1];
    let old_action = initial.get::<Actions>().unwrap().0[&target].id;
    let mut b = builder();
    b.before::<game_physiology::CheckRestActivities>(
        "test.change-continuing-body",
        move |tx, r| {
            if !r.starting && r.people.contains(&target) {
                tx.call(game_soma::SetResting {
                    person: target,
                    resting: false,
                })?;
            }
            Ok(())
        },
    )
    .unwrap();
    let mut host = b.load(&initial.save().unwrap(), 4).unwrap();
    support::run(&mut host, SimTime(HOUR.0 + 15 * MINUTE.0));
    assert!(host.get::<Seen>().unwrap().rested.contains(&target));
    assert_ne!(
        host.get::<Actions>().unwrap().0.get(&target).map(|a| a.id),
        Some(old_action)
    );
}

#[test]
fn completion_callbacks_cannot_leave_an_invalidated_peer_sleeping_until_the_next_epoch() {
    for hunger in [false, true] {
        let (mut initial, people) = setup(2, 30_000);
        let first = people[0];
        let target = people[1];
        initial
            .dispatch(game_physiology::Exert {
                person: target,
                energy: 0,
                fatigue: 470_000,
            })
            .unwrap();
        support::run(&mut initial, SimTime(15 * MINUTE.0));
        let old_action = initial.get::<Actions>().unwrap().0[&target].id;
        let saved = initial.save().unwrap();
        let mut worlds = Vec::new();
        for workers in [1, 4, 12] {
            let mut b = builder();
            b.subscribe::<game_physiology::Rested>("test.disturb-retained-peer", move |tx, e| {
                if e.person == first {
                    if hunger {
                        tx.call(game_physiology::Exert {
                            person: target,
                            energy: 50_000,
                            fatigue: 0,
                        })?;
                    } else {
                        tx.call(game_soma::SetResting {
                            person: target,
                            resting: false,
                        })?;
                    }
                }
                Ok(())
            })
            .unwrap();
            let mut host = b.load(&saved, workers).unwrap();
            support::run(&mut host, SimTime(HOUR.0 + 15 * MINUTE.0));
            assert_eq!(host.get::<Seen>().unwrap().rested, people);
            assert_ne!(
                host.get::<Actions>().unwrap().0.get(&target).map(|a| a.id),
                Some(old_action)
            );
            worlds.push(host.save().unwrap());
        }
        assert!(worlds.windows(2).all(|w| w[0] == w[1]));
    }
}

#[test]
fn completion_body_hook_cannot_replace_an_action_and_commit_obsolete_flags() {
    let (mut initial, people) = setup(2, 30_000);
    support::run(&mut initial, SimTime(15 * MINUTE.0));
    let target = people[0];
    let mut b = builder();
    b.before::<game_soma::SetRestingMany>("test.replace-during-rest-flags", move |tx, r| {
        if !r.resting && r.people.contains(&target) {
            tx.call(game_physiology::InterruptRest(target))?;
            tx.call(game_choice::Undertake {
                person: target,
                kind: "replacement".into(),
                duration: DAY,
                request: Pending::immediate(game_choice::Wait),
            })?;
        }
        Ok(())
    })
    .unwrap();
    let mut host = b.load(&initial.save().unwrap(), 4).unwrap();
    loop {
        let before = host.save().unwrap();
        match host.advance(SimTime(HOUR.0 + 15 * MINUTE.0), 1) {
            Err(_) => {
                assert_eq!(host.save().unwrap(), before);
                break;
            }
            Ok(progress) => assert_ne!(
                progress.stop,
                Stop::Target,
                "the incompatible callback must reject"
            ),
        }
    }
}
