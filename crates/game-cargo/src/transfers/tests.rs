use super::*;
fn stock(id: u64, holder: Option<u64>, amount: u64, capacity: u64) -> Inventory {
    Inventory {
        id: InventoryId(id),
        location: holder.map_or(InventoryLocation::Area(AreaId(1)), |id| {
            InventoryLocation::Carried(PersonId(id))
        }),
        quantity: Quantity(amount),
        capacity: Quantity(capacity),
        good: Good::Timber,
        access: BTreeSet::from([PersonId(1), PersonId(2)]),
        last_granted: None,
        spoils_at: None,
    }
}
fn facts() -> ClosingFacts {
    ClosingFacts {
        alive: true,
        health: 100,
        source_contact: true,
        source_access: true,
        receiver_contact: true,
        receiver_access: true,
        receiver_accepted: false,
        record_permission: true,
    }
}
fn request(actor: u64, source: u64, destination: u64, quantity: u64) -> TransferRequest {
    TransferRequest {
        intent: TransferIntent {
            actor: PersonId(actor),
            source: InventoryId(source),
            destination: InventoryId(destination),
            quantity: Quantity(quantity),
        },
        facts: facts(),
    }
}
fn plan(
    stocks: &BTreeMap<InventoryId, Inventory>,
    opening: Option<&BTreeMap<InventoryId, u64>>,
    requests: Vec<TransferRequest>,
    room: Option<&BTreeMap<PersonId, u64>>,
) -> PreparedTransfers {
    prepare_transfers(
        &stocks.iter().map(|(id, stock)| (*id, stock)).collect(),
        opening,
        requests,
        room,
        |_| true,
    )
    .unwrap()
}
fn stocks(rows: Vec<Inventory>) -> BTreeMap<InventoryId, Inventory> {
    rows.into_iter().map(|stock| (stock.id, stock)).collect()
}
#[test]
fn incoming_goods_cannot_be_spent_in_the_same_phase_in_any_request_order() {
    let initial = stocks(vec![
        stock(1, None, 1000, 3000),
        stock(2, Some(1), 0, 3000),
        stock(3, None, 0, 3000),
    ]);
    let requests = vec![request(1, 1, 2, 1000), request(1, 2, 3, 1000)];
    let room = BTreeMap::from([(PersonId(1), 3000)]);
    let mut forward = initial.clone();
    let result = plan(&initial, None, requests.clone(), Some(&room))
        .commit(&mut forward)
        .unwrap();
    assert_eq!(
        result.iter().map(|(_, yes)| *yes).collect::<Vec<_>>(),
        vec![true, false]
    );
    let mut reverse = initial.clone();
    let mut reversed = requests;
    reversed.reverse();
    assert_eq!(
        plan(&initial, None, reversed, Some(&room))
            .commit(&mut reverse)
            .unwrap(),
        result
    );
    assert_eq!(forward, reverse);
    assert_eq!(
        forward.values().map(|stock| stock.quantity.0).sum::<u64>(),
        1000
    );
    assert_eq!(forward[&InventoryId(2)].quantity, Quantity(1000));
}
#[test]
fn opening_spend_clips_new_stock_but_current_occupied_room_is_never_forgotten() {
    let mut rows = stocks(vec![
        stock(1, None, 1000, 1000),
        stock(2, Some(1), 1000, 1000),
        stock(3, None, 0, 1000),
    ]);
    let opening = BTreeMap::from([(InventoryId(2), 0)]);
    let requests = vec![request(1, 1, 2, 1000), request(1, 2, 3, 1000)];
    let result = plan(&rows, Some(&opening), requests, None)
        .commit(&mut rows)
        .unwrap();
    assert!(result.iter().all(|(_, yes)| !yes));
    assert_eq!(rows[&InventoryId(2)].quantity, Quantity(1000));
}
#[test]
fn source_cursor_fairness_is_stable_across_request_order_and_updates_actual_stock() {
    let mut rows = stocks(vec![
        stock(1, None, 1000, 1000),
        stock(2, Some(1), 0, 2000),
        stock(3, Some(2), 0, 2000),
    ]);
    rows.get_mut(&InventoryId(1)).unwrap().last_granted = Some(PersonId(1));
    let grants = plan(
        &rows,
        None,
        vec![request(1, 1, 2, 1000), request(2, 1, 3, 1000)],
        None,
    )
    .commit(&mut rows)
    .unwrap();
    assert_eq!(grants[0], (request(2, 1, 3, 1000).intent, true));
    assert!(!grants[1].1);
    assert_eq!(rows[&InventoryId(1)].last_granted, Some(PersonId(2)));
}
#[test]
fn same_holder_consolidation_at_full_capacity_never_reserves_incoming_bearer_room() {
    let mut rows = stocks(vec![
        stock(1, Some(1), 1000, 1000),
        stock(2, Some(1), 0, 1000),
    ]);
    let refs = rows.iter().map(|(id, stock)| (*id, stock)).collect();
    let room = BTreeMap::from([(PersonId(1), 0)]);
    let prepared = prepare_transfers(
        &refs,
        None,
        vec![request(1, 1, 2, 1000)],
        Some(&room),
        |_| panic!("same holder has no incoming mass"),
    )
    .unwrap();
    assert!(prepared.commit(&mut rows).unwrap()[0].1);
    assert_eq!(rows[&InventoryId(2)].quantity, Quantity(1000));
}
#[test]
fn incoming_compartments_share_room_and_outgoing_does_not_release_it_early() {
    let mut rows = stocks(vec![
        stock(1, Some(1), 2000, 3000),
        stock(2, None, 0, 3000),
        stock(10, None, 2000, 3000),
        stock(11, Some(1), 0, 3000),
        stock(12, Some(1), 0, 3000),
    ]);
    let room = BTreeMap::from([(PersonId(1), 1000)]);
    let result = plan(
        &rows,
        None,
        vec![
            request(1, 1, 2, 2000),
            request(1, 10, 11, 1000),
            request(1, 10, 12, 1000),
        ],
        Some(&room),
    )
    .commit(&mut rows)
    .unwrap();
    assert_eq!(
        result.iter().map(|(_, yes)| *yes).collect::<Vec<_>>(),
        vec![true, true, false]
    );
    assert_eq!(rows[&InventoryId(12)].quantity, Quantity(0));
    assert_eq!(
        rows.values().map(|stock| stock.quantity.0).sum::<u64>(),
        4000
    );
}
#[test]
fn distinct_carried_people_share_only_the_explicit_bearer_reservation_snapshot() {
    let mut rows = stocks(vec![
        stock(1, None, 2000, 2000),
        stock(2, Some(1), 0, 2000),
        stock(3, Some(2), 0, 2000),
    ]);
    let room = BTreeMap::from([(PersonId(1), 2000), (PersonId(2), 2000)]);
    let refs = rows.iter().map(|(id, stock)| (*id, stock)).collect();
    let mut reserved = 0;
    let prepared = prepare_transfers(
        &refs,
        None,
        vec![request(1, 1, 2, 1000), request(2, 1, 3, 1000)],
        Some(&room),
        |request| {
            if reserved + request.quantity.0 > 1500 {
                return false;
            }
            reserved += request.quantity.0;
            true
        },
    )
    .unwrap();
    assert_eq!(reserved, 1000);
    assert_eq!(
        prepared
            .commit(&mut rows)
            .unwrap()
            .iter()
            .filter(|(_, yes)| *yes)
            .count(),
        1
    );
}
#[test]
fn stale_quantity_access_location_or_cursor_rejects_all_effects_before_any_debit() {
    for mutation in 0..4 {
        let mut rows = stocks(vec![
            stock(1, None, 2000, 2000),
            stock(2, Some(1), 0, 2000),
            stock(3, Some(2), 0, 2000),
        ]);
        let prepared = plan(
            &rows,
            None,
            vec![request(1, 1, 2, 1000), request(2, 1, 3, 1000)],
            None,
        );
        let slot = rows.get_mut(&InventoryId(3)).unwrap();
        match mutation {
            0 => slot.quantity = Quantity(1),
            1 => slot.access.clear(),
            2 => slot.location = InventoryLocation::Area(AreaId(2)),
            _ => slot.last_granted = Some(PersonId(2)),
        }
        let before = rows.clone();
        assert_eq!(prepared.commit(&mut rows), Err(Error::Unavailable));
        assert_eq!(rows, before);
    }
}
#[test]
fn closing_load_requires_elapsed_time_and_fresh_living_contact_permission_and_records() {
    let job = LoadJob {
        actor: PersonId(1),
        source: InventoryId(1),
        destination: InventoryId(2),
        quantity: Quantity(1000),
        completes: SimTime(100),
    };
    assert!(job.closing_request(SimTime(99), facts()).is_err());
    for closed in [
        ClosingFacts {
            alive: false,
            ..facts()
        },
        ClosingFacts {
            health: 0,
            ..facts()
        },
        ClosingFacts {
            source_contact: false,
            ..facts()
        },
        ClosingFacts {
            receiver_contact: false,
            ..facts()
        },
        ClosingFacts {
            source_access: false,
            ..facts()
        },
        ClosingFacts {
            receiver_access: false,
            ..facts()
        },
        ClosingFacts {
            record_permission: false,
            ..facts()
        },
    ] {
        let mut rows = stocks(vec![stock(1, None, 1000, 1000), stock(2, Some(1), 0, 1000)]);
        let before = rows.clone();
        assert!(
            !plan(
                &rows,
                None,
                vec![job.closing_request(SimTime(100), closed).unwrap()],
                None
            )
            .commit(&mut rows)
            .unwrap()[0]
                .1
        );
        assert_eq!(rows, before);
    }
}
#[test]
fn checked_extreme_quantities_conserve_without_wrapping() {
    let mut rows = stocks(vec![
        stock(1, None, u64::MAX, u64::MAX),
        stock(2, Some(1), 0, u64::MAX),
    ]);
    assert!(
        plan(&rows, None, vec![request(1, 1, 2, u64::MAX)], None)
            .commit(&mut rows)
            .unwrap()[0]
            .1
    );
    assert_eq!(rows[&InventoryId(1)].quantity, Quantity(0));
    assert_eq!(rows[&InventoryId(2)].quantity, Quantity(u64::MAX));
}

