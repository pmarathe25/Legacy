use game_api::*;
use game_assembly::{
    decisions::{Aspiration, Personality, Relationship},
    development::PregnancyState,
    *,
};

fn sim(workers: usize) -> Simulation {
    Simulation::life_fixture(
        1,
        Execution {
            workers,
            reverse_jobs: true,
        },
    )
    .unwrap()
}

fn same(a: &Simulation, b: &Simulation) {
    fn difference(
        a: &serde_json::Value,
        b: &serde_json::Value,
        path: String,
        out: &mut Vec<String>,
    ) {
        if a == b || out.len() >= 12 {
            return;
        }
        match (a, b) {
            (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                for (key, value) in a {
                    difference(
                        value,
                        b.get(key).unwrap_or(&serde_json::Value::Null),
                        format!("{path}.{key}"),
                        out,
                    );
                }
            }
            (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
                if a.len() != b.len() {
                    out.push(format!("{path}.length: {} != {}", a.len(), b.len()));
                }
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    difference(a, b, format!("{path}[{i}]"), out);
                }
            }
            _ => out.push(format!("{path}: {a} != {b}")),
        }
    }
    let mut out = Vec::new();
    difference(
        &serde_json::to_value(a.canonical_state()).unwrap(),
        &serde_json::to_value(b.canonical_state()).unwrap(),
        "$".into(),
        &mut out,
    );
    assert!(
        a.canonical_state() == b.canonical_state(),
        "{}",
        out.join("\n")
    );
}
fn first_child(sim: &Simulation) -> PersonId {
    game_lifecycle::all_lives(&sim.canonical_state().components)
        .map(|(_, row)| row)
        .find(|l| !l.parents.is_empty())
        .unwrap()
        .id
}
fn food(sim: &Simulation) -> InventoryId {
    sim.canonical_state()
        .inventories
        .values()
        .find(|i| i.good == Good::Food && i.location == InventoryLocation::Area(AreaId(1)))
        .unwrap()
        .id
}

#[test]
fn two_autonomous_adults_choose_a_timed_conception_and_a_real_birth() {
    let mut sim = sim(4);
    let initial = sim.canonical_state().people.len();
    sim.advance(MINUTE).unwrap();
    let p = game_lifecycle::all_pregnancies(&sim.canonical_state().components)
        .map(|(_, row)| row)
        .next()
        .unwrap();
    assert_eq!(
        p.state,
        PregnancyState::Conceiving {
            completes: SimTime(120_000)
        }
    );
    assert_eq!(sim.canonical_state().people.len(), initial);
    sim.advance(MINUTE).unwrap();
    assert!(matches!(
        game_lifecycle::all_pregnancies(&sim.canonical_state().components)
            .map(|(_, row)| row)
            .next()
            .unwrap()
            .state,
        PregnancyState::Gestating {
            due: SimTime(300_000)
        }
    ));
    sim.advance(SimDuration(179_999)).unwrap();
    assert_eq!(sim.canonical_state().people.len(), initial);
    sim.advance(SimDuration(1)).unwrap();
    assert_eq!(sim.canonical_state().people.len(), initial + 1);
    let child = first_child(&sim);
    let life =
        &game_lifecycle::life(&sim.canonical_state().components, child).expect("life exists");
    assert_eq!(life.born, SimTime(300_000));
    assert_eq!(life.stage, LifeStage::Infant);
    assert_eq!(life.parents.len(), 2);
    assert!(life.caregivers.is_empty());
    assert_eq!(sim.canonical_state().people.last().unwrap().work_units, 0);
    assert!(
        game_characters::decisions::mind::<game_api::Command>(
            &sim.canonical_state().components,
            child
        )
        .expect("mind exists")
        .values
        .is_empty()
    );
    let inventory = sim.canonical_state().people.last().unwrap().inventory;
    assert_eq!(
        sim.canonical_state().inventories[&inventory].quantity,
        Quantity(0)
    );
    assert!(
        !sim.canonical_state().inventories[&inventory]
            .access
            .contains(&life.parents[0])
    );
}

