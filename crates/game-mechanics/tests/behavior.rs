use game_core::*;
use game_mechanics::*;
use game_sim::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CancelTrial(PersonId);
impl Request for CancelTrial {
    const KEY: &'static str = "mechanics-test.cancel";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_communication::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_testimony::plugin(&mut b).unwrap();
    game_choice::plugin(&mut b).unwrap();
    game_inquiry::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    game_invention::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    game_workshop::plugin(&mut b).unwrap();
    b.plugin("mechanics-test", 1).unwrap();
    b.request::<CancelTrial>(|tx, r| tx.cancel_action(r.0))
        .unwrap();
    b
}
fn setup(limit: u64) -> (Host, PersonId, InventoryId, GoodId) {
    setup_seed(limit, 31)
}
fn setup_seed(limit: u64, seed: u64) -> (Host, PersonId, InventoryId, GoodId) {
    let mut h = builder().build(seed, 1).unwrap();
    h.dispatch(game_objects::Install(
        game_objects::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    h.advance(SimTime(30 * 365 * DAY.0), 1).unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Workshop".into(),
        })
        .unwrap();
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person, area }).unwrap();
    h.dispatch(game_physiology::Express { person }).unwrap();
    let good = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("timber")
        .unwrap();
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: 100000,
        })
        .unwrap();
    h.dispatch(game_property::ClaimInventory {
        claimant: person,
        inventory,
        recognized_by: [person].into(),
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good,
        quantity: 3000,
    })
    .unwrap();
    h.dispatch(Install(Config {
        supports: Default::default(),
        materials: [(
            good,
            Material {
                span: 12,
                limit,
                efficiency: 1000,
                supports: Default::default(),
            },
        )]
        .into(),
    }))
    .unwrap();
    (h, person, inventory, good)
}
fn run(h: &mut Host, until: SimTime) {
    while h.advance(until, 128).unwrap().stop == Stop::Budget {}
}
fn device(h: &mut Host, p: PersonId, inventory: InventoryId, good: GoodId) -> u64 {
    h.dispatch(Assemble {
        person: p,
        inventory,
        parts: vec![Part {
            mechanism: game_physics::Mechanism::Lever,
            good,
            input: None,
            drive: 2,
            load: 1,
        }],
    })
    .unwrap()
}
#[test]
fn finite_parts_and_loaded_trials_replay_and_teach_a_useful_working_capacity() {
    let (mut h, p, inventory, good) = setup(4000);
    let initial = h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(good);
    let id = device(&mut h, p, inventory, good);
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(good),
        initial - 1
    );
    let until = h
        .dispatch(Experiment {
            person: p,
            device: id,
            weights: inventory,
            load: 1500,
            outlet: 0,
        })
        .unwrap();
    run(&mut h, SimTime(until.0 - HOUR.0 / 2));
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    run(&mut h, until);
    run(&mut loaded, until);
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    assert!(
        h.get::<game_knowledge::Knowledge>().unwrap().0[&p]
            .contains_key(format!("mechanics:device:{id}:0").as_str())
    );
    let body = h.get::<game_physiology::Bodies>().unwrap().people[&p];
    assert!(body.hunger > 0 || body.hunger_fraction > 0);
    let output = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("medicine")
        .unwrap();
    h.dispatch(game_workshop::Define {
        name: "heavy-job".into(),
        recipe: game_workshop::Recipe {
            work: Some(Task {
                force: 1500,
                travel: 100,
            }),
            inputs: [(good, 1)].into(),
            output,
            quantity: 1,
            duration: HOUR,
            skill: "carpentry".into(),
        },
    })
    .unwrap();
    let until = h
        .dispatch(game_workshop::Craft {
            person: p,
            inventory,
            recipe: "heavy-job".into(),
        })
        .unwrap();
    run(&mut h, until);
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(output),
        1
    );
}
#[test]
fn overload_breakage_preserves_material_as_debris_and_weak_manual_work_can_stall() {
    let (mut h, p, inventory, good) = setup(1000);
    let id = device(&mut h, p, inventory, good);
    let until = h
        .dispatch(Experiment {
            person: p,
            device: id,
            weights: inventory,
            load: 1500,
            outlet: 0,
        })
        .unwrap();
    run(&mut h, until);
    assert!(
        h.get::<Machines>().unwrap().devices[&id]
            .broken
            .contains(&0)
    );
    assert_eq!(h.get::<Machines>().unwrap().waste[&good], 1);
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(good),
        2999
    );
}
#[test]
fn a_newborn_cannot_assemble_or_operate_even_with_extreme_potential() {
    let (mut h, p, inventory, good) = setup(4000);
    let id = device(&mut h, p, inventory, good);
    let area = game_transport::area(h.get().unwrap(), p).unwrap();
    let child = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: h.now(),
        })
        .unwrap();
    h.dispatch(game_transport::Place {
        person: child,
        area,
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: child,
        profile: [
            ("agility".into(), 16000),
            ("strength".into(), 16000),
            ("intelligence".into(), 16000),
        ]
        .into(),
    })
    .unwrap();
    h.dispatch(game_physiology::Express { person: child })
        .unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Assemble {
            person: child,
            inventory,
            parts: vec![Part {
                mechanism: game_physics::Mechanism::Lever,
                good,
                input: None,
                drive: 2,
                load: 1
            }]
        })
        .is_err()
    );
    assert!(
        h.dispatch(Experiment {
            person: child,
            device: id,
            weights: inventory,
            load: 500,
            outlet: 0
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
}

#[test]
fn impossible_manual_work_consumes_inputs_without_creating_the_product() {
    let (mut h, p, inventory, good) = setup(4000);
    let output = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("medicine")
        .unwrap();
    h.dispatch(game_workshop::Define {
        name: "unattainable-press".into(),
        recipe: game_workshop::Recipe {
            work: Some(Task {
                force: 1_000_000,
                travel: 100,
            }),
            inputs: [(good, 1)].into(),
            output,
            quantity: 1,
            duration: HOUR,
            skill: "carpentry".into(),
        },
    })
    .unwrap();
    let until = h
        .dispatch(game_workshop::Craft {
            person: p,
            inventory,
            recipe: "unattainable-press".into(),
        })
        .unwrap();
    run(&mut h, until);
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(output),
        0
    );
    assert_eq!(h.get::<game_workshop::Workshop>().unwrap().waste[&good], 1);
    let facts = &h.get::<game_knowledge::Knowledge>().unwrap().0[&p];
    let problem: serde_json::Value =
        game_knowledge::known(facts, "mechanics:need:unattainable-press").unwrap();
    assert_eq!(problem["met"], false);
}

#[test]
fn known_devices_can_be_retested_and_travellers_do_not_reject_choice_collection() {
    let (mut h, p, inventory, good) = setup(4000);
    let id = device(&mut h, p, inventory, good);
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p),
            capacity: 10000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good,
        quantity: 10,
    })
    .unwrap();
    let until = h
        .dispatch(Experiment {
            person: p,
            device: id,
            weights: pack,
            load: 1,
            outlet: 0,
        })
        .unwrap();
    run(&mut h, until);
    let offers = h.dispatch(game_choice::Collect { person: p }).unwrap();
    assert!(
        offers
            .iter()
            .any(|o| o.key == format!("mechanics:test:{id}:0"))
    );
    assert!(offers.iter().any(|o| o.key.starts_with("mechanics:build:")));
    let from = game_transport::area(h.get().unwrap(), p).unwrap();
    let to = h
        .dispatch(game_land::CreateArea {
            name: "Distant workshop".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from,
        to,
        duration: HOUR,
    })
    .unwrap();
    h.dispatch(game_transport::Move {
        person: p,
        destination: to,
    })
    .unwrap();
    assert!(
        h.dispatch(game_choice::Collect { person: p })
            .unwrap()
            .is_empty()
    );
}