#[test]
fn malformed_self_zero_missing_and_wrong_good_requests_are_inert() {
    let mut rows = stocks(vec![stock(1, None, 1000, 1000), stock(2, Some(1), 0, 1000)]);
    rows.get_mut(&InventoryId(2)).unwrap().good = Good::Water;
    let before = rows.clone();
    let decisions = plan(
        &rows,
        None,
        vec![
            request(1, 1, 1, 1000),
            request(1, 1, 2, 0),
            request(1, 1, 2, 1000),
            request(1, 1, 3, 1000),
        ],
        None,
    )
    .commit(&mut rows)
    .unwrap();
    assert!(decisions.iter().all(|(_, granted)| !granted));
    assert_eq!(rows, before);
}

#[test]
fn guarded_ordered_effects_expose_each_real_post_movement_balance_before_the_next_receipt() {
    let mut rows = stocks(vec![
        stock(1, None, 2000, 3000),
        stock(2, Some(1), 0, 3000),
        stock(3, Some(2), 0, 3000),
    ]);
    let mut phase = plan(
        &rows,
        None,
        vec![request(1, 1, 2, 1000), request(2, 1, 3, 1000)],
        None,
    )
    .begin(&rows)
    .unwrap();
    let mut publications = Vec::new();
    while let Some((intent, granted)) = phase.apply_next(&mut rows) {
        assert!(granted);
        publications.push((
            intent.actor,
            rows[&InventoryId(1)].quantity,
            rows[&InventoryId(2)].quantity,
            rows[&InventoryId(3)].quantity,
        ));
    }
    assert_eq!(
        publications,
        vec![
            (PersonId(1), Quantity(1000), Quantity(1000), Quantity(0)),
            (PersonId(2), Quantity(0), Quantity(1000), Quantity(1000))
        ]
    );
    assert!(phase.apply_next(&mut rows).is_none());
}
