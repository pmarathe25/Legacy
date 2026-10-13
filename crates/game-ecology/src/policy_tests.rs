use crate::*;
use game_ecs::{Batch, Schema, StableId, Store};

fn runtime() -> Runtime {
    Runtime::new(EcologyRules::default(), 0).unwrap()
}
fn source(id: u64, kind: ResourceKind, area: AreaId) -> Resource {
    Resource {
        id: ResourceId(id),
        area,
        kind,
        remaining: Quantity(5_000),
        capacity: Quantity(10_000),
        daily_growth: Quantity(500),
        growth_remainder: 0,
        decay_permille: 0,
        soil_water: 75,
        water_capacity: 120,
        optimal_water: 50,
        minimum_temperature: 0,
        optimum_temperature: 20,
        contamination: 120,
        runoff_contamination: 0,
        labor_per_unit: MINUTE,
        workers: 2,
        access: BTreeSet::from([PersonId(1)]),
        custodian: Some(PersonId(2)),
    }
}
fn store() -> Store {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_land::register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    game_health::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    store.ensure(StableId(1)).unwrap();
    store.ensure(StableId(2)).unwrap();
    runtime()
        .insert_resource(&mut store, source(100, ResourceKind::Spring, AreaId(1)))
        .unwrap();
    store
}
fn condition() -> ActorCondition {
    ActorCondition {
        location: Some(Location::AtArea(AreaId(1))),
        vitals: Some(Vitals {
            hunger: 0,
            fatigue: 0,
            health: 100,
            alive: true,
        }),
        stage: Some(LifeStage::Adult),
        development_present: true,
        combat_restricted: false,
        receiving_care: false,
        fatigue_limit: 100,
        carrying_capacity: Quantity(4_000),
    }
}
fn facts() -> GatherFacts {
    condition().gather_facts(SimTime(0), 0, None).unwrap()
}

#[test]
fn bodily_observation_and_gathering_admission_are_owned_and_distinct() {
    let mut body = condition();
    body.stage = Some(LifeStage::Infant);
    assert_eq!(body.observation_area(), Some(AreaId(1)));
    assert!(!body.able());
    body.stage = Some(LifeStage::Child);
    assert!(body.able());
    assert!(!body.gather_facts(SimTime(0), 0, None).unwrap().adult);
    body.stage = None;
    assert!(!body.able());
    body.development_present = false;
    assert!(body.able());
    assert!(body.gather_facts(SimTime(0), 0, None).unwrap().adult);
    body.vitals.as_mut().unwrap().fatigue = 100;
    assert!(body.able());
    assert!(!body.gather_facts(SimTime(0), 0, None).unwrap().capable);
    body.vitals.as_mut().unwrap().fatigue = 0;
    assert_eq!(
        body.gather_facts(SimTime(0), u128::MAX, None)
            .unwrap()
            .carrying_room,
        Quantity(0)
    );
    for reason in 0..4 {
        let mut restricted = condition();
        match reason {
            0 => restricted.combat_restricted = true,
            1 => restricted.receiving_care = true,
            2 => restricted.vitals.as_mut().unwrap().alive = false,
            _ => restricted.vitals.as_mut().unwrap().health = 0,
        }
        assert!(!restricted.able());
    }
    assert_eq!(
        Runtime::select_tool(
            [
                ToolCandidate {
                    id: InventoryId(3),
                    good: Good::Food
                },
                ToolCandidate {
                    id: InventoryId(4),
                    good: Good::Tool
                },
                ToolCandidate {
                    id: InventoryId(5),
                    good: Good::Tool
                },
                ToolCandidate {
                    id: InventoryId(6),
                    good: Good::Tool
                },
            ],
            |id| id != InventoryId(4)
        ),
        Some(InventoryId(5))
    );
}