#[test]
fn observed_capacities_can_be_taught_without_inspecting_private_material_limits() {
    let (mut h, p, inventory, good) = setup(4000);
    let id = device(&mut h, p, inventory, good);
    let until = h
        .dispatch(Experiment {
            person: p,
            device: id,
            weights: inventory,
            load: 1500,
            outlet: 0,
        })
        .unwrap();
    run(&mut h, until);
    let listener = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    let area = game_transport::area(h.get().unwrap(), p).unwrap();
    h.dispatch(game_transport::Place {
        person: listener,
        area,
    })
    .unwrap();
    let key = format!("mechanics:device:{id}:0");
    h.dispatch(game_testimony::Tell {
        sender: p,
        recipient: listener,
        key: key.clone(),
    })
    .unwrap();
    let now = h.now();
    run(&mut h, now);
    let knowledge = h.get::<game_knowledge::Knowledge>().unwrap();
    let teacher: Measurement = game_knowledge::known(&knowledge.0[&p], &key).unwrap();
    let learner: Measurement = game_knowledge::known(&knowledge.0[&listener], &key).unwrap();
    assert_eq!(teacher, learner);
    let raw: serde_json::Value = game_knowledge::known(&knowledge.0[&listener], &key).unwrap();
    assert_eq!(raw["load"], 1500);
    assert!(raw.get("limit").is_none());
}

