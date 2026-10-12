use super::*;

fn setup() -> (Runtime, Store, AcquisitionNeed, GatherFacts) {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let runtime = Runtime::new(EcologyRules::default(), 0).unwrap();
    runtime
        .insert_resource(
            &mut store,
            Resource {
                id: ResourceId(100),
                area: AreaId(1),
                kind: ResourceKind::Coppice,
                remaining: Quantity(500),
                capacity: Quantity(5_000),
                daily_growth: Quantity(10),
                growth_remainder: 0,
                decay_permille: 0,
                soil_water: 75,
                water_capacity: 120,
                optimal_water: 50,
                minimum_temperature: 0,
                optimum_temperature: 20,
                contamination: 0,
                runoff_contamination: 0,
                labor_per_unit: MINUTE,
                workers: 2,
                access: BTreeSet::from([PersonId(1)]),
                custodian: Some(PersonId(1)),
            },
        )
        .unwrap();
    runtime
        .insert_knowledge(
            &mut store,
            PersonId(1),
            Knowledge {
                known_resources: BTreeMap::from([(
                    ResourceId(100),
                    ResourceView {
                        id: ResourceId(100),
                        area: AreaId(1),
                        kind: ResourceKind::Coppice,
                        available: Quantity(500),
                        accessible: true,
                        custodian: Some(PersonId(1)),
                        contamination: 0,
                        observed_at: SimTime(5),
                    },
                )]),
                known_plots: BTreeMap::new(),
                observed_areas: BTreeMap::from([(AreaId(1), SimTime(5))]),
            },
        )
        .unwrap();
    let need = AcquisitionNeed {
        actor: PersonId(1),
        area: AreaId(1),
        now: SimTime(10),
        good: Good::Timber,
        quantity: Quantity(1_000),
        room: Quantity(800),
    };
    let facts = GatherFacts {
        now: need.now,
        area: need.area,
        capable: true,
        adult: true,
        carrying_room: need.room,
        tool: Some(InventoryId(10)),
    };
    (runtime, store, need, facts)
}

#[test]
fn local_gather_offer_composes_with_logistics_and_requires_a_real_tool_lot() {
    let (runtime, store, need, facts) = setup();
    let offers = runtime.acquisition_offers(&store, need, facts, &[], &[]);
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0].quantity, OfferYield::Known(Quantity(500)));
    let action = AcquisitionAction::Resource(EcologyAction::Gather {
        resource: ResourceId(100),
        quantity: Quantity(500),
        unauthorized: false,
    });
    assert_eq!(offers[0].action, action);
    let destination = game_logistics::KnownDestination {
        destination: game_logistics::Destination::Contact(PersonId(2)),
        location: Location::AtArea(AreaId(1)),
        observed_at: SimTime(5),
        accessible: true,
        room: None,
        direct_transfer: true,
    };
    let delivery = game_logistics::Delivery::new(
        Requirement {
            good: Good::Timber,
            quantity: Quantity(500),
            source: None,
        },
        destination.destination,
    )
    .unwrap();
    let room = BTreeMap::from([(Good::Tool, Quantity(1_000))]);
    let routes = BTreeMap::new();
    let contacts = BTreeSet::from([PersonId(2)]);
    let cargo = [game_logistics::Cargo {
        inventory: InventoryId(10),
        good: Good::Tool,
        quantity: Quantity(1_000),
        origin: None,
        origin_quantity: Quantity(0),
        room: Quantity(0),
    }];
    let mut input = game_logistics::Input {
        now: need.now,
        place: &Location::AtArea(need.area),
        free: true,
        can_travel: true,
        room: need.room,
        prerequisite_room: &room,
        minimum_handoff: Quantity(1),
        cargo: &[],
        stocks: &[],
        destination: Some(&destination),
        contacts: &contacts,
        routes: &routes,
        offers: &offers,
    };
    assert_eq!(
        delivery.plan(&input),
        game_logistics::Step::Blocked(game_logistics::BlockReason::NoSupply)
    );
    input.cargo = &cargo;
    assert_eq!(delivery.plan(&input), game_logistics::Step::Acquire(action));
    assert_eq!(delivery.remaining, Quantity(500));
    assert_eq!(
        runtime.resource(&store, ResourceId(100)).unwrap().remaining,
        Quantity(500)
    );
}

