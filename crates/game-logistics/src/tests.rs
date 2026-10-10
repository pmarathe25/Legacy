use super::*;
struct Frame {
    now: SimTime,
    place: Location,
    free: bool,
    can_travel: bool,
    room: Quantity,
    minimum: Quantity,
    prerequisite_room: BTreeMap<Good, Quantity>,
    cargo: Vec<Cargo>,
    stocks: Vec<KnownStock>,
    destination: KnownDestination,
    contacts: BTreeSet<PersonId>,
    routes: KnownRoutes,
    offers: Vec<AcquisitionOffer<u8>>,
}
impl Frame {
    fn input(&self) -> Input<'_, u8> {
        Input {
            now: self.now,
            place: &self.place,
            free: self.free,
            can_travel: self.can_travel,
            room: self.room,
            minimum_handoff: self.minimum,
            prerequisite_room: &self.prerequisite_room,
            cargo: &self.cargo,
            stocks: &self.stocks,
            destination: Some(&self.destination),
            contacts: &self.contacts,
            routes: &self.routes,
            offers: &self.offers,
        }
    }
}
fn frame(good: Good, destination: Destination) -> Frame {
    Frame {
        now: SimTime(10),
        place: Location::AtArea(AreaId(1)),
        free: true,
        can_travel: true,
        room: Quantity(1_000),
        minimum: Quantity(1),
        prerequisite_room: BTreeMap::from([
            (Good::Tool, Quantity(1_000)),
            (Good::Timber, Quantity(1_000)),
            (Good::Seed, Quantity(1_000)),
            (Good::Food, Quantity(1_000)),
        ]),
        cargo: vec![],
        stocks: vec![KnownStock {
            inventory: InventoryId(8),
            good,
            quantity: Quantity(4_000),
            accessible: true,
            location: Location::AtArea(AreaId(1)),
            observed_at: SimTime(1),
            room: Some(Quantity(5_000)),
        }],
        destination: KnownDestination {
            destination,
            location: Location::AtArea(AreaId(2)),
            observed_at: SimTime(1),
            accessible: true,
            room: None,
            direct_transfer: matches!(destination, Destination::Contact(_)),
        },
        contacts: BTreeSet::from([PersonId(4)]),
        routes: BTreeMap::from([
            (AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(100))])),
            (AreaId(2), BTreeMap::from([(AreaId(1), SimDuration(100))])),
        ]),
        offers: vec![],
    }
}
fn delivery(good: Good, quantity: u64, destination: Destination) -> Delivery {
    Delivery::new(
        Requirement {
            good,
            quantity: Quantity(quantity),
            source: None,
        },
        destination,
    )
    .unwrap()
}
fn cargo(id: u64, good: Good, quantity: u64) -> Cargo {
    Cargo {
        inventory: InventoryId(id),
        good,
        quantity: Quantity(quantity),
        origin: None,
        origin_quantity: Quantity(0),
        room: Quantity(5_000),
    }
}
#[test]
fn every_good_uses_the_same_real_carry_travel_handoff_and_receipt_steps() {
    for good in [
        Good::Food,
        Good::Water,
        Good::Timber,
        Good::Tool,
        Good::Seed,
        Good::Herb,
    ] {
        let target = Destination::Contact(PersonId(4));
        let mut delivery = delivery(good, 1_000, target);
        let mut f = frame(good, target);
        assert_eq!(
            delivery.plan(&f.input()),
            Step::Carry {
                source: InventoryId(8),
                quantity: Quantity(1_000)
            }
        );
        assert_eq!(delivery.remaining, Quantity(1_000));
        f.cargo.push(cargo(9, good, 1_000));
        assert_eq!(
            delivery.plan(&f.input()),
            Step::Travel(Location::AtArea(AreaId(2)))
        );
        f.place = Location::AtArea(AreaId(2));
        assert_eq!(
            delivery.plan(&f.input()),
            Step::Arrived {
                source: InventoryId(9),
                quantity: Quantity(1_000)
            }
        );
        assert_eq!(delivery.remaining, Quantity(1_000));
        delivery
            .delivered(&DeliveryReceipt {
                sequence: 1,
                destination: target,
                good,
                quantity: Quantity(1_000),
            })
            .unwrap();
        assert_eq!(delivery.plan(&f.input()), Step::Complete);
    }
}
#[test]
fn partial_room_produces_actual_chunks_and_multiple_trips_remain_outstanding() {
    let target = Destination::Inventory(InventoryId(7));
    let mut delivery = delivery(Good::Timber, 3_000, target);
    let mut f = frame(Good::Timber, target);
    f.room = Quantity(600);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Carry {
            source: InventoryId(8),
            quantity: Quantity(600)
        }
    );
    f.cargo.push(cargo(9, Good::Timber, 600));
    f.place = Location::AtArea(AreaId(2));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Transfer {
            source: InventoryId(9),
            destination: InventoryId(7),
            quantity: Quantity(600)
        }
    );
    delivery
        .delivered(&DeliveryReceipt {
            sequence: 1,
            destination: target,
            good: Good::Timber,
            quantity: Quantity(600),
        })
        .unwrap();
    assert_eq!(delivery.remaining, Quantity(2_400));
    f.cargo.clear();
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Travel(Location::AtArea(AreaId(1)))
    );
}
#[test]
fn a_consumer_batch_accumulates_real_cargo_and_consolidates_without_pretending_to_feed() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Water, 1_000, target);
    let mut f = frame(Good::Water, target);
    f.minimum = Quantity(1_000);
    f.cargo.push(cargo(9, Good::Water, 400));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Carry {
            source: InventoryId(8),
            quantity: Quantity(600)
        }
    );
    f.cargo.push(cargo(10, Good::Water, 600));
    f.room = Quantity(0);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Transfer {
            source: InventoryId(9),
            destination: InventoryId(10),
            quantity: Quantity(400)
        }
    );
    f.cargo[0].quantity = Quantity(0);
    f.cargo[1].quantity = Quantity(1_000);
    f.place = Location::AtArea(AreaId(2));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Arrived {
            source: InventoryId(10),
            quantity: Quantity(1_000)
        }
    );
}
#[test]
fn stale_sources_are_leads_and_missing_contacts_never_appear_from_memory() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Food, 1_000, target);
    let mut f = frame(Good::Food, target);
    f.stocks[0].location = Location::AtArea(AreaId(2));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Travel(Location::AtArea(AreaId(2)))
    );
    f.place = Location::AtArea(AreaId(2));
    f.stocks[0].quantity = Quantity(0);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::NoSupply)
    );
    f.cargo.push(cargo(9, Good::Food, 1_000));
    f.contacts.clear();
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::MissingContact)
    );
    f.destination.location = Location::AtArea(AreaId(1));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Travel(Location::AtArea(AreaId(1)))
    );
}
#[test]
fn inaccessible_supplies_unknown_routes_and_receiving_room_are_grounded() {
    let target = Destination::Inventory(InventoryId(7));
    let delivery = delivery(Good::Food, 1_000, target);
    let mut f = frame(Good::Food, target);
    f.stocks[0].accessible = false;
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::NoSupply)
    );
    f.cargo.push(cargo(9, Good::Food, 1_000));
    f.routes.clear();
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::UnknownRoute)
    );
    f.place = Location::AtArea(AreaId(2));
    f.destination.room = Some(Quantity(300));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Transfer {
            source: InventoryId(9),
            destination: InventoryId(7),
            quantity: Quantity(300)
        }
    );
    f.minimum = Quantity(1_000);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::DestinationFull)
    );
}
#[test]
fn acquisition_preconditions_and_capacity_do_not_predict_a_successful_yield() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Timber, 1_000, target);
    let mut f = frame(Good::Timber, target);
    f.stocks.clear();
    f.offers.push(AcquisitionOffer {
        key: 1,
        good: Good::Timber,
        source: None,
        location: f.place.clone(),
        observed_at: SimTime(1),
        accessible: true,
        quantity: OfferYield::Known(Quantity(1_000)),
        requires_carrying_room: true,
        requirements: vec![Prerequisite {
            requirement: Requirement {
                good: Good::Tool,
                quantity: Quantity(1_000),
                source: None,
            },
            placement: PrerequisitePlacement::Carried,
        }],
        action: 42,
    });
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::NoSupply)
    );
    f.cargo.push(cargo(9, Good::Tool, 1_000));
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
    assert_eq!(delivery.remaining, Quantity(1_000));
    f.room = Quantity(900);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::NoSupply)
    );
}
#[test]
fn permission_and_exploration_actions_are_opaque_attempts_without_invented_supply() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Water, 1_000, target);
    let mut f = frame(Good::Water, target);
    f.stocks[0].accessible = false;
    f.offers.push(AcquisitionOffer {
        key: 1,
        good: Good::Water,
        source: None,
        location: f.place.clone(),
        observed_at: SimTime(1),
        accessible: true,
        quantity: OfferYield::Known(Quantity(0)),
        requires_carrying_room: false,
        requirements: vec![],
        action: 41,
    });
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(41));
    f.offers[0].quantity = OfferYield::Unknown;
    f.offers[0].action = 42;
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
    assert_eq!(delivery.remaining, Quantity(1_000));
    f.stocks[0].accessible = true;
    f.stocks[0].location = Location::AtArea(AreaId(2));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Travel(Location::AtArea(AreaId(2)))
    );
}
#[test]
fn source_constrained_work_uses_only_actual_qualified_cargo_and_receipts_are_idempotent() {
    let target = Destination::Contact(PersonId(4));
    let mut delivery = Delivery::new(
        Requirement {
            good: Good::Food,
            quantity: Quantity(1_000),
            source: Some(InventoryId(8)),
        },
        target,
    )
    .unwrap();
    let mut f = frame(Good::Food, target);
    f.cargo.push(cargo(9, Good::Food, 1_000));
    assert!(matches!(
        delivery.plan(&f.input()),
        Step::Carry {
            source: InventoryId(8),
            ..
        }
    ));
    f.cargo[0].origin = Some(InventoryId(8));
    f.cargo[0].origin_quantity = Quantity(1_000);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Travel(Location::AtArea(AreaId(2)))
    );
    let receipt = DeliveryReceipt {
        sequence: 8,
        destination: target,
        good: Good::Food,
        quantity: Quantity(500),
    };
    assert!(delivery.delivered(&receipt).unwrap());
    assert!(!delivery.delivered(&receipt).unwrap());
    assert_eq!(delivery.remaining, Quantity(500));
    let before = delivery.clone();
    let mut wrong = receipt.clone();
    wrong.sequence = 9;
    wrong.good = Good::Water;
    assert_eq!(delivery.delivered(&wrong), Err(Error::Unavailable));
    assert_eq!(delivery, before);
}
#[test]
fn passive_planning_and_input_order_do_not_change_state_or_ties() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Food, 1_000, target);
    let mut f = frame(Good::Food, target);
    let mut second = f.stocks[0].clone();
    second.inventory = InventoryId(6);
    f.stocks.push(second);
    let before = delivery.clone();
    let chosen = delivery.plan(&f.input());
    f.stocks.reverse();
    assert_eq!(delivery.plan(&f.input()), chosen);
    assert_eq!(delivery, before);
    f.free = false;
    assert_eq!(delivery.plan(&f.input()), Step::Continue);
    f.free = true;
    f.can_travel = false;
    f.cargo.push(cargo(9, Good::Food, 1_000));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::CannotTravel)
    );
}
#[test]
fn actual_partial_delivery_roundtrips_and_road_contact_geometry_is_exact() {
    let target = Destination::Contact(PersonId(4));
    let mut delivery = delivery(Good::Food, 2_000, target);
    delivery
        .delivered(&DeliveryReceipt {
            sequence: 7,
            destination: target,
            good: Good::Food,
            quantity: Quantity(1_000),
        })
        .unwrap();
    let restored: Delivery =
        serde_json::from_slice(&serde_json::to_vec(&delivery).unwrap()).unwrap();
    assert_eq!(restored, delivery);
    let mut f = frame(Good::Food, target);
    f.cargo.push(cargo(9, Good::Food, 1_000));
    f.place = Location::StoppedOnRoad {
        from: AreaId(1),
        to: AreaId(2),
        progress: SimDuration(1),
        duration: SimDuration(3),
    };
    f.destination.location = Location::StoppedOnRoad {
        from: AreaId(2),
        to: AreaId(1),
        progress: SimDuration(4),
        duration: SimDuration(6),
    };
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Arrived {
            source: InventoryId(9),
            quantity: Quantity(1_000)
        }
    );
}

