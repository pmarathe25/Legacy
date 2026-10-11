use crate::*;
use game_core::*;

fn world() -> (Runtime, game_ecs::Store) {
    let runtime = Runtime {
        rules: Rules::fixture(),
        ..Runtime::default()
    };
    let mut store = component_store();
    for actor in [PersonId(1), PersonId(2)] {
        insert_clinical(
            &mut store,
            actor,
            Clinical {
                thirst: 90_000,
                ..Clinical::default()
            },
        )
        .unwrap();
    }
    (runtime, store)
}

fn actor(id: u64, now: u64) -> ActorFacts {
    ActorFacts {
        actor: PersonId(id),
        now: SimTime(now),
        area: Some(AreaId(10)),
        present: true,
        capable: true,
        at_area: true,
        carried: false,
        tactical: false,
    }
}
fn stock(id: u64, good: Good, q: u64) -> StockFacts {
    StockFacts {
        id: InventoryId(id),
        good,
        quantity: Quantity(q),
        available: Quantity(q),
        usable: true,
        tool_available: true,
    }
}
fn physiology(world: &Runtime) -> ClinicalPhysiology {
    clinical_physiology(&world.rules, 1_000, 0).unwrap()
}
fn patient(world: &Runtime) -> PatientFacts {
    PatientFacts {
        id: PersonId(2),
        present: true,
        contact: true,
        resting: true,
        physiology: physiology(world),
    }
}