#[test]
fn local_admission_does_not_fabricate_remote_yield_or_exploration_observations() {
    let (runtime, mut store, need, facts) = setup();
    let conditional = runtime.acquisition_offers(
        &store,
        need,
        GatherFacts {
            tool: None,
            ..facts
        },
        &[],
        &[],
    );
    assert_eq!(conditional.len(), 1);
    assert_eq!(
        conditional[0].requirements[0].placement,
        PrerequisitePlacement::CarriedInOneLot
    );
    assert!(
        runtime
            .acquisition_offers(
                &store,
                need,
                GatherFacts {
                    capable: false,
                    ..facts
                },
                &[],
                &[],
            )
            .is_empty()
    );
    let mut knowledge = runtime.knowledge(&store, need.actor).unwrap().clone();
    let source = knowledge.known_resources.get_mut(&ResourceId(100)).unwrap();
    source.area = AreaId(2);
    source.available = Quantity(2_000);
    runtime
        .insert_knowledge(&mut store, need.actor, knowledge)
        .unwrap();
    let offers = runtime.acquisition_offers(
        &store,
        need,
        GatherFacts {
            tool: None,
            ..facts
        },
        &[],
        &[AreaId(1), AreaId(3)],
    );
    assert_eq!(offers[0].quantity, OfferYield::Known(Quantity(800)));
    assert_eq!(offers[0].location, Location::AtArea(AreaId(2)));
    assert_eq!(offers[0].observed_at, SimTime(5));
    assert!(matches!(
        offers[1].action,
        AcquisitionAction::Explore {
            destination: AreaId(3)
        }
    ));
    assert_eq!(offers[1].quantity, OfferYield::Unknown);
    assert_eq!(offers[1].observed_at, SimTime(5));
    let mut knowledge = runtime.knowledge(&store, need.actor).unwrap().clone();
    knowledge.observed_areas.clear();
    runtime
        .insert_knowledge(&mut store, need.actor, knowledge)
        .unwrap();
    assert_eq!(
        runtime
            .acquisition_offers(&store, need, facts, &[], &[AreaId(3)])
            .len(),
        1
    );
}

#[test]
fn access_preparation_respects_request_cooldown_without_hiding_gather_prospects() {
    let (runtime, mut store, need, facts) = setup();
    let mut knowledge = runtime.knowledge(&store, need.actor).unwrap().clone();
    let source = knowledge.known_resources.get_mut(&ResourceId(100)).unwrap();
    source.area = AreaId(2);
    source.accessible = false;
    source.custodian = Some(PersonId(3));
    runtime
        .insert_knowledge(&mut store, need.actor, knowledge)
        .unwrap();
    let offers = runtime.acquisition_offers(&store, need, facts, &[], &[]);
    assert_eq!(offers.len(), 2);
    assert_eq!(offers[0].quantity, OfferYield::Known(Quantity(0)));
    assert!(offers[0].requirements.is_empty());
    assert!(!offers[0].requires_carrying_room);
    runtime
        .insert_requests(
            &mut store,
            need.actor,
            Requests {
                next_request: BTreeMap::from([(ResourceId(100), SimTime(11))]),
            },
        )
        .unwrap();
    let offers = runtime.acquisition_offers(&store, need, facts, &[], &[]);
    assert_eq!(offers.len(), 1);
    assert!(matches!(
        offers[0].action,
        AcquisitionAction::Resource(EcologyAction::Gather {
            unauthorized: true,
            ..
        })
    ));
}