#[test]
fn remembered_cargo_is_not_counted_twice_for_acquisition_equipment() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Timber, 1_000, target);
    let mut f = frame(Good::Timber, target);
    f.stocks.clear();
    f.cargo.push(cargo(9, Good::Tool, 1_000));
    f.stocks.push(KnownStock {
        inventory: InventoryId(9),
        good: Good::Tool,
        quantity: Quantity(1_000),
        accessible: true,
        location: f.place.clone(),
        observed_at: SimTime(1),
        room: Some(Quantity(5_000)),
    });
    f.offers.push(AcquisitionOffer {
        key: 1,
        good: Good::Timber,
        source: None,
        location: f.place.clone(),
        observed_at: SimTime(1),
        accessible: true,
        quantity: OfferYield::Known(Quantity(1_000)),
        requires_carrying_room: true,
        requirements: vec![Prerequisite {
            requirement: Requirement {
                good: Good::Tool,
                quantity: Quantity(2_000),
                source: None,
            },
            placement: PrerequisitePlacement::Carried,
        }],
        action: 42,
    });
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::NoSupply)
    );
}

#[test]
fn actual_partial_cargo_overrides_its_stale_stock_snapshot_during_replenishment() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Water, 1_000, target);
    let mut f = frame(Good::Water, target);
    f.minimum = Quantity(1_000);
    f.cargo.push(cargo(6, Good::Water, 500));
    let mut remembered = f.stocks[0].clone();
    remembered.inventory = InventoryId(6);
    remembered.quantity = Quantity(1_000);
    f.stocks.push(remembered);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Carry {
            source: InventoryId(8),
            quantity: Quantity(500)
        }
    );
    f.stocks.remove(0);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::NoSupply)
    );
}

