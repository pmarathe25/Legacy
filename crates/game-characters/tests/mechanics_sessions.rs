mod support;
use game_core::*;
use game_mechanics::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Seen {
    trials: Vec<PersonId>,
    collects: u64,
    requests: Vec<Experiment>,
}
impl Resource for Seen {
    const KEY: &'static str = "test.mechanical-sessions";
}
#[derive(Clone, Serialize, Deserialize)]
struct Configure(Vec<Experiment>);
impl Request for Configure {
    const KEY: &'static str = "test.mechanical-configure";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cue(PersonId);
impl Request for Cue {
    const KEY: &'static str = "test.mechanical-cue";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = support::builder();
    game_mechanics::plugin(&mut b).unwrap();
    game_autonomy_frames::register(&mut b).unwrap();
    b.plugin("mechanical-session-test", 1).unwrap();
    b.resource::<Seen>().unwrap();
    b.request::<Configure>(|tx, r| {
        tx.edit::<Seen>()?.requests = r.0;
        Ok(())
    })
    .unwrap();
    b.request::<Cue>(|tx, r| tx.notify(game_characters::Alert { person: r.0 }))
        .unwrap();
    b.subscribe::<Tested>("test.mechanical-evidence", |tx, e| {
        tx.edit::<Seen>()?.trials.push(e.person);
        Ok(())
    })
    .unwrap();
    b.before::<game_choice::Collect>("test.mechanical-collects", |tx, _| {
        tx.edit::<Seen>()?.collects += 1;
        Ok(())
    })
    .unwrap();
    game_choice::provider(&mut b, "test.mechanical-options", |tx, e| {
        if let Some(r) = tx
            .get::<Seen>()?
            .requests
            .iter()
            .find(|r| r.person == e.person)
            .cloned()
        {
            game_choice::offer(
                tx,
                e.person,
                "mechanics:test",
                game_choice::motive(10_000, 0, 0, 0, 0),
                r,
            )?;
        }
        Ok(())
    })
    .unwrap();
    b
}
fn setup(count: usize, hunger: u64) -> (Host, Vec<PersonId>) {
    let mut h = builder().build(25, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        hunger_per_tick: hunger,
        fatigue_per_tick: 0,
        ..Default::default()
    }))
    .unwrap();
    h.dispatch(game_characters::Configure(game_characters::Rules {
        meal_hunger: 32,
        rest_fatigue: 100,
        ..Default::default()
    }))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "workshop".into(),
        })
        .unwrap();
    let good = game_cargo::TIMBER;
    h.dispatch(Install(Config {
        supports: Default::default(),
        materials: [(
            good,
            Material {
                span: 12,
                limit: 1_000_000,
                efficiency: 1000,
                supports: Default::default(),
            },
        )]
        .into(),
    }))
    .unwrap();
    let mut people = vec![];
    let mut requests = vec![];
    for _ in 0..count {
        let person = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person, area }).unwrap();
        let weights = h
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Area(area),
                capacity: 100_000,
            })
            .unwrap();
        h.dispatch(game_property::ClaimInventory {
            claimant: person,
            inventory: weights,
            recognized_by: [person].into(),
        })
        .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory: weights,
            good,
            quantity: 3000,
        })
        .unwrap();
        let device = h
            .dispatch(Assemble {
                person,
                inventory: weights,
                parts: vec![Part {
                    mechanism: Mechanism::Lever,
                    good,
                    input: None,
                    drive: 2,
                    load: 1,
                }],
            })
            .unwrap();
        requests.push(Experiment {
            person,
            device,
            weights,
            load: 100,
            outlet: 0,
        });
        people.push(person);
        h.dispatch(game_characters::Register {
            person,
            mind: Default::default(),
        })
        .unwrap();
    }
    h.dispatch(Configure(requests)).unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    h.dispatch(game_characters::Start).unwrap();
    h.dispatch(game_choice::ConfigureFrames {
        interval: Some(SimDuration(15 * MINUTE.0)),
    })
    .unwrap();
    h.dispatch(game_choice::ConfigureFrameDemand(true)).unwrap();
    h.dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
        mechanics_sessions: true,
        batch_decisions: true,
        commit_batch: 128,
        urgent_immediate: true,
        ..Default::default()
    }))
    .unwrap();
    (h, people)
}
#[test]
fn selected_experiments_remain_persistent_without_recollecting_providers_and_replay() {
    let (initial, people) = setup(16, 0);
    let saved = initial.save().unwrap();
    let mut worlds = vec![];
    for workers in [1, 4, 12] {
        let mut h = builder().load(&saved, workers).unwrap();
        support::run(&mut h, SimTime(15 * MINUTE.0));
        assert!(
            people
                .iter()
                .all(|p| h.get::<Machines>().unwrap().work[p].session.is_some())
        );
        let ids: Vec<_> = people
            .iter()
            .map(|p| h.get::<Actions>().unwrap().0[p].id)
            .collect();
        let collects = h.get::<Seen>().unwrap().collects;
        support::run(&mut h, SimTime(HOUR.0 + 15 * MINUTE.0));
        assert_eq!(h.get::<Seen>().unwrap().trials, people);
        let mut h = builder().load(&h.save().unwrap(), workers).unwrap();
        support::run(&mut h, SimTime(2 * HOUR.0 + 15 * MINUTE.0));
        assert_eq!(h.get::<Seen>().unwrap().collects, collects);
        for (p, id) in people.iter().zip(ids) {
            assert_eq!(h.get::<Actions>().unwrap().0[p].id, id);
        }
        worlds.push(h.save().unwrap());
    }
    assert!(worlds.windows(2).all(|p| p[0] == p[1]));
}
#[test]
fn urgent_damage_control_death_and_toggle_release_mechanics_before_the_next_epoch() {
    for cause in 0..5 {
        let (mut h, people) = setup(2, 0);
        support::run(&mut h, SimTime(15 * MINUTE.0));
        let p = people[0];
        match cause {
            0 => h.dispatch(Cue(p)).unwrap(),
            1 => h
                .dispatch(game_physiology::Damage {
                    person: p,
                    amount: 1,
                })
                .unwrap(),
            2 => h.dispatch(game_lineage::Begin { person: p }).unwrap(),
            3 => h
                .dispatch(game_physiology::Damage {
                    person: p,
                    amount: u64::MAX,
                })
                .unwrap(),
            _ => h
                .dispatch(game_choice::TuneFrames(game_choice::FrameTuning {
                    batch_decisions: true,
                    commit_batch: 128,
                    urgent_immediate: true,
                    ..Default::default()
                }))
                .unwrap(),
        }
        assert!(!h.get::<Actions>().unwrap().0.contains_key(&p));
        assert!(!h.get::<Machines>().unwrap().work.contains_key(&p));
        let mut h = builder().load(&h.save().unwrap(), 4).unwrap();
        support::run(&mut h, SimTime(HOUR.0 + 15 * MINUTE.0));
    }
}
#[test]
fn session_hunger_crossing_interrupts_on_its_exact_off_grid_deadline() {
    let (mut h, people) = setup(1, 1);
    support::run(&mut h, SimTime(15 * MINUTE.0));
    let id = h.get::<Actions>().unwrap().0[&people[0]].id;
    support::run(&mut h, SimTime(31 * MINUTE.0));
    assert_eq!(h.get::<Actions>().unwrap().0[&people[0]].id, id);
    support::run(&mut h, SimTime(32 * MINUTE.0));
    assert!(
        h.get::<Actions>()
            .unwrap()
            .0
            .get(&people[0])
            .is_none_or(|a| a.id != id)
    );
    assert_eq!(h.get::<Seen>().unwrap().trials, people);
}