#[test]
fn small_exertions_accumulate_and_inherited_endurance_changes_the_cost() {
    let (h, p, _, _) = setup(4000);
    let mut once = h.get::<game_physiology::Bodies>().unwrap().people[&p];
    let mut partitioned = once;
    let mut robust = once;
    once.exert(1000, 1000, 1000);
    for _ in 0..1000 {
        partitioned.exert(1, 1, 1000);
    }
    assert_eq!(once, partitioned);
    robust.exert(1000, 1000, 4000);
    assert!(robust.hunger < once.hunger || robust.hunger_fraction < once.hunger_fraction);
    assert!(robust.fatigue < once.fatigue || robust.fatigue_fraction < once.fatigue_fraction);
}

#[test]
fn a_person_can_decline_a_known_aid_without_losing_the_observed_technique() {
    let (mut h, p, stock, g) = setup(4000);
    let id = device(&mut h, p, stock, g);
    let until = h
        .dispatch(Experiment {
            person: p,
            device: id,
            weights: stock,
            load: 1500,
            outlet: 0,
        })
        .unwrap();
    run(&mut h, until);
    let saved = h.save().unwrap();
    for (mechanics, manual, expected) in [(-100, 100, None), (100, -100, Some((id, 0)))] {
        let mut h = builder().load(&saved, 1).unwrap();
        h.dispatch(game_relationships::Initialize {
            person: p,
            beliefs: Default::default(),
            values: [
                ("mechanics".into(), mechanics),
                ("manual-work".into(), manual),
            ]
            .into(),
        })
        .unwrap();
        let selected = h
            .dispatch(SelectAid(PrepareLabor {
                person: p,
                label: "heavy-task".into(),
                task: Task {
                    force: 1500,
                    travel: 100,
                },
                baseline: HOUR,
            }))
            .unwrap();
        assert_eq!(selected, expected);
        assert!(
            h.get::<game_knowledge::Knowledge>().unwrap().0[&p]
                .contains_key(&format!("mechanics:device:{id}:0"))
        );
    }
}

#[test]
fn a_failed_work_need_can_motivate_copying_an_observed_useful_design() {
    let (mut h, p, stock, g) = setup(4000);
    let id = device(&mut h, p, stock, g);
    let duration = h
        .dispatch(PrepareLabor {
            person: p,
            label: "hard-work".into(),
            task: Task {
                force: 1200,
                travel: 1000,
            },
            baseline: HOUR,
        })
        .unwrap();
    let at = h.now().after(duration).unwrap();
    run(&mut h, at);
    assert!(!h.dispatch(FinishLabor { person: p }).unwrap());
    let end = h
        .dispatch(Experiment {
            person: p,
            device: id,
            weights: stock,
            load: 800,
            outlet: 0,
        })
        .unwrap();
    run(&mut h, end);
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_cargo::Transfer {
        from: stock,
        to: pack,
        good: g,
        quantity: 100,
    })
    .unwrap();
    let original = h.get::<Machines>().unwrap().devices[&id].parts.clone();
    let proposals = h.dispatch(game_choice::Collect { person: p }).unwrap();
    let build = proposals
        .into_iter()
        .find(|o| o.key == format!("mechanics:build:{}", pack.0))
        .unwrap();
    h.dispatch(game_choice::Run(build.request)).unwrap();
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    let end = h.now().after(HOUR).unwrap();
    for host in [&mut h, &mut loaded] {
        run(host, end);
        assert_eq!(host.get::<Machines>().unwrap().devices.len(), 2);
        assert_eq!(
            host.get::<Machines>()
                .unwrap()
                .devices
                .values()
                .last()
                .unwrap()
                .parts,
            original
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn ongoing_work_cannot_keep_its_original_speed_after_agility_degrades() {
    let (mut h, p, _, _) = setup(4000);
    let duration = h
        .dispatch(PrepareLabor {
            person: p,
            label: "careful-work".into(),
            task: Task {
                force: 100,
                travel: 1000,
            },
            baseline: HOUR,
        })
        .unwrap();
    let halfway = h.now().after(SimDuration(duration.0 / 2)).unwrap();
    run(&mut h, halfway);
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("agility".into(), 0)].into(),
    })
    .unwrap();
    let finish = h.now().after(SimDuration(duration.0 / 2)).unwrap();
    run(&mut h, finish);
    assert!(!h.dispatch(FinishLabor { person: p }).unwrap());
}

