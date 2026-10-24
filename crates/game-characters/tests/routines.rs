mod support;
use game_characters::{Configure, Minds, Register, Rules, Start};
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Activity {
    reviews: u64,
    completed: u64,
    fail: bool,
}
impl Resource for Activity {
    const KEY: &'static str = "test.routine-activity";
}
#[derive(Clone, Serialize, Deserialize)]
struct Work;
impl Request for Work {
    const KEY: &'static str = "test.routine-work";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Evidence(PersonId);
impl Request for Evidence {
    const KEY: &'static str = "test.routine-evidence";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Signal(PersonId);
impl Request for Signal {
    const KEY: &'static str = "test.care-signal";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Fail;
impl Request for Fail {
    const KEY: &'static str = "test.routine-fail";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = support::builder();
    b.plugin("routine-test", 1).unwrap();
    b.resource::<Activity>().unwrap();
    b.request::<Work>(|tx, _| {
        let state = tx.edit::<Activity>()?;
        state.completed += 1;
        require(!state.fail, "activity no longer available")
    })
    .unwrap();
    b.request::<Evidence>(|tx, e| tx.notify(game_choice::Changed { person: e.0 }))
        .unwrap();
    b.request::<Signal>(|tx, r| {
        tx.notify(game_communication::Signalled {
            person: r.0,
            cue: "expressed-need".into(),
            intensity: 0,
        })
    })
    .unwrap();
    b.request::<Fail>(|tx, _| {
        tx.edit::<Activity>()?.fail = true;
        Ok(())
    })
    .unwrap();
    game_choice::serial_provider(&mut b, "test.routine-offer", |tx, e| {
        tx.edit::<Activity>()?.reviews += 1;
        game_choice::offer(
            tx,
            e.person,
            "routine",
            game_choice::motive(0, 1000, 0, 0, 0),
            game_choice::Undertake {
                person: e.person,
                kind: "routine-test".into(),
                duration: MINUTE,
                request: Pending::immediate(Work),
            },
        )?;
        Ok(())
    })
    .unwrap();
    b
}
fn world(enabled: bool) -> (Host, PersonId) {
    world_with_floor(enabled, None)
}
fn world_with_floor(enabled: bool, floor: Option<SimDuration>) -> (Host, PersonId) {
    world_with_builder(builder(), enabled, floor)
}
fn world_with_builder(b: Builder, enabled: bool, floor: Option<SimDuration>) -> (Host, PersonId) {
    world_with_policy(b, enabled, floor, false, false)
}
fn world_with_policy(
    b: Builder,
    enabled: bool,
    floor: Option<SimDuration>,
    preserve: bool,
    signal_changes: bool,
) -> (Host, PersonId) {
    let mut h = b.build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(Configure(Rules {
        care_signal_changes_only: signal_changes,
        routine: enabled.then(|| game_autonomy_policy::Policy {
            review_floor: floor,
            preserve_on_care_completion: preserve,
            minimum: SimDuration(10 * MINUTE.0),
            maximum: SimDuration(10 * MINUTE.0),
            kinds: [if floor.is_some() {
                "unselected"
            } else {
                "routine-test"
            }
            .into()]
            .into(),
        }),
        ..Default::default()
    }))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    if floor.is_some() {
        h.dispatch(game_physiology::Configure(game_physiology::Rules {
            tick: MINUTE,
            hunger_per_tick: 25,
            fatigue_per_tick: 0,
            deprivation: 10000,
            ..Default::default()
        }))
        .unwrap();
    }
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person, area }).unwrap();
    h.dispatch(Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(Start).unwrap();
    (h, person)
}
#[test]
fn opt_in_reuses_work_until_attention_expires_without_skipping_completions() {
    let (mut ordinary, _) = world(false);
    let (mut routine, person) = world(true);
    for h in [&mut ordinary, &mut routine] {
        support::run(h, SimTime(9 * MINUTE.0));
        assert_eq!(h.get::<Activity>().unwrap().completed, 8);
    }
    assert_eq!(routine.get::<Activity>().unwrap().reviews, 1);
    assert!(ordinary.get::<Activity>().unwrap().reviews > 1);
    let deadline = routine.get::<Minds>().unwrap().people[&person]
        .routine
        .as_ref()
        .unwrap()
        .until;
    support::run(&mut routine, deadline);
    assert_eq!(routine.get::<Activity>().unwrap().reviews, 2);
}
#[test]
fn cached_executables_replay_after_loading_with_other_workers_and_budgets() {
    let (mut one, _) = world(true);
    support::run(&mut one, SimTime(5 * MINUTE.0));
    let mut four = builder().load(&one.save().unwrap(), 4).unwrap();
    let target = SimTime(25 * MINUTE.0);
    while one.advance(target, 1).unwrap().stop == Stop::Budget {}
    while four.advance(target, 13).unwrap().stop == Stop::Budget {}
    assert_eq!(one.save().unwrap(), four.save().unwrap());
}
#[test]
fn new_evidence_injury_and_abandonment_invalidate_a_routine_immediately() {
    for cause in 0..3 {
        let (mut h, person) = world(true);
        support::run(&mut h, SimTime(4 * MINUTE.0));
        assert!(h.get::<Minds>().unwrap().people[&person].routine.is_some());
        match cause {
            0 => h.dispatch(Evidence(person)).unwrap(),
            1 => h
                .dispatch(game_physiology::Damage { person, amount: 1 })
                .unwrap(),
            _ => h.dispatch(game_characters::Abandon { person }).unwrap(),
        }
        assert!(h.get::<Minds>().unwrap().people[&person].routine.is_none());
        support::run(&mut h, SimTime(5 * MINUTE.0));
        assert!(h.get::<Activity>().unwrap().reviews > 1);
    }
}
#[test]
fn failed_completion_ends_reuse_and_rolls_back_the_failed_owner_effect() {
    let (mut h, person) = world(true);
    support::run(&mut h, SimTime(4 * MINUTE.0));
    let before = h.get::<Activity>().unwrap().completed;
    h.dispatch(Fail).unwrap();
    support::run(&mut h, SimTime(5 * MINUTE.0));
    assert_eq!(h.get::<Activity>().unwrap().completed, before);
    assert!(h.get::<Activity>().unwrap().reviews > 1);
    assert!(h.get::<Minds>().unwrap().people[&person].failures > 0);
}

#[test]
fn fatigue_forces_deliberation_even_without_an_external_wake_event() {
    let (mut h, person) = world(true);
    support::run(&mut h, SimTime(4 * MINUTE.0));
    h.dispatch(game_physiology::Strain {
        person,
        amount: 100,
    })
    .unwrap();
    assert!(h.get::<Minds>().unwrap().people[&person].routine.is_some());
    support::run(&mut h, SimTime(5 * MINUTE.0));
    assert!(h.get::<Activity>().unwrap().reviews > 1);
}

#[test]
fn review_floor_coalesces_evidence_wakes_and_injury_replaces_the_pending_deadline() {
    let (mut h, person) = world_with_floor(true, Some(SimDuration(5 * MINUTE.0)));
    support::run(&mut h, SimTime(3 * MINUTE.0));
    assert_eq!(h.get::<Activity>().unwrap().reviews, 1);
    for _ in 0..100 {
        h.dispatch(Evidence(person)).unwrap();
    }
    let save: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    assert_eq!(
        save["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|j| j["request"]["key"] == "characters.resume")
            .count(),
        1
    );
    h.dispatch(game_physiology::Damage { person, amount: 1 })
        .unwrap();
    support::run(&mut h, SimTime(3 * MINUTE.0));
    assert_eq!(h.get::<Activity>().unwrap().reviews, 2);
    support::run(&mut h, SimTime(6 * MINUTE.0));
    assert_eq!(
        h.get::<Activity>().unwrap().reviews,
        2,
        "the replaced callback must not wake early"
    );
}
#[test]
fn urgent_hunger_bypasses_a_deferred_nonurgent_review() {
    let (mut h, person) = world_with_floor(true, Some(SimDuration(5 * MINUTE.0)));
    support::run(&mut h, SimTime(3 * MINUTE.0));
    h.dispatch(game_physiology::Start).unwrap();
    support::run(&mut h, SimTime(4 * MINUTE.0));
    assert!(h.get::<game_physiology::Bodies>().unwrap().people[&person].hunger >= 100);
    assert!(h.get::<Activity>().unwrap().reviews > 1);
}
#[test]
fn deferred_reviews_survive_checkpoint_continuation_across_workers() {
    let (mut one, _) = world_with_floor(true, Some(SimDuration(5 * MINUTE.0)));
    support::run(&mut one, SimTime(3 * MINUTE.0));
    let mut four = builder().load(&one.save().unwrap(), 4).unwrap();
    let target = SimTime(25 * MINUTE.0);
    while one.advance(target, 1).unwrap().stop == Stop::Budget {}
    while four.advance(target, 13).unwrap().stop == Stop::Budget {}
    assert_eq!(one.save().unwrap(), four.save().unwrap());
}

#[test]
fn urgent_wakes_inside_a_full_review_do_not_queue_a_redundant_review() {
    let mut b = builder();
    game_choice::serial_provider(&mut b, "test.zz-interrupt", |tx, e| {
        if tx.get::<Activity>()?.reviews == 1 {
            tx.call(game_physiology::Damage {
                person: e.person,
                amount: 1,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let (mut h, _) = world_with_builder(b, true, Some(SimDuration(5 * MINUTE.0)));
    support::run(&mut h, SimTime(MINUTE.0));
    assert_eq!(h.get::<Activity>().unwrap().reviews, 1);
}

#[test]
fn injury_after_a_review_replaces_a_same_time_nonurgent_wake() {
    let (mut h, person) = world_with_floor(true, Some(SimDuration(5 * MINUTE.0)));
    support::run(&mut h, SimTime(MINUTE.0));
    h.dispatch(Evidence(person)).unwrap();
    h.dispatch(game_physiology::Damage { person, amount: 1 })
        .unwrap();
    support::run(&mut h, SimTime(MINUTE.0));
    assert_eq!(h.get::<Activity>().unwrap().reviews, 2);
}

fn care_builder() -> Builder {
    let mut b = builder();
    game_communication::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_households::plugin(&mut b).unwrap();
    game_health::plugin(&mut b).unwrap();
    game_care::plugin(&mut b).unwrap();
    b
}
fn care_world(preserve: bool) -> (Host, PersonId, PersonId, AreaId) {
    care_world_with_signals(preserve, false)
}
fn care_world_with_signals(
    preserve: bool,
    signal_changes: bool,
) -> (Host, PersonId, PersonId, AreaId) {
    let (mut h, person) = world_with_policy(care_builder(), true, None, preserve, signal_changes);
    let area = game_transport::area(h.get().unwrap(), person).unwrap();
    let neighbor = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place {
        person: neighbor,
        area,
    })
    .unwrap();
    for (person, name) in [(person, "actor"), (neighbor, "neighbor")] {
        h.dispatch(game_social_life::Register {
            person,
            name: name.into(),
        })
        .unwrap();
    }
    h.dispatch(game_households::Enable).unwrap();
    (h, person, neighbor, area)
}
#[test]
fn completion_policy_preserves_routines_but_actual_evidence_still_interrupts() {
    let (mut ordinary, _, _, _) = care_world(false);
    let (mut retained, person, _, _) = care_world(true);
    for h in [&mut ordinary, &mut retained] {
        support::run(h, SimTime(4 * MINUTE.0));
        assert_eq!(h.get::<Activity>().unwrap().completed, 3);
    }
    assert_eq!(retained.get::<Activity>().unwrap().reviews, 1);
    assert_eq!(ordinary.get::<Activity>().unwrap().reviews, 4);
    retained.dispatch(Evidence(person)).unwrap();
    assert!(
        retained.get::<Minds>().unwrap().people[&person]
            .routine
            .is_none()
    );
    support::run(&mut retained, SimTime(4 * MINUTE.0));
    assert_eq!(retained.get::<Activity>().unwrap().reviews, 2);
    let mut restored = care_builder().load(&retained.save().unwrap(), 4).unwrap();
    support::run(&mut retained, SimTime(15 * MINUTE.0));
    while restored.advance(SimTime(15 * MINUTE.0), 13).unwrap().stop == Stop::Budget {}
    assert_eq!(retained.save().unwrap(), restored.save().unwrap());
}
#[test]
fn an_existing_food_delivery_still_interrupts_on_completion() {
    let (mut h, person, neighbor, area) = care_world(true);
    support::run(&mut h, SimTime(4 * MINUTE.0));
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(person),
            capacity: 100,
        })
        .unwrap();
    h.dispatch(game_households::RememberFoodCare {
        caregiver: person,
        intent: Some(game_provisioning::FoodCare {
            beneficiary: neighbor,
            home: area,
            observed_need: 1,
            observed_at: h.now(),
            returning: false,
            plan: game_provisioning::Plan::Meal {
                inventory,
                good: None,
            },
        }),
    })
    .unwrap();
    support::run(&mut h, SimTime(5 * MINUTE.0));
    assert_eq!(h.get::<Activity>().unwrap().reviews, 2);
}

#[test]
fn unchanged_signals_record_perception_without_interrupting_but_active_delivery_still_wakes() {
    for filtered in [false, true] {
        let (mut h, person, neighbor, area) = care_world_with_signals(true, filtered);
        h.dispatch(game_communication::Enable).unwrap();
        support::run(&mut h, SimTime(4 * MINUTE.0));
        assert!(h.get::<Minds>().unwrap().people[&person].routine.is_some());
        h.dispatch(Signal(neighbor)).unwrap();
        assert_eq!(
            h.get::<Minds>().unwrap().people[&person].routine.is_some(),
            filtered
        );
        if !filtered {
            continue;
        }
        let inventory = h
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Carried(person),
                capacity: 100,
            })
            .unwrap();
        h.dispatch(game_households::RememberFoodCare {
            caregiver: person,
            intent: Some(game_provisioning::FoodCare {
                beneficiary: neighbor,
                home: area,
                observed_need: 1,
                observed_at: h.now(),
                returning: false,
                plan: game_provisioning::Plan::Meal {
                    inventory,
                    good: None,
                },
            }),
        })
        .unwrap();
        h.dispatch(Signal(neighbor)).unwrap();
        assert!(h.get::<Minds>().unwrap().people[&person].routine.is_none());
    }
}

#[test]
fn care_experiments_keep_working_without_the_optional_household_owner() {
    let mut b = builder();
    game_communication::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_health::plugin(&mut b).unwrap();
    game_care::plugin(&mut b).unwrap();
    let (mut h, person) = world_with_policy(b, true, None, true, true);
    h.dispatch(game_communication::Enable).unwrap();
    let area = game_transport::area(h.get().unwrap(), person).unwrap();
    let neighbor = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place {
        person: neighbor,
        area,
    })
    .unwrap();
    for (person, name) in [(person, "actor"), (neighbor, "neighbor")] {
        h.dispatch(game_social_life::Register {
            person,
            name: name.into(),
        })
        .unwrap();
    }
    support::run(&mut h, SimTime(4 * MINUTE.0));
    assert_eq!(h.get::<Activity>().unwrap().reviews, 1);
    assert_eq!(h.get::<Activity>().unwrap().completed, 3);
    h.dispatch(Signal(neighbor)).unwrap();
    assert!(h.get::<Minds>().unwrap().people[&person].routine.is_some());
}
