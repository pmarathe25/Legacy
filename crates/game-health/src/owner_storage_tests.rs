use crate::*;
use game_core::*;
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

fn shelter_row(id: u64) -> Shelter {
    Shelter {
        id: ShelterId(id),
        area: AreaId(10),
        custodian: PersonId(1),
        access: BTreeSet::from([PersonId(1), PersonId(2), PersonId(3)]),
        capacity: 2,
        insulation: 500,
        built_at: SimTime(0),
        timber_used: Quantity(2_000),
        labor_ms: 60_000,
    }
}
fn promise(caregiver: u64, patient: u64) -> Offer {
    Offer {
        caregiver: PersonId(caregiver),
        patient: PersonId(patient),
        herbs: InventoryId(20),
        water: InventoryId(21),
        expires: SimTime(60_000),
        accepted: false,
    }
}
fn actor(id: u64) -> ActorFacts {
    ActorFacts {
        actor: PersonId(id),
        now: SimTime(0),
        area: Some(AreaId(10)),
        present: true,
        capable: true,
        at_area: true,
        carried: false,
        tactical: false,
    }
}
fn admission(runtime: &Runtime, store: &Store) -> PreparedHealth {
    runtime
        .prepare_action(
            store,
            actor(1),
            &HealthAction::EnterShelter {
                shelter: ShelterId(30),
            },
            ActionFacts::ShelterOccupancy {
                leaving: vec![PersonId(1)],
            },
        )
        .unwrap()
}