#[test]
fn actual_observer_port_retains_personal_memories_and_omits_unable_bodies() {
    struct Port;
    impl ObservationPort for Port {
        fn actors(&self) -> Vec<PersonId> {
            vec![PersonId(1), PersonId(2)]
        }
        fn condition(&self, actor: PersonId) -> ActorCondition {
            let mut body = condition();
            if actor == PersonId(2) {
                body.vitals.as_mut().unwrap().alive = false;
            }
            body
        }
    }
    let mut store = store();
    let runtime = runtime();
    runtime
        .prepare_observations(&store, SimTime(10), &Port)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    let view = runtime.view(&store, PersonId(1));
    assert_eq!(view.resources.len(), 1);
    assert_eq!(view.resources[0].observed_at, SimTime(10));
    assert_eq!(runtime.view(&store, PersonId(2)), EcologyView::default());
    assert_eq!(
        runtime
            .knowledge(&store, PersonId(1))
            .unwrap()
            .observed_areas[&AreaId(1)],
        SimTime(10)
    );
}

#[test]
fn prepared_gather_start_rejects_changed_stock_reservations_and_foreign_batch_edits() {
    for changed in 0..4 {
        let runtime = runtime();
        let mut store = store();
        let prepared = runtime
            .prepare_gathering_start(
                &store,
                PersonId(1),
                ResourceId(100),
                Quantity(1_000),
                false,
                facts(),
            )
            .unwrap();
        assert_eq!(prepared.plan().good, Good::Water);
        assert_eq!(prepared.opposition(), None);
        match changed {
            0 => {
                runtime
                    .resource_mut(&mut store, ResourceId(100))
                    .unwrap()
                    .remaining = Quantity(4_999)
            }
            1 => runtime
                .insert_gathering(
                    &mut store,
                    PersonId(2),
                    Gathering {
                        resource: ResourceId(100),
                        destination: InventoryId(400),
                        quantity: Quantity(1_000),
                        tool: None,
                        completes: SimTime(999),
                        unauthorized: false,
                    },
                )
                .unwrap(),
            _ => (),
        }
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let result = prepared.stage_into(&store, InventoryId(300), &mut batch);
        if changed <= 1 {
            assert!(result.is_err());
        } else {
            let guard = result.unwrap();
            if changed == 2 {
                batch.remove::<Resource>(StableId(100));
            } else {
                batch.remove::<Gathering>(StableId(1));
            }
            assert!(guard.validate(&store, &batch).is_err());
        }
        assert_eq!(store.snapshot().unwrap(), before);
    }
}