#[test]
fn source_at_the_destination_can_be_consumed_or_transferred_without_fake_carrying() {
    for good in [Good::Food, Good::Water, Good::Timber] {
        let target = Destination::Contact(PersonId(4));
        let delivery = delivery(good, 1_000, target);
        let mut f = frame(good, target);
        f.destination.location = f.place.clone();
        f.room = Quantity(0);
        assert_eq!(
            delivery.plan(&f.input()),
            Step::Arrived {
                source: InventoryId(8),
                quantity: Quantity(1_000)
            }
        );
        assert!(f.cargo.is_empty());
        assert_eq!(delivery.remaining, Quantity(1_000));
        f.contacts.clear();
        assert_eq!(
            delivery.plan(&f.input()),
            Step::Blocked(BlockReason::MissingContact)
        );
        f.contacts.insert(PersonId(4));
        f.stocks[0].quantity = Quantity(0);
        assert_eq!(
            delivery.plan(&f.input()),
            Step::Blocked(BlockReason::Capacity)
        );
        f.stocks[0].quantity = Quantity(4_000);
        f.destination.destination = Destination::Inventory(InventoryId(7));
        f.destination.room = Some(Quantity(300));
        f.destination.direct_transfer = true;
        let inventory_delivery = Delivery::new(
            Requirement {
                good,
                quantity: Quantity(1_000),
                source: None,
            },
            f.destination.destination,
        )
        .unwrap();
        assert_eq!(
            inventory_delivery.plan(&f.input()),
            Step::Transfer {
                source: InventoryId(8),
                destination: InventoryId(7),
                quantity: Quantity(300)
            }
        );
        f.minimum = Quantity(1_000);
        assert_eq!(
            inventory_delivery.plan(&f.input()),
            Step::Blocked(BlockReason::DestinationFull)
        );
    }
}
#[test]
fn source_equal_target_needs_current_observation_and_never_counts_as_a_receipt() {
    let target = Destination::Inventory(InventoryId(8));
    let mut delivery = Delivery::new(
        Requirement {
            good: Good::Timber,
            quantity: Quantity(1_000),
            source: Some(InventoryId(8)),
        },
        target,
    )
    .unwrap();
    let mut f = frame(Good::Timber, target);
    f.destination.location = f.place.clone();
    f.destination.room = Some(Quantity(0));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::DestinationFull)
    );
    f.stocks[0].observed_at = f.now;
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Arrived {
            source: InventoryId(8),
            quantity: Quantity(1_000)
        }
    );
    assert_eq!(delivery.remaining, Quantity(1_000));
    delivery
        .delivered(&DeliveryReceipt {
            sequence: 1,
            destination: target,
            good: Good::Timber,
            quantity: Quantity(1_000),
        })
        .unwrap();
    assert_eq!(delivery.remaining, Quantity(0));
}