#[test]
fn a_player_request_cannot_supply_the_other_adults_consent() {
    let mut sim = sim(1);
    let ruler = sim.canonical_state().ruler;
    let partner = sim.canonical_state().people[3].id;
    sim.debug_set_personality(partner, Personality::restful(), Aspiration::Comfort)
        .unwrap();
    let allocator = sim.canonical_state().next_id;
    sim.submit(Submission {
        caller_sequence: 1,
        command: Command::Conceive { partner },
    })
    .unwrap();
    assert!(
        game_lifecycle::all_pregnancies(&sim.canonical_state().components)
            .next()
            .is_none()
    );
    assert_eq!(sim.canonical_state().next_id, allocator);
    assert!(sim.canonical_state().events.iter().any(|e| e.kind
        == EventKind::ConceptionRefused {
            actor: ruler,
            partner
        }));
    let people: Vec<_> = sim.canonical_state().people.iter().map(|p| p.id).collect();
    for id in people {
        sim.debug_set_personality(id, Personality::restful(), Aspiration::Comfort)
            .unwrap();
    }
    sim.advance(SimDuration(600_000)).unwrap();
    assert_eq!(sim.canonical_state().people.len(), 15);
    assert!(
        game_lifecycle::all_pregnancies(&sim.canonical_state().components)
            .next()
            .is_none()
    );
}