#[test]
fn gathering_start_guards_its_scope_and_preserves_unrelated_owner_rows() {
    let runtime = runtime();
    let mut store = store();
    let prepared = runtime
        .prepare_gathering_start(
            &store,
            PersonId(1),
            ResourceId(100),
            Quantity(1_000),
            true,
            facts(),
        )
        .unwrap();
    assert_eq!(prepared.opposition(), Some(PersonId(2)));
    runtime
        .insert_resource(&mut store, source(101, ResourceKind::Herbs, AreaId(2)))
        .unwrap();
    runtime
        .insert_requests(
            &mut store,
            PersonId(1),
            Requests {
                next_request: BTreeMap::from([(ResourceId(101), SimTime(77))]),
            },
        )
        .unwrap();
    let known = Requests {
        next_request: BTreeMap::from([(ResourceId(101), SimTime(77))]),
    };
    let mut batch = Batch::new();
    let guard = prepared
        .stage_into(&store, InventoryId(300), &mut batch)
        .unwrap();
    guard.validate(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(runtime.requests(&store, PersonId(1)), Some(&known));
    assert_eq!(
        runtime.resource(&store, ResourceId(101)).unwrap().remaining,
        Quantity(5_000)
    );
}

#[test]
fn staged_access_and_collection_reject_late_foreign_edits_to_their_exact_effects() {
    let runtime = runtime();
    let mut store = store();
    let request = EcologyAction::RequestAccess {
        resource: ResourceId(100),
    };
    let access = runtime
        .prepare_access(
            &store,
            SimTime(10),
            &request,
            AccessFacts {
                actor: PersonId(1),
                area: AreaId(1),
                capable: true,
            },
            |_| Some(AreaId(1)),
        )
        .unwrap();
    let mut batch = Batch::new();
    let (_, guard) = access.stage_into(&runtime, &store, &mut batch).unwrap();
    batch.remove::<Resource>(StableId(100));
    assert!(guard.validate(&store, &batch).is_err());
    runtime
        .insert_gathering(
            &mut store,
            PersonId(1),
            Gathering {
                resource: ResourceId(100),
                destination: InventoryId(300),
                quantity: Quantity(1_000),
                tool: None,
                completes: SimTime(50),
                unauthorized: false,
            },
        )
        .unwrap();
    let finish = runtime
        .prepare_collection_finish(
            &store,
            PersonId(1),
            CollectionFacts {
                now: SimTime(50),
                area: AreaId(1),
                capable: true,
                cargo_available: true,
                tool_available: true,
            },
        )
        .unwrap()
        .unwrap();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let (_, guard) = finish.stage_into(&runtime, &store, &mut batch).unwrap();
    let mut altered = runtime.resource(&store, ResourceId(100)).unwrap().clone();
    altered.remaining = Quantity(2_000);
    batch.insert(StableId(100), altered);
    assert!(guard.validate(&store, &batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn start_and_collection_compose_real_cargo_and_water_owners_without_assembly() {
    let runtime = runtime();
    let mut store = store();
    let prepared = runtime
        .prepare_gathering_start(
            &store,
            PersonId(1),
            ResourceId(100),
            Quantity(1_000),
            false,
            facts(),
        )
        .unwrap();
    let completes = prepared.plan().completes;
    let inventory = game_cargo::Inventory {
        id: InventoryId(300),
        location: game_cargo::InventoryLocation::Carried(PersonId(1)),
        quantity: Quantity(0),
        capacity: Quantity(4_000),
        access: BTreeSet::from([PersonId(1)]),
        last_granted: None,
        good: Good::Water,
        spoils_at: None,
    };
    let creation = game_cargo::prepare_inventory_creation(&store, inventory).unwrap();
    let mut batch = Batch::new();
    let guard = prepared
        .stage_into(&store, InventoryId(300), &mut batch)
        .unwrap();
    let created = creation.stage_into(&store, &mut batch).unwrap();
    guard.validate(&store, &batch).unwrap();
    created.validate_batch(&batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(300))
            .unwrap()
            .quantity,
        Quantity(0)
    );
    let finish = runtime
        .prepare_collection_finish(
            &store,
            PersonId(1),
            CollectionFacts {
                now: completes,
                area: AreaId(1),
                capable: true,
                cargo_available: true,
                tool_available: true,
            },
        )
        .unwrap()
        .unwrap();
    let CollectionOutcome::Collected(effect) = finish.outcome() else {
        panic!("collectable work");
    };
    let cargo =
        game_cargo::prepare_deposit(&store, effect.destination, effect.good, effect.quantity)
            .unwrap();
    let water = game_health::prepare_water_collection(
        &store,
        game_health::WaterCollectionFacts {
            inventory: effect.destination,
            quantity_after: cargo.quantity_after(),
            collected: effect.quantity,
            contamination: effect.contamination.unwrap(),
            holder: Some(PersonId(1)),
            now: completes,
        },
    )
    .unwrap();
    let before = store.snapshot().unwrap();
    assert_eq!(
        runtime.resource(&store, ResourceId(100)).unwrap().remaining,
        Quantity(5_000)
    );
    let mut batch = Batch::new();
    let (_, ecology_guard) = finish.stage_into(&runtime, &store, &mut batch).unwrap();
    let cargo_guard = cargo.stage_into(&store, &mut batch).unwrap();
    let water_guard = water.stage_into(&store, &mut batch).unwrap();
    ecology_guard.validate(&store, &batch).unwrap();
    cargo_guard.validate(&store, &batch).unwrap();
    water_guard.validate(&store, &batch).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    store.apply_batch(batch).unwrap();
    assert_eq!(
        runtime.resource(&store, ResourceId(100)).unwrap().remaining,
        Quantity(4_000)
    );
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(300))
            .unwrap()
            .quantity,
        Quantity(1_000)
    );
    assert_eq!(
        game_health::water_lot(&store, InventoryId(300))
            .unwrap()
            .load(),
        120_000
    );
    assert!(runtime.gathering(&store, PersonId(1)).is_none());
}

#[test]
fn provision_targets_derive_from_dated_knowledge_and_reachability() {
    let runtime = runtime();
    let mut store = store();
    runtime
        .insert_resource(&mut store, source(101, ResourceKind::Coppice, AreaId(2)))
        .unwrap();
    runtime
        .prepare_local_observations(
            &store,
            SimTime(50),
            &[ObserverArea {
                actor: PersonId(1),
                area: AreaId(1),
            }],
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    // This distinct visit supplies remote timber memory; current source changes
    // must not fabricate newer remote amounts or revoke the remembered goal.
    runtime
        .prepare_local_observations(
            &store,
            SimTime(51),
            &[ObserverArea {
                actor: PersonId(1),
                area: AreaId(2),
            }],
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    runtime
        .resource_mut(&mut store, ResourceId(101))
        .unwrap()
        .remaining = Quantity(0);
    let local = [EcologyAction::Gather {
        resource: ResourceId(100),
        quantity: Quantity(500),
        unauthorized: false,
    }];
    let opportunities = runtime.provision_opportunities(
        &store,
        PersonId(1),
        AreaId(1),
        &local,
        Quantity(750),
        |_| true,
    );
    assert_eq!(opportunities.len(), 2);
    assert_eq!(opportunities[0].requirement.good, Good::Water);
    assert_eq!(opportunities[0].requirement.quantity, Quantity(750));
    assert_eq!(
        opportunities[1].action,
        AcquisitionAction::Explore {
            destination: AreaId(2)
        }
    );
    assert_eq!(opportunities[1].requirement.good, Good::Timber);
    assert_eq!(opportunities[1].requirement.quantity, Quantity(5_000));
    assert_eq!(
        runtime
            .provision_opportunities(
                &store,
                PersonId(1),
                AreaId(1),
                &local,
                Quantity(750),
                |_| false
            )
            .len(),
        1
    );
}

struct Saved<'a> {
    store: &'a Store,
    now: SimTime,
    next_id: u64,
    deadline: bool,
    occupied: bool,
    cultivated: bool,
}
impl SavePort for Saved<'_> {
    fn context(&self) -> SaveContext {
        SaveContext {
            now: self.now,
            next_id: self.next_id,
            physical_present: true,
            decisions_present: true,
            environment_day: Some(0),
        }
    }
    fn actor_exists(&self, actor: PersonId) -> bool {
        matches!(actor.0, 1 | 2)
    }
    fn area_exists(&self, area: AreaId) -> bool {
        matches!(area.0, 1 | 2)
    }
    fn occupied_plot(&self, _: SiteId) -> bool {
        self.occupied
    }
    fn cultivated_plot(&self, _: SiteId) -> bool {
        self.cultivated
    }
    fn inventory(&self, id: InventoryId) -> Option<InventoryFacts> {
        let row = game_cargo::inventory(self.store, id)?;
        Some(InventoryFacts {
            good: row.good,
            location: match row.location {
                game_cargo::InventoryLocation::Area(area) => StockPosition::Area(area),
                game_cargo::InventoryLocation::Carried(actor) => StockPosition::Carried(actor),
            },
            quantity: row.quantity,
            capacity: row.capacity,
        })
    }
    fn gathering_activity(&self, actor: PersonId) -> Option<GatheringActivity> {
        let job = gathering(self.store, actor)?;
        Some(GatheringActivity {
            resource: job.resource,
            destination: job.destination,
            quantity: job.quantity,
            completes: job.completes,
        })
    }
    fn physical_deadline(&self, _: PersonId, _: SimTime) -> bool {
        self.deadline
    }
}
fn saved(store: &Store) -> Saved<'_> {
    Saved {
        store,
        now: SimTime(10),
        next_id: 1_000,
        deadline: true,
        occupied: false,
        cultivated: false,
    }
}