#[test]
fn direct_inventory_transfer_needs_a_primitive_endpoint_capability() {
    let target = Destination::Inventory(InventoryId(7));
    let delivery = delivery(Good::Timber, 1_000, target);
    let mut f = frame(Good::Timber, target);
    f.destination.location = f.place.clone();
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Carry {
            source: InventoryId(8),
            quantity: Quantity(1_000)
        }
    );
    f.destination.direct_transfer = true;
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Transfer {
            source: InventoryId(8),
            destination: InventoryId(7),
            quantity: Quantity(1_000)
        }
    );
}

fn prerequisite(good: Good, quantity: u64, placement: PrerequisitePlacement) -> Prerequisite {
    Prerequisite {
        requirement: Requirement {
            good,
            quantity: Quantity(quantity),
            source: None,
        },
        placement,
    }
}
fn offer(
    good: Good,
    key: u64,
    action: u8,
    requirements: Vec<Prerequisite>,
) -> AcquisitionOffer<u8> {
    AcquisitionOffer {
        key,
        good,
        source: None,
        location: Location::AtArea(AreaId(1)),
        observed_at: SimTime(1),
        accessible: true,
        quantity: OfferYield::Known(Quantity(1_000)),
        requires_carrying_room: true,
        requirements,
        action,
    }
}
fn known_stock(id: u64, good: Good, quantity: u64, area: u64) -> KnownStock {
    KnownStock {
        inventory: InventoryId(id),
        good,
        quantity: Quantity(quantity),
        accessible: true,
        location: Location::AtArea(AreaId(area)),
        observed_at: SimTime(1),
        room: Some(Quantity(5_000)),
    }
}
#[test]
fn carried_tool_decomposes_through_a_real_carry_before_the_original_operator() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Timber, 1_000, target);
    let before = delivery.clone();
    let mut f = frame(Good::Timber, target);
    f.stocks = vec![known_stock(20, Good::Tool, 1_000, 1)];
    f.offers = vec![offer(
        Good::Timber,
        1,
        42,
        vec![prerequisite(
            Good::Tool,
            1_000,
            PrerequisitePlacement::Carried,
        )],
    )];
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Carry {
            source: InventoryId(20),
            quantity: Quantity(1_000)
        }
    );
    // A selected action is never treated as a successful pickup.
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Carry {
            source: InventoryId(20),
            quantity: Quantity(1_000)
        }
    );
    f.stocks[0].quantity = Quantity(0);
    f.cargo.push(cargo(21, Good::Tool, 1_000));
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
    assert_eq!(delivery, before);
    f.offers[0].requirements[0].placement =
        PrerequisitePlacement::AccessibleAtProvider { inventory: None };
    f.cargo.clear();
    f.stocks[0].quantity = Quantity(1_000);
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
}
#[test]
fn missing_factory_material_composes_acquisition_travel_and_actual_input_transfer() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Food, 1_000, target);
    let mut f = frame(Good::Food, target);
    f.stocks = vec![
        known_stock(20, Good::Timber, 0, 1),
        known_stock(22, Good::Timber, 1_000, 2),
    ];
    f.offers = vec![offer(
        Good::Food,
        1,
        42,
        vec![prerequisite(
            Good::Timber,
            1_000,
            PrerequisitePlacement::AccessibleAtProvider {
                inventory: Some(InventoryId(20)),
            },
        )],
    )];
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Travel(Location::AtArea(AreaId(2)))
    );
    f.place = Location::AtArea(AreaId(2));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Carry {
            source: InventoryId(22),
            quantity: Quantity(1_000)
        }
    );
    f.stocks[1].quantity = Quantity(0);
    f.cargo.push(cargo(23, Good::Timber, 1_000));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Travel(Location::AtArea(AreaId(1)))
    );
    f.place = Location::AtArea(AreaId(1));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Transfer {
            source: InventoryId(23),
            destination: InventoryId(20),
            quantity: Quantity(1_000)
        }
    );
    assert_eq!(delivery.remaining, Quantity(1_000));
    f.cargo[0].quantity = Quantity(0);
    f.stocks[0].quantity = Quantity(1_000);
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
}
#[test]
fn nested_material_dependencies_use_actual_tool_then_material_then_production() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Food, 1_000, target);
    let mut f = frame(Good::Food, target);
    f.stocks = vec![
        known_stock(20, Good::Timber, 0, 1),
        known_stock(22, Good::Tool, 1_000, 1),
    ];
    f.offers = vec![
        offer(
            Good::Food,
            1,
            42,
            vec![prerequisite(
                Good::Timber,
                1_000,
                PrerequisitePlacement::AccessibleAtProvider {
                    inventory: Some(InventoryId(20)),
                },
            )],
        ),
        offer(
            Good::Timber,
            2,
            43,
            vec![prerequisite(
                Good::Tool,
                1_000,
                PrerequisitePlacement::Carried,
            )],
        ),
    ];
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Carry {
            source: InventoryId(22),
            quantity: Quantity(1_000)
        }
    );
    f.cargo.push(cargo(23, Good::Tool, 1_000));
    f.stocks[1].quantity = Quantity(0);
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(43));
    f.cargo.push(cargo(24, Good::Timber, 1_000));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Transfer {
            source: InventoryId(24),
            destination: InventoryId(20),
            quantity: Quantity(1_000)
        }
    );
    f.cargo[1].quantity = Quantity(0);
    f.stocks[0].quantity = Quantity(1_000);
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
}
#[test]
fn cycles_and_dependency_depth_stop_without_crediting_or_inventing_progress() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Timber, 1_000, target);
    let before = delivery.clone();
    let mut f = frame(Good::Timber, target);
    f.stocks.clear();
    f.offers = vec![
        offer(
            Good::Timber,
            1,
            42,
            vec![prerequisite(
                Good::Tool,
                1_000,
                PrerequisitePlacement::Carried,
            )],
        ),
        offer(
            Good::Tool,
            2,
            43,
            vec![prerequisite(
                Good::Timber,
                1_000,
                PrerequisitePlacement::Carried,
            )],
        ),
    ];
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::PrerequisiteCycle)
    );
    assert_eq!(delivery, before);
    f.offers.clear();
    f.offers.push(offer(
        Good::Timber,
        1,
        42,
        vec![Prerequisite {
            requirement: Requirement {
                good: Good::Tool,
                quantity: Quantity(1_000),
                source: Some(InventoryId(20)),
            },
            placement: PrerequisitePlacement::Carried,
        }],
    ));
    for id in 20..30 {
        let requirements = if id == 29 {
            vec![]
        } else {
            vec![Prerequisite {
                requirement: Requirement {
                    good: Good::Tool,
                    quantity: Quantity(1_000),
                    source: Some(InventoryId(id + 1)),
                },
                placement: PrerequisitePlacement::Carried,
            }]
        };
        let mut repair = offer(Good::Tool, id, id as u8, requirements);
        repair.source = Some(InventoryId(id));
        f.offers.push(repair);
    }
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::PrerequisiteLimit)
    );
    assert_eq!(delivery, before);
}
#[test]
fn refused_depleted_or_unknown_capacity_prerequisites_never_enable_the_operator() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Timber, 1_000, target);
    let mut f = frame(Good::Timber, target);
    f.stocks = vec![known_stock(20, Good::Tool, 1_000, 1)];
    f.offers = vec![offer(
        Good::Timber,
        1,
        42,
        vec![prerequisite(
            Good::Tool,
            1_000,
            PrerequisitePlacement::Carried,
        )],
    )];
    f.stocks[0].accessible = false;
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::NoSupply)
    );
    f.stocks[0].accessible = true;
    f.stocks[0].quantity = Quantity(0);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::NoSupply)
    );
    f.stocks[0].quantity = Quantity(1_000);
    f.prerequisite_room.clear();
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::Capacity)
    );
    assert_eq!(delivery.remaining, Quantity(1_000));
}
#[test]
fn origin_qualified_parent_cargo_does_not_hide_unqualified_same_good_prerequisites() {
    let target = Destination::Contact(PersonId(4));
    let delivery = Delivery::new(
        Requirement {
            good: Good::Timber,
            quantity: Quantity(1_000),
            source: Some(InventoryId(8)),
        },
        target,
    )
    .unwrap();
    let mut f = frame(Good::Timber, target);
    f.minimum = Quantity(1_000);
    f.stocks.clear();
    let mut held = cargo(9, Good::Timber, 2_000);
    held.origin = Some(InventoryId(8));
    held.origin_quantity = Quantity(500);
    f.cargo.push(held);
    let mut transform = offer(
        Good::Timber,
        1,
        42,
        vec![prerequisite(
            Good::Timber,
            1_000,
            PrerequisitePlacement::Carried,
        )],
    );
    transform.source = Some(InventoryId(8));
    f.offers.push(transform);
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
    assert_eq!(delivery.remaining, Quantity(1_000));
}
#[test]
fn partial_actual_input_cargo_is_counted_once_and_replenishment_is_positive() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Food, 1_000, target);
    let mut f = frame(Good::Food, target);
    f.stocks = vec![
        known_stock(20, Good::Timber, 1_000, 1),
        known_stock(22, Good::Timber, 1_000, 1),
    ];
    f.cargo.push(cargo(20, Good::Timber, 400));
    f.offers = vec![offer(
        Good::Food,
        1,
        42,
        vec![prerequisite(
            Good::Timber,
            1_000,
            PrerequisitePlacement::AccessibleAtProvider {
                inventory: Some(InventoryId(20)),
            },
        )],
    )];
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Carry {
            source: InventoryId(22),
            quantity: Quantity(600)
        }
    );
}

