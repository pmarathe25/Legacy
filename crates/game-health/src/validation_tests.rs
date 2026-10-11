use crate::*;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

struct Fixture {
    world: Runtime,
    store: game_ecs::Store,
    people: BTreeMap<PersonId, PersonCatalog>,
    areas: BTreeSet<AreaId>,
    inventories: BTreeMap<InventoryId, InventoryCatalog>,
}
impl Fixture {
    fn new() -> Self {
        let mut store = component_store();
        let world = Runtime {
            rules: Rules::fixture(),
            ..Runtime::default()
        };
        insert_water_lot(&mut store, InventoryId(10), WaterLot::new(1_000, 120)).unwrap();
        insert_clinical(&mut store, PersonId(1), Clinical::default()).unwrap();
        insert_shelter(
            &mut store,
            Shelter {
                id: ShelterId(20),
                area: AreaId(5),
                custodian: PersonId(1),
                access: BTreeSet::from([PersonId(1)]),
                capacity: 1,
                insulation: 500,
                built_at: SimTime(0),
                timber_used: Quantity(1_000),
                labor_ms: 30_000,
            },
        )
        .unwrap();
        insert_occupancy(
            &mut store,
            PersonId(1),
            Occupancy {
                shelter: ShelterId(20),
            },
        )
        .unwrap();
        insert_occupancy(
            &mut store,
            PersonId(2),
            Occupancy {
                shelter: ShelterId(20),
            },
        )
        .unwrap();
        insert_job(
            &mut store,
            Job {
                actor: PersonId(1),
                area: AreaId(5),
                completes: SimTime(120_000),
                work: Work::Boil {
                    source: InventoryId(10),
                    quantity: Quantity(1_000),
                    tool: InventoryId(11),
                },
            },
        )
        .unwrap();
        insert_offer(
            &mut store,
            Offer {
                caregiver: PersonId(1),
                patient: PersonId(2),
                herbs: InventoryId(12),
                water: InventoryId(10),
                expires: SimTime(180_000),
                accepted: true,
            },
        )
        .unwrap();
        // Deliberately stale facts remain valid beliefs.
        insert_knowledge(
            &mut store,
            PersonId(2),
            Knowledge {
                people: BTreeMap::from([(
                    PersonId(1),
                    SymptomsView {
                        person: PersonId(1),
                        fever: true,
                        nausea: true,
                        cough: false,
                        shivering: false,
                        wounded: true,
                        observed_at: SimTime(1),
                    },
                )]),
                shelters: BTreeMap::from([(
                    ShelterId(20),
                    ShelterView {
                        id: ShelterId(20),
                        area: AreaId(5),
                        capacity: 2,
                        insulation: 700,
                        custodian: PersonId(2),
                        accessible: true,
                        known_occupants: BTreeSet::new(),
                        observed_at: SimTime(1),
                    },
                )]),
                water: BTreeMap::from([(
                    InventoryId(10),
                    WaterSafetyView {
                        inventory: InventoryId(10),
                        purified: true,
                        suspected_unsafe: false,
                        observed_at: SimTime(1),
                    },
                )]),
            },
        )
        .unwrap();
        Self {
            world,
            store,
            people: BTreeMap::from([
                (
                    PersonId(1),
                    PersonCatalog {
                        alive: true,
                        location: Location::AtArea(AreaId(5)),
                        health_work: Some(HealthWorkLink {
                            kind: HealthWorkKind::Boil,
                            completes: SimTime(120_000),
                        }),
                    },
                ),
                (
                    PersonId(2),
                    PersonCatalog {
                        alive: true,
                        location: Location::AtArea(AreaId(5)),
                        health_work: None,
                    },
                ),
            ]),
            areas: BTreeSet::from([AreaId(5)]),
            inventories: BTreeMap::from([
                (
                    InventoryId(10),
                    InventoryCatalog {
                        good: Good::Water,
                        quantity: Quantity(1_000),
                    },
                ),
                (
                    InventoryId(11),
                    InventoryCatalog {
                        good: Good::Tool,
                        quantity: Quantity(0),
                    },
                ),
                (
                    InventoryId(12),
                    InventoryCatalog {
                        good: Good::Herb,
                        quantity: Quantity(0),
                    },
                ),
            ]),
        }
    }
    fn validate(&self) -> Result<(), Error> {
        validate_runtime(
            &self.world,
            &self.store,
            ValidationCatalog {
                now: SimTime(60_000),
                people: &self.people,
                areas: &self.areas,
                inventories: &self.inventories,
            },
        )
    }
}

#[test]
fn current_roundtrip_accepts_stale_beliefs_and_physical_newborn_occupancy() {
    let mut fixture = Fixture::new();
    fixture.validate().unwrap();
    let json = serde_json::to_vec(&fixture.world).unwrap();
    let restored: Runtime = serde_json::from_slice(&json).unwrap();
    assert_eq!(restored, fixture.world);
    fixture.world = restored;
    let snapshot = serde_json::to_vec(&fixture.store.snapshot().unwrap()).unwrap();
    fixture.store = game_ecs::Store::restore(
        fixture.store.schema().clone(),
        serde_json::from_slice(&snapshot).unwrap(),
    )
    .unwrap();
    fixture.validate().unwrap();
}

#[test]
fn unknown_clinical_person_is_rejected_before_any_world_access() {
    let mut fixture = Fixture::new();
    insert_clinical(&mut fixture.store, PersonId(999), Clinical::default()).unwrap();
    assert!(matches!(fixture.validate(), Err(Error::InvalidSave(_))));
}

