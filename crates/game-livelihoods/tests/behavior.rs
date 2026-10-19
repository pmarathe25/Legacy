mod support;
use game_core::*;
use game_livelihoods::*;
use support::*;
fn enable(h: &mut game_sim::Host, p: PersonId, i: InventoryId, object: &str, quantity: u64) {
    h.dispatch(
        game_workshop::load(&mut game_content::Store::standard(), h.get().unwrap()).unwrap(),
    )
    .unwrap();
    h.dispatch(Install {
        rules: Rules {
            interval: HOUR,
            goals: vec![Goal {
                object: object.into(),
                quantity,
                priority: 50,
                affinity: "discipline".into(),
                concept: "craft".into(),
            }],
        },
        stamps: vec![],
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person: p,
        inventory: i,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveSurroundings { person: p })
        .unwrap();
    h.dispatch(Enable).unwrap();
}
#[test]
fn a_person_gathers_and_crafts_a_chain_using_actual_time_and_supplies() {
    let (mut h, p, i, area) = setup();
    let patch = h
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area,
            good: game_cargo::TIMBER,
            stock: 100,
            capacity: 100,
            renew: 0,
        }))
        .unwrap();
    enable(&mut h, p, i, "charcoal", 6);
    assert!(h.dispatch(Consider { person: p }).unwrap());
    assert_eq!(amount(&h, i, "timber"), 0);
    run(&mut h, 5 * MINUTE.0);
    assert_eq!(amount(&h, i, "timber"), 12);
    assert_eq!(
        h.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock,
        88
    );
    assert!(h.dispatch(Consider { person: p }).unwrap());
    let save = h.save().unwrap();
    let mut loaded = builder().load(&save, 4).unwrap();
    for host in [&mut h, &mut loaded] {
        run(host, 50 * MINUTE.0);
        assert_eq!(amount(host, i, "charcoal"), 6);
        assert_eq!(amount(host, i, "timber"), 0);
        assert!(!host.dispatch(Consider { person: p }).unwrap());
        assert_eq!(host.get::<Livelihoods>().unwrap().achieved[&p], 1);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn writing_supplies_can_be_planned_from_warm_region_resources_without_nettles() {
    let (mut h, p, i, area) = setup();
    for object in ["bamboo", "fresh-water"] {
        let good = good(&h, object);
        h.dispatch(game_ecology::Create(game_ecology::Patch {
            area,
            good,
            stock: 40,
            capacity: 40,
            renew: 0,
        }))
        .unwrap();
    }
    enable(&mut h, p, i, "paper", 16);
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        for _ in 0..12 {
            host.dispatch(Consider { person: p }).unwrap();
            let to = host.now().0 + 3 * HOUR.0;
            run(host, to);
            if amount(host, i, "paper") >= 16 {
                break;
            }
        }
        assert_eq!(amount(host, i, "paper"), 16);
        assert_eq!(amount(host, i, "bamboo"), 0);
        assert_eq!(amount(host, i, "fresh-water"), 0);
        assert!(
            host.now().0 >= 4 * HOUR.0,
            "paper takes real labor and supplies"
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn cancellation_loses_paid_inputs_without_awarding_achievement() {
    let (mut h, p, i, _) = setup();
    deposit(&mut h, i, "timber", 12);
    enable(&mut h, p, i, "charcoal", 6);
    assert!(h.dispatch(Consider { person: p }).unwrap());
    h.dispatch(Cancel(p)).unwrap();
    run(&mut h, HOUR.0);
    h.dispatch(Consider { person: p }).unwrap();
    assert_eq!(amount(&h, i, "charcoal"), 0);
    assert!(!h.get::<Livelihoods>().unwrap().achieved.contains_key(&p));
    assert_eq!(
        h.get::<game_workshop::Workshop>().unwrap().waste[&game_cargo::TIMBER],
        12
    );
}
#[test]
fn exhausted_memories_fail_once_and_back_off() {
    let (mut h, p, i, area) = setup();
    let patch = h
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area,
            good: game_cargo::TIMBER,
            stock: 12,
            capacity: 12,
            renew: 0,
        }))
        .unwrap();
    enable(&mut h, p, i, "charcoal", 6);
    h.dispatch(game_ecology::Gather {
        person: p,
        patch,
        quantity: 12,
        destination: i,
    })
    .unwrap();
    run(&mut h, 5 * MINUTE.0);
    // The last observation still described twelve units, but another action depleted them.
    h.dispatch(game_cargo::Consume {
        inventory: i,
        good: game_cargo::TIMBER,
        quantity: 12,
    })
    .unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert_eq!(h.get::<Livelihoods>().unwrap().failures[&p], 1);
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert!(h.get::<game_sim::Actions>().unwrap().0.is_empty());
}
#[test]
fn goals_do_not_read_distant_resources() {
    let (mut h, p, i, _) = setup();
    let away = h
        .dispatch(game_land::CreateArea {
            name: "Hidden forest".into(),
        })
        .unwrap();
    h.dispatch(game_ecology::Create(game_ecology::Patch {
        area: away,
        good: game_cargo::TIMBER,
        stock: 100,
        capacity: 100,
        renew: 0,
    }))
    .unwrap();
    enable(&mut h, p, i, "charcoal", 6);
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert!(h.get::<game_sim::Actions>().unwrap().0.is_empty());
}
#[test]
fn cyclic_recipes_do_not_create_free_inputs_or_unbounded_plans() {
    let (mut h, p, i, _) = setup();
    enable(&mut h, p, i, "tool", 1);
    h.dispatch(game_workshop::Define {
        name: "impossible".into(),
        recipe: game_workshop::Recipe {
            work: None,
            inputs: [(game_cargo::FOOD, 1)].into(),
            output: game_cargo::TIMBER,
            quantity: 1,
            duration: MINUTE,
            skill: "craft".into(),
        },
    })
    .unwrap();
    h.dispatch(game_workshop::Define {
        name: "cycle".into(),
        recipe: game_workshop::Recipe {
            work: None,
            inputs: [(game_cargo::TIMBER, 1)].into(),
            output: game_cargo::FOOD,
            quantity: 1,
            duration: MINUTE,
            skill: "craft".into(),
        },
    })
    .unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert_eq!(amount(&h, i, "tool"), 0);
}