#[test]
fn the_same_local_stock_uses_the_declared_consumer_capability_for_handoff() {
    for good in [Good::Food, Good::Water, Good::Timber] {
        let target = Destination::Contact(PersonId(4));
        let delivery = delivery(good, 1_000, target);
        let mut f = frame(good, target);
        f.destination.location = f.place.clone();
        f.destination.direct_transfer = false;
        assert_eq!(
            delivery.plan(&f.input()),
            Step::Carry {
                source: InventoryId(8),
                quantity: Quantity(1_000)
            }
        );
        f.destination.direct_transfer = true;
        assert_eq!(
            delivery.plan(&f.input()),
            Step::Arrived {
                source: InventoryId(8),
                quantity: Quantity(1_000)
            }
        );
        assert_eq!(delivery.remaining, Quantity(1_000));
    }
}

#[test]
fn full_body_can_process_real_carried_input_into_local_storage_without_fulfilling_delivery() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Food, 1_000, target);
    let mut f = frame(Good::Food, target);
    f.room = Quantity(0);
    f.stocks.clear();
    f.cargo.push(cargo(20, Good::Timber, 1_000));
    let mut work = offer(
        Good::Food,
        1,
        42,
        vec![prerequisite(
            Good::Timber,
            1_000,
            PrerequisitePlacement::AccessibleAtProvider { inventory: None },
        )],
    );
    work.requires_carrying_room = false;
    f.offers.push(work);
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
    assert_eq!(delivery.remaining, Quantity(1_000));
    // Merely planning the work does not claim consumption or future yield.
    assert_eq!(f.cargo[0].quantity, Quantity(1_000));
    f.offers[0].requires_carrying_room = true;
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::Capacity)
    );
}
#[test]
fn empty_body_room_still_allows_permission_preparation_and_blocks_cargo_yields() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Water, 1_000, target);
    let mut f = frame(Good::Water, target);
    f.room = Quantity(0);
    f.stocks.clear();
    f.offers.push(offer(Good::Water, 1, 42, vec![]));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::Capacity)
    );
    f.offers[0].quantity = OfferYield::Known(Quantity(0));
    f.offers[0].requires_carrying_room = false;
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
    assert_eq!(delivery.remaining, Quantity(1_000));
    f.offers[0].quantity = OfferYield::Unknown;
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
}