#[test]
fn pending_job_and_actual_activity_must_match_in_both_directions() {
    let mut fixture = Fixture::new();
    fixture
        .people
        .get_mut(&PersonId(1))
        .unwrap()
        .health_work
        .as_mut()
        .unwrap()
        .kind = HealthWorkKind::Drink;
    assert!(fixture.validate().is_err());
    let mut fixture = Fixture::new();
    remove_job(&mut fixture.store, PersonId(1)).unwrap();
    assert!(fixture.validate().is_err());
    let mut fixture = Fixture::new();
    fixture.people.get_mut(&PersonId(1)).unwrap().health_work = None;
    assert!(fixture.validate().is_err());
}

#[test]
fn corrupted_owned_references_and_numeric_invariants_are_rejected() {
    let corruptions: &[fn(&mut Fixture)] = &[
        |f| {
            clinical_mut(&mut f.store, PersonId(1))
                .unwrap()
                .cold_remainder = 1_000
        },
        |f| {
            clinical_mut(&mut f.store, PersonId(1))
                .unwrap()
                .doses
                .insert(
                    IllnessKind::Enteric,
                    Dose {
                        amount: 1,
                        remainder: 1_000,
                    },
                );
        },
        |f| {
            water_lot_mut(&mut f.store, InventoryId(10))
                .unwrap()
                .removed_quantity = 2_000
        },
        |f| shelter_mut(&mut f.store, ShelterId(20)).unwrap().capacity = 0,
        |f| shelter_mut(&mut f.store, ShelterId(20)).unwrap().area = AreaId(999),
        |f| {
            shelter_mut(&mut f.store, ShelterId(20))
                .unwrap()
                .access
                .insert(PersonId(999));
        },
        |f| f.people.get_mut(&PersonId(2)).unwrap().location = Location::AtArea(AreaId(999)),
        |f| {
            offer_mut(&mut f.store, PersonId(1), PersonId(2))
                .unwrap()
                .patient = PersonId(1)
        },
        |f| {
            offer_mut(&mut f.store, PersonId(1), PersonId(2))
                .unwrap()
                .herbs = InventoryId(10)
        },
        |f| job_mut(&mut f.store, PersonId(1)).unwrap().actor = PersonId(2),
        |f| f.world.rules.incubation = SimDuration(0),
        |f| f.world.last_review = SimTime(60_001),
        |f| {
            knowledge_mut(&mut f.store, PersonId(2))
                .unwrap()
                .people
                .get_mut(&PersonId(1))
                .unwrap()
                .observed_at = SimTime(60_001)
        },
        |f| {
            knowledge_mut(&mut f.store, PersonId(2))
                .unwrap()
                .shelters
                .get_mut(&ShelterId(20))
                .unwrap()
                .known_occupants
                .insert(PersonId(999));
        },
        |f| {
            knowledge_mut(&mut f.store, PersonId(2))
                .unwrap()
                .water
                .get_mut(&InventoryId(10))
                .unwrap()
                .inventory = InventoryId(999)
        },
    ];
    for (index, corrupt) in corruptions.iter().enumerate() {
        let mut fixture = Fixture::new();
        corrupt(&mut fixture);
        assert!(
            matches!(fixture.validate(), Err(Error::InvalidSave(_))),
            "case {index}"
        );
    }
}

#[test]
fn dead_clinical_history_can_remain_unreconciled() {
    let mut fixture = Fixture::new();
    remove_occupancy(&mut fixture.store, PersonId(2)).unwrap();
    fixture.people.get_mut(&PersonId(2)).unwrap().alive = false;
    insert_clinical(
        &mut fixture.store,
        PersonId(2),
        Clinical {
            illnesses: BTreeMap::from([(
                IllnessKind::Respiratory,
                Infection {
                    kind: IllnessKind::Respiratory,
                    acquired: SimTime(1),
                    onset: SimTime(2),
                    recovers: SimTime(3),
                    symptomatic: false,
                },
            )]),
            treatment: Some(Course {
                until: SimTime(4),
                healing: 1_000,
            }),
            ..Clinical::default()
        },
    )
    .unwrap();
    fixture.validate().unwrap();
    fixture.people.get_mut(&PersonId(2)).unwrap().alive = true;
    assert!(fixture.validate().is_err());
}

#[test]
fn stopped_road_work_is_valid_but_cannot_materialize_a_shelter_at_an_endpoint() {
    let mut fixture = Fixture::new();
    fixture.areas.insert(AreaId(6));
    remove_occupancy(&mut fixture.store, PersonId(1)).unwrap();
    fixture.people.get_mut(&PersonId(1)).unwrap().location = Location::StoppedOnRoad {
        from: AreaId(5),
        to: AreaId(6),
        progress: SimDuration(20),
        duration: SimDuration(100),
    };
    fixture.validate().unwrap(); // Boiling with the held toolkit is ordinary hand work.
    job_mut(&mut fixture.store, PersonId(1)).unwrap().work = Work::Shelter {
        timber_used: Quantity(1_000),
        tool: InventoryId(11),
        capacity: 1,
        insulation: 500,
        started: SimTime(0),
    };
    fixture
        .people
        .get_mut(&PersonId(1))
        .unwrap()
        .health_work
        .as_mut()
        .unwrap()
        .kind = HealthWorkKind::Shelter;
    assert!(fixture.validate().is_err());
}
