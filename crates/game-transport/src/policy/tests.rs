use super::*;
fn fixture() -> (Runtime, game_ecs::Store, BTreeMap<PersonId, Body>) {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    let store = game_ecs::Store::with_schema(schema);
    let bodies = (1..=4)
        .map(|id| {
            (
                PersonId(id),
                Body {
                    position: Position::Area(AreaId(10)),
                    mass: Quantity(16_000),
                    cargo: Quantity(0),
                    capacity: Quantity(10_000),
                    can_hold: true,
                    can_walk: true,
                    can_bear: true,
                },
            )
        })
        .collect();
    (Runtime::new(Rules::default()).unwrap(), store, bodies)
}
fn group_row() -> CarryGroup {
    CarryGroup {
        passenger: PersonId(3),
        bearers: BTreeSet::from([PersonId(1), PersonId(2)]),
        anchor: PersonId(1),
        authority: Authority::Consented,
        accepted_at: SimTime(0),
    }
}
#[test]
fn departure_admission_and_load_use_the_connected_actual_group() {
    let (runtime, mut store, mut bodies) = fixture();
    assert!(runtime.can_depart(&store, &bodies, PersonId(1)));
    assert!(!runtime.can_depart(&store, &bodies, PersonId(999)));
    insert_group(&mut store, PersonId(3), group_row()).unwrap();
    assert_eq!(
        runtime.body_load(&store, &bodies, PersonId(1)),
        Quantity(8_000)
    );
    assert_eq!(
        runtime.body_load(&store, &bodies, PersonId(2)),
        Quantity(8_000)
    );
    assert_eq!(runtime.body_load(&store, &bodies, PersonId(4)), Quantity(0));
    assert!(runtime.can_depart(&store, &bodies, PersonId(1)));
    assert!(!runtime.can_depart(&store, &bodies, PersonId(3)));
    bodies.get_mut(&PersonId(2)).unwrap().can_bear = false;
    assert!(!runtime.can_depart(&store, &bodies, PersonId(1)));
    assert!(runtime.can_depart(&store, &bodies, PersonId(4)));
    bodies.get_mut(&PersonId(4)).unwrap().cargo = Quantity(10_001);
    assert!(!runtime.can_depart(&store, &bodies, PersonId(4)));
}
#[test]
fn joining_selects_unexpired_offers_and_surviving_bearers_without_duplicates() {
    let (runtime, mut store, mut bodies) = fixture();
    insert_group(&mut store, PersonId(3), group_row()).unwrap();
    bodies.get_mut(&PersonId(2)).unwrap().can_hold = false;
    let offers = [
        (PersonId(1), SimTime(60_000)),
        (PersonId(4), SimTime(30_000)),
    ]
    .into_iter()
    .map(|(bearer, expires)| {
        (
            bearer,
            Offer {
                bearer,
                passenger: PersonId(3),
                offered_at: SimTime(0),
                expires,
            },
        )
    })
    .collect();
    insert_offer_inbox(&mut store, PersonId(3), OfferInbox { by_bearer: offers }).unwrap();
    assert_eq!(
        runtime.join_bearers(&store, &bodies, PersonId(3), SimTime(30_000)),
        BTreeSet::from([PersonId(1)])
    );
    assert_eq!(
        runtime.join_bearers(&store, &bodies, PersonId(3), SimTime(29_999)),
        BTreeSet::from([PersonId(1), PersonId(4)])
    );
    assert!(
        runtime
            .join_bearers(&store, &bodies, PersonId(999), SimTime(0))
            .is_empty()
    );
}
#[test]
fn lift_participant_order_preserves_an_escaping_passengers_action() {
    let lift = Lift {
        passenger: PersonId(3),
        bearers: BTreeSet::from([PersonId(2), PersonId(1)]),
        authority: Authority::Consented,
        started: SimTime(0),
        completes: SimTime(6_000),
    };
    assert_eq!(
        lift.participants(false),
        vec![PersonId(1), PersonId(2), PersonId(3)]
    );
    assert_eq!(lift.participants(true), vec![PersonId(1), PersonId(2)]);
}