#[test]
fn transferring_real_material_can_free_the_room_needed_for_another_prerequisite() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Food, 1_000, target);
    let mut f = frame(Good::Food, target);
    f.room = Quantity(0);
    f.prerequisite_room.insert(Good::Tool, Quantity(0));
    f.cargo.push(cargo(21, Good::Timber, 1_000));
    f.stocks = vec![
        known_stock(20, Good::Timber, 0, 1),
        known_stock(22, Good::Tool, 1_000, 1),
    ];
    let mut work = offer(
        Good::Food,
        1,
        42,
        vec![
            prerequisite(Good::Tool, 1_000, PrerequisitePlacement::Carried),
            prerequisite(
                Good::Timber,
                1_000,
                PrerequisitePlacement::AccessibleAtProvider {
                    inventory: Some(InventoryId(20)),
                },
            ),
        ],
    );
    work.requires_carrying_room = false;
    f.offers.push(work);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Transfer {
            source: InventoryId(21),
            destination: InventoryId(20),
            quantity: Quantity(1_000)
        }
    );
    f.cargo[0].quantity = Quantity(0);
    f.stocks[0].quantity = Quantity(1_000);
    f.room = Quantity(1_000);
    f.prerequisite_room.insert(Good::Tool, Quantity(1_000));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Carry {
            source: InventoryId(22),
            quantity: Quantity(1_000)
        }
    );
    f.cargo.push(cargo(23, Good::Tool, 1_000));
    f.room = Quantity(0);
    f.prerequisite_room.insert(Good::Tool, Quantity(0));
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
    assert_eq!(delivery.remaining, Quantity(1_000));
}

