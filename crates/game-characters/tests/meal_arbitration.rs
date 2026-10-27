#[path = "local_phases/mod.rs"]
mod local_phases;
mod support;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
struct FoodOptions(PersonId);
impl Request for FoodOptions {
    const KEY: &'static str = "test.real-food-options";
    type Response = Vec<game_provisioning::Opportunity>;
}
#[derive(Clone, Serialize, Deserialize)]
struct KickFrame(SimTime);
impl Request for KickFrame {
    const KEY: &'static str = "test.kick-meal-frame";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cue(PersonId);
impl Request for Cue {
    const KEY: &'static str = "test.meal-evidence-cue";
    type Response = ();
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct WaveSeen {
    target: Option<PersonId>,
    hunger: Option<u64>,
}
impl Resource for WaveSeen {
    const KEY: &'static str = "test.selected-meal-wave-seen";
}
#[derive(Clone, Serialize, Deserialize)]
struct WatchWave(PersonId);
impl Request for WatchWave {
    const KEY: &'static str = "test.watch-meal-wave";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct CaptureWave;
impl Request for CaptureWave {
    const KEY: &'static str = "test.capture-meal-wave";
    type Response = ();
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct ReplanGate(bool);
impl Resource for ReplanGate {
    const KEY: &'static str = "test.replan-gate";
}
#[derive(Clone, Serialize, Deserialize)]
struct ArmReplan;
impl Request for ArmReplan {
    const KEY: &'static str = "test.arm-replan";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct ExertOnce(PersonId);
impl Request for ExertOnce {
    const KEY: &'static str = "test.exert-once";
    type Response = ();
}

fn wave_builder() -> Builder {
    let mut b = builder();
    b.resource::<WaveSeen>().unwrap();
    b.request::<WatchWave>(|tx, r| {
        tx.edit::<WaveSeen>()?.target = Some(r.0);
        Ok(())
    })
    .unwrap();
    b.request::<CaptureWave>(|tx, _| {
        let state = tx.get::<WaveSeen>()?;
        if let (Some(person), None) = (state.target, state.hunger) {
            let hunger = tx.get::<game_physiology::Bodies>()?.people[&person].hunger;
            tx.edit::<WaveSeen>()?.hunger = Some(hunger);
        }
        Ok(())
    })
    .unwrap();
    b.subscribe::<game_physiology::Fed>("test.capture-meal-wave", |tx, _| tx.call(CaptureWave))
        .unwrap();
    b
}

fn builder() -> Builder {
    builder_with_fixed_provider(false)
}

fn builder_with_fixed_provider(always_offer: bool) -> Builder {
    let mut b = support::builder();
    b.plugin("meal-arbitration-test", 1).unwrap();
    b.request::<KickFrame>(|tx, request| game_decision_frames::wake(tx, request.0))
        .unwrap();
    b.request::<Cue>(|tx, request| tx.notify(game_choice::Changed { person: request.0 }))
        .unwrap();
    game_choice::provider(&mut b, "test.selected-local-meal", move |tx, event| {
        let inventory = tx
            .get::<game_cargo::Cargo>()?
            .0
            .iter()
            .find(|(_, inv)| always_offer || inv.amount(game_cargo::FOOD) >= 10)
            .map(|(&id, _)| id);
        if let Some(inventory) = inventory {
            game_choice::offer(
                tx,
                event.person,
                "food:fixture",
                game_choice::motive(1_000_000_000, 0, 0, 0, 0),
                game_decision_intents::Proceed {
                    person: event.person,
                    plan: game_provisioning::Plan::Meal {
                        inventory,
                        good: Some(game_cargo::FOOD),
                    },
                },
            )?;
        }
        Ok(())
    })
    .unwrap();
    b.subscribe::<game_decision_frames::Gather>("test.meal-cohort", |tx, _| {
        tx.call(game_decision_frames::Batch {
            people: tx
                .get::<game_characters::Minds>()?
                .people
                .keys()
                .copied()
                .collect(),
        })
    })
    .unwrap();
    b
}

fn prepared(arbitrate: bool) -> (Host, Vec<PersonId>, InventoryId) {
    prepared_with(builder(), arbitrate)
}

fn local_tuning(host: &mut Host) {
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.local_interactions = true;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
}

#[test]
fn local_interactions_settle_all_personal_effects_before_observation_and_replay() {
    let (mut initial, people, store) = prepared_many(wave_builder(), true, 128, None, 8, 0, 1);
    initial
        .dispatch(game_cargo::Deposit {
            inventory: store,
            good: game_cargo::FOOD,
            quantity: 70,
        })
        .unwrap();
    for &person in &people {
        initial
            .dispatch(game_physiology::Exert {
                person,
                energy: 40_000,
                fatigue: 0,
            })
            .unwrap();
    }
    initial
        .dispatch(WatchWave(*people.last().unwrap()))
        .unwrap();
    local_tuning(&mut initial);
    let saved = initial.save().unwrap();
    let mut reference = None;
    for workers in [1, 4, 12] {
        let mut host = wave_builder().load(&saved, workers).unwrap();
        support::run(&mut host, SimTime(30_000));
        assert_eq!(
            host.notices()
                .filter(|n| n.kind == game_physiology::Fed::KEY)
                .count(),
            8
        );
        assert_eq!(
            host.get::<WaveSeen>().unwrap().hunger,
            Some(
                host.get::<game_physiology::Bodies>().unwrap().people[people.last().unwrap()]
                    .hunger
            )
        );
        assert!(
            host.get::<game_characters::Minds>()
                .unwrap()
                .paused_reviews
                .is_empty()
        );
        let after = host.save().unwrap();
        if let Some(expected) = &reference {
            assert_eq!(&after, expected);
        } else {
            reference = Some(after);
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct LocalCareTargets(
    std::collections::BTreeMap<PersonId, PersonId>,
    Vec<Vec<(PersonId, PersonId)>>,
);
impl Resource for LocalCareTargets {
    const KEY: &'static str = "test.local-care-targets";
}
#[derive(Clone, Serialize, Deserialize)]
struct LocalCareTarget(PersonId, PersonId);
impl Request for LocalCareTarget {
    const KEY: &'static str = "test.local-care-target";
    type Response = ();
}
fn local_care_builder() -> Builder {
    let mut b = wave_builder();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_households::plugin(&mut b).unwrap();
    game_family_life::plugin(&mut b).unwrap();
    game_communication::plugin(&mut b).unwrap();
    game_health::plugin(&mut b).unwrap();
    game_care::plugin(&mut b).unwrap();
    b.plugin("local-care-test", 1).unwrap();
    b.resource::<LocalCareTargets>().unwrap();
    b.before::<game_physiology::ServeLocalMeals>("test.capture-local-participants", |tx, r| {
        tx.edit::<LocalCareTargets>()?
            .1
            .push(r.meals.iter().map(|m| (m.caregiver, m.patient)).collect());
        Ok(())
    })
    .unwrap();
    b.request::<LocalCareTarget>(|tx, r| {
        tx.edit::<LocalCareTargets>()?.0.insert(r.0, r.1);
        Ok(())
    })
    .unwrap();
    game_choice::provider(&mut b, "test.local-care", |tx, event| {
        if let Some(&patient) = tx.get::<LocalCareTargets>()?.0.get(&event.person) {
            game_choice::offer(
                tx,
                event.person,
                "care:fixture",
                game_choice::motive(2_000_000_000, 0, 0, 0, 0),
                game_care::Assist {
                    caregiver: event.person,
                    beneficiary: patient,
                },
            )?;
        }
        Ok(())
    })
    .unwrap();
    b
}

fn prepared_local_care() -> (Host, Vec<PersonId>, InventoryId, Vec<PersonId>) {
    prepared_local_care_with(local_care_builder())
}
fn prepared_local_care_with(builder: Builder) -> (Host, Vec<PersonId>, InventoryId, Vec<PersonId>) {
    let (mut initial, people, store) = prepared_many(builder, true, 128, None, 3, 0, 1);
    initial
        .dispatch(game_traits::Install(
            game_traits::load(&mut game_content::Store::standard()).unwrap(),
        ))
        .unwrap();
    initial.dispatch(game_households::Enable).unwrap();
    initial
        .dispatch(game_cargo::Deposit {
            inventory: store,
            good: game_cargo::FOOD,
            quantity: 20,
        })
        .unwrap();
    let area = game_transport::area(initial.get().unwrap(), people[0]).unwrap();
    let mut patients = Vec::new();
    for &caregiver in &people[..2] {
        let patient = initial
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        initial
            .dispatch(game_transport::Place {
                person: patient,
                area,
            })
            .unwrap();
        initial
            .dispatch(game_physiology::Exert {
                person: patient,
                energy: 80_000,
                fatigue: 0,
            })
            .unwrap();
        initial
            .dispatch(game_traits::Set {
                person: caregiver,
                profile: [
                    ("altruism".into(), 1000),
                    ("empathy".into(), 1000),
                    ("compassion".into(), 1000),
                ]
                .into(),
            })
            .unwrap();
        initial
            .dispatch(LocalCareTarget(caregiver, patient))
            .unwrap();
        patients.push(patient);
    }
    initial
        .dispatch(game_physiology::Exert {
            person: people[2],
            energy: 40_000,
            fatigue: 0,
        })
        .unwrap();
    initial.dispatch(WatchWave(patients[1])).unwrap();
    local_tuning(&mut initial);
    (initial, people, store, patients)
}

#[test]
fn local_care_and_self_meal_share_real_stock_and_owner_batch() {
    let (initial, _, store, patients) = prepared_local_care();
    let saved = initial.save().unwrap();
    let mut reference = None;
    for workers in [1, 4, 12] {
        let mut host = local_care_builder().load(&saved, workers).unwrap();
        support::run(&mut host, SimTime(30_000));
        assert_eq!(host.get::<LocalCareTargets>().unwrap().1[0].len(), 3);
        let care: Vec<_> = host
            .notices()
            .filter(|n| {
                n.kind == game_physiology::Fed::KEY && n.data["caregiver"] != n.data["patient"]
            })
            .collect();
        assert_eq!(
            care.len(),
            2,
            "both actual care selections should participate"
        );
        assert_eq!(
            host.get::<WaveSeen>().unwrap().hunger,
            Some(host.get::<game_physiology::Bodies>().unwrap().people[&patients[1]].hunger),
            "care must use the settled owner batch"
        );
        assert!(
            host.get::<game_characters::Minds>()
                .unwrap()
                .paused_reviews
                .is_empty()
        );
        assert_eq!(
            host.get::<game_cargo::Cargo>().unwrap().0[&store].amount(game_cargo::FOOD),
            0
        );
        for p in &patients {
            assert!(host.get::<game_physiology::Bodies>().unwrap().people[p].hunger < 80);
        }
        let after = host.save().unwrap();
        if let Some(expected) = &reference {
            assert_eq!(&after, expected);
        } else {
            reference = Some(after);
        }
    }
}

#[test]
fn local_care_arbitrates_a_shared_patient_before_owner_execution() {
    let (mut initial, people, _, patients) = prepared_local_care();
    initial
        .dispatch(LocalCareTarget(people[1], patients[0]))
        .unwrap();
    let saved = initial.save().unwrap();
    let mut reference = None;
    for workers in [1, 4] {
        let mut host = local_care_builder().load(&saved, workers).unwrap();
        support::run(&mut host, SimTime(30_000));
        let batches = &host.get::<LocalCareTargets>().unwrap().1;
        assert_eq!(batches[0].len(), 2, "one care and one self-meal should win");
        assert_eq!(
            batches[0].iter().filter(|(_, p)| *p == patients[0]).count(),
            1
        );
        for batch in batches {
            let mut claimed = std::collections::BTreeSet::new();
            for &(actor, patient) in batch {
                assert!(claimed.insert(actor));
                if actor != patient {
                    assert!(claimed.insert(patient));
                }
            }
        }
        let after = host.save().unwrap();
        if let Some(expected) = &reference {
            assert_eq!(&after, expected);
        } else {
            reference = Some(after);
        }
    }
}

#[test]
fn local_care_revalidates_personal_knowledge_after_all_serve_hooks() {
    let mut b = local_care_builder();
    b.before::<game_physiology::Serve>("zzzz.test.changed-personal-knowledge", |tx, meal| {
        if meal.caregiver != meal.patient {
            let area = tx.call(game_land::CreateArea { name: "hook-misremembered".into() })?;
            tx.call(game_knowledge::Conclude {
                person: meal.caregiver, key: game_knowledge::location_key(meal.inventory),
                value: serde_json::json!({"area": area.0, "capacity": 1000, "carrier": null, "mass": 0}),
                confidence: 100,
            })?;
        }
        Ok(())
    }).unwrap();
    let (mut host, _, _, _) = prepared_local_care_with(b);
    support::run(&mut host, SimTime(30_000));
    assert!(
        host.get::<LocalCareTargets>().unwrap().1.is_empty(),
        "the invalidated batch and its admission record must roll back before scalar retry"
    );
    assert!(
        host.get::<game_characters::Minds>()
            .unwrap()
            .paused_reviews
            .is_empty()
    );
    host.save().unwrap();
}

#[test]
fn local_interaction_urgent_hook_rolls_back_before_ordered_retry() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let victim = Arc::new(AtomicU64::new(0));
    let target = victim.clone();
    let mut b = builder();
    b.subscribe::<game_physiology::Fed>("test.local-wave-urgent", move |tx, e| {
        if e.patient.0 != target.load(Ordering::Relaxed) {
            tx.call(game_physiology::Damage {
                person: PersonId(target.load(Ordering::Relaxed)),
                amount: 1,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let (mut host, people, store) = prepared_with(b, true);
    victim.store(people[0].0, Ordering::Relaxed);
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    local_tuning(&mut host);
    let before = host.get::<game_physiology::Bodies>().unwrap().people[&people[0]].health;
    support::run(&mut host, SimTime(30_000));
    assert_eq!(
        host.get::<game_physiology::Bodies>().unwrap().people[&people[0]].health,
        before - 1
    );
    assert_eq!(
        host.notices()
            .filter(|n| n.kind == game_physiology::Damaged::KEY)
            .count(),
        1
    );
    assert!(
        host.get::<game_characters::Minds>()
            .unwrap()
            .paused_reviews
            .is_empty()
    );
    host.save().unwrap();
}

fn prepared_with(builder: Builder, arbitrate: bool) -> (Host, Vec<PersonId>, InventoryId) {
    prepared_with_options(builder, arbitrate, 64, None)
}

fn prepared_with_options(
    builder: Builder,
    arbitrate: bool,
    batch: usize,
    hungry: Option<usize>,
) -> (Host, Vec<PersonId>, InventoryId) {
    prepared_many(builder, arbitrate, batch, hungry, 2, 1, 1)
}

fn prepared_many(
    builder: Builder,
    arbitrate: bool,
    batch: usize,
    hungry: Option<usize>,
    count: usize,
    round: u64,
    slot: u64,
) -> (Host, Vec<PersonId>, InventoryId) {
    let mut host = builder.build(3, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "shared pantry".into(),
        })
        .unwrap();
    let store = host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: 1000,
        })
        .unwrap();
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 10,
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
        people.push(person);
    }
    if let Some(index) = hungry {
        host.dispatch(game_physiology::Exert {
            person: people[index],
            energy: 1_000_000_000,
            fatigue: 0,
        })
        .unwrap();
    }
    // A nonzero round tests rotating precedence rather than fixed ID order.
    let at = SimTime(slot * 30_000);
    support::run(&mut host, at);
    for &person in &people {
        host.dispatch(game_characters::Register {
            person,
            mind: Default::default(),
        })
        .unwrap();
        host.dispatch(game_knowledge::ObserveStore {
            person,
            inventory: store,
        })
        .unwrap();
    }
    host.dispatch(game_characters::Start).unwrap();
    host.dispatch(game_choice::ConfigureFrames {
        interval: Some(SimDuration(30_000)),
    })
    .unwrap();
    host.dispatch(game_choice::ConfigureFrameDemand(true))
        .unwrap();
    host.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
        batch_decisions: true,
        recheck_conflicts: true,
        commit_batch: batch,
        meal_arbitration: arbitrate,
        ..Default::default()
    }))
    .unwrap();
    if arbitrate {
        for _ in 0..round {
            host.dispatch(game_choice::NextMealRound).unwrap();
        }
    }
    host.advance(at, 1).unwrap();
    assert!(
        host.scheduled()
            .any(|job| job.request.key() == "characters.resolve-frame")
    );
    (host, people, store)
}

#[test]
fn selected_meals_use_fair_arbitration_and_replay_across_workers_and_reload() {
    let (initial, people, store) = prepared(true);
    let saved = initial.save().unwrap();
    let mut worlds = Vec::new();
    for workers in [1, 4, 12] {
        let mut host = builder().load(&saved, workers).unwrap();
        support::run(&mut host, SimTime(30_000));
        assert_eq!(
            host.get::<game_cargo::Cargo>().unwrap().0[&store].amount(game_cargo::FOOD),
            0
        );
        let fed: Vec<_> = host
            .notices()
            .filter(|n| n.kind == game_physiology::Fed::KEY)
            .collect();
        assert_eq!(fed.len(), 1);
        assert_eq!(fed[0].data["patient"].as_u64(), Some(people[1].0));
        assert_eq!(
            host.get::<game_characters::Minds>().unwrap().people[&people[0]].failures,
            0
        );
        assert!(
            host.get::<game_characters::Minds>()
                .unwrap()
                .paused_reviews
                .is_empty()
        );
        worlds.push(host.save().unwrap());
    }
    assert!(worlds.windows(2).all(|pair| pair[0] == pair[1]));
    let (mut baseline, people, _) = prepared(false);
    support::run(&mut baseline, SimTime(30_000));
    let fed = baseline
        .notices()
        .find(|n| n.kind == game_physiology::Fed::KEY)
        .unwrap();
    assert_eq!(fed.data["patient"].as_u64(), Some(people[0].0));
}

#[test]
fn meal_wave_requires_arbitration_and_rolls_back_invalid_tuning() {
    let (mut host, _, _) = prepared(true);
    let before = host.save().unwrap();
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.meal_arbitration = false;
    tuning.meal_wave = true;
    assert!(host.dispatch(game_choice::TuneFrames(tuning)).is_err());
    assert_eq!(host.save().unwrap(), before);
}

#[test]
fn admitted_selected_meals_settle_together_and_replay_after_reload() {
    let (mut initial, people, store) = prepared_with(wave_builder(), true);
    initial
        .dispatch(game_cargo::Deposit {
            inventory: store,
            good: game_cargo::FOOD,
            quantity: 10,
        })
        .unwrap();
    for &person in &people {
        initial
            .dispatch(game_physiology::Exert {
                person,
                energy: 40_000,
                fatigue: 0,
            })
            .unwrap();
    }
    initial.dispatch(WatchWave(people[0])).unwrap();
    let mut tuning = initial
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.meal_wave = true;
    initial.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    let saved = initial.save().unwrap();
    let mut reference = None;
    for workers in [1, 4, 12] {
        let mut host = wave_builder().load(&saved, workers).unwrap();
        support::run(&mut host, SimTime(30_000));
        assert_eq!(
            host.get::<game_cargo::Cargo>().unwrap().0[&store].amount(game_cargo::FOOD),
            0
        );
        assert_eq!(
            host.notices()
                .filter(|n| n.kind == game_physiology::Fed::KEY)
                .count(),
            2
        );
        assert_eq!(
            host.get::<WaveSeen>().unwrap().hunger,
            Some(host.get::<game_physiology::Bodies>().unwrap().people[&people[0]].hunger)
        );
        assert!(
            host.get::<game_characters::Minds>()
                .unwrap()
                .paused_reviews
                .is_empty()
        );
        let after = host.save().unwrap();
        if let Some(expected) = &reference {
            assert_eq!(&after, expected);
        } else {
            reference = Some(after.clone());
        }
        assert_eq!(
            wave_builder().load(&after, 4).unwrap().save().unwrap(),
            after
        );
    }
}

#[test]
fn large_selected_meal_wave_matches_across_workers() {
    let (mut initial, people, store) = prepared_many(wave_builder(), true, 128, None, 64, 0, 1);
    initial
        .dispatch(game_cargo::Deposit {
            inventory: store,
            good: game_cargo::FOOD,
            quantity: 630,
        })
        .unwrap();
    for &person in &people {
        initial
            .dispatch(game_physiology::Exert {
                person,
                energy: 40_000,
                fatigue: 0,
            })
            .unwrap();
    }
    initial
        .dispatch(WatchWave(*people.last().unwrap()))
        .unwrap();
    let mut tuning = initial
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.meal_wave = true;
    initial.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    let saved = initial.save().unwrap();
    let mut reference = None;
    for workers in [1, 4, 12] {
        let mut host = wave_builder().load(&saved, workers).unwrap();
        support::run(&mut host, SimTime(30_000));
        assert_eq!(
            host.notices()
                .filter(|n| n.kind == game_physiology::Fed::KEY)
                .count(),
            64
        );
        assert_eq!(
            host.get::<WaveSeen>().unwrap().hunger,
            Some(
                host.get::<game_physiology::Bodies>().unwrap().people[people.last().unwrap()]
                    .hunger
            )
        );
        assert_eq!(
            host.get::<game_cargo::Cargo>().unwrap().0[&store].amount(game_cargo::FOOD),
            0
        );
        let after = host.save().unwrap();
        if let Some(expected) = &reference {
            assert_eq!(&after, expected);
        } else {
            reference = Some(after);
        }
    }
}

#[test]
fn urgent_cue_during_wave_rolls_back_then_uses_ordered_reviews() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let winner = Arc::new(AtomicU64::new(0));
    let victim = Arc::new(AtomicU64::new(0));
    let mut b = builder();
    let a = winner.clone();
    let v = victim.clone();
    b.subscribe::<game_physiology::Fed>("test.wave-urgent", move |tx, event| {
        if event.patient.0 == a.load(Ordering::Relaxed) {
            tx.call(game_physiology::Damage {
                person: PersonId(v.load(Ordering::Relaxed)),
                amount: 1,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let (mut host, people, store) = prepared_with(b, true);
    winner.store(people[1].0, Ordering::Relaxed);
    victim.store(people[0].0, Ordering::Relaxed);
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.meal_wave = true;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    let before = host.get::<game_physiology::Bodies>().unwrap().people[&people[0]].health;
    support::run(&mut host, SimTime(30_000));
    assert_eq!(
        host.get::<game_physiology::Bodies>().unwrap().people[&people[0]].health,
        before - 1
    );
    assert_eq!(
        host.notices()
            .filter(|n| n.kind == game_physiology::Damaged::KEY)
            .count(),
        1
    );
    assert!(
        host.get::<game_characters::Minds>()
            .unwrap()
            .paused_reviews
            .is_empty()
    );
    host.save().unwrap();
}

#[test]
fn selected_meal_wave_preserves_proceed_hooks_by_falling_back() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let hits = calls.clone();
    let mut b = builder();
    b.before::<game_decision_intents::Proceed>("test.proceed-hook", move |_, _| {
        hits.fetch_add(1, Ordering::Relaxed);
        Ok(())
    })
    .unwrap();
    let (mut host, _, store) = prepared_with(b, true);
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.meal_wave = true;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    support::run(&mut host, SimTime(30_000));
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert_eq!(
        host.notices()
            .filter(|n| n.kind == game_physiology::Fed::KEY)
            .count(),
        2
    );
}

#[test]
fn finish_review_hook_observes_the_unfed_state() {
    let mut b = wave_builder();
    b.before::<game_character_clock::FinishPausedReview>("test.finish-before-meal", |tx, _| {
        tx.call(CaptureWave)
    })
    .unwrap();
    let (mut host, people, store) = prepared_with(b, true);
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    host.dispatch(game_physiology::Exert {
        person: people[0],
        energy: 40_000,
        fatigue: 0,
    })
    .unwrap();
    host.dispatch(WatchWave(people[0])).unwrap();
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.meal_wave = true;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    support::run(&mut host, SimTime(30_000));
    let after = host.get::<game_physiology::Bodies>().unwrap().people[&people[0]].hunger;
    assert!(host.get::<WaveSeen>().unwrap().hunger.unwrap() > after);
}

#[test]
fn wave_rechecks_an_earlier_claim_after_later_actor_replanning() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let ids: Vec<_> = (0..3).map(|_| Arc::new(AtomicU64::new(0))).collect();
    let mut b = builder();
    b.resource::<ReplanGate>().unwrap();
    b.request::<ArmReplan>(|tx, _| {
        tx.edit::<ReplanGate>()?.0 = true;
        Ok(())
    })
    .unwrap();
    b.request::<ExertOnce>(|tx, r| {
        if tx.get::<ReplanGate>()?.0 {
            tx.edit::<ReplanGate>()?.0 = false;
            tx.call(game_physiology::Exert {
                person: r.0,
                energy: 1_000,
                fatigue: 0,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let bad = ids[1].clone();
    let trigger = ids[2].clone();
    b.before::<game_character_clock::PauseReview>("test.arm-late-replan", move |tx, r| {
        if r.0.0 == trigger.load(Ordering::Relaxed) {
            tx.call(game_physiology::Damage {
                person: PersonId(bad.load(Ordering::Relaxed)),
                amount: 1,
            })?;
            tx.call(ArmReplan)?;
        }
        Ok(())
    })
    .unwrap();
    let bad = ids[1].clone();
    let earlier = ids[0].clone();
    b.subscribe::<game_choice::Review>("test.replan-changes-earlier", move |tx, event| {
        if event.person.0 == bad.load(Ordering::Relaxed) && tx.get::<ReplanGate>()?.0 {
            tx.call(ExertOnce(PersonId(earlier.load(Ordering::Relaxed))))?;
        }
        Ok(())
    })
    .unwrap();
    let (mut host, people, store) = prepared_many(b, true, 64, None, 3, 0, 1);
    for (slot, person) in ids.iter().zip(&people) {
        slot.store(person.0, Ordering::Relaxed);
    }
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 20,
    })
    .unwrap();
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.meal_wave = true;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    support::run(&mut host, SimTime(30_000));
    assert!(!host.get::<ReplanGate>().unwrap().0);
    assert!(
        !host
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .people
            .contains(&people[0])
    );
    assert!(
        host.get::<game_characters::Minds>()
            .unwrap()
            .paused_reviews
            .is_empty()
    );
}

#[test]
fn wave_rechecks_personal_store_location_before_each_meal() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let winner = Arc::new(AtomicU64::new(0));
    let victim = Arc::new(AtomicU64::new(0));
    let store = Arc::new(AtomicU64::new(0));
    let away = Arc::new(AtomicU64::new(0));
    let mut b = builder();
    let (a, v, s, destination) = (winner.clone(), victim.clone(), store.clone(), away.clone());
    b.subscribe::<game_physiology::Eating>("test.invalidate-meal-knowledge", move |tx, event| {
        if event.person.0 == a.load(Ordering::Relaxed) {
            let inventory = InventoryId(s.load(Ordering::Relaxed));
            tx.call(game_knowledge::Conclude {
                person: PersonId(v.load(Ordering::Relaxed)),
                key: game_knowledge::location_key(inventory),
                value: serde_json::json!({"area": destination.load(Ordering::Relaxed), "capacity": 1000, "carrier": null, "mass": 0}),
                confidence: 100,
            })?;
        }
        Ok(())
    }).unwrap();
    let (mut host, people, inventory) = prepared_with(b, true);
    let distant = host
        .dispatch(game_land::CreateArea {
            name: "mistaken store location".into(),
        })
        .unwrap();
    winner.store(people[1].0, Ordering::Relaxed);
    victim.store(people[0].0, Ordering::Relaxed);
    store.store(inventory.0, Ordering::Relaxed);
    away.store(distant.0, Ordering::Relaxed);
    host.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.meal_wave = true;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    support::run(&mut host, SimTime(30_000));
    let fed: Vec<_> = host
        .notices()
        .filter(|n| n.kind == game_physiology::Fed::KEY)
        .collect();
    assert_eq!(fed.len(), 1);
    assert_eq!(fed[0].data["patient"].as_u64(), Some(people[1].0));
    assert_eq!(
        host.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::FOOD),
        10
    );
    assert!(
        !host
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .people
            .contains(&people[0])
    );
    host.save().unwrap();
}

#[test]
fn urgent_injury_invalidates_an_already_selected_meal_before_it_can_resume() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder();
    let first = Arc::new(AtomicU64::new(0));
    let second = Arc::new(AtomicU64::new(0));
    let victim = first.clone();
    let winner = second.clone();
    b.before::<game_physiology::ConsumeEdible>("test.injure-paused-review", move |tx, request| {
        if request.person.0 == winner.load(Ordering::Relaxed) {
            tx.call(game_physiology::Damage {
                person: PersonId(victim.load(Ordering::Relaxed)),
                amount: 1,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let (mut host, people, store) = prepared_with(b, true);
    first.store(people[0].0, Ordering::Relaxed);
    second.store(people[1].0, Ordering::Relaxed);
    let health = host.get::<game_physiology::Bodies>().unwrap().people[&people[0]].health;
    support::run(&mut host, SimTime(30_000));
    assert_eq!(
        host.get::<game_cargo::Cargo>().unwrap().0[&store].amount(game_cargo::FOOD),
        0
    );
    assert_eq!(
        host.get::<game_physiology::Bodies>().unwrap().people[&people[0]].health,
        health - 1
    );
    assert!(
        !host
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .people
            .contains(&people[0])
    );
    assert!(
        host.get::<game_characters::Minds>()
            .unwrap()
            .paused_reviews
            .is_empty()
    );
}

#[test]
fn urgent_cue_preempts_later_bounded_groups_at_the_same_time() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder();
    let target = Arc::new(AtomicU64::new(0));
    let winner = Arc::new(AtomicU64::new(0));
    let target_id = target.clone();
    let winner_id = winner.clone();
    b.before::<game_physiology::ConsumeEdible>("test.injure-later-group", move |tx, request| {
        if request.person.0 == winner_id.load(Ordering::Relaxed) {
            tx.call(game_physiology::Damage {
                person: PersonId(target_id.load(Ordering::Relaxed)),
                amount: 1,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let (mut host, people, _) = prepared_with_options(b, true, 1, None);
    target.store(people[0].0, Ordering::Relaxed);
    winner.store(people[1].0, Ordering::Relaxed);
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.urgent_immediate = true;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    host.advance(SimTime(30_000), 1).unwrap();
    let urgent = host
        .scheduled()
        .find(|job| job.request.key() == "characters.urgent-frame-resume")
        .unwrap();
    assert_eq!(urgent.priority, 48);
    assert!(
        host.scheduled()
            .any(|job| job.request.key() == "characters.resolve-frame")
    );
    host.advance(SimTime(30_000), 1).unwrap();
    assert!(
        !host
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .people
            .contains(&people[0])
    );
    support::run(&mut host, SimTime(30_000));
}

#[test]
fn urgent_cue_preempts_a_later_group_even_when_its_wake_is_already_due() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder();
    let target = Arc::new(AtomicU64::new(0));
    let winner = Arc::new(AtomicU64::new(0));
    let target_id = target.clone();
    let winner_id = winner.clone();
    b.before::<game_physiology::ConsumeEdible>("test.injure-due-group", move |tx, request| {
        if request.person.0 == winner_id.load(Ordering::Relaxed) {
            tx.call(game_physiology::Damage {
                person: PersonId(target_id.load(Ordering::Relaxed)),
                amount: 1,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let (mut host, people, store) = prepared_many(b, true, 1, None, 2, 0, 0);
    target.store(people[0].0, Ordering::Relaxed);
    winner.store(people[1].0, Ordering::Relaxed);
    support::run(&mut host, SimTime(0));
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    let mut tuning = host
        .get::<game_choice::Offers>()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .tuning;
    tuning.urgent_immediate = true;
    host.dispatch(game_choice::TuneFrames(tuning)).unwrap();
    host.dispatch(Cue(people[0])).unwrap();
    assert_eq!(
        host.get::<game_characters::Minds>()
            .unwrap()
            .schedule
            .waking
            .get(&people[0]),
        Some(&SimTime(30_000))
    );
    host.advance(SimTime(30_000), 1).unwrap();
    host.advance(SimTime(30_000), 1).unwrap();
    let urgent = host
        .scheduled()
        .find(|job| job.request.key() == "characters.urgent-frame-resume")
        .unwrap();
    assert_eq!(urgent.priority, 48);
    assert!(
        !host
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .people
            .contains(&people[0])
    );
    host.advance(SimTime(30_000), 1).unwrap();
    support::run(&mut host, SimTime(30_000));
    assert!(
        host.scheduled()
            .all(|job| job.request.key() != "characters.urgent-frame-resume")
    );
    assert_eq!(
        host.notices()
            .filter(|n| n.kind == game_physiology::Fed::KEY)
            .count(),
        2
    );
}

#[test]
fn first_urgent_review_marks_its_clock_before_the_same_time_frame() {
    let mut host = builder().build(3, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "new actor".into(),
        })
        .unwrap();
    let store = host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: 100,
        })
        .unwrap();
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    let person = host
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    host.dispatch(game_transport::Place { person, area })
        .unwrap();
    host.dispatch(game_characters::Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    host.dispatch(game_knowledge::ObserveStore {
        person,
        inventory: store,
    })
    .unwrap();
    host.dispatch(game_characters::Start).unwrap();
    host.dispatch(game_choice::ConfigureFrames {
        interval: Some(SimDuration(30_000)),
    })
    .unwrap();
    host.dispatch(game_choice::ConfigureFrameDemand(true))
        .unwrap();
    host.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
        batch_decisions: true,
        commit_batch: 64,
        meal_arbitration: true,
        urgent_immediate: true,
        recover_idle: true,
        ..Default::default()
    }))
    .unwrap();
    assert!(
        host.get::<game_characters::Minds>().unwrap().people[&person]
            .review_clock
            .is_none()
    );
    host.dispatch(game_physiology::Damage { person, amount: 1 })
        .unwrap();
    assert!(
        host.scheduled()
            .any(|job| job.request.key() == "characters.urgent-frame-resume" && job.priority == 48)
    );
    host.advance(SimTime(0), 1).unwrap();
    assert_eq!(
        host.get::<game_characters::Minds>().unwrap().people[&person]
            .review_clock
            .unwrap()
            .reviewed,
        Some(SimTime(0))
    );
    support::run(&mut host, SimTime(0));
    assert_eq!(
        host.notices()
            .filter(|notice| notice.kind == game_physiology::Fed::KEY)
            .count(),
        1
    );
    assert_eq!(
        host.get::<game_characters::Minds>().unwrap().people[&person].failures,
        0
    );
}

#[test]
fn resume_hooks_fall_back_to_ordered_resolution_and_roll_back_the_group() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder();
    let reject = Arc::new(AtomicU64::new(0));
    let blocked = reject.clone();
    b.before::<game_characters::Resume>("test.reject-resume", move |_, request| {
        require(
            request.person.0 != blocked.load(Ordering::Relaxed),
            "injected resume failure",
        )
    })
    .unwrap();
    let (mut host, people, _) = prepared_with(b, true);
    let before = host.save().unwrap();
    reject.store(people[1].0, Ordering::Relaxed);
    assert!(host.advance(SimTime(30_000), 1).is_err());
    assert_eq!(host.save().unwrap(), before);
}

#[test]
fn a_new_joint_action_supersedes_a_paused_meal() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder();
    let target = Arc::new(AtomicU64::new(0));
    let winner = Arc::new(AtomicU64::new(0));
    let target_id = target.clone();
    let winner_id = winner.clone();
    b.before::<game_physiology::ConsumeEdible>("test.joint-action", move |tx, request| {
        if request.person.0 == winner_id.load(Ordering::Relaxed) {
            tx.call(game_choice::Undertake {
                person: PersonId(target_id.load(Ordering::Relaxed)),
                kind: "joint".into(),
                duration: HOUR,
                request: Pending::new(game_choice::Wait)?,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let (mut host, people, _) = prepared_with(b, true);
    target.store(people[0].0, Ordering::Relaxed);
    winner.store(people[1].0, Ordering::Relaxed);
    support::run(&mut host, SimTime(30_000));
    assert_eq!(host.get::<Actions>().unwrap().0[&people[0]].kind, "joint");
    assert_eq!(
        host.get::<game_characters::Minds>().unwrap().people[&people[0]].failures,
        0
    );
    assert!(
        host.get::<game_characters::Minds>()
            .unwrap()
            .paused_reviews
            .is_empty()
    );
}

#[test]
fn player_control_acquired_during_a_batch_prevents_autonomous_resumption() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder();
    let target = Arc::new(AtomicU64::new(0));
    let winner = Arc::new(AtomicU64::new(0));
    let target_id = target.clone();
    let winner_id = winner.clone();
    b.before::<game_physiology::ConsumeEdible>("test.take-control", move |tx, request| {
        if request.person.0 == winner_id.load(Ordering::Relaxed) {
            tx.call(game_lineage::Begin {
                person: PersonId(target_id.load(Ordering::Relaxed)),
            })?;
        }
        Ok(())
    })
    .unwrap();
    let (mut host, people, _) = prepared_with(b, true);
    target.store(people[0].0, Ordering::Relaxed);
    winner.store(people[1].0, Ordering::Relaxed);
    support::run(&mut host, SimTime(30_000));
    assert_eq!(
        host.get::<game_lineage::Lineage>().unwrap().current,
        Some(people[0])
    );
    assert!(
        host.get::<game_characters::Minds>().unwrap().people[&people[0]]
            .last
            .is_none()
    );
    assert!(
        !host
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .people
            .contains(&people[0])
    );
}

#[test]
fn failed_winner_releases_stock_for_a_previously_denied_meal() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder();
    let reject = Arc::new(AtomicU64::new(0));
    let rejected = reject.clone();
    b.before::<game_physiology::ConsumeEdible>("test.reject-winner", move |_, request| {
        require(
            request.person.0 != rejected.load(Ordering::Relaxed),
            "injected refusal",
        )
    })
    .unwrap();
    let (mut host, people, store) = prepared_with(b, true);
    reject.store(people[1].0, Ordering::Relaxed);
    support::run(&mut host, SimTime(30_000));
    let fed: Vec<_> = host
        .notices()
        .filter(|n| n.kind == game_physiology::Fed::KEY)
        .collect();
    assert_eq!(fed.len(), 1);
    assert_eq!(fed[0].data["patient"].as_u64(), Some(people[0].0));
    assert_eq!(
        host.get::<game_cargo::Cargo>().unwrap().0[&store].amount(game_cargo::FOOD),
        0
    );
    assert_eq!(
        host.get::<game_characters::Minds>().unwrap().people[&people[0]].failures,
        0
    );
    assert_eq!(
        host.get::<game_characters::Minds>().unwrap().people[&people[1]].failures,
        1
    );
}

#[test]
fn bounded_groups_rotate_order_and_urgent_actors_precede_nonurgent_actors() {
    for (hungry, expected) in [(None, 1), (Some(0), 0)] {
        let (mut host, people, _) = prepared_with_options(builder(), true, 1, hungry);
        support::run(&mut host, SimTime(30_000));
        let fed = host
            .notices()
            .find(|n| n.kind == game_physiology::Fed::KEY)
            .unwrap();
        assert_eq!(fed.data["patient"].as_u64(), Some(people[expected].0));
    }
}

#[test]
fn four_equal_claimants_each_win_once_across_four_replenished_rounds() {
    for round in 0..4 {
        let (mut host, people, _) = prepared_many(builder(), true, 2, None, 4, round, round * 2);
        support::run(&mut host, SimTime(round * 60_000));
        let fed = host
            .notices()
            .find(|n| n.kind == game_physiology::Fed::KEY)
            .unwrap();
        assert_eq!(fed.data["patient"].as_u64(), Some(people[round as usize].0));
    }
}

#[test]
fn real_sparse_cohorts_advance_persisted_precedence_across_reload() {
    let (mut host, people, store) = prepared_many(builder(), true, 2, None, 4, 0, 0);
    support::run(&mut host, SimTime(0));
    assert_eq!(
        host.get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .meal_round,
        1
    );
    let saved = host.save().unwrap();
    host = builder().load(&saved, 4).unwrap();
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    host.dispatch(KickFrame(SimTime(60_000))).unwrap();
    support::run(&mut host, SimTime(60_000));
    let fed: Vec<_> = host
        .notices()
        .filter(|n| n.kind == game_physiology::Fed::KEY)
        .collect();
    assert_eq!(fed.len(), 2);
    assert_eq!(fed[0].data["patient"].as_u64(), Some(people[0].0));
    assert_eq!(fed[1].data["patient"].as_u64(), Some(people[1].0));
    assert_eq!(
        host.get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .meal_round,
        2
    );
}

#[test]
fn a_failed_gather_rolls_back_the_participation_counter_and_scheduled_groups() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let mut b = builder();
    let armed = Arc::new(AtomicBool::new(false));
    let gate = armed.clone();
    b.subscribe::<game_choice::PlanCohort>("zz.test.fail-after-arbitration-round", move |tx, _| {
        if gate.load(Ordering::Relaxed) {
            assert_eq!(
                tx.get::<game_choice::Offers>()?
                    .frame
                    .as_ref()
                    .unwrap()
                    .meal_round,
                2
            );
            return Err(Error::Rejected("fail after scheduling cohort".into()));
        }
        Ok(())
    })
    .unwrap();
    let (mut host, _, store) = prepared_many(b, true, 2, None, 4, 0, 0);
    support::run(&mut host, SimTime(0));
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    host.dispatch(KickFrame(SimTime(60_000))).unwrap();
    let before = host.save().unwrap();
    armed.store(true, Ordering::Relaxed);
    assert!(host.advance(SimTime(60_000), 1).is_err());
    assert_eq!(host.save().unwrap(), before);
}

#[test]
fn final_denial_refreshes_real_personal_stock_without_failure_learning() {
    let mut b = support::builder();
    b.plugin("real-stale-meal-test", 1).unwrap();
    b.request::<FoodOptions>(|tx, r| game_provisioning::options(tx, r.0, 1))
        .unwrap();
    b.subscribe::<game_decision_frames::Gather>("test.real-cohort", |tx, _| {
        tx.call(game_decision_frames::Batch {
            people: tx
                .get::<game_characters::Minds>()?
                .people
                .keys()
                .copied()
                .collect(),
        })
    })
    .unwrap();
    let mut host = b.build(3, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "stale pantry".into(),
        })
        .unwrap();
    let store = host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: 1000,
        })
        .unwrap();
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
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
        energy: 1_000_000_000,
        fatigue: 0,
    })
    .unwrap();
    host.dispatch(game_characters::Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    host.dispatch(game_knowledge::ObserveStore {
        person,
        inventory: store,
    })
    .unwrap();
    host.dispatch(game_cargo::Consume {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    let options = host.query(FoodOptions(person)).unwrap();
    assert!(options.iter().any(|o| matches!(o.plan, game_provisioning::Plan::Meal { inventory, good: Some(game_cargo::FOOD) } if inventory == store)));
    host.dispatch(game_characters::Start).unwrap();
    host.dispatch(game_choice::ConfigureFrames {
        interval: Some(SimDuration(30_000)),
    })
    .unwrap();
    host.dispatch(game_choice::ConfigureFrameDemand(true))
        .unwrap();
    host.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
        batch_decisions: true,
        commit_batch: 64,
        meal_arbitration: true,
        ..Default::default()
    }))
    .unwrap();
    support::run(&mut host, SimTime(0));
    let facts = game_knowledge::view(&host, person).unwrap();
    let stock: game_knowledge::Stock =
        game_knowledge::known(&facts, &game_knowledge::store_key(store)).unwrap();
    assert_eq!(stock.get(&game_cargo::FOOD).copied().unwrap_or(0), 0);
    assert_eq!(
        host.get::<game_characters::Minds>().unwrap().people[&person].failures,
        0
    );
}

#[test]
fn stock_consumed_by_a_pause_hook_is_not_learned_as_a_failed_attempt() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder();
    let actor = Arc::new(AtomicU64::new(0));
    let store = Arc::new(AtomicU64::new(0));
    let actor_id = actor.clone();
    let store_id = store.clone();
    b.before::<game_character_clock::PauseReview>(
        "test.intervening-consumption",
        move |tx, request| {
            if request.0.0 == actor_id.load(Ordering::Relaxed) {
                tx.call(game_cargo::Consume {
                    inventory: InventoryId(store_id.load(Ordering::Relaxed)),
                    good: game_cargo::FOOD,
                    quantity: 10,
                })?;
            }
            Ok(())
        },
    )
    .unwrap();
    let (mut host, people, inventory) = prepared_with(b, true);
    actor.store(people[0].0, Ordering::Relaxed);
    store.store(inventory.0, Ordering::Relaxed);
    support::run(&mut host, SimTime(30_000));
    assert_eq!(
        host.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::FOOD),
        0
    );
    assert_eq!(
        host.notices()
            .filter(|notice| notice.kind == game_physiology::Fed::KEY)
            .count(),
        0
    );
    assert!(
        people
            .iter()
            .all(|p| host.get::<game_characters::Minds>().unwrap().people[p].failures == 0)
    );
}

#[test]
fn urgent_cue_in_pause_hook_invalidates_selection_before_marker_is_armed() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder();
    let target = Arc::new(AtomicU64::new(0));
    let target_id = target.clone();
    b.before::<game_character_clock::PauseReview>(
        "test.urgent-before-pause",
        move |tx, request| {
            if request.0.0 == target_id.load(Ordering::Relaxed) {
                tx.notify(game_character_clock::Alert { person: request.0 })?;
            }
            Ok(())
        },
    )
    .unwrap();
    let (mut host, people, _) = prepared_with(b, true);
    target.store(people[0].0, Ordering::Relaxed);
    support::run(&mut host, SimTime(30_000));
    assert!(
        !host
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .people
            .contains(&people[0])
    );
    assert!(
        host.get::<game_characters::Minds>()
            .unwrap()
            .cohort_urgency
            .is_empty()
    );
}

#[test]
fn urgent_cue_during_denial_observation_invalidates_the_remaining_review() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder_with_fixed_provider(true);
    let target = Arc::new(AtomicU64::new(0));
    let target_id = target.clone();
    b.before::<game_knowledge::ObserveStore>(
        "test.urgent-during-observation",
        move |tx, request| {
            if request.person.0 == target_id.load(Ordering::Relaxed) {
                tx.notify(game_character_clock::Alert {
                    person: request.person,
                })?;
            }
            Ok(())
        },
    )
    .unwrap();
    let (mut host, people, _) = prepared_with(b, true);
    target.store(people[0].0, Ordering::Relaxed);
    support::run(&mut host, SimTime(30_000));
    assert!(
        !host
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .people
            .contains(&people[0])
    );
    assert!(
        host.get::<game_characters::Minds>()
            .unwrap()
            .cohort_urgency
            .is_empty()
    );
}

#[test]
fn death_of_a_paused_actor_discards_its_selection_and_cleans_up() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder();
    let target = Arc::new(AtomicU64::new(0));
    let winner = Arc::new(AtomicU64::new(0));
    let target_id = target.clone();
    let winner_id = winner.clone();
    b.before::<game_physiology::ConsumeEdible>("test.kill-paused-actor", move |tx, request| {
        if request.person.0 == winner_id.load(Ordering::Relaxed) {
            tx.call(game_physiology::Damage {
                person: PersonId(target_id.load(Ordering::Relaxed)),
                amount: u64::MAX,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let (mut host, people, _) = prepared_with(b, true);
    target.store(people[0].0, Ordering::Relaxed);
    winner.store(people[1].0, Ordering::Relaxed);
    support::run(&mut host, SimTime(30_000));
    assert!(!host.get::<game_physiology::Bodies>().unwrap().people[&people[0]].alive);
    assert!(
        !host
            .get::<game_characters::Minds>()
            .unwrap()
            .people
            .contains_key(&people[0])
    );
    assert!(
        host.get::<game_characters::Minds>()
            .unwrap()
            .paused_reviews
            .is_empty()
    );
    host.save().unwrap();
}

#[test]
fn a_late_cohort_failure_rolls_back_prior_consumption_and_events() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let mut b = builder();
    let reject = Arc::new(AtomicU64::new(0));
    let rejected = reject.clone();
    b.before::<game_character_clock::FinishPausedReview>(
        "test.fail-after-first-meal",
        move |_, request| {
            require(
                request.0.0 != rejected.load(Ordering::Relaxed),
                "injected late failure",
            )
        },
    )
    .unwrap();
    let (mut host, people, _) = prepared_with(b, true);
    let before = host.save().unwrap();
    reject.store(people[0].0, Ordering::Relaxed);
    assert!(host.advance(SimTime(30_000), 1).is_err());
    assert_eq!(host.save().unwrap(), before);
}