#[test]
fn interrupting_conception_cancels_both_primary_actions_and_creates_no_child() {
    let mut sim = sim(4);
    sim.advance(MINUTE).unwrap();
    let p = game_lifecycle::all_pregnancies(&sim.canonical_state().components)
        .map(|(_, row)| row)
        .next()
        .unwrap()
        .clone();
    sim.debug_actor_command(p.gestator, Command::Cancel)
        .unwrap();
    assert_eq!(
        *sim.canonical_state()
            .components
            .activity(p.partner)
            .expect("body exists"),
        Activity::Idle
    );
    assert_eq!(
        game_lifecycle::pregnancy(&sim.canonical_state().components, p.id)
            .expect("pregnancy exists")
            .state,
        PregnancyState::Ended
    );
    for parent in [p.gestator, p.partner] {
        sim.debug_set_personality(parent, Personality::restful(), Aspiration::Comfort)
            .unwrap();
    }
    sim.advance(SimDuration(400_000)).unwrap();
    assert_eq!(sim.canonical_state().people.len(), 15);
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn a_broken_relationship_does_not_undo_gestation_but_gestator_death_does() {
    let mut sim = sim(1);
    sim.advance(SimDuration(120_000)).unwrap();
    let p = game_lifecycle::all_pregnancies(&sim.canonical_state().components)
        .map(|(_, row)| row)
        .next()
        .unwrap()
        .clone();
    sim.debug_set_relationship(
        p.gestator,
        p.partner,
        Relationship {
            trust: -1_000,
            affection: -1_000,
        },
    )
    .unwrap();
    sim.debug_set_relationship(
        p.partner,
        p.gestator,
        Relationship {
            trust: -1_000,
            affection: -1_000,
        },
    )
    .unwrap();
    let mut after_death = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    after_death.debug_injure(p.gestator, 180_000).unwrap();
    sim.advance(SimDuration(180_000)).unwrap();
    after_death.advance(SimDuration(180_000)).unwrap();
    assert_eq!(sim.canonical_state().people.len(), 16);
    assert_eq!(after_death.canonical_state().people.len(), 15);
    assert_eq!(
        game_lifecycle::pregnancy(&after_death.canonical_state().components, p.id)
            .expect("pregnancy exists")
            .state,
        PregnancyState::Ended
    );
    Simulation::load(&after_death.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn someone_other_than_a_parent_can_choose_care_and_food_is_actually_consumed() {
    let mut sim = sim(4);
    sim.advance(SimDuration(300_000)).unwrap();
    let child = first_child(&sim);
    let caregiver = sim.canonical_state().people[5].id;
    let source = food(&sim);
    let before = sim.canonical_state().inventories[&source].quantity.0;
    sim.debug_actor_command(caregiver, Command::Care { child, source })
        .unwrap();
    let second = sim.canonical_state().people[6].id;
    assert_eq!(
        sim.debug_actor_command(second, Command::Care { child, source }),
        Err(Error::Unavailable)
    );
    sim.advance(MINUTE).unwrap();
    assert!(sim.canonical_state().inventories[&source].quantity.0 <= before - 1_000);
    let life =
        &game_lifecycle::life(&sim.canonical_state().components, child).expect("life exists");
    assert!(life.caregivers.contains(&caregiver));
    assert!(!life.parents.contains(&caregiver));
    assert_eq!(
        game_physiology::body(&sim.canonical_state().components, child)
            .expect("body exists")
            .vitals
            .hunger,
        0
    );
    assert!(
        game_characters::decisions::mind::<game_api::Command>(
            &sim.canonical_state().components,
            child
        )
        .expect("mind exists")
        .relationships[&caregiver]
            .affection
            > 0
    );
    assert!(sim.canonical_state().events.iter().any(|e| matches!(e.kind, EventKind::Cared { caregiver: id, child: baby, quantity: Quantity(1_000), .. } if id == caregiver && baby == child)));
}

#[test]
fn public_water_care_uses_configured_drink_dose_and_replays_an_active_job() {
    for dose in [Quantity(250), Quantity(2_000)] {
        let mut sim = sim(1);
        let mut rules = game_assembly::health::Rules::human();
        rules.drink_quantity = dose;
        let relief = rules.drink_relief;
        sim.initialize_health(rules).unwrap();
        sim.advance(SimDuration(300_000)).unwrap();
        let child = first_child(&sim);
        let caregiver = sim.canonical_state().ruler;
        let source = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(AreaId(1)),
                dose,
                dose,
                std::collections::BTreeSet::from([caregiver]),
            )
            .unwrap();
        let command = Command::Care { child, source };
        let view = sim.observe();
        assert!(view.available_commands.contains(&command));
        let before_thirst = game_health::clinical(&sim.canonical_state().components, child)
            .expect("clinical state exists")
            .thirst;
        let before_hunger = game_physiology::body(&sim.canonical_state().components, child)
            .expect("body exists")
            .vitals
            .hunger;
        let duration = sim
            .canonical_state()
            .physical
            .as_ref()
            .unwrap()
            .rules
            .eating_time;
        sim.submit(Submission {
            caller_sequence: 1,
            command,
        })
        .unwrap();
        sim.advance(SimDuration(duration.0 / 2)).unwrap();
        assert_eq!(sim.canonical_state().inventories[&source].quantity, dose);
        let mut loaded = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(SimDuration(duration.0 - duration.0 / 2))
            .unwrap();
        loaded
            .advance(SimDuration(duration.0 - duration.0 / 2))
            .unwrap();
        same(&sim, &loaded);
        assert_eq!(
            sim.canonical_state().inventories[&source].quantity,
            Quantity(0)
        );
        assert_eq!(
            game_health::clinical(&sim.canonical_state().components, child)
                .expect("clinical state exists")
                .thirst,
            (before_thirst + duration.0).saturating_sub(relief)
        );
        assert_eq!(
            game_physiology::body(&sim.canonical_state().components, child)
                .expect("body exists")
                .vitals
                .hunger,
            before_hunger + duration.0
        );
        assert!(
            sim.canonical_state()
                .events
                .iter()
                .any(|event| matches!(event.kind,
            EventKind::Cared { caregiver: actual, child: recipient, source: lot, quantity }
                if actual == caregiver && recipient == child && lot == source && quantity == dose))
        );
    }
}

#[test]
fn pregnancies_births_aging_and_care_preserve_chunking_and_worker_equivalence() {
    let mut reference = sim(1);
    reference.advance(SimDuration(720_003)).unwrap();
    for workers in [2, 4] {
        let mut split = sim(workers);
        for dt in [1, 119_999, 13_003, 66_999, 100_003, 120_000, 299_998] {
            split.advance(SimDuration(dt)).unwrap();
        }
        same(&reference, &split);
        let mut loaded = Simulation::load(&split.save().unwrap(), Execution::default()).unwrap();
        split.advance(SimDuration(100_007)).unwrap();
        loaded.advance(SimDuration(100_007)).unwrap();
        same(&split, &loaded);
    }
}

#[test]
fn mortals_age_and_die_without_deleting_goods_while_guardians_remain() {
    let mut sim = sim(1);
    let source = food(&sim);
    for sequence in 1..=3 {
        sim.submit(Submission {
            caller_sequence: sequence,
            command: Command::Eat { source },
        })
        .unwrap();
        sim.advance(SimDuration(300_000)).unwrap();
    }
    let ruler = sim.canonical_state().ruler;
    assert!(
        !game_physiology::body(&sim.canonical_state().components, ruler)
            .expect("body exists")
            .vitals
            .alive
    );
    assert_eq!(sim.time(), SimTime(900_000));
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .any(|e| e.time == SimTime(900_000) && e.kind == EventKind::Died { actor: ruler })
    );
    for guardian in sim
        .canonical_state()
        .people
        .iter()
        .filter(|p| p.role == Role::Guardian)
    {
        assert!(
            game_physiology::body(&sim.canonical_state().components, guardian.id)
                .expect("body exists")
                .vitals
                .alive
        );
    }
    assert!(
        sim.canonical_state()
            .inventories
            .contains_key(&sim.canonical_state().people[0].inventory)
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn a_motivated_nonparent_responds_to_observed_distress_without_player_orders() {
    let mut sim = sim(4);
    sim.advance(SimDuration(300_000)).unwrap();
    let child = first_child(&sim);
    let caregiver = sim.canonical_state().people[5].id;
    let mut personality = Personality::restful();
    personality.empathy = 1_000;
    sim.debug_set_personality(caregiver, personality, Aspiration::SupportOthers)
        .unwrap();
    sim.advance(SimDuration(180_000)).unwrap();
    let source = food(&sim);
    sim.debug_actor_command(caregiver, Command::Eat { source })
        .unwrap();
    sim.advance(SimDuration(180_000)).unwrap();
    assert!(sim.canonical_state().events.iter().any(|e| matches!(e.kind, EventKind::Cared { caregiver: id, child: baby, .. } if id == caregiver && baby == child)));
    assert!(
        game_lifecycle::life(&sim.canonical_state().components, child)
            .expect("life exists")
            .caregivers
            .contains(&caregiver)
    );
}

#[test]
fn saving_mid_conception_and_gestation_preserves_continuation_and_new_ids() {
    let mut original = sim(1);
    original.advance(SimDuration(72_345)).unwrap();
    let mut loaded = Simulation::load(
        &original.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    original.advance(SimDuration(60_000)).unwrap();
    loaded.advance(SimDuration(60_000)).unwrap();
    same(&original, &loaded);
    let mut loaded = Simulation::load(&original.save().unwrap(), Execution::default()).unwrap();
    let allocator = original.canonical_state().next_id;
    original.advance(SimDuration(200_000)).unwrap();
    loaded.advance(SimDuration(200_000)).unwrap();
    same(&original, &loaded);
    assert!(first_child(&original).0 >= allocator);
    let mut ids = std::collections::BTreeSet::new();
    for id in original
        .canonical_state()
        .people
        .iter()
        .map(|p| p.id.0)
        .chain(original.canonical_state().inventories.keys().map(|id| id.0))
        .chain(
            game_lifecycle::all_pregnancies(&original.canonical_state().components)
                .map(|(id, _)| id)
                .map(|id| id.0),
        )
    {
        assert!(ids.insert(id));
    }
}

#[test]
fn current_saves_roundtrip_and_other_schema_versions_are_rejected() {
    let sim = Simulation::life_fixture(7, Execution::default()).unwrap();
    let bytes = sim.save().unwrap();
    let restored = Simulation::load(&bytes, Execution::default()).unwrap();
    assert_eq!(sim.canonical_state(), restored.canonical_state());
    let mut different: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    different["version"] = 0.into();
    assert!(
        Simulation::load(
            &serde_json::to_vec(&different).unwrap(),
            Execution::default()
        )
        .is_err()
    );
}
