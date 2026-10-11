use super::*;
fn schema() -> game_ecs::Schema {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_land::register_components(&mut schema).unwrap();
    schema
}
fn runtime() -> Runtime {
    Runtime::new(EcologyRules::default(), 0).unwrap()
}
fn deposit(id: u64, area: u64) -> Resource {
    Resource {
        id: ResourceId(id),
        area: AreaId(area),
        kind: ResourceKind::Berries,
        remaining: Quantity(5_000),
        capacity: Quantity(20_000),
        daily_growth: Quantity(250),
        growth_remainder: 0,
        decay_permille: 0,
        soil_water: 75,
        water_capacity: 120,
        optimal_water: 50,
        minimum_temperature: 0,
        optimum_temperature: 20,
        contamination: 10,
        runoff_contamination: 0,
        labor_per_unit: MINUTE,
        workers: 2,
        access: BTreeSet::from([PersonId(1)]),
        custodian: Some(PersonId(1)),
    }
}
fn setup(reverse: bool) -> (Runtime, Store) {
    let runtime = runtime();
    let mut store = Store::with_schema(schema());
    let mut ids: Vec<_> = (100..119).collect();
    if reverse {
        ids.reverse();
    }
    for id in 1..=3 {
        store.ensure(game_ecs::StableId(id)).unwrap();
    }
    runtime.insert_resource(&mut store, deposit(99, 2)).unwrap();
    for id in ids {
        let mut source = deposit(id, 1);
        if id % 3 == 0 {
            source.kind = ResourceKind::Coppice;
        }
        if id % 4 == 0 {
            source.access.clear();
        }
        if id == 103 {
            source.custodian = Some(PersonId(3));
        }
        if id == 104 {
            source.remaining = Quantity(0);
        }
        runtime.insert_resource(&mut store, source).unwrap();
    }
    (runtime, store)
}
fn suggestions(_: Good, _: Quantity) -> Vec<Quantity> {
    vec![Quantity(0), Quantity(1), Quantity(2_000), Quantity(50_000)]
}
// Independent old assembly enumeration; the existing collection kernels remain
// the numerical oracle while ordering, source selection and filtering differ.
fn gather_oracle(
    runtime: &Runtime,
    store: &Store,
    actor: PersonId,
    facts: GatherFacts,
) -> Vec<GatherOpportunity> {
    let mut output = Vec::new();
    let mut count = 0;
    for (_, source) in runtime.resources(store) {
        if source.area != facts.area {
            continue;
        }
        count += 1;
        if count > 16 {
            break;
        }
        if let Ok(upper) = runtime.gather_upper_bound(store, actor, source.id, facts) {
            let unauthorized = !source.access.contains(&actor);
            let amounts = suggestions(source.kind.good(), upper);
            let mut admitted = Vec::new();
            for amount in amounts {
                if runtime
                    .prepare_gather(store, actor, source.id, amount, unauthorized, facts)
                    .is_ok()
                {
                    admitted.push(amount);
                }
            }
            if !admitted.is_empty() {
                output.push(GatherOpportunity {
                    resource: source.id,
                    good: source.kind.good(),
                    upper_bound: upper,
                    unauthorized,
                    suggested: admitted,
                });
            }
        }
    }
    output
}
#[test]
fn gather_choices_match_old_enumeration_body_gear_reservations_and_deadlines() {
    let (runtime, mut store) = setup(false);
    runtime
        .insert_gathering(
            &mut store,
            PersonId(2),
            Gathering {
                resource: ResourceId(100),
                destination: InventoryId(400),
                quantity: Quantity(4_999),
                tool: None,
                completes: SimTime(500),
                unauthorized: false,
            },
        )
        .unwrap();
    for adult in [false, true] {
        for capable in [false, true] {
            for tool in [None, Some(InventoryId(400))] {
                for now in [SimTime(100), SimTime(u64::MAX - 1)] {
                    let facts = GatherFacts {
                        now,
                        area: AreaId(1),
                        capable,
                        adult,
                        carrying_room: Quantity(3_000),
                        tool,
                    };
                    let before = store.snapshot().unwrap();
                    let output =
                        runtime.local_gather_opportunities(&store, PersonId(1), facts, suggestions);
                    assert_eq!(output, gather_oracle(&runtime, &store, PersonId(1), facts));
                    assert!(output.iter().all(|row| row.resource.0 < 116));
                    assert_eq!(store.snapshot().unwrap(), before);
                }
            }
        }
    }
    let facts = GatherFacts {
        now: SimTime(100),
        area: AreaId(1),
        capable: true,
        adult: true,
        carrying_room: Quantity(3_000),
        tool: Some(InventoryId(400)),
    };
    let rows = runtime.local_gather_opportunities(&store, PersonId(1), facts, suggestions);
    let reserved = rows
        .iter()
        .find(|row| row.resource == ResourceId(100))
        .unwrap();
    assert_eq!(reserved.upper_bound, Quantity(1));
    assert_eq!(reserved.suggested, vec![Quantity(1)]);
    assert!(reserved.unauthorized);
}
#[test]
fn source_and_input_order_save_restore_and_first16_cutoff_preserve_options() {
    let (runtime, store) = setup(false);
    let (_, reversed) = setup(true);
    let facts = GatherFacts {
        now: SimTime(100),
        area: AreaId(1),
        capable: true,
        adult: true,
        carrying_room: Quantity(3_000),
        tool: Some(InventoryId(400)),
    };
    let expected = runtime.local_gather_opportunities(&store, PersonId(1), facts, suggestions);
    assert_eq!(
        expected,
        runtime.local_gather_opportunities(&reversed, PersonId(1), facts, suggestions)
    );
    let json = serde_json::to_string(&store.snapshot().unwrap()).unwrap();
    let restored = Store::restore(schema(), serde_json::from_str(&json).unwrap()).unwrap();
    assert_eq!(
        expected,
        runtime.local_gather_opportunities(&restored, PersonId(1), facts, suggestions)
    );
    let mut calls = 0;
    runtime.local_gather_opportunities(&store, PersonId(1), facts, |good, upper| {
        calls += 1;
        suggestions(good, upper)
    });
    assert_eq!(calls, 16); // Depleted/unusable sources still consume one discovered slot.
}
#[test]
fn access_discovery_uses_local_custodian_and_ignores_execution_cooldown() {
    let (runtime, mut store) = setup(false);
    let present = [
        ObserverArea {
            actor: PersonId(1),
            area: AreaId(1),
        },
        ObserverArea {
            actor: PersonId(3),
            area: AreaId(2),
        },
    ];
    let actor = PersonId(2);
    runtime
        .insert_requests(
            &mut store,
            actor,
            Requests {
                next_request: BTreeMap::from([(ResourceId(100), SimTime(1_000))]),
            },
        )
        .unwrap();
    let mut looked_up = Vec::new();
    let actual = runtime.local_access_actions(&store, actor, AreaId(1), |custodian| {
        looked_up.push(custodian);
        present
            .iter()
            .find(|facts| facts.actor == custodian)
            .map(|facts| facts.area)
    });
    assert_eq!(looked_up.len(), 16);
    let mut expected = Vec::new();
    for (_, source) in runtime
        .resources(&store)
        .filter(|(_, source)| source.area == AreaId(1))
        .take(16)
    {
        if !source.access.contains(&actor)
            && source.custodian.is_some_and(|custodian| {
                custodian != actor
                    && present
                        .iter()
                        .any(|facts| facts.actor == custodian && facts.area == AreaId(1))
            })
        {
            expected.push(EcologyAction::RequestAccess {
                resource: source.id,
            });
        }
    }
    assert_eq!(actual, expected);
    assert!(actual.contains(&EcologyAction::RequestAccess {
        resource: ResourceId(100)
    }));
    assert!(!actual.contains(&EcologyAction::RequestAccess {
        resource: ResourceId(103)
    }));
    assert!(!actual.contains(&EcologyAction::RequestAccess {
        resource: ResourceId(116)
    }));
    runtime
        .insert_knowledge(
            &mut store,
            actor,
            Knowledge {
                known_resources: BTreeMap::from([(
                    ResourceId(100),
                    ResourceView {
                        id: ResourceId(100),
                        area: AreaId(1),
                        kind: ResourceKind::Berries,
                        available: Quantity(5_000),
                        accessible: false,
                        custodian: Some(PersonId(1)),
                        contamination: 0,
                        observed_at: SimTime(100),
                    },
                )]),
                ..Knowledge::default()
            },
        )
        .unwrap();
    let acquisitions = runtime.acquisition_prospects(
        &store,
        AcquisitionNeed {
            actor,
            area: AreaId(1),
            now: SimTime(100),
            good: Good::Food,
            quantity: Quantity(1_000),
            room: Quantity(1_000),
        },
        &[LocalAcquisition {
            resource: ResourceId(100),
            can_prepare: false,
            can_request: true,
        }],
    );
    assert!(acquisitions.is_empty()); // Execution/acquisition retains its distinct cooldown gate.
}
#[test]
fn autonomy_uses_personal_known_ids_positive_remote_memories_and_route_reachability() {
    let (runtime, mut store) = setup(false);
    let actor = PersonId(2);
    let view = |id, area, amount| ResourceView {
        id: ResourceId(id),
        area: AreaId(area),
        kind: ResourceKind::Berries,
        available: Quantity(amount),
        accessible: false,
        custodian: None,
        contamination: 0,
        observed_at: SimTime(500),
    };
    runtime
        .insert_knowledge(
            &mut store,
            actor,
            Knowledge {
                known_resources: BTreeMap::from([
                    (ResourceId(100), view(100, 1, 1)),
                    (ResourceId(900), view(900, 2, 1)),
                    (ResourceId(901), view(901, 3, 0)),
                    (ResourceId(902), view(902, 4, 1)),
                    (ResourceId(903), view(903, 2, 1)),
                ]),
                ..Knowledge::default()
            },
        )
        .unwrap();
    let local = [
        EcologyAction::RequestAccess {
            resource: ResourceId(100),
        },
        EcologyAction::Gather {
            resource: ResourceId(101),
            quantity: Quantity(1),
            unauthorized: false,
        },
        EcologyAction::Gather {
            resource: ResourceId(100),
            quantity: Quantity(2),
            unauthorized: true,
        },
        EcologyAction::GrantAccess {
            resource: ResourceId(100),
            recipient: PersonId(1),
            allowed: true,
        },
    ];
    let reachable = [AreaId(3), AreaId(2), AreaId(2)];
    let before = store.snapshot().unwrap();
    let options = runtime.autonomous_options(&store, actor, AreaId(1), &local, |area| {
        reachable.contains(&area)
    });
    assert_eq!(options.actions, vec![local[0].clone(), local[2].clone()]);
    assert_eq!(options.destinations, vec![AreaId(2)]);
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(
        runtime.autonomous_options(&store, PersonId(3), AreaId(1), &local, |area| reachable
            .contains(&area)),
        AutonomousOptions::default()
    );
    runtime
        .insert_gathering(
            &mut store,
            actor,
            Gathering {
                resource: ResourceId(100),
                destination: InventoryId(400),
                quantity: Quantity(1),
                tool: None,
                completes: SimTime(600),
                unauthorized: true,
            },
        )
        .unwrap();
    assert_eq!(
        runtime.autonomous_options(&store, actor, AreaId(1), &local, |area| reachable
            .contains(&area)),
        AutonomousOptions::default()
    );
}
