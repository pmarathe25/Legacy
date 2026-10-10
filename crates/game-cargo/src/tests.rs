use super::*;
const ACTOR: PersonId = PersonId(1);
fn actor() -> ActorFacts {
    ActorFacts {
        actor: ACTOR,
        alive: true,
        health: 100,
        independent: true,
        confined: false,
        in_combat: false,
        receiving_care: false,
        stationary: true,
        fatigue: 0,
        fatigue_limit: 100,
    }
}
fn stock(id: u64, place: InventoryLocation, quantity: u64) -> Inventory {
    Inventory {
        id: InventoryId(id),
        location: place,
        quantity: Quantity(quantity),
        capacity: Quantity(10000),
        access: BTreeSet::from([ACTOR]),
        last_granted: None,
        good: Good::Timber,
        spoils_at: None,
    }
}
fn supply() -> SourceFacts {
    SourceFacts {
        contact: true,
        accessible: true,
        unreserved: Quantity(2000),
        record_permission: true,
    }
}
fn receiver() -> ReceiverFacts {
    ReceiverFacts {
        contact: true,
        accessible: true,
        accepted: false,
        body_room: Quantity(5000),
        transport_allows: true,
    }
}
fn proposed() -> NewCompartment {
    NewCompartment {
        id: InventoryId(30),
        holder: ACTOR,
        alive: true,
        capacity: Quantity(10000),
        receiver: receiver(),
    }
}
fn load(a: ActorFacts, s: SourceFacts, r: ReceiverFacts) -> Result<LoadJob, Error> {
    prepare_load(
        a,
        &stock(10, InventoryLocation::Area(AreaId(2)), 2000),
        &stock(20, InventoryLocation::Carried(ACTOR), 0),
        Quantity(1000),
        s,
        r,
        SimTime(500),
        SimDuration(100),
    )
}
#[test]
fn admission_requires_each_actual_actor_capability() {
    let invalid = [
        ActorFacts {
            alive: false,
            ..actor()
        },
        ActorFacts {
            health: 0,
            ..actor()
        },
        ActorFacts {
            independent: false,
            ..actor()
        },
        ActorFacts {
            confined: true,
            ..actor()
        },
        ActorFacts {
            in_combat: true,
            ..actor()
        },
        ActorFacts {
            receiving_care: true,
            ..actor()
        },
        ActorFacts {
            stationary: false,
            ..actor()
        },
        ActorFacts {
            fatigue: 100,
            ..actor()
        },
    ];
    for a in invalid {
        assert_eq!(load(a, supply(), receiver()), Err(Error::Unavailable));
    }
    assert_eq!(
        load(actor(), supply(), receiver()).unwrap().completes,
        SimTime(600)
    );
}
#[test]
fn contact_permission_reservation_records_and_distinct_receiver_limits_are_required() {
    for s in [
        SourceFacts {
            contact: false,
            ..supply()
        },
        SourceFacts {
            accessible: false,
            ..supply()
        },
        SourceFacts {
            unreserved: Quantity(999),
            ..supply()
        },
        SourceFacts {
            record_permission: false,
            ..supply()
        },
    ] {
        assert_eq!(load(actor(), s, receiver()), Err(Error::Unavailable));
    }
    for r in [
        ReceiverFacts {
            contact: false,
            ..receiver()
        },
        ReceiverFacts {
            accessible: false,
            ..receiver()
        },
        ReceiverFacts {
            body_room: Quantity(999),
            ..receiver()
        },
        ReceiverFacts {
            transport_allows: false,
            ..receiver()
        },
    ] {
        assert_eq!(load(actor(), supply(), r), Err(Error::Unavailable));
    }
    assert!(
        load(
            actor(),
            supply(),
            ReceiverFacts {
                accessible: false,
                accepted: true,
                ..receiver()
            }
        )
        .is_ok()
    );
}
#[test]
fn zero_self_wrong_good_actual_depletion_and_room_never_admit() {
    let from = stock(10, InventoryLocation::Area(AreaId(2)), 2000);
    let mut to = stock(20, InventoryLocation::Carried(ACTOR), 0);
    let attempt = |src: &Inventory, dst: &Inventory, qty| {
        prepare_load(
            actor(),
            src,
            dst,
            qty,
            supply(),
            receiver(),
            SimTime(0),
            SimDuration(1),
        )
    };
    assert_eq!(attempt(&from, &to, Quantity(0)), Err(Error::Unavailable));
    assert_eq!(attempt(&to, &to, Quantity(1000)), Err(Error::Unavailable));
    to.good = Good::Water;
    assert_eq!(attempt(&from, &to, Quantity(1000)), Err(Error::Unavailable));
    to.good = Good::Timber;
    to.quantity = Quantity(9500);
    assert_eq!(attempt(&from, &to, Quantity(1000)), Err(Error::Unavailable));
    to.quantity = Quantity(11000);
    assert_eq!(attempt(&from, &to, Quantity(1000)), Err(Error::Unavailable));
    let depleted = Inventory {
        quantity: Quantity(999),
        ..from.clone()
    };
    to.quantity = Quantity(0);
    assert_eq!(
        attempt(&depleted, &to, Quantity(1000)),
        Err(Error::Unavailable)
    );
    to.location = InventoryLocation::Area(AreaId(2));
    assert_eq!(attempt(&from, &to, Quantity(1000)), Err(Error::Unavailable));
}
#[test]
fn reorganizing_the_same_carried_load_does_not_increase_body_or_transport_burden() {
    let from = stock(10, InventoryLocation::Carried(ACTOR), 2000);
    let to = stock(20, InventoryLocation::Carried(ACTOR), 0);
    assert!(
        prepare_load(
            actor(),
            &from,
            &to,
            Quantity(1000),
            supply(),
            ReceiverFacts {
                body_room: Quantity(0),
                transport_allows: false,
                ..receiver()
            },
            SimTime(0),
            SimDuration(1)
        )
        .is_ok()
    );
}
#[test]
fn selection_is_stable_and_only_allocates_after_complete_native_admission() {
    let from = stock(10, InventoryLocation::Area(AreaId(2)), 2000);
    let first = stock(20, InventoryLocation::Carried(ACTOR), 0);
    let second = stock(21, InventoryLocation::Carried(ACTOR), 0);
    let candidates = [
        Compartment {
            stock: &second,
            receiver: receiver(),
        },
        Compartment {
            stock: &first,
            receiver: receiver(),
        },
    ];
    let plan = prepare_carry(
        actor(),
        &from,
        Quantity(1000),
        supply(),
        &candidates,
        proposed(),
        SimTime(0),
        SimDuration(1),
    )
    .unwrap();
    assert_eq!(plan.selection, Selection::Existing(first.id));
    assert_eq!(plan.job.destination, first.id);
    let denied = SourceFacts {
        record_permission: false,
        ..supply()
    };
    assert_eq!(
        prepare_carry(
            actor(),
            &from,
            Quantity(1000),
            denied,
            &[],
            proposed(),
            SimTime(0),
            SimDuration(1)
        ),
        Err(Error::Unavailable)
    );
    let plan = prepare_carry(
        actor(),
        &from,
        Quantity(1000),
        supply(),
        &[],
        proposed(),
        SimTime(0),
        SimDuration(1),
    )
    .unwrap();
    let Selection::Proposed(slot) = plan.selection else {
        panic!("new slot");
    };
    assert_eq!(slot.id, proposed().id);
    assert_eq!(slot.quantity, Quantity(0));
    assert_eq!(slot.good, from.good);
    assert_eq!(slot.access, BTreeSet::from([ACTOR]));
    assert_eq!(from.quantity, Quantity(2000));
    let owned = Inventory {
        location: InventoryLocation::Carried(ACTOR),
        ..from
    };
    assert_eq!(
        prepare_carry(
            actor(),
            &owned,
            Quantity(1000),
            supply(),
            &[],
            proposed(),
            SimTime(0),
            SimDuration(1)
        ),
        Err(Error::Unavailable)
    );
}
#[test]
fn handoff_requires_actual_held_source_and_named_living_contact_acceptance() {
    let from = stock(10, InventoryLocation::Carried(ACTOR), 2000);
    let allocation = NewCompartment {
        holder: PersonId(2),
        receiver: ReceiverFacts {
            accessible: false,
            accepted: true,
            ..receiver()
        },
        ..proposed()
    };
    let plan = prepare_handoff(
        actor(),
        &from,
        Quantity(1000),
        supply(),
        &[],
        allocation,
        SimTime(0),
        SimDuration(1),
    )
    .unwrap();
    let Selection::Proposed(slot) = plan.selection else {
        panic!("new recipient slot");
    };
    assert_eq!(slot.location, InventoryLocation::Carried(PersonId(2)));
    assert_eq!(slot.access, BTreeSet::from([PersonId(2)]));
    for denied in [
        NewCompartment {
            alive: false,
            ..allocation
        },
        NewCompartment {
            receiver: ReceiverFacts {
                accepted: false,
                ..allocation.receiver
            },
            ..allocation
        },
        NewCompartment {
            receiver: ReceiverFacts {
                contact: false,
                ..allocation.receiver
            },
            ..allocation
        },
    ] {
        assert_eq!(
            prepare_handoff(
                actor(),
                &from,
                Quantity(1000),
                supply(),
                &[],
                denied,
                SimTime(0),
                SimDuration(1)
            ),
            Err(Error::Unavailable)
        );
    }
    let distant = Inventory {
        location: InventoryLocation::Area(AreaId(2)),
        ..from
    };
    assert_eq!(
        prepare_handoff(
            actor(),
            &distant,
            Quantity(1000),
            supply(),
            &[],
            allocation,
            SimTime(0),
            SimDuration(1)
        ),
        Err(Error::Unavailable)
    );
}
#[test]
fn deadlines_are_checked_before_returning_a_proposed_slot_and_jobs_roundtrip() {
    let from = stock(10, InventoryLocation::Area(AreaId(2)), 2000);
    let to = stock(20, InventoryLocation::Carried(ACTOR), 0);
    assert_eq!(
        prepare_carry(
            actor(),
            &from,
            Quantity(1000),
            supply(),
            &[],
            proposed(),
            SimTime(u64::MAX),
            SimDuration(1)
        ),
        Err(Error::Overflow)
    );
    let job = load(actor(), supply(), receiver()).unwrap();
    let encoded = serde_json::to_string(&job).unwrap();
    assert_eq!(job, serde_json::from_str(&encoded).unwrap());
    assert!(job.validate(SimTime(550), &from, &to).is_ok());
    assert_eq!(
        job.validate(job.completes, &from, &to),
        Err(Error::Unavailable)
    );
    assert_eq!(
        LoadJob {
            destination: from.id,
            ..job
        }
        .validate(SimTime(550), &from, &from),
        Err(Error::Unavailable)
    );
}