#[test]
fn revoked_permission_does_not_make_physically_present_supplies_unusable() {
    let (mut h, p, i, area) = setup();
    h.dispatch(game_ecology::Create(game_ecology::Patch {
        area,
        good: game_cargo::TIMBER,
        stock: 100,
        capacity: 100,
        renew: 0,
    }))
    .unwrap();
    enable(&mut h, p, i, "charcoal", 6);
    assert!(h.dispatch(Consider { person: p }).unwrap());
    run(&mut h, 5 * MINUTE.0);
    h.dispatch(game_property::Permit {
        issuer: p,
        person: p,
        inventory: i,
        allow: false,
    })
    .unwrap();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    assert!(h.notices().any(|n| n.kind == "property.interfered"));
    assert!(!h.get::<game_property::Property>().unwrap().allowed(p, i));
    run(&mut h, HOUR.0);
    assert!(amount(&h, i, "charcoal") > 0);
}
#[test]
fn a_plan_that_would_overfill_the_pack_is_not_started() {
    let (mut h, p, i, area) = setup();
    deposit(&mut h, i, "stone", 400);
    h.dispatch(game_ecology::Create(game_ecology::Patch {
        area,
        good: game_cargo::TIMBER,
        stock: 100,
        capacity: 100,
        renew: 0,
    }))
    .unwrap();
    enable(&mut h, p, i, "charcoal", 6);
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert!(h.get::<game_sim::Actions>().unwrap().0.is_empty());
}

#[test]
fn greater_expressed_intelligence_enables_a_longer_physical_production_plan() {
    let (mut h, p, i, area) = setup();
    h.dispatch(game_ecology::Create(game_ecology::Patch {
        area,
        good: game_cargo::TIMBER,
        stock: 100,
        capacity: 100,
        renew: 0,
    }))
    .unwrap();
    enable(&mut h, p, i, "stone", 1);
    let mut previous = game_cargo::TIMBER;
    for (index, key) in [
        "iron-ore",
        "copper-ore",
        "tin-ore",
        "slate",
        "limestone",
        "sand",
        "clay",
        "stone",
    ]
    .iter()
    .enumerate()
    {
        let output = good(&h, key);
        h.dispatch(game_workshop::Define {
            name: format!("ability-chain-{index}"),
            recipe: game_workshop::Recipe {
                work: None,
                inputs: [(previous, 1)].into(),
                output,
                quantity: 1,
                duration: MINUTE,
                skill: "work".into(),
            },
        })
        .unwrap();
        previous = output;
    }
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("intelligence".into(), 0)].into(),
    })
    .unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert_eq!(amount(&h, i, "stone"), 0);
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("intelligence".into(), 12000)].into(),
    })
    .unwrap();
    run(&mut h, 2 * HOUR.0);
    for _ in 0..20 {
        if amount(&h, i, "stone") > 0 {
            break;
        }
        assert!(h.dispatch(Consider { person: p }).unwrap());
        let at = h.get::<game_sim::Actions>().unwrap().0[&p].until.unwrap();
        run(&mut h, at.0);
    }
    assert_eq!(amount(&h, i, "stone"), 1);
    h.save().unwrap();
}