#[test]
fn save_owner_validates_foreign_references_time_allocations_and_land_use() {
    let runtime = runtime();
    let mut store = store();
    runtime
        .prepare_local_observations(
            &store,
            SimTime(10),
            &[ObserverArea {
                actor: PersonId(1),
                area: AreaId(1),
            }],
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert!(validate_save(Some(&runtime), &store, &saved(&store)).is_ok());
    assert!(validate_save(None, &store, &saved(&store)).is_err());
    for changed in 0..4 {
        let mut port = saved(&store);
        match changed {
            0 => port.next_id = 100,
            1 => port.now = SimTime(9),
            2 => (),
            _ => (),
        }
        let mut invalid_runtime = runtime.clone();
        if changed == 2 {
            invalid_runtime.day = 1;
        }
        if changed == 3 {
            invalid_runtime.rules.search_retry = SimDuration(0);
        }
        assert!(validate_save(Some(&invalid_runtime), &store, &port).is_err());
    }
    runtime
        .insert_plot_soil(
            &mut store,
            Plot {
                id: SiteId(400),
                area: AreaId(1),
                access: BTreeSet::from([PersonId(1)]),
                custodian: None,
            },
            Soil {
                fertility: 500,
                water: 50,
                capacity: 100,
            },
        )
        .unwrap();
    let mut port = saved(&store);
    port.occupied = true;
    assert!(validate_save(Some(&runtime), &store, &port).is_err());
    runtime
        .resource_mut(&mut store, ResourceId(100))
        .unwrap()
        .access
        .insert(PersonId(999));
    assert!(validate_save(Some(&runtime), &store, &saved(&store)).is_err());
}

#[test]
fn saved_gathering_requires_real_compatible_compartment_and_deadline() {
    let runtime = runtime();
    let mut store = store();
    runtime
        .insert_gathering(
            &mut store,
            PersonId(1),
            Gathering {
                resource: ResourceId(100),
                destination: InventoryId(300),
                quantity: Quantity(1_000),
                tool: None,
                completes: SimTime(500),
                unauthorized: false,
            },
        )
        .unwrap();
    assert!(validate_save(Some(&runtime), &store, &saved(&store)).is_err());
    game_cargo::insert_inventory(
        &mut store,
        game_cargo::Inventory {
            id: InventoryId(300),
            location: game_cargo::InventoryLocation::Carried(PersonId(1)),
            quantity: Quantity(0),
            capacity: Quantity(2_000),
            access: BTreeSet::from([PersonId(1)]),
            last_granted: None,
            good: Good::Water,
            spoils_at: None,
        },
    )
    .unwrap();
    assert!(validate_save(Some(&runtime), &store, &saved(&store)).is_ok());
    let mut port = saved(&store);
    port.deadline = false;
    assert!(validate_save(Some(&runtime), &store, &port).is_err());
    game_cargo::inventory_mut(&mut store, InventoryId(300))
        .unwrap()
        .quantity = Quantity(1_001);
    assert!(validate_save(Some(&runtime), &store, &saved(&store)).is_err());
}

#[test]
fn releasing_undeveloped_land_preserves_canonical_soil_and_rejects_foreign_changes() {
    let runtime = runtime();
    let mut store = store();
    let plot = Plot {
        id: SiteId(400),
        area: AreaId(1),
        access: BTreeSet::from([PersonId(1)]),
        custodian: Some(PersonId(2)),
    };
    let soil = Soil {
        fertility: 500,
        water: 50,
        capacity: 100,
    };
    runtime
        .insert_plot_soil(&mut store, plot.clone(), soil.clone())
        .unwrap();
    assert!(
        runtime
            .prepare_plot_development(&store, SiteId(400), AreaId(2))
            .is_err()
    );
    let release = runtime
        .prepare_plot_development(&store, SiteId(400), AreaId(1))
        .unwrap();
    assert_eq!(release.plot(), &plot);
    let mut batch = Batch::new();
    let guard = release.stage_into(&store, &mut batch).unwrap();
    guard.validate(&store, &batch).unwrap();
    let before = store.snapshot().unwrap();
    let mut overwritten = soil.clone();
    overwritten.fertility = 700;
    batch.insert(StableId(400), overwritten);
    assert!(guard.validate(&store, &batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let mut batch = Batch::new();
    let release = runtime
        .prepare_plot_development(&store, SiteId(400), AreaId(1))
        .unwrap();
    let guard = release.stage_into(&store, &mut batch).unwrap();
    guard.validate(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert!(runtime.plot(&store, SiteId(400)).is_none());
    assert_eq!(runtime.soil(&store, SiteId(400)), Some(&soil));
    assert!(store.contains(StableId(400)));
}

#[test]
fn epoch_reset_ends_only_gathering_work_and_retains_personal_requests_and_stock() {
    let runtime = runtime();
    let mut store = store();
    runtime
        .insert_gathering(
            &mut store,
            PersonId(1),
            Gathering {
                resource: ResourceId(100),
                destination: InventoryId(300),
                quantity: Quantity(1_000),
                tool: None,
                completes: SimTime(50),
                unauthorized: false,
            },
        )
        .unwrap();
    runtime
        .insert_requests(
            &mut store,
            PersonId(1),
            Requests {
                next_request: BTreeMap::from([(ResourceId(100), SimTime(77))]),
            },
        )
        .unwrap();
    runtime.reset_gatherings(&mut store).unwrap();
    assert!(runtime.gatherings(&store).next().is_none());
    assert_eq!(
        runtime.resource(&store, ResourceId(100)).unwrap().remaining,
        Quantity(5_000)
    );
    assert_eq!(
        runtime.requests(&store, PersonId(1)).unwrap().next_request[&ResourceId(100)],
        SimTime(77)
    );
}

#[test]
fn admitted_resistance_cancels_both_jobs_without_transient_yield_or_reservation() {
    let runtime = runtime();
    let mut store = store();
    for actor in [PersonId(1), PersonId(2)] {
        runtime
            .insert_gathering(
                &mut store,
                actor,
                Gathering {
                    resource: ResourceId(100),
                    destination: InventoryId(300 + actor.0),
                    quantity: Quantity(1_000),
                    tool: None,
                    completes: SimTime(50),
                    unauthorized: false,
                },
            )
            .unwrap();
    }
    let prepared = runtime
        .prepare_gathering_start(
            &store,
            PersonId(1),
            ResourceId(100),
            Quantity(1_000),
            true,
            facts(),
        )
        .unwrap();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let (effect, guard) = prepared
        .stage_opposed(&store, PersonId(2), &mut batch)
        .unwrap();
    assert_eq!(effect.actor, PersonId(1));
    assert_eq!(effect.resource, ResourceId(100));
    assert!(effect.unauthorized);
    guard.validate(&store, &batch).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    store.apply_batch(batch).unwrap();
    assert!(runtime.gatherings(&store).next().is_none());
    assert_eq!(
        runtime.resource(&store, ResourceId(100)).unwrap().remaining,
        Quantity(5_000)
    );
    assert!(!store.contains(StableId(301)));
    let prepared = runtime
        .prepare_gathering_start(
            &store,
            PersonId(1),
            ResourceId(100),
            Quantity(1_000),
            true,
            facts(),
        )
        .unwrap();
    assert!(
        prepared
            .stage_opposed(&store, PersonId(1), &mut Batch::new())
            .is_err()
    );
}

#[test]
fn malformed_source_identity_cannot_admit_or_offer_gathering() {
    let runtime = runtime();
    let mut store = store();
    runtime
        .resource_mut(&mut store, ResourceId(100))
        .unwrap()
        .id = ResourceId(101);
    let before = store.snapshot().unwrap();
    assert!(
        runtime
            .gather_upper_bound(&store, PersonId(1), ResourceId(100), facts())
            .is_err()
    );
    assert!(
        runtime
            .prepare_gathering_start(
                &store,
                PersonId(1),
                ResourceId(100),
                Quantity(1_000),
                true,
                facts()
            )
            .is_err()
    );
    assert!(
        runtime
            .local_gather_opportunities(&store, PersonId(1), facts(), |_, _| vec![Quantity(1_000)])
            .is_empty()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn resource_authorship_rejects_zero_actor_references_without_attaching_rows() {
    let runtime = runtime();
    let mut store = store();
    let before = store.snapshot().unwrap();
    for invalid_custodian in [false, true] {
        let mut row = source(101, ResourceKind::Berries, AreaId(1));
        if invalid_custodian {
            row.custodian = Some(PersonId(0));
        } else {
            row.access.insert(PersonId(0));
        }
        assert!(runtime.insert_resource(&mut store, row).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
    }
}