#[test]
fn quantity_ceiling_respects_stock_room_but_does_not_claim_permission() {
    let from = stock(10, InventoryLocation::Area(AreaId(2)), 250);
    let mut to = stock(20, InventoryLocation::Carried(ACTOR), 0);
    to.capacity = Quantity(100);
    let denied = ReceiverFacts {
        accessible: false,
        accepted: false,
        ..receiver()
    };
    assert_eq!(
        load_upper_bound(&from, &to, supply(), denied).unwrap(),
        Quantity(100)
    );
    assert_eq!(
        prepare_load(
            actor(),
            &from,
            &to,
            Quantity(100),
            supply(),
            denied,
            SimTime(0),
            SimDuration(1)
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(
        pickup_upper_bound(&from, supply(), Quantity(75), Quantity(1000)).unwrap(),
        Quantity(75)
    );
}

#[test]
fn quantity_ceiling_preserves_consolidation_at_full_body_load() {
    let from = stock(10, InventoryLocation::Carried(ACTOR), 600);
    let mut to = stock(20, InventoryLocation::Carried(ACTOR), 400);
    to.capacity = Quantity(1000);
    let full = ReceiverFacts {
        body_room: Quantity(0),
        transport_allows: false,
        ..receiver()
    };
    let limit = load_upper_bound(&from, &to, supply(), full).unwrap();
    assert_eq!(limit, Quantity(600));
    assert!(
        prepare_load(
            actor(),
            &from,
            &to,
            limit,
            supply(),
            full,
            SimTime(0),
            SimDuration(1)
        )
        .is_ok()
    );
}