#[test]
fn drinking_is_timed_and_closing_permission_and_quantity_are_revalidated() {
    let (mut world, mut store) = world();
    let water = stock(20, Good::Water, 2_000);
    let change = world
        .prepare_action(
            &store,
            actor(1, 0),
            &HealthAction::Drink { source: water.id },
            ActionFacts::Drink {
                water,
                physiology: physiology(&world),
            },
        )
        .unwrap();
    assert!(
        !change
            .effects()
            .iter()
            .any(|e| matches!(e, HealthEffect::Consume { .. }))
    );
    change.commit(&mut world, &mut store).unwrap();
    let end = job(&store, PersonId(1)).unwrap().completes;
    assert_eq!(clinical(&store, PersonId(1)).unwrap().thirst, 90_000);
    assert!(
        world
            .prepare_finish(
                &store,
                PersonId(1),
                SimTime(end.0 - 1),
                true,
                FinishFacts::Drink {
                    water,
                    physiology: physiology(&world)
                }
            )
            .is_err()
    );
    let denied = world
        .prepare_finish(
            &store,
            PersonId(1),
            end,
            true,
            FinishFacts::Drink {
                water: StockFacts {
                    usable: false,
                    ..water
                },
                physiology: physiology(&world),
            },
        )
        .unwrap();
    let effects = denied.commit(&mut world, &mut store).unwrap();
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, HealthEffect::Consume { .. }))
    );
    assert_eq!(clinical(&store, PersonId(1)).unwrap().thirst, 90_000);
}
#[test]
fn unsafe_intake_prepares_physical_consumption_and_one_actual_infection_dose() {
    let (mut world, mut store) = world();
    let water = stock(20, Good::Water, 1_000);
    insert_water_lot(&mut store, water.id, WaterLot::new(1_000, 1_000_000)).unwrap();
    world
        .prepare_action(
            &store,
            actor(1, 0),
            &HealthAction::Drink { source: water.id },
            ActionFacts::Drink {
                water,
                physiology: physiology(&world),
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    let end = job(&store, PersonId(1)).unwrap().completes;
    let effects = world
        .prepare_finish(
            &store,
            PersonId(1),
            end,
            true,
            FinishFacts::Drink {
                water,
                physiology: physiology(&world),
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(effects.contains(&HealthEffect::Consume {
        actor: PersonId(1),
        source: water.id,
        quantity: world.rules.drink_quantity
    }));
    assert!(
        clinical(&store, PersonId(1))
            .unwrap()
            .illnesses
            .contains_key(&IllnessKind::Enteric)
    );
    // Quantity/lot removal belongs to the stock event port, not a second debit here.
    assert_eq!(water_lot(&store, water.id).unwrap().quantity(), 1_000);
}
#[test]
fn treatment_requires_specific_willing_offer_acceptance_supplies_contact_and_time() {
    let (mut world, mut store) = world();
    let herbs = stock(21, Good::Herb, 1_000);
    let water = stock(20, Good::Water, 1_000);
    let treatment = HealthAction::Treat {
        patient: PersonId(2),
        herbs: herbs.id,
        water: water.id,
    };
    assert!(
        world
            .prepare_action(
                &store,
                actor(1, 0),
                &treatment,
                ActionFacts::Treat {
                    patient: patient(&world),
                    medicine: 1,
                    herbs,
                    water
                }
            )
            .is_err()
    );
    world
        .prepare_action(
            &store,
            actor(1, 0),
            &HealthAction::OfferTreatment {
                patient: PersonId(2),
                herbs: herbs.id,
                water: water.id,
            },
            ActionFacts::OfferCare {
                patient: patient(&world),
                medicine: 1,
                herbs,
                water,
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    world
        .prepare_action(
            &store,
            actor(2, 0),
            &HealthAction::AcceptTreatment {
                caregiver: PersonId(1),
            },
            ActionFacts::AcceptCare {
                giver_present: true,
                contact: true,
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(
        world
            .prepare_action(
                &store,
                actor(1, 0),
                &treatment,
                ActionFacts::Treat {
                    patient: PatientFacts {
                        contact: false,
                        ..patient(&world)
                    },
                    medicine: 1,
                    herbs,
                    water
                }
            )
            .is_err()
    );
    let effects = world
        .prepare_action(
            &store,
            actor(1, 0),
            &treatment,
            ActionFacts::Treat {
                patient: patient(&world),
                medicine: 1,
                herbs,
                water,
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(e, HealthEffect::Consume { .. }))
            .count(),
        2
    );
    assert!(clinical(&store, PersonId(2)).unwrap().treatment.is_none());
    let end = job(&store, PersonId(1)).unwrap().completes;
    world
        .prepare_finish(
            &store,
            PersonId(1),
            end,
            true,
            FinishFacts::Treat {
                patient: patient(&world),
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(clinical(&store, PersonId(2)).unwrap().treatment.is_some());
}
#[test]
fn refusal_cancels_the_selected_course_without_refunds_or_fabricated_healing() {
    let (mut world, mut store) = world();
    let herbs = stock(21, Good::Herb, 1_000);
    let water = stock(20, Good::Water, 1_000);
    insert_offer(
        &mut store,
        Offer {
            caregiver: PersonId(1),
            patient: PersonId(2),
            herbs: herbs.id,
            water: water.id,
            expires: SimTime(180_000),
            accepted: true,
        },
    )
    .unwrap();
    world
        .prepare_action(
            &store,
            actor(1, 0),
            &HealthAction::Treat {
                patient: PersonId(2),
                herbs: herbs.id,
                water: water.id,
            },
            ActionFacts::Treat {
                patient: patient(&world),
                medicine: 1,
                herbs,
                water,
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    // Starting consumes this acceptance. A later explicit offer can be refused;
    // refusal also withdraws that patient's participation in a running course.
    insert_offer(
        &mut store,
        Offer {
            caregiver: PersonId(1),
            patient: PersonId(2),
            herbs: herbs.id,
            water: water.id,
            expires: SimTime(180_000),
            accepted: true,
        },
    )
    .unwrap();
    let effects = world
        .prepare_action(
            &store,
            actor(2, 1),
            &HealthAction::RefuseTreatment {
                caregiver: PersonId(1),
            },
            ActionFacts::RefuseCare,
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(jobs(&store).next().is_none());
    assert!(clinical(&store, PersonId(2)).unwrap().treatment.is_none());
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, HealthEffect::Consume { .. }))
    );
}
#[test]
fn shelter_cost_time_access_capacity_and_occupancy_are_owned_decisions() {
    let (mut world, mut store) = world();
    let timber = stock(22, Good::Timber, 10_000);
    let tool = stock(23, Good::Tool, 1_000);
    let action = HealthAction::BuildShelter {
        timber: timber.id,
        tool: tool.id,
        capacity: 1,
        insulation: 500,
    };
    assert!(
        world
            .prepare_action(
                &store,
                ActorFacts {
                    tactical: true,
                    ..actor(1, 0)
                },
                &action,
                ActionFacts::ShelterBuild { timber, tool }
            )
            .is_err()
    );
    world
        .prepare_action(
            &store,
            actor(1, 0),
            &action,
            ActionFacts::ShelterBuild { timber, tool },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    let end = job(&store, PersonId(1)).unwrap().completes;
    world
        .prepare_finish(
            &store,
            PersonId(1),
            end,
            true,
            FinishFacts::Shelter { id: ShelterId(30) },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    world
        .prepare_action(
            &store,
            actor(1, end.0),
            &HealthAction::EnterShelter {
                shelter: ShelterId(30),
            },
            ActionFacts::ShelterOccupancy {
                leaving: vec![PersonId(1)],
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    world
        .prepare_action(
            &store,
            actor(1, end.0),
            &HealthAction::ShelterAccess {
                shelter: ShelterId(30),
                person: PersonId(2),
                allowed: true,
            },
            ActionFacts::ShelterAccess {
                recipient_exists: true,
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert!(
        world
            .prepare_action(
                &store,
                actor(2, end.0),
                &HealthAction::EnterShelter {
                    shelter: ShelterId(30)
                },
                ActionFacts::ShelterOccupancy {
                    leaving: vec![PersonId(2)]
                }
            )
            .is_err()
    );
    world
        .prepare_action(
            &store,
            actor(1, end.0),
            &HealthAction::ShelterAccess {
                shelter: ShelterId(30),
                person: PersonId(2),
                allowed: false,
            },
            ActionFacts::ShelterAccess {
                recipient_exists: true,
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    assert_eq!(
        occupancy(&store, PersonId(1)).unwrap().shelter,
        ShelterId(30)
    );
}
#[test]
fn stale_water_and_owned_state_or_numeric_overflow_fail_without_partial_transition() {
    let (mut world, mut store) = world();
    let water = stock(20, Good::Water, 1_000);
    world
        .prepare_action(
            &store,
            actor(1, 0),
            &HealthAction::Drink { source: water.id },
            ActionFacts::Drink {
                water,
                physiology: physiology(&world),
            },
        )
        .unwrap()
        .commit(&mut world, &mut store)
        .unwrap();
    let end = job(&store, PersonId(1)).unwrap().completes;
    let prepared = world
        .prepare_finish(
            &store,
            PersonId(1),
            end,
            true,
            FinishFacts::Drink {
                water,
                physiology: physiology(&world),
            },
        )
        .unwrap();
    insert_water_lot(&mut store, water.id, WaterLot::new(1_000, 10)).unwrap();
    let before = (world.clone(), store.snapshot().unwrap());
    assert!(prepared.commit(&mut world, &mut store).is_err());
    assert_eq!((world.clone(), store.snapshot().unwrap()), before);
    assert!(
        world
            .prepare_action(
                &store,
                actor(1, u64::MAX),
                &HealthAction::Drink { source: water.id },
                ActionFacts::Drink {
                    water,
                    physiology: physiology(&world)
                }
            )
            .is_err()
    );
}

#[test]
fn ongoing_work_rules_require_actual_location_tool_and_patient_cooperation() {
    let mut job = Job {
        actor: PersonId(1),
        area: AreaId(10),
        completes: SimTime(100),
        work: Work::Drink {
            source: InventoryId(20),
            quantity: Quantity(1_000),
        },
    };
    let ready = ProgressFacts {
        area: Some(AreaId(10)),
        fatigue: 10,
        fatigue_limit: 10,
        tool_available: true,
        patient_present: true,
        patient_contact: true,
        patient_resting: false,
    };
    assert!(can_progress(&job, ready));
    assert!(!can_progress(
        &job,
        ProgressFacts {
            area: Some(AreaId(11)),
            ..ready
        }
    ));
    job.work = Work::Boil {
        source: InventoryId(20),
        quantity: Quantity(1_000),
        tool: InventoryId(21),
    };
    assert!(!can_progress(&job, ready));
    assert!(!can_progress(
        &job,
        ProgressFacts {
            fatigue: 0,
            tool_available: false,
            ..ready
        }
    ));
    assert!(can_progress(
        &job,
        ProgressFacts {
            fatigue: 0,
            ..ready
        }
    ));
    job.work = Work::Treat {
        patient: PersonId(2),
        skill_ms: 0,
        elapsed: 0,
    };
    assert!(!can_progress(
        &job,
        ProgressFacts {
            fatigue: 0,
            ..ready
        }
    ));
    assert!(can_progress(
        &job,
        ProgressFacts {
            fatigue: 0,
            patient_resting: true,
            ..ready
        }
    ));
    assert!(!can_progress(
        &job,
        ProgressFacts {
            fatigue: 0,
            patient_resting: true,
            patient_contact: false,
            ..ready
        }
    ));
    job.work = Work::Treat {
        patient: PersonId(1),
        skill_ms: 0,
        elapsed: 0,
    };
    assert!(can_progress(
        &job,
        ProgressFacts {
            fatigue: 0,
            ..ready
        }
    ));
}
