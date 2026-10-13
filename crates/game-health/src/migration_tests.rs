use crate::*;
use game_core::*;
use game_ecs::{Batch, StableId};

fn actor() -> ActorFacts {
    ActorFacts {
        actor: PersonId(1),
        now: SimTime(100),
        area: Some(AreaId(1)),
        present: true,
        capable: true,
        at_area: true,
        carried: false,
        tactical: false,
    }
}
fn stock(id: u64, good: Good, quantity: u64) -> RememberedStock {
    RememberedStock {
        fact: StockKnowledge {
            inventory: InventoryId(id),
            area: AreaId(1),
            good,
            quantity: Quantity(quantity),
            accessible: true,
            observed_at: SimTime(0),
            learned_at: SimTime(0),
            source: KnowledgeSource::Observed(PersonId(1)),
        },
        here: true,
        held: true,
    }
}
fn physical(stock: &RememberedStock) -> StockFacts {
    StockFacts {
        id: stock.fact.inventory,
        good: stock.fact.good,
        quantity: stock.fact.quantity,
        available: stock.fact.quantity,
        usable: true,
        tool_available: true,
    }
}

#[test]
fn shelter_entry_eligibility_uses_stationary_unheld_facts() {
    assert!(can_enter_shelter(true, false));
    assert!(!can_enter_shelter(false, false));
    assert!(!can_enter_shelter(true, true));
    assert!(!can_enter_shelter(false, true));
}

