use super::*;

fn treatment(actor: u64, patient: u64) -> Job {
    Job {
        actor: PersonId(actor),
        area: AreaId(1),
        completes: SimTime(1000),
        work: Work::Treat {
            patient: PersonId(patient),
            skill_ms: 0,
            elapsed: 0,
        },
    }
}
fn consent(giver: u64, patient: u64) -> Offers {
    Offers {
        caregiver: PersonId(giver),
        patients: BTreeMap::from([(
            PersonId(patient),
            Offer {
                caregiver: PersonId(giver),
                patient: PersonId(patient),
                herbs: InventoryId(10),
                water: InventoryId(11),
                expires: SimTime(1000),
                accepted: false,
            },
        )]),
    }
}
fn fixture() -> (Runtime, Store) {
    let mut store = crate::component_store();
    insert_job(&mut store, treatment(3, 1)).unwrap();
    insert_job(&mut store, treatment(4, 2)).unwrap();
    insert_job(&mut store, treatment(5, 6)).unwrap();
    insert_offers(&mut store, PersonId(3), consent(3, 1)).unwrap();
    insert_offers(&mut store, PersonId(5), consent(5, 6)).unwrap();
    (Runtime::default(), store)
}
#[test]
fn cancellation_union_retains_exact_rows_when_a_job_or_offer_moves_between_actor_scopes() {
    for change_offer in [false, true] {
        let (runtime, mut store) = fixture();
        let plan = runtime.prepare_cancel_many(&store, &[PersonId(1), PersonId(2)]);
        if change_offer {
            insert_offers(&mut store, PersonId(3), consent(3, 2)).unwrap();
        } else {
            insert_job(&mut store, treatment(3, 2)).unwrap();
        }
        let before = store.clone();
        assert_eq!(plan.validate(&runtime, &store), Err(Error::Unavailable));
        assert!(
            plan.stage_into(&runtime, &store, &mut Batch::new())
                .is_err()
        );
        assert_eq!(store, before);
    }
}
#[test]
fn final_cancellation_union_rejects_new_or_entering_jobs_and_offers_but_allows_unrelated_scopes() {
    let (runtime, store) = fixture();
    for kind in 0..5 {
        let plan = runtime.prepare_cancel_many(&store, &[PersonId(1), PersonId(2)]);
        let mut batch = Batch::new();
        let finish = plan.stage_into(&runtime, &store, &mut batch).unwrap();
        match kind {
            0 => {
                batch.insert(StableId(7), treatment(7, 2));
            }
            1 => {
                batch.insert(StableId(5), treatment(5, 1));
            }
            2 => {
                batch.insert(StableId(7), consent(7, 2));
            }
            3 => {
                batch.insert(StableId(5), consent(5, 1));
            }
            _ => {
                let mut unrelated = treatment(5, 6);
                unrelated.work = Work::Treat {
                    patient: PersonId(6),
                    skill_ms: 10,
                    elapsed: 1,
                };
                batch.insert(StableId(5), unrelated);
                let mut unrelated = consent(5, 6);
                unrelated.patients.get_mut(&PersonId(6)).unwrap().expires = SimTime(2000);
                batch.insert(StableId(5), unrelated);
            }
        }
        assert_eq!(
            finish.validate_batch(&runtime, &store, &batch),
            if kind == 4 {
                Ok(())
            } else {
                Err(Error::Unavailable)
            }
        );
        assert_eq!(jobs(&store).count(), 3);
        assert_eq!(offer_rows(&store).count(), 2);
    }
}
#[test]
fn shelter_scope_union_keeps_exact_occupants_and_allows_other_shelters() {
    let runtime = Runtime::default();
    let mut store = crate::component_store();
    for (actor, shelter) in [(1, 10), (2, 20), (3, 30)] {
        insert_occupancy(
            &mut store,
            PersonId(actor),
            Occupancy {
                shelter: ShelterId(shelter),
            },
        )
        .unwrap();
    }
    let prepare = |store: &Store| {
        let mut change = PreparedHealth::new(&runtime);
        for shelter in [ShelterId(10), ShelterId(20)] {
            change
                .occupancy_reads
                .insert(shelter, shelter_members(store, shelter));
        }
        change
    };
    for (actor, shelter, allowed) in [(1, 20, false), (4, 20, false), (3, 31, true)] {
        let mut batch = Batch::new();
        let finish = prepare(&store)
            .stage_into(&runtime, &store, &mut batch)
            .unwrap();
        batch.insert(
            StableId(actor),
            Occupancy {
                shelter: ShelterId(shelter),
            },
        );
        assert_eq!(
            finish.validate_batch(&runtime, &store, &batch),
            if allowed {
                Ok(())
            } else {
                Err(Error::Unavailable)
            }
        );
    }
    let prepared = prepare(&store);
    insert_occupancy(
        &mut store,
        PersonId(1),
        Occupancy {
            shelter: ShelterId(20),
        },
    )
    .unwrap();
    assert_eq!(prepared.validate(&runtime, &store), Err(Error::Unavailable));
}
#[test]
fn indexed_cancellation_preserves_first_actor_effect_order_and_coalesces_shared_work() {
    let (runtime, mut store) = fixture();
    let grouped = runtime
        .prepare_cancel_many(
            &store,
            &[PersonId(1), PersonId(3), PersonId(2), PersonId(1)],
        )
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(
        grouped.iter().map(|(actor, _)| *actor).collect::<Vec<_>>(),
        vec![PersonId(1), PersonId(3), PersonId(2)]
    );
    assert_eq!(
        grouped
            .iter()
            .flat_map(|(_, effects)| effects)
            .filter(|effect| matches!(effect, HealthEffect::ClearWork { .. }))
            .count(),
        2
    );
    assert_eq!(
        jobs(&store).map(|(actor, _)| actor).collect::<Vec<_>>(),
        vec![PersonId(5)]
    );
    assert_eq!(
        all_offers(&store)
            .map(|(giver, patient, _)| (giver, patient))
            .collect::<Vec<_>>(),
        vec![(PersonId(5), PersonId(6))]
    );
}