#[test]
fn single_lot_prerequisites_consolidate_actual_cargo_before_using_equipment() {
    let target = Destination::Contact(PersonId(4));
    let delivery = delivery(Good::Food, 1_000, target);
    let mut f = frame(Good::Food, target);
    f.stocks.clear();
    f.cargo = vec![cargo(21, Good::Tool, 600), cargo(22, Good::Tool, 400)];
    f.offers.push(offer(
        Good::Food,
        1,
        42,
        vec![prerequisite(
            Good::Tool,
            1_000,
            PrerequisitePlacement::CarriedInOneLot,
        )],
    ));
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Transfer {
            source: InventoryId(22),
            destination: InventoryId(21),
            quantity: Quantity(400),
        }
    );
    assert_eq!(f.cargo[0].quantity, Quantity(600));
    f.cargo[0].room = Quantity(0);
    f.cargo[1].room = Quantity(0);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::Capacity)
    );
    // Only the actual transfer makes one usable lot; forecasts change no cargo.
    f.cargo[0].quantity = Quantity(1_000);
    f.cargo[1].quantity = Quantity(0);
    assert_eq!(delivery.plan(&f.input()), Step::Acquire(42));
}

#[test]
fn empty_compartments_can_receive_qualified_consolidation_for_deliveries_and_prerequisites() {
    let target = Destination::Contact(PersonId(4));
    let mut delivery = delivery(Good::Tool, 1_000, target);
    delivery.requirement.source = Some(InventoryId(8));
    let mut f = frame(Good::Tool, target);
    f.minimum = Quantity(1_000);
    f.stocks.clear();
    let mut a = cargo(20, Good::Tool, 500);
    let mut b = cargo(21, Good::Tool, 500);
    for lot in [&mut a, &mut b] {
        lot.origin = Some(InventoryId(8));
        lot.origin_quantity = Quantity(500);
        lot.room = Quantity(0);
    }
    f.cargo = vec![a, b, cargo(22, Good::Tool, 0)];
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Transfer {
            source: InventoryId(20),
            destination: InventoryId(22),
            quantity: Quantity(500),
        }
    );
    f.cargo[0].quantity = Quantity(0);
    f.cargo[0].origin_quantity = Quantity(0);
    f.cargo[2].quantity = Quantity(500);
    // Material with no origin receipt cannot satisfy the source-qualified goal.
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Blocked(BlockReason::NoSupply)
    );
    f.cargo[2].origin = Some(InventoryId(8));
    f.cargo[2].origin_quantity = Quantity(500);
    assert_eq!(
        delivery.plan(&f.input()),
        Step::Transfer {
            source: InventoryId(21),
            destination: InventoryId(22),
            quantity: Quantity(500),
        }
    );
    let food = Delivery::new(
        Requirement {
            good: Good::Food,
            quantity: Quantity(1_000),
            source: None,
        },
        target,
    )
    .unwrap();
    let mut equipment = prerequisite(Good::Tool, 1_000, PrerequisitePlacement::CarriedInOneLot);
    equipment.requirement.source = Some(InventoryId(8));
    f.offers.push(offer(Good::Food, 1, 42, vec![equipment]));
    f.cargo[0].quantity = Quantity(500);
    f.cargo[0].origin_quantity = Quantity(500);
    f.cargo[2].quantity = Quantity(0);
    f.cargo[2].origin_quantity = Quantity(0);
    assert_eq!(
        food.plan(&f.input()),
        Step::Transfer {
            source: InventoryId(20),
            destination: InventoryId(22),
            quantity: Quantity(500),
        }
    );
    assert_eq!(f.cargo[2].quantity, Quantity(0));
}