#[test]
fn seeded_apparatus_simulations_conserve_material_and_work_across_breakage_interruption_and_reload()
{
    let mut trials = 0;
    let mut interrupted = 0;
    let mut breaks = 0;
    let mut replays = 0;
    for seed in [1, 31, 77, 2026] {
        let (mut h, p, inventory, good) = setup_seed(200 + random(seed, 7, 9) % 8000, seed);
        for trial in 0..64 {
            let n = 1 + (random(seed, trial, 1) % 4) as usize;
            let parts = (0..n)
                .map(|i| {
                    let r = random(seed, trial, 10 + i as u64);
                    Part {
                        mechanism: game_physics::Mechanism::Lever,
                        good,
                        input: if i == 0 || r.is_multiple_of(3) {
                            None
                        } else {
                            Some(i - 1)
                        },
                        drive: 1 + (r % 6) as u32,
                        load: 1 + ((r >> 10) % 6) as u32,
                    }
                })
                .collect();
            let id = h
                .dispatch(Assemble {
                    person: p,
                    inventory,
                    parts,
                })
                .unwrap();
            let sequence = h.notices().last().map_or(0, |e| e.sequence);
            let until = h
                .dispatch(Experiment {
                    person: p,
                    device: id,
                    weights: inventory,
                    load: 1 + random(seed, trial, 83) % 2000,
                    outlet: n - 1,
                })
                .unwrap();
            let half = SimTime(h.now().0 + (until.0 - h.now().0) / 2);
            run(&mut h, half);
            let mut replay = trial
                .is_multiple_of(4)
                .then(|| builder().load(&h.save().unwrap(), 4).unwrap());
            let cancel = trial.is_multiple_of(5);
            if cancel {
                interrupted += 1;
            }
            for host in std::iter::once(&mut h).chain(replay.iter_mut()) {
                if cancel {
                    host.dispatch(CancelTrial(p)).unwrap();
                }
                run(host, until);
                let records: Vec<Tested> = host
                    .notices_since(sequence)
                    .filter(|e| e.kind == Tested::KEY)
                    .map(|e| serde_json::from_value(e.data.clone()).unwrap())
                    .collect();
                assert_eq!(records.len(), 1);
                for e in records {
                    assert!(u128::from(e.lifted) * u128::from(e.travel) <= e.source.work());
                }
                host.dispatch(Dismantle {
                    person: p,
                    device: id,
                    inventory,
                })
                .unwrap();
                let timber: u64 = host
                    .get::<game_cargo::Cargo>()
                    .unwrap()
                    .0
                    .values()
                    .map(|i| i.amount(good))
                    .sum();
                let waste = host
                    .get::<Machines>()
                    .unwrap()
                    .waste
                    .get(&good)
                    .copied()
                    .unwrap_or(0);
                assert_eq!(timber + waste, 3000);
            }
            if let Some(replay) = replay {
                assert_eq!(h.save().unwrap(), replay.save().unwrap());
                replays += 1;
            }
            breaks += h
                .get::<Machines>()
                .unwrap()
                .waste
                .get(&good)
                .copied()
                .unwrap_or(0);
            trials += 1;
        }
    }
    if let Some(folder) = std::env::var_os("LEGACY_PHYSICS_REPORT_DIR") {
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(std::path::PathBuf::from(folder).join("apparatus.json"),
            format!("{{\"seeds\":4,\"trials\":{trials},\"interrupted\":{interrupted},\"replays\":{replays},\"material_conservation\":true,\"work_conservation\":true}}\n")).unwrap();
    }
    assert_eq!(trials, 256);
    assert_eq!(replays, 64);
    assert!(breaks > 0);
}

#[test]
fn an_observed_machine_processes_real_food_that_manual_strength_cannot_process() {
    let (mut h, p, stock, material) = setup(4000);
    let id = device(&mut h, p, stock, material);
    let recipes =
        game_workshop::load(&mut game_content::Store::standard(), h.get().unwrap()).unwrap();
    h.dispatch(recipes).unwrap();
    let grain = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("grain")
        .unwrap();
    let meal = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("pressed-grain-meal")
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: stock,
        good: grain,
        quantity: 80,
    })
    .unwrap();
    let mut unaided = builder().load(&h.save().unwrap(), 1).unwrap();
    let until = unaided
        .dispatch(game_workshop::Craft {
            person: p,
            inventory: stock,
            recipe: "press-grain-meal".into(),
        })
        .unwrap();
    run(&mut unaided, until);
    assert_eq!(
        unaided.get::<game_cargo::Cargo>().unwrap().0[&stock].amount(meal),
        0
    );
    let until = h
        .dispatch(Experiment {
            person: p,
            device: id,
            weights: stock,
            load: 1500,
            outlet: 0,
        })
        .unwrap();
    run(&mut h, until);
    let until = h
        .dispatch(game_workshop::Craft {
            person: p,
            inventory: stock,
            recipe: "press-grain-meal".into(),
        })
        .unwrap();
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for world in [&mut h, &mut loaded] {
        run(world, until);
        assert_eq!(
            world.get::<game_cargo::Cargo>().unwrap().0[&stock].amount(meal),
            32
        );
        assert!(world.notices().any(|e| e.kind == Tested::KEY
            && e.data["device"] == id
            && e.data["task"][0] == "press-grain-meal"));
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[path = "support/sessions.rs"]
mod sessions;