#[test]
fn cancellation_projection_clears_only_matching_health_work() {
    let effects = vec![HealthEffect::ClearWork {
        actor: PersonId(3),
        completes: SimTime(8),
    }];
    assert_eq!(
        cancellation_activity_resets(&effects, |_| HealthActivityState::Working {
            completes: SimTime(8)
        }),
        vec![PersonId(3)]
    );
    assert!(
        cancellation_activity_resets(&effects, |_| HealthActivityState::Working {
            completes: SimTime(9)
        })
        .is_empty()
    );
    assert!(cancellation_activity_resets(&effects, |_| HealthActivityState::Other).is_empty());
}

#[test]
fn water_job_reservations_select_only_consuming_native_jobs_and_requested_stock() {
    let (owner, mut store) = fixture();
    for (actor, work) in [
        (
            PersonId(7),
            Work::Drink {
                source: InventoryId(10),
                quantity: Quantity(3),
            },
        ),
        (
            PersonId(8),
            Work::Boil {
                source: InventoryId(11),
                quantity: Quantity(5),
                tool: InventoryId(12),
            },
        ),
    ] {
        insert_job(
            &mut store,
            Job {
                actor,
                area: AreaId(1),
                completes: SimTime(1000),
                work,
            },
        )
        .unwrap();
    }
    assert_eq!(
        owner.stock_reservations(&store, None),
        vec![
            (PersonId(7), InventoryId(10), Quantity(3)),
            (PersonId(8), InventoryId(11), Quantity(5))
        ]
    );
    assert_eq!(
        owner.stock_reservations(&store, Some(InventoryId(11))),
        vec![(PersonId(8), InventoryId(11), Quantity(5))]
    );
}