#[test]
fn a_branched_plan_longer_than_32_actions_can_finish_after_reload() {
    let (mut h, p, i, area) = setup();
    h.dispatch(game_ecology::Create(game_ecology::Patch {
        area,
        good: game_cargo::TIMBER,
        stock: 2,
        capacity: 2,
        renew: 0,
    }))
    .unwrap();
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("intelligence".into(), 16000)].into(),
    })
    .unwrap();
    let goal = good(&h, "stone");
    let intermediates: Vec<_> = h
        .get::<game_objects::Objects>()
        .unwrap()
        .items
        .values()
        .map(|o| o.id)
        .filter(|id| *id != goal && *id != game_cargo::TIMBER)
        .take(40)
        .collect();
    let mut ends = vec![];
    for (branch, chain) in intermediates.chunks(20).enumerate() {
        let mut input = game_cargo::TIMBER;
        for (index, output) in chain.iter().enumerate() {
            h.dispatch(game_workshop::Define {
                name: format!("branch-{branch}-{index}"),
                recipe: game_workshop::Recipe {
                    work: None,
                    inputs: [(input, 1)].into(),
                    output: *output,
                    quantity: 1,
                    duration: MINUTE,
                    skill: "work".into(),
                },
            })
            .unwrap();
            input = *output;
        }
        ends.push(input);
    }
    h.dispatch(game_workshop::Define {
        name: "finish".into(),
        recipe: game_workshop::Recipe {
            work: None,
            inputs: ends.into_iter().map(|g| (g, 1)).collect(),
            output: goal,
            quantity: 1,
            duration: MINUTE,
            skill: "work".into(),
        },
    })
    .unwrap();
    h.dispatch(Install {
        rules: Rules {
            interval: HOUR,
            goals: vec![Goal {
                object: "stone".into(),
                quantity: 1,
                priority: 50,
                affinity: "discipline".into(),
                concept: "craft".into(),
            }],
        },
        stamps: vec![],
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person: p,
        inventory: i,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveSurroundings { person: p })
        .unwrap();
    h.dispatch(Enable).unwrap();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        let mut actions = 0;
        while amount(host, i, "stone") == 0 {
            let at = host.get::<game_sim::Actions>().unwrap().0[&p]
                .until
                .unwrap();
            run(host, at.0);
            actions += 1;
            host.dispatch(Consider { person: p }).unwrap();
            assert!(actions <= 50);
        }
        assert!(actions > 32);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn an_immediate_need_can_request_supplies_outside_authored_stock_goals() {
    let (mut h, p, pack, area) = setup();
    let herb = good(&h, "healing-herbs");
    h.dispatch(game_ecology::Create(game_ecology::Patch {
        area,
        good: herb,
        stock: 100,
        capacity: 100,
        renew: 0,
    }))
    .unwrap();
    deposit(&mut h, pack, "cloth", 1);
    enable(&mut h, p, pack, "timber", 1);
    let goal = Goal {
        object: "medicine".into(),
        quantity: 2,
        priority: 100,
        affinity: "altruism".into(),
        concept: "care".into(),
    };
    assert_eq!(
        h.dispatch(Supply {
            person: p,
            goal: goal.clone()
        })
        .unwrap(),
        game_choice::Outcome::Performed
    );
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        run(host, 10 * MINUTE.0);
        host.dispatch(Supply {
            person: p,
            goal: goal.clone(),
        })
        .unwrap();
        run(host, 40 * MINUTE.0);
        assert_eq!(
            host.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(game_cargo::MEDICINE),
            2
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
