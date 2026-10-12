pub(crate) fn store() -> game_ecs::Store {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_ecs::Store::with_schema(schema)
}
pub(crate) fn snapshot(
    runtime: &Runtime,
    store: &game_ecs::Store,
) -> (Runtime, game_ecs::Snapshot) {
    (runtime.clone(), store.snapshot().unwrap())
}
fn group_store(rows: BTreeMap<PersonId, CarryGroup>) -> game_ecs::Store {
    let mut store = store();
    for (id, row) in rows {
        insert_group(&mut store, id, row).unwrap();
    }
    store
}
use super::*;

fn body(area: u64, mass: u64, capacity: u64) -> Body {
    Body {
        position: Position::Area(AreaId(area)),
        mass: Quantity(mass),
        cargo: Quantity(0),
        capacity: Quantity(capacity),
        can_hold: true,
        can_walk: true,
        can_bear: true,
    }
}
fn people() -> BTreeMap<PersonId, Body> {
    BTreeMap::from([
        (PersonId(1), body(1, 60_000, 40_000)),
        (PersonId(2), body(1, 60_000, 40_000)),
        (PersonId(3), body(1, 60_000, 20_000)),
    ])
}
fn group() -> CarryGroup {
    CarryGroup {
        passenger: PersonId(3),
        anchor: PersonId(1),
        bearers: BTreeSet::from([PersonId(1), PersonId(2)]),
        authority: Authority::Consented,
        accepted_at: SimTime(30_000),
    }
}
#[test]
fn stopping_and_resuming_freezes_exact_position_instead_of_an_endpoint() {
    let leg = Leg {
        from: AreaId(1),
        to: AreaId(2),
        departed: SimTime(100),
        road_start: None,
        arrives: SimTime(100_100),
    };
    let Position::Road(position) = leg.position(SimTime(33_437)).unwrap() else {
        panic!()
    };
    assert_eq!(position.progress, SimDuration(33_337));
    let resumed = resume(position, SimTime(900_000)).unwrap();
    assert_eq!(
        resumed.position(SimTime(900_000)).unwrap(),
        Position::Road(position)
    );
    assert_eq!(resumed.arrives, SimTime(966_663));
    assert_eq!(
        resumed.position(resumed.arrives).unwrap(),
        Position::Area(AreaId(2))
    );
    assert_eq!(
        serde_json::from_str::<RoadPosition>(&serde_json::to_string(&position).unwrap()).unwrap(),
        position
    );
}
#[test]
fn willing_group_shares_a_heavy_person_and_their_actual_cargo() {
    let mut bodies = people();
    bodies.get_mut(&PersonId(1)).unwrap().cargo = Quantity(10_000);
    bodies.get_mut(&PersonId(3)).unwrap().cargo = Quantity(10_000);
    let store = group_store(BTreeMap::from([(PersonId(3), group())]));
    let plan = capacity_plan(&store, &bodies).unwrap();
    assert!(plan.overloaded.is_empty());
    assert_eq!(plan.shares[&(PersonId(3), PersonId(1))], Quantity(30_000));
    assert_eq!(plan.shares[&(PersonId(3), PersonId(2))], Quantity(40_000));
    bodies.get_mut(&PersonId(2)).unwrap().cargo = Quantity(1);
    assert_eq!(
        capacity_plan(&store, &bodies).unwrap().overloaded,
        BTreeSet::from([PersonId(3)])
    );
}
#[test]
fn an_able_passenger_requires_their_own_consent_and_real_personal_offers() {
    let bodies = people();
    let mut world = Runtime::new(Rules::default()).unwrap();
    let mut store = crate::tests::store();
    let bearers = group().bearers;
    let before = crate::tests::snapshot(&world, &store);
    assert_eq!(
        world.begin_lift(
            &mut store,
            PersonId(3),
            bearers.clone(),
            Permission::default(),
            SimTime(0),
            &bodies
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(crate::tests::snapshot(&world, &store), before);
    world
        .offer(&mut store, PersonId(1), PersonId(3), SimTime(0), &bodies)
        .unwrap();
    world
        .offer(&mut store, PersonId(2), PersonId(3), SimTime(0), &bodies)
        .unwrap();
    let before = crate::tests::snapshot(&world, &store);
    assert_eq!(
        world.begin_lift(
            &mut store,
            PersonId(3),
            BTreeSet::from([PersonId(1)]),
            Permission {
                passenger_accepted: true,
                ..Permission::default()
            },
            SimTime(0),
            &bodies
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(crate::tests::snapshot(&world, &store), before);
    world
        .begin_lift(
            &mut store,
            PersonId(3),
            bearers,
            Permission {
                passenger_accepted: true,
                ..Permission::default()
            },
            SimTime(0),
            &bodies,
        )
        .unwrap();
    assert!(groups(&store).next().is_none());
    assert!(world.busy(&store, PersonId(2)));
    world
        .complete_lift(&mut store, PersonId(3), SimTime(30_000), &bodies)
        .unwrap();
    assert_eq!(
        crate::group(&store, PersonId(3)).unwrap().authority,
        Authority::Consented
    );
    validate_runtime(&world, &store, &bodies, SimTime(30_000)).unwrap();
}
#[test]
fn lifting_revalidates_capacity_and_position_and_cancellation_releases_every_bearer() {
    let mut bodies = people();
    let mut world = Runtime::new(Rules::default()).unwrap();
    let mut store = crate::tests::store();
    for bearer in [PersonId(1), PersonId(2)] {
        world
            .offer(&mut store, bearer, PersonId(3), SimTime(0), &bodies)
            .unwrap();
    }
    world
        .begin_lift(
            &mut store,
            PersonId(3),
            group().bearers,
            Permission {
                unable_to_consent: true,
                ..Permission::default()
            },
            SimTime(0),
            &bodies,
        )
        .unwrap();
    bodies.get_mut(&PersonId(2)).unwrap().position = Position::Area(AreaId(2));
    let before = crate::tests::snapshot(&world, &store);
    assert_eq!(
        world.complete_lift(&mut store, PersonId(3), SimTime(30_000), &bodies),
        Err(Error::Unavailable)
    );
    assert_eq!(crate::tests::snapshot(&world, &store), before);
    assert_eq!(
        world.cancel_lifts(&mut store, PersonId(2)).unwrap().len(),
        1
    );
    assert!(!world.busy(&store, PersonId(1)));
    assert!(groups(&store).next().is_none());
}
#[test]
fn actual_newborn_attachment_can_overload_and_does_not_copy_a_journey() {
    let mut bodies = people();
    let position = Position::Road(RoadPosition {
        from: AreaId(1),
        to: AreaId(2),
        progress: SimDuration(10_000),
        duration: SimDuration(60_000),
    });
    bodies.get_mut(&PersonId(1)).unwrap().position = position;
    bodies.get_mut(&PersonId(1)).unwrap().cargo = Quantity(39_000);
    bodies.insert(
        PersonId(4),
        Body {
            position,
            ..body(1, 3_000, 1_000)
        },
    );
    let mut world = Runtime::new(Rules::default()).unwrap();
    let mut store = crate::tests::store();
    world
        .attach_newborn(
            &mut store,
            PersonId(4),
            PersonId(1),
            SimTime(10_000),
            &bodies,
        )
        .unwrap();
    assert_eq!(
        crate::group(&store, PersonId(4)).unwrap().authority,
        Authority::Newborn
    );
    assert_eq!(
        capacity_plan(&store, &bodies).unwrap().overloaded,
        BTreeSet::from([PersonId(4)])
    );
    assert_eq!(
        world.component(&store, PersonId(4)),
        BTreeSet::from([PersonId(1), PersonId(4)])
    );
    bodies.get_mut(&PersonId(1)).unwrap().can_walk = false;
    assert!(
        capacity_plan(&store, &bodies)
            .unwrap()
            .unable
            .contains(&PersonId(1))
    );
    validate_runtime(&world, &store, &bodies, SimTime(10_000)).unwrap();
}
#[test]
fn capacity_apportionment_and_growth_are_integer_and_order_independent() {
    let a = BTreeMap::from([
        (PersonId(2), Quantity(9)),
        (PersonId(1), Quantity(7)),
        (PersonId(3), Quantity(3)),
    ]);
    let b = BTreeMap::from([
        (PersonId(3), Quantity(3)),
        (PersonId(1), Quantity(7)),
        (PersonId(2), Quantity(9)),
    ]);
    assert_eq!(
        share_load(Quantity(17), &a).unwrap(),
        share_load(Quantity(17), &b).unwrap()
    );
    assert_eq!(
        share_load(Quantity(17), &a)
            .unwrap()
            .values()
            .map(|q| q.0)
            .sum::<u64>(),
        17
    );
    let rules = Rules::default();
    assert_eq!(
        body_mass(&rules, SimDuration(0), SimDuration(180)),
        Quantity(3_000)
    );
    assert_eq!(
        body_mass(&rules, SimDuration(90), SimDuration(180)),
        Quantity(31_500)
    );
    assert_eq!(
        body_mass(&rules, SimDuration(9_000), SimDuration(180)),
        Quantity(60_000)
    );
}
#[test]
fn reaching_a_roadside_body_requires_actual_time_from_either_endpoint() {
    let target = RoadPosition {
        from: AreaId(1),
        to: AreaId(2),
        progress: SimDuration(31_337),
        duration: SimDuration(120_000),
    };
    let (leg, a) = approach(target, AreaId(1), SimDuration(120_000), SimTime(800_000)).unwrap();
    assert_eq!(a.completes, SimTime(831_337));
    assert_eq!(leg.position(a.completes).unwrap(), Position::Road(target));
    let (back, b) = approach(target, AreaId(2), SimDuration(120_000), SimTime(800_000)).unwrap();
    assert_eq!(b.completes, SimTime(888_663));
    assert_eq!(back.from, target.to);
    assert_eq!(back.to, target.from);
    assert_eq!(b.target, target);
    assert!(approach(target, AreaId(3), SimDuration(120_000), SimTime(800_000)).is_err());
}
#[test]
fn transferred_personal_cargo_and_stopped_overloads_have_real_body_weight() {
    let mut bodies = people();
    let store = group_store(BTreeMap::from([(PersonId(3), group())]));
    assert!(transfer_capacity(&store, &bodies, PersonId(3), Quantity(20_000)).unwrap());
    assert!(!transfer_capacity(&store, &bodies, PersonId(1), Quantity(30_000)).unwrap());
    bodies.get_mut(&PersonId(1)).unwrap().cargo = Quantity(40_000);
    bodies.get_mut(&PersonId(2)).unwrap().cargo = Quantity(40_000);
    let plan = capacity_plan(&store, &bodies).unwrap();
    assert_eq!(plan.shares.values().map(|q| q.0).sum::<u64>(), 60_000);
    assert!(transfer_capacity(&store, &bodies, PersonId(1), Quantity(0)).unwrap());
    assert!(!transfer_capacity(&store, &bodies, PersonId(1), Quantity(1)).unwrap());
}
#[test]
fn resolved_body_catalog_follows_shared_anchors_and_rejects_cycles() {
    let mut locations = BTreeMap::from([
        (PersonId(1), Location::AtArea(AreaId(2))),
        (
            PersonId(2),
            Location::Carried {
                carrier: PersonId(1),
            },
        ),
        (
            PersonId(3),
            Location::Carried {
                carrier: PersonId(1),
            },
        ),
    ]);
    let resolved = resolve_locations(&locations).unwrap();
    assert!(resolved.values().all(|l| *l == Location::AtArea(AreaId(2))));
    locations.insert(
        PersonId(1),
        Location::Carried {
            carrier: PersonId(2),
        },
    );
    assert!(resolve_locations(&locations).is_err());
}
#[test]
fn early_asymmetric_retreat_uses_real_clock_and_preserves_start_geometry() {
    let start = RoadPosition {
        from: AreaId(1),
        to: AreaId(2),
        progress: SimDuration(1),
        duration: SimDuration(120_000),
    };
    let leg = retreat(start, AreaId(1), SimDuration(180_000), SimTime(1)).unwrap();
    assert_eq!(leg.departed, SimTime(1));
    assert_eq!(leg.arrives, SimTime(3));
    assert_eq!(leg.position(SimTime(1)).unwrap(), Position::Road(start));
    assert_eq!(leg.position(SimTime(3)).unwrap(), Position::Area(AreaId(1)));
    let forward = resume_on_link(start, SimDuration(240_000), SimTime(50)).unwrap();
    assert_eq!(forward.departed, SimTime(50));
    assert_eq!(forward.arrives, SimTime(240_048));
    assert_eq!(
        forward.position(SimTime(50)).unwrap(),
        Position::Road(start)
    );
    let mirrored = Position::Road(RoadPosition {
        from: AreaId(2),
        to: AreaId(1),
        progress: SimDuration(119_999),
        duration: SimDuration(120_000),
    });
    assert_ne!(Position::Road(start), mirrored);
    assert_eq!(Position::Road(start).key(), mirrored.key());
}
#[test]
fn actual_confinement_takes_priority_over_old_yield_or_incapacity() {
    assert_eq!(
        Permission {
            captive: true,
            yielded: true,
            unable_to_consent: true,
            ..Permission::default()
        }
        .authority()
        .unwrap(),
        Authority::Captive
    );
    assert_eq!(
        Permission {
            passenger_accepted: true,
            captive: true,
            ..Permission::default()
        }
        .authority()
        .unwrap(),
        Authority::Consented
    );
}
#[test]
fn personal_offer_and_support_choices_have_no_family_or_office_inputs() {
    let willing = OfferChoice {
        need_help: true,
        urgent_own_need: false,
        busy: false,
        trust: 500,
        affection: 100,
        independence_goal: false,
        sufficient_support: true,
    };
    assert_eq!(judge_offer(willing).0, ResponseKind::Accepted);
    assert_eq!(
        judge_offer(OfferChoice {
            independence_goal: true,
            ..willing
        })
        .0,
        ResponseKind::Refused
    );
    assert_eq!(
        judge_offer(OfferChoice {
            sufficient_support: false,
            ..willing
        })
        .0,
        ResponseKind::WaitingForSupport
    );
    let support = SupportChoice {
        empathy: 900,
        trust: 200,
        affection: 0,
        own_need: false,
        busy: false,
        has_capacity: true,
    };
    assert!(willing_support(support));
    assert!(!willing_support(SupportChoice {
        own_need: true,
        ..support
    }));
    assert!(!willing_support(SupportChoice {
        busy: true,
        ..support
    }));
}