#[test]
fn every_health_column_roundtrips_without_runtime_maps_or_foreign_row_loss() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct ForeignTag(u64);
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<ForeignTag>("fixture/tag", |_| Ok(()))
        .unwrap();
    let mut store = Store::with_schema(schema.clone());
    insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
    insert_job(
        &mut store,
        Job {
            actor: PersonId(1),
            area: AreaId(10),
            completes: SimTime(30_000),
            work: Work::Drink {
                source: InventoryId(21),
                quantity: Quantity(1_000),
            },
        },
    )
    .unwrap();
    insert_water_lot(&mut store, InventoryId(21), WaterLot::new(1_000, 500)).unwrap();
    insert_shelter(&mut store, shelter_row(30)).unwrap();
    insert_occupancy(
        &mut store,
        PersonId(1),
        Occupancy {
            shelter: ShelterId(30),
        },
    )
    .unwrap();
    insert_knowledge(&mut store, PersonId(1), Knowledge::default()).unwrap();
    insert_offer(&mut store, promise(1, 2)).unwrap();
    store.insert(StableId(1), ForeignTag(9)).unwrap();
    let bytes = serde_json::to_vec(&store.snapshot().unwrap()).unwrap();
    let mut restored = Store::restore(schema, serde_json::from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(restored, store);
    assert_eq!(
        all_offers(&restored)
            .map(|(giver, patient, _)| (giver, patient))
            .collect::<Vec<_>>(),
        vec![(PersonId(1), PersonId(2))]
    );
    assert_eq!(
        remove_offer(&mut restored, PersonId(1), PersonId(2)).unwrap(),
        Some(promise(1, 2))
    );
    assert!(offers(&restored, PersonId(1)).is_none());
    remove_occupancy(&mut restored, PersonId(1)).unwrap();
    remove_knowledge(&mut restored, PersonId(1)).unwrap();
    remove_water_lot(&mut restored, InventoryId(21)).unwrap();
    remove_shelter(&mut restored, ShelterId(30)).unwrap();
    assert_eq!(
        restored.get::<ForeignTag>(StableId(1)),
        Some(&ForeignTag(9))
    );
    assert!(clinical(&restored, PersonId(1)).is_some());
    assert!(job(&restored, PersonId(1)).is_some());
    assert_eq!(
        serde_json::to_value(Runtime::default())
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn remaining_component_schemas_reject_intrinsic_corruption_without_partial_writes() {
    let mut store = component_store();
    let before = store.snapshot().unwrap();
    let mut invalid_water = WaterLot::new(1_000, 500);
    invalid_water.removed_quantity = 1_001;
    assert!(insert_water_lot(&mut store, InventoryId(21), invalid_water.clone()).is_err());
    let mut invalid_shelter = shelter_row(30);
    invalid_shelter.capacity = 0;
    assert!(insert_shelter(&mut store, invalid_shelter.clone()).is_err());
    assert!(
        insert_occupancy(
            &mut store,
            PersonId(1),
            Occupancy {
                shelter: ShelterId(0)
            }
        )
        .is_err()
    );
    let invalid_knowledge = Knowledge {
        water: BTreeMap::from([(
            InventoryId(21),
            WaterSafetyView {
                inventory: InventoryId(22),
                purified: false,
                suspected_unsafe: false,
                observed_at: SimTime(0),
            },
        )]),
        ..Knowledge::default()
    };
    assert!(insert_knowledge(&mut store, PersonId(1), invalid_knowledge.clone()).is_err());
    let invalid_offers = Offers {
        caregiver: PersonId(1),
        patients: BTreeMap::from([(PersonId(2), promise(3, 2))]),
    };
    assert!(insert_offers(&mut store, PersonId(1), invalid_offers.clone()).is_err());
    assert!(
        insert_offers(
            &mut store,
            PersonId(2),
            Offers {
                caregiver: PersonId(1),
                patients: BTreeMap::new()
            }
        )
        .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    insert_water_lot(&mut store, InventoryId(21), WaterLot::new(1_000, 500)).unwrap();
    insert_shelter(&mut store, shelter_row(30)).unwrap();
    insert_occupancy(
        &mut store,
        PersonId(1),
        Occupancy {
            shelter: ShelterId(30),
        },
    )
    .unwrap();
    insert_knowledge(&mut store, PersonId(1), Knowledge::default()).unwrap();
    insert_offer(&mut store, promise(1, 2)).unwrap();
    for (key, value) in [
        (
            "health/water",
            serde_json::value::to_raw_value(&invalid_water).unwrap(),
        ),
        (
            "health/shelter",
            serde_json::value::to_raw_value(&invalid_shelter).unwrap(),
        ),
        (
            "health/occupancy",
            serde_json::value::to_raw_value(&Occupancy {
                shelter: ShelterId(0),
            })
            .unwrap(),
        ),
        (
            "health/knowledge",
            serde_json::value::to_raw_value(&invalid_knowledge).unwrap(),
        ),
        (
            "health/offers",
            serde_json::value::to_raw_value(&invalid_offers).unwrap(),
        ),
    ] {
        let mut corrupted = store.snapshot().unwrap();
        corrupted.columns.get_mut(key).unwrap()[0].value = value;
        assert!(Store::restore(store.schema().clone(), corrupted).is_err());
    }
    let before = store.snapshot().unwrap();
    assert!(
        remove_offer(&mut store, PersonId(99), PersonId(2))
            .unwrap()
            .is_none()
    );
    assert!(
        remove_offer(&mut store, PersonId(1), PersonId(99))
            .unwrap()
            .is_none()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn shelter_admission_guards_capacity_and_exact_relevant_occupant_membership() {
    for change in 0..3 {
        let mut runtime = Runtime::default();
        let mut store = component_store();
        insert_shelter(&mut store, shelter_row(30)).unwrap();
        let prepared = admission(&runtime, &store);
        match change {
            0 => {
                insert_occupancy(
                    &mut store,
                    PersonId(2),
                    Occupancy {
                        shelter: ShelterId(30),
                    },
                )
                .unwrap();
            }
            1 => shelter_mut(&mut store, ShelterId(30)).unwrap().capacity = 3,
            2 => {
                insert_shelter(&mut store, shelter_row(40)).unwrap();
                insert_occupancy(
                    &mut store,
                    PersonId(99),
                    Occupancy {
                        shelter: ShelterId(40),
                    },
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let before = store.snapshot().unwrap();
        let result = prepared.commit(&mut runtime, &mut store);
        if change < 2 {
            assert!(result.is_err());
            assert_eq!(store.snapshot().unwrap(), before);
        } else {
            result.unwrap();
            assert_eq!(
                occupancy(&store, PersonId(1)).unwrap().shelter,
                ShelterId(30)
            );
            assert!(occupancy(&store, PersonId(99)).is_some());
        }
    }
}

#[test]
fn cancellation_and_expiry_guard_new_related_offers_without_replacing_unrelated_rows() {
    for change in 0..3 {
        let mut runtime = Runtime::default();
        let mut store = component_store();
        insert_offer(&mut store, promise(1, 2)).unwrap();
        let prepared = runtime.prepare_cancel(&store, PersonId(2));
        match change {
            0 => {
                insert_offer(&mut store, promise(3, 2)).unwrap();
            }
            1 => {
                insert_offer(&mut store, promise(3, 4)).unwrap();
            }
            2 => {
                offer_mut(&mut store, PersonId(1), PersonId(2))
                    .unwrap()
                    .accepted = true;
            }
            _ => unreachable!(),
        }
        let before = store.snapshot().unwrap();
        let result = prepared.commit(&mut runtime, &mut store);
        if change == 1 {
            result.unwrap();
            assert!(offer(&store, PersonId(1), PersonId(2)).is_none());
            assert_eq!(
                offer(&store, PersonId(3), PersonId(4)),
                Some(&promise(3, 4))
            );
        } else {
            assert!(result.is_err());
            assert_eq!(store.snapshot().unwrap(), before);
        }
    }
    let mut runtime = Runtime::default();
    let mut store = component_store();
    insert_offer(&mut store, promise(1, 2)).unwrap();
    let prepared = runtime.prepare_expire_offers(&store, SimTime(60_000));
    insert_offer(&mut store, promise(3, 4)).unwrap();
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&mut runtime, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    runtime
        .prepare_expire_offers(&store, SimTime(60_000))
        .commit(&mut runtime, &mut store)
        .unwrap();
    assert_eq!(all_offers(&store).count(), 0);
}

#[test]
fn one_rejected_column_prevents_all_health_rows_and_effects_from_publishing() {
    let mut schema = Schema::new();
    schema
        .register::<Clinical>("health/clinical", Clinical::validate)
        .unwrap();
    schema.register::<Job>("health/job", Job::validate).unwrap();
    schema
        .register::<Offers>("health/offers", Offers::validate)
        .unwrap();
    schema
        .register::<Shelter>("health/shelter", Shelter::validate)
        .unwrap();
    schema
        .register::<Occupancy>("health/occupancy", |_| Err(Error::Unavailable))
        .unwrap();
    let mut store = Store::with_schema(schema);
    let mut runtime = Runtime::default();
    insert_shelter(&mut store, shelter_row(30)).unwrap();
    insert_offer(&mut store, promise(1, 2)).unwrap();
    insert_job(
        &mut store,
        Job {
            actor: PersonId(1),
            area: AreaId(10),
            completes: SimTime(60_000),
            work: Work::Drink {
                source: InventoryId(21),
                quantity: Quantity(1_000),
            },
        },
    )
    .unwrap();
    let prepared = admission(&runtime, &store);
    let before = (runtime.clone(), store.snapshot().unwrap());
    assert!(prepared.commit(&mut runtime, &mut store).is_err());
    assert_eq!((runtime, store.snapshot().unwrap()), before);
}
