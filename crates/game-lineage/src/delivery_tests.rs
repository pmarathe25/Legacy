use crate::*;
use game_core::*;
use game_logistics::Destination;
use std::collections::BTreeMap;
const REQUESTER: PersonId = PersonId(1);
const GUARDIAN: PersonId = PersonId(2);
const SOURCE: InventoryId = InventoryId(3);
fn world() -> World {
    World {
        incarnations: vec![Incarnation {
            person: REQUESTER,
            bound_at: SimTime(0),
            died_at: None,
        }],
        waiting: None,
        guardians: BTreeMap::from([(
            GUARDIAN,
            Guardian {
                target: Some(REQUESTER),
                sensed_location: Some(Location::AtArea(AreaId(1))),
                sensed_at: SimTime(0),
                goal: GuardianGoal::Mentoring,
                explanation: String::new(),
                assessments: BTreeMap::new(),
                assignment: None,
                requested_delivery: None,
                provision: None,
                knows_invocation: true,
            },
        )]),
        rules: MentorshipRules::default(),
        invocation_learning: BTreeMap::new(),
    }
}
fn facts(good: Good) -> SupplyFacts {
    SupplyFacts {
        now: SimTime(10),
        requester_independent: true,
        requester_available: true,
        fresh_contact: true,
        guardian_capable: true,
        guardian_free: true,
        known_source: Some(StockKnowledge {
            inventory: SOURCE,
            good,
            quantity: Quantity(1000),
            accessible: true,
            area: AreaId(1),
            observed_at: SimTime(0),
            learned_at: SimTime(0),
            source: KnowledgeSource::Observed(GUARDIAN),
        }),
        source_route_known: true,
        immediate_care: false,
        requester_area: Some(AreaId(1)),
        source_carried: false,
        room: Quantity(2000),
        carrying_capacity: Quantity(2000),
        cursor: 10,
    }
}
fn accepted(good: Good) -> RequestedDelivery {
    let SupplyAnswer::Accept(r) = world()
        .judge_supply(
            GUARDIAN,
            REQUESTER,
            good,
            SOURCE,
            Quantity(1000),
            &facts(good),
        )
        .unwrap()
    else {
        panic!("valid request declined");
    };
    r
}
#[test]
fn the_same_rule_admits_different_goods_as_unfulfilled_source_constrained_requests() {
    for good in [
        Good::Food,
        Good::Water,
        Good::Herb,
        Good::Timber,
        Good::Record,
    ] {
        let r = accepted(good);
        assert_eq!(r.delivery.requirement.good, good);
        assert_eq!(r.delivery.requirement.source, Some(SOURCE));
        assert_eq!(r.delivery.destination, Destination::Contact(REQUESTER));
        assert_eq!(r.delivery.remaining, Quantity(1000));
        assert!(r.cargo.is_empty());
    }
}
#[test]
fn remote_unavailable_and_unrelated_requests_are_inert_while_personal_unknown_sources_can_be_refused()
 {
    let w = world();
    let mut f = facts(Good::Water);
    f.fresh_contact = false;
    assert_eq!(
        w.judge_supply(GUARDIAN, REQUESTER, Good::Water, SOURCE, Quantity(1000), &f),
        Err(Error::Unavailable)
    );
    f.fresh_contact = true;
    f.requester_independent = false;
    assert!(
        w.judge_supply(GUARDIAN, REQUESTER, Good::Water, SOURCE, Quantity(1000), &f)
            .is_err()
    );
    f.requester_independent = true;
    f.known_source = None;
    assert!(matches!(
        w.judge_supply(GUARDIAN, REQUESTER, Good::Water, SOURCE, Quantity(1000), &f)
            .unwrap(),
        SupplyAnswer::Refuse(_)
    ));
    assert!(w.guardians[&GUARDIAN].requested_delivery.is_none());
}
#[test]
fn held_goods_do_not_require_new_room_or_departure_during_observed_urgent_care() {
    let mut f = facts(Good::Herb);
    f.room = Quantity(0);
    f.source_route_known = false;
    f.immediate_care = true;
    f.requester_area = None;
    f.source_carried = true;
    let answer = world()
        .judge_supply(GUARDIAN, REQUESTER, Good::Herb, SOURCE, Quantity(1000), &f)
        .unwrap();
    let SupplyAnswer::Accept(r) = answer else {
        panic!("held handoff refused");
    };
    assert_eq!(r.cargo, BTreeMap::from([(SOURCE, 1000)]));
    f.source_carried = false;
    assert!(matches!(
        world()
            .judge_supply(GUARDIAN, REQUESTER, Good::Herb, SOURCE, Quantity(1000), &f)
            .unwrap(),
        SupplyAnswer::Refuse(_)
    ));
}
#[test]
fn conserved_origin_tags_follow_staging_and_actual_theft_loss_without_double_receipts() {
    let mut r = accepted(Good::Water);
    let pickup = SupplyReceipt::Transfer {
        actor: GUARDIAN,
        source: SOURCE,
        destination: InventoryId(4),
        good: Good::Water,
        quantity: Quantity(500),
        destination_holder: Some(GUARDIAN),
    };
    assert_eq!(
        r.receive(GUARDIAN, EventId(11), &pickup).unwrap(),
        Quantity(0)
    );
    let once = r.clone();
    r.receive(GUARDIAN, EventId(11), &pickup).unwrap();
    assert_eq!(r, once);
    r.receive(
        GUARDIAN,
        EventId(12),
        &SupplyReceipt::Transfer {
            actor: GUARDIAN,
            source: InventoryId(4),
            destination: InventoryId(5),
            good: Good::Water,
            quantity: Quantity(200),
            destination_holder: Some(GUARDIAN),
        },
    )
    .unwrap();
    let moved = r.clone();
    r.receive(
        GUARDIAN,
        EventId(12),
        &SupplyReceipt::Transfer {
            actor: GUARDIAN,
            source: InventoryId(4),
            destination: InventoryId(5),
            good: Good::Water,
            quantity: Quantity(200),
            destination_holder: Some(GUARDIAN),
        },
    )
    .unwrap();
    assert_eq!(r, moved);
    r.receive(
        GUARDIAN,
        EventId(13),
        &SupplyReceipt::Transfer {
            actor: PersonId(99),
            source: InventoryId(4),
            destination: InventoryId(6),
            good: Good::Water,
            quantity: Quantity(200),
            destination_holder: Some(PersonId(99)),
        },
    )
    .unwrap();
    assert_eq!(
        r.receive(
            GUARDIAN,
            EventId(14),
            &SupplyReceipt::Transfer {
                actor: GUARDIAN,
                source: InventoryId(5),
                destination: InventoryId(7),
                good: Good::Water,
                quantity: Quantity(200),
                destination_holder: Some(REQUESTER)
            }
        )
        .unwrap(),
        Quantity(200)
    );
    r.receive(
        GUARDIAN,
        EventId(15),
        &SupplyReceipt::Lost {
            inventory: InventoryId(4),
            quantity: Quantity(100),
        },
    )
    .unwrap();
    assert!(r.cargo.is_empty());
    assert_eq!(r.delivery.remaining, Quantity(800));
    let json = serde_json::to_string(&r).unwrap();
    assert_eq!(serde_json::from_str::<RequestedDelivery>(&json).unwrap(), r);
}
#[test]
fn wrong_source_or_good_cannot_fulfill_the_requested_handoff() {
    let mut r = accepted(Good::Timber);
    r.receive(
        GUARDIAN,
        EventId(11),
        &SupplyReceipt::Transfer {
            actor: GUARDIAN,
            source: InventoryId(99),
            destination: InventoryId(5),
            good: Good::Timber,
            quantity: Quantity(1000),
            destination_holder: Some(REQUESTER),
        },
    )
    .unwrap();
    r.receive(
        GUARDIAN,
        EventId(12),
        &SupplyReceipt::Transfer {
            actor: GUARDIAN,
            source: SOURCE,
            destination: InventoryId(5),
            good: Good::Food,
            quantity: Quantity(1000),
            destination_holder: Some(REQUESTER),
        },
    )
    .unwrap();
    assert_eq!(r.delivery.remaining, Quantity(1000));
    assert!(r.cargo.is_empty());
}