#[test]
fn public_availability_selects_local_intake_boiling_and_consent_without_assembly() {
    let runtime = Runtime {
        rules: Rules::fixture(),
        ..Runtime::default()
    };
    let mut store = component_store();
    insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
    let stocks = vec![
        stock(10, Good::Water, 1000),
        stock(11, Good::Timber, 1000),
        stock(12, Good::Tool, 1000),
    ];
    let input = HealthPlanning {
        now: actor().now,
        area: AreaId(1),
        stationary: true,
        values: IntakeValues {
            risk_tolerance: 500,
        },
        stocks: &stocks,
    };
    let actions = runtime.available_actions(
        &store,
        actor(),
        Some(&input),
        |_| CaregiverAvailability {
            capable: true,
            observable: true,
        },
        |id| {
            stocks
                .iter()
                .find(|stock| stock.fact.inventory == id)
                .map(physical)
                .ok_or(Error::Unavailable)
        },
    );
    assert_eq!(
        actions,
        vec![
            HealthAction::Drink {
                source: InventoryId(10)
            },
            HealthAction::Boil {
                source: InventoryId(10),
                fuel: InventoryId(11),
                tool: InventoryId(12)
            }
        ]
    );
    let mut passive = actor();
    passive.capable = false;
    assert!(
        runtime
            .available_actions(
                &store,
                passive,
                Some(&input),
                |_| unreachable!(),
                |_| unreachable!()
            )
            .is_empty()
    );
    let mut denied = actor();
    denied.present = false;
    assert!(
        runtime
            .available_actions(
                &store,
                denied,
                Some(&input),
                |_| unreachable!(),
                |_| unreachable!()
            )
            .is_empty()
    );
}
#[test]
fn planning_refresh_and_stock_contact_judgments_are_owned() {
    let old = stock(10, Good::Water, 9000);
    let updated = planning_stocks(
        PersonId(1),
        SimTime(100),
        std::iter::once(&old.fact),
        |_| RememberedStockFacts {
            here: true,
            held: false,
            available: Quantity(500),
            tool_available: true,
        },
    );
    assert_eq!(updated[0].fact.quantity, Quantity(500));
    assert_eq!(updated[0].fact.observed_at, SimTime(100));
    let facts = StockAccessFacts {
        good: Good::Water,
        access: true,
        capable: true,
        place: StockPlaceFacts::Carried {
            same_place: true,
            contact_clear: false,
        },
        available: Quantity(1000),
    };
    assert!(!stock_usable(facts, Good::Water, Quantity(1000)));
    assert!(stock_usable(
        StockAccessFacts {
            place: StockPlaceFacts::Carried {
                same_place: true,
                contact_clear: true
            },
            ..facts
        },
        Good::Water,
        Quantity(1000)
    ));
    assert!(!stock_usable(
        StockAccessFacts {
            place: StockPlaceFacts::Area {
                same_area: true,
                actor_tactical: true
            },
            ..facts
        },
        Good::Water,
        Quantity(1000)
    ));
}
#[test]
fn water_transfer_prepares_conserved_load_and_subjective_memory_before_publication() {
    let mut runtime = Runtime::default();
    let mut store = component_store();
    insert_water_lot(&mut store, InventoryId(10), WaterLot::new(1000, 600000)).unwrap();
    insert_water_lot(&mut store, InventoryId(11), WaterLot::new(500, 0)).unwrap();
    let mut known = Knowledge::default();
    known.water.insert(
        InventoryId(10),
        WaterSafetyView {
            inventory: InventoryId(10),
            purified: false,
            suspected_unsafe: true,
            observed_at: SimTime(1),
        },
    );
    insert_knowledge(&mut store, PersonId(1), known).unwrap();
    let reaction = WaterReaction::Transfer {
        actor: PersonId(1),
        source: InventoryId(10),
        destination: InventoryId(11),
        quantity: Quantity(500),
        source_before: Quantity(1000),
        destination_before: Quantity(500),
    };
    let before = store.clone();
    let prepared = runtime
        .prepare_water_reaction(&store, SimTime(10), reaction)
        .unwrap();
    let mut batch = Batch::new();
    let finish = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    finish.validate_batch(&runtime, &store, &batch).unwrap();
    assert_eq!(store, before);
    batch.insert(StableId(1), Knowledge::default());
    assert_eq!(
        finish.validate_batch(&runtime, &store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(store, before);
    runtime
        .prepare_water_reaction(&store, SimTime(10), reaction)
        .unwrap()
        .commit(&mut runtime, &mut store)
        .unwrap();
    assert_eq!(water_lot(&store, InventoryId(10)).unwrap().load(), 300000);
    assert_eq!(water_lot(&store, InventoryId(11)).unwrap().load(), 300000);
    assert!(knowledge(&store, PersonId(1)).unwrap().water[&InventoryId(11)].suspected_unsafe);
    runtime
        .prepare_water_reaction(
            &store,
            SimTime(11),
            WaterReaction::Produce {
                actor: PersonId(1),
                output: InventoryId(11),
                quantity: Quantity(0),
                quantity_before: Quantity(1000),
            },
        )
        .unwrap()
        .commit(&mut runtime, &mut store)
        .unwrap();
    assert!(
        !knowledge(&store, PersonId(1))
            .unwrap()
            .water
            .contains_key(&InventoryId(11))
    );
}
#[test]
fn illness_and_birth_reactions_use_exact_owner_entrypoints_and_reject_stale_inputs() {
    let mut runtime = Runtime {
        rules: Rules::fixture(),
        ..Runtime::default()
    };
    let mut store = component_store();
    insert_clinical(
        &mut store,
        PersonId(1),
        Clinical {
            last_drink: Some((InventoryId(10), SimTime(1))),
            ..Clinical::default()
        },
    )
    .unwrap();
    let reaction = HealthReaction::IllnessStarted {
        actor: PersonId(1),
        kind: IllnessKind::Enteric,
    };
    let prepared = runtime
        .prepare_reaction(&store, SimTime(10), reaction)
        .unwrap();
    insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
    let before = store.clone();
    assert_eq!(
        prepared.commit(&mut runtime, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!(store, before);
    runtime
        .prepare_reaction(
            &store,
            SimTime(10),
            HealthReaction::Born {
                child: PersonId(2),
                inherited_shelter: None,
            },
        )
        .unwrap()
        .commit(&mut runtime, &mut store)
        .unwrap();
    assert_eq!(clinical(&store, PersonId(2)), Some(&Clinical::default()));
    assert_eq!(
        runtime.birth_shelter(&[(PersonId(1), false), (PersonId(2), true)], |parent| {
            (parent == PersonId(2)).then_some(ShelterId(20))
        }),
        Some(ShelterId(20))
    );
}
#[test]
fn actor_interval_and_reconciliation_own_living_selection_job_deadlines_and_effects() {
    let mut runtime = Runtime {
        rules: Rules::fixture(),
        ..Runtime::default()
    };
    let mut store = component_store();
    let physiology = clinical_physiology(&runtime.rules, 1000, 0).unwrap();
    insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
    insert_clinical(&mut store, PersonId(2), Clinical::default()).unwrap();
    insert_job(
        &mut store,
        Job {
            actor: PersonId(1),
            area: AreaId(1),
            completes: SimTime(500),
            work: Work::Drink {
                source: InventoryId(10),
                quantity: Quantity(1000),
            },
        },
    )
    .unwrap();
    let facts = ClinicalActorFacts {
        actor: PersonId(1),
        alive: true,
        thirst_enabled: true,
        resting: false,
        exposure: ExposureInput::default(),
        physiology,
    };
    let actors = [
        facts,
        ClinicalActorFacts {
            actor: PersonId(2),
            alive: false,
            ..facts
        },
    ];
    assert_eq!(
        runtime
            .actor_boundary(&store, SimTime(0), SimTime(1000), &actors, |_| true)
            .unwrap(),
        SimTime(500)
    );
    runtime
        .prepare_actor_interval(&store, SimTime(0), SimTime(100), &actors, |_| (true, 0))
        .unwrap()
        .commit(&runtime, &mut store, |_| true)
        .unwrap();
    assert_eq!(clinical(&store, PersonId(1)).unwrap().thirst, 100);
    assert_eq!(clinical(&store, PersonId(2)).unwrap().thirst, 0);
    assert_eq!(
        runtime.work_deadlines(&store),
        vec![(PersonId(1), SimTime(500))]
    );
    runtime
        .prepare_clinical_reconciliation(&store, SimTime(100), |_| false)
        .unwrap()
        .commit(&mut runtime, &mut store)
        .unwrap();
    assert!(job(&store, PersonId(1)).is_none());
}
#[test]
fn late_membership_addition_rejects_a_guarded_cancellation_without_publication() {
    let runtime = Runtime::default();
    let mut store = component_store();
    insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
    let change = runtime.prepare_cancel(&store, PersonId(1));
    let mut batch = Batch::new();
    let finish = change.stage_into(&runtime, &store, &mut batch).unwrap();
    batch.insert(
        StableId(2),
        Job {
            actor: PersonId(2),
            area: AreaId(1),
            completes: SimTime(10),
            work: Work::Treat {
                patient: PersonId(1),
                skill_ms: 0,
                elapsed: 0,
            },
        },
    );
    assert_eq!(
        finish.validate_batch(&runtime, &store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(jobs(&store).count(), 0);
}

#[test]
fn composed_actual_cargo_consumption_and_health_intake_fail_atomically_then_preserve_conservation()
{
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    schema
        .register::<String>("test/receipt", |row| {
            if row == "invalid" {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut store = game_ecs::Store::with_schema(schema);
    let runtime = Runtime {
        rules: Rules::fixture(),
        ..Runtime::default()
    };
    game_cargo::insert_inventory(
        &mut store,
        game_cargo::Inventory {
            id: InventoryId(10),
            good: Good::Water,
            quantity: Quantity(2000),
            capacity: Quantity(2000),
            location: game_cargo::InventoryLocation::Area(AreaId(1)),
            access: std::collections::BTreeSet::from([PersonId(1)]),
            last_granted: None,
            spoils_at: None,
        },
    )
    .unwrap();
    insert_clinical(
        &mut store,
        PersonId(1),
        Clinical {
            thirst: 100000,
            ..Clinical::default()
        },
    )
    .unwrap();
    insert_water_lot(&mut store, InventoryId(10), WaterLot::new(2000, 800000)).unwrap();
    let source = StockFacts {
        id: InventoryId(10),
        good: Good::Water,
        quantity: Quantity(2000),
        available: Quantity(2000),
        usable: true,
        tool_available: false,
    };
    let before = store.clone();
    for rejected in [true, false] {
        let received = runtime
            .prepare_care_water_received(
                &store,
                PersonId(1),
                source,
                Quantity(1000),
                clinical_physiology(&runtime.rules, 1000, 0).unwrap(),
                SimTime(1),
            )
            .unwrap();
        let cargo =
            game_cargo::prepare_consumptions(&store, &[(source.id, Quantity(1000))]).unwrap();
        let mut batch = Batch::new();
        let intake_finish = received.stage_into(&runtime, &store, &mut batch).unwrap();
        let cargo_guard = cargo.stage_guarded_into(&store, &mut batch).unwrap();
        cargo_guard.validate(&store, &batch).unwrap();
        intake_finish
            .validate_batch(&runtime, &store, &batch)
            .unwrap();
        if rejected {
            batch.insert(StableId(1), "invalid".to_owned());
            assert_eq!(store.apply_batch(batch), Err(Error::Unavailable));
            assert_eq!(store, before);
        } else {
            store.apply_batch(batch).unwrap();
        }
    }
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(10))
            .unwrap()
            .quantity,
        Quantity(1000)
    );
    assert_eq!(water_lot(&store, InventoryId(10)).unwrap().load(), 400000);
    assert_eq!(clinical(&store, PersonId(1)).unwrap().thirst, 0);
}

#[test]
fn timed_drink_effect_operations_and_water_consumption_are_selected_by_owner() {
    let mut runtime = Runtime {
        rules: Rules::fixture(),
        ..Runtime::default()
    };
    let mut store = component_store();
    insert_clinical(
        &mut store,
        PersonId(1),
        Clinical {
            thirst: 100000,
            ..Clinical::default()
        },
    )
    .unwrap();
    insert_water_lot(&mut store, InventoryId(10), WaterLot::new(2000, 800000)).unwrap();
    let water = physical(&stock(10, Good::Water, 2000));
    let physiology = clinical_physiology(&runtime.rules, 1000, 0).unwrap();
    let start = runtime
        .prepare_action(
            &store,
            actor(),
            &HealthAction::Drink {
                source: InventoryId(10),
            },
            ActionFacts::Drink { water, physiology },
        )
        .unwrap();
    let operations = start.operations(|_| HealthActivityState::Other);
    assert_eq!(operations.interruptions, vec![PersonId(1)]);
    assert!(operations.consumptions.is_empty());
    start.commit(&mut runtime, &mut store).unwrap();
    let work = job(&store, PersonId(1)).unwrap().clone();
    assert!(!finish_ready(
        &work,
        FinishReadiness {
            area: Some(AreaId(1)),
            equipment_available: true,
            active_deadline: Some(SimTime(work.completes.0 + 1))
        }
    ));
    assert!(finish_ready(
        &work,
        FinishReadiness {
            area: Some(AreaId(1)),
            equipment_available: false,
            active_deadline: Some(work.completes)
        }
    ));
    let finish = runtime
        .prepare_finish(
            &store,
            PersonId(1),
            work.completes,
            true,
            FinishFacts::Drink { water, physiology },
        )
        .unwrap()
        .with_consumption_reactions(&store, |_| Ok((Good::Water, Quantity(2000))))
        .unwrap();
    let operations = finish.operations(|_| HealthActivityState::Working {
        completes: work.completes,
    });
    assert_eq!(
        operations.consumptions,
        vec![(InventoryId(10), Quantity(1000))]
    );
    assert!(matches!(
        operations.activities[&PersonId(1)],
        HealthActivityChange::Idle
    ));
    finish.commit(&mut runtime, &mut store).unwrap();
    assert_eq!(water_lot(&store, InventoryId(10)).unwrap().quantity(), 1000);
    assert_eq!(water_lot(&store, InventoryId(10)).unwrap().load(), 400000);
    assert_eq!(clinical(&store, PersonId(1)).unwrap().thirst, 0);
}

#[test]
fn care_willingness_owns_self_preservation_and_dependent_signal_thresholds() {
    let runtime = Runtime {
        rules: Rules::fixture(),
        ..Runtime::default()
    };
    let stocks = [stock(10, Good::Herb, 1000), stock(11, Good::Water, 1000)];
    for (hunger, fatigue, urgencies, expected) in [
        (0, 0, vec![19999], true),
        (100, 0, vec![], false),
        (0, 50, vec![], false),
        (0, 0, vec![20000], false),
    ] {
        let choice = runtime.willing_supplies_for(CareWillingnessFacts {
            capable: true,
            medicine: 1,
            busy: false,
            body: WorkNeeds {
                hunger,
                meal_relief: 100,
                fatigue,
                fatigue_limit: 100,
                health: 100,
                max_health: 100,
            },
            dependent_urgencies: &urgencies,
            empathy: 300,
            trust: None,
            stocks: &stocks,
        });
        assert_eq!(choice.is_some(), expected);
    }
    assert_eq!(
        runtime.quantity_examples(Good::Water),
        vec![runtime.rules.drink_quantity, runtime.rules.treatment_water]
    );
}
