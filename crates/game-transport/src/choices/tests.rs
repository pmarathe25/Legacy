use super::*;

#[test]
fn autonomous_offer_response_requires_an_independent_non_ruler() {
    for (actor, independent, eligible) in [
        (PersonId(1), true, false),
        (PersonId(1), false, false),
        (PersonId(2), false, false),
        (PersonId(2), true, true),
    ] {
        assert_eq!(
            OfferEligibilityFacts {
                actor,
                ruler: PersonId(1),
                independent,
            }
            .autonomous_response(),
            eligible,
        );
    }
}

#[test]
fn support_is_occupied_by_missing_activity_other_work_or_any_feature_duty() {
    for activity in [None, Some(SupportActivity::Other)] {
        assert!(
            SupportAvailabilityFacts {
                activity,
                lineage_duty: false,
                health_duty: false,
                mission_duty: false,
            }
            .occupied()
        );
    }
    for activity in [SupportActivity::Idle, SupportActivity::Rest] {
        for duties in 0u8..8 {
            assert_eq!(
                SupportAvailabilityFacts {
                    activity: Some(activity),
                    lineage_duty: duties & 1 != 0,
                    health_duty: duties & 2 != 0,
                    mission_duty: duties & 4 != 0,
                }
                .occupied(),
                duties != 0,
                "{activity:?} with duty flags {duties}",
            );
        }
    }
}

#[test]
fn shared_local_candidates_reborrow_current_contact_before_stable_witness_cap() {
    let mut schema = game_ecs::Schema::new();
    game_spatial::register_components(&mut schema).unwrap();
    let mut store = game_ecs::Store::with_schema(schema);
    for id in 1..=100 {
        game_spatial::set_location(
            &mut store,
            game_ecs::StableId(id),
            Location::AtArea(AreaId(10)),
        )
        .unwrap();
    }
    for id in 101..=200 {
        game_spatial::set_location(
            &mut store,
            game_ecs::StableId(id),
            Location::AtArea(AreaId(11)),
        )
        .unwrap();
    }
    let candidates = game_spatial::LocalRoster::build(&store)
        .unwrap()
        .into_candidates();
    struct Contact {
        excluded: BTreeSet<PersonId>,
        reads: std::cell::Cell<usize>,
    }
    impl SupportWitnessFacts for Contact {
        fn person(&self, actor: PersonId) -> bool {
            actor.0 <= 200
        }
        fn tactical_contact(&self, actor: PersonId, _: PersonId) -> bool {
            self.reads.set(self.reads.get() + 1);
            !self.excluded.contains(&actor)
        }
    }
    let mut facts = Contact {
        excluded: BTreeSet::from([PersonId(17)]),
        reads: std::cell::Cell::new(0),
    };
    let before = store.snapshot().unwrap();
    let observers =
        local_support_observers(&store, &candidates, PersonId(1), SimTime(0), &facts).unwrap();
    assert_eq!(observers.len(), 64);
    assert_eq!(observers.first(), Some(&PersonId(2)));
    assert_eq!(observers.last(), Some(&PersonId(66)));
    assert!(!observers.contains(&PersonId(17)));
    assert_eq!(facts.reads.get(), 65);
    facts.excluded.insert(PersonId(2));
    facts.reads.set(0);
    let observers =
        local_support_observers(&store, &candidates, PersonId(1), SimTime(0), &facts).unwrap();
    assert_eq!(observers.first(), Some(&PersonId(3)));
    assert_eq!(observers.last(), Some(&PersonId(67)));
    assert_eq!(facts.reads.get(), 66);
    assert_eq!(store.snapshot().unwrap(), before);
}

fn bodies() -> BTreeMap<PersonId, Body> {
    (1..=4)
        .map(|id| {
            (
                PersonId(id),
                Body {
                    position: Position::Area(AreaId(10)),
                    mass: Quantity(60_000),
                    cargo: Quantity(0),
                    capacity: Quantity(100_000),
                    can_hold: true,
                    can_walk: true,
                    can_bear: true,
                },
            )
        })
        .collect()
}
fn facts() -> OfferResponseFacts {
    OfferResponseFacts {
        autonomous: true,
        alive: true,
        combat_restricted: false,
        idle_or_resting: true,
        health: 50,
        max_health: 100,
        hunger: 0,
        hunger_harm: 100,
        fatigue: 0,
        fatigue_limit: 100,
        independence_goal: false,
    }
}
fn relation(_: PersonId) -> Option<RelationFacts> {
    Some(RelationFacts {
        trust: 200,
        affection: 0,
    })
}
fn fixture() -> (Runtime, game_ecs::Store, BTreeMap<PersonId, Body>) {
    let mut world = Runtime::new(Rules::default()).unwrap();
    let mut store = crate::tests::store();
    let bodies = bodies();
    world
        .offer(&mut store, PersonId(2), PersonId(1), SimTime(0), &bodies)
        .unwrap();
    (world, store, bodies)
}

#[test]
fn transport_selects_and_executes_a_consented_lift_without_assembly() {
    let (mut world, mut store, bodies) = fixture();
    world
        .offer(&mut store, PersonId(3), PersonId(1), SimTime(0), &bodies)
        .unwrap();
    let local = world
        .offer_response(
            &store,
            PersonId(1),
            SimTime(1),
            &bodies,
            facts(),
            relation,
            |bearer| bearer == PersonId(3),
        )
        .unwrap();
    assert_eq!(local.response.bearer, PersonId(3));
    assert_eq!(
        local.actions,
        [Action::AcceptCarry {
            bearers: vec![PersonId(3)]
        }]
    );
    let before = crate::tests::snapshot(&world, &store);
    let choice = world
        .offer_response(
            &store,
            PersonId(1),
            SimTime(10),
            &bodies,
            facts(),
            relation,
            |_| true,
        )
        .unwrap();
    assert_eq!(
        crate::tests::snapshot(&world, &store),
        before,
        "selection is read-only"
    );
    assert_eq!(choice.response.kind, ResponseKind::Accepted);
    assert_eq!(choice.response.bearer, PersonId(2));
    assert_eq!(
        choice.actions,
        [Action::AcceptCarry {
            bearers: vec![PersonId(2), PersonId(3)]
        }]
    );
    assert!(choice.support.is_none());
    let Action::AcceptCarry { bearers } = &choice.actions[0] else {
        unreachable!()
    };
    let lift = world
        .begin_lift(
            &mut store,
            PersonId(1),
            bearers.iter().copied().collect(),
            Permission {
                passenger_accepted: true,
                ..Permission::default()
            },
            SimTime(10),
            &bodies,
        )
        .unwrap();
    world.record_response(&mut store, choice.response).unwrap();
    let group = world
        .complete_lift(&mut store, PersonId(1), lift.completes, &bodies)
        .unwrap();
    assert_eq!(group.authority, Authority::Consented);
    assert_eq!(group.bearers, BTreeSet::from([PersonId(2), PersonId(3)]));
}

#[test]
fn offer_filtering_and_personal_judgment_use_current_facts() {
    let (mut world, mut store, mut bodies) = fixture();
    world
        .offer(&mut store, PersonId(3), PersonId(1), SimTime(0), &bodies)
        .unwrap();
    offer_inbox_mut(&mut store, PersonId(1))
        .unwrap()
        .by_bearer
        .get_mut(&PersonId(2))
        .unwrap()
        .expires = SimTime(5);
    let choice = world
        .offer_response(
            &store,
            PersonId(1),
            SimTime(10),
            &bodies,
            facts(),
            |bearer| {
                assert_eq!(bearer, PersonId(3));
                Some(RelationFacts {
                    trust: -1,
                    affection: 1_000,
                })
            },
            |_| true,
        )
        .unwrap();
    assert_eq!(choice.response.kind, ResponseKind::Refused);
    assert_eq!(
        choice.actions,
        [Action::RefuseCarry {
            bearer: PersonId(3)
        }]
    );
    bodies.get_mut(&PersonId(3)).unwrap().position = Position::Area(AreaId(11));
    assert!(
        world
            .offer_response(
                &store,
                PersonId(1),
                SimTime(10),
                &bodies,
                facts(),
                |_| panic!("no live local offer"),
                |_| true,
            )
            .is_none()
    );
    let (world, store, bodies) = fixture();
    for changed in [
        OfferResponseFacts {
            autonomous: false,
            ..facts()
        },
        OfferResponseFacts {
            alive: false,
            ..facts()
        },
        OfferResponseFacts {
            combat_restricted: true,
            ..facts()
        },
    ] {
        assert!(
            world
                .offer_response(
                    &store,
                    PersonId(1),
                    SimTime(10),
                    &bodies,
                    changed,
                    |_| panic!("ineligible actor"),
                    |_| true,
                )
                .is_none()
        );
    }
    for changed in [
        OfferResponseFacts {
            idle_or_resting: false,
            ..facts()
        },
        OfferResponseFacts {
            independence_goal: true,
            ..facts()
        },
        OfferResponseFacts {
            hunger: 100,
            ..facts()
        },
        OfferResponseFacts {
            health: 100,
            ..facts()
        },
    ] {
        assert_eq!(
            world
                .offer_response(
                    &store,
                    PersonId(1),
                    SimTime(10),
                    &bodies,
                    changed,
                    relation,
                    |_| { true },
                )
                .unwrap()
                .response
                .kind,
            ResponseKind::Refused
        );
    }
}

#[test]
fn insufficient_load_support_becomes_a_local_request_and_a_willing_intent() {
    let (mut world, mut store, mut bodies) = fixture();
    bodies.get_mut(&PersonId(2)).unwrap().capacity = Quantity(30_000);
    let choice = world
        .offer_response(
            &store,
            PersonId(1),
            SimTime(10),
            &bodies,
            facts(),
            relation,
            |_| true,
        )
        .unwrap();
    assert_eq!(choice.response.kind, ResponseKind::WaitingForSupport);
    assert!(choice.actions.is_empty());
    let request = choice.support.unwrap();
    world
        .observe_support(&mut store, &request, [PersonId(1), PersonId(3)])
        .unwrap();
    assert!(!support_knowledge(&store, PersonId(1)).is_some());
    assert_eq!(
        support_knowledge(&store, PersonId(3)).unwrap().by_passenger[&PersonId(1)],
        request
    );
    let facts = SupportFacts {
        autonomous: true,
        empathy: 700,
        relation: RelationFacts::default(),
        hunger: 0,
        meal_relief: 100,
        fatigue: 0,
        fatigue_limit: 100,
        occupied: false,
        co_present: true,
    };
    assert_eq!(
        world.support_intent(&store, PersonId(3), &request, SimTime(10), &bodies, facts),
        Some(Action::JoinCarry {
            passenger: PersonId(1)
        })
    );
    for changed in [
        SupportFacts {
            empathy: 699,
            ..facts
        },
        SupportFacts {
            autonomous: false,
            ..facts
        },
        SupportFacts {
            hunger: 100,
            ..facts
        },
        SupportFacts {
            fatigue: 50,
            ..facts
        },
        SupportFacts {
            occupied: true,
            ..facts
        },
        SupportFacts {
            co_present: false,
            ..facts
        },
    ] {
        assert!(
            world
                .support_intent(&store, PersonId(3), &request, SimTime(10), &bodies, changed)
                .is_none()
        );
    }
    assert!(
        world
            .support_intent(
                &store,
                PersonId(3),
                &request,
                request.expires,
                &bodies,
                facts
            )
            .is_none()
    );
    world
        .offer(&mut store, PersonId(3), PersonId(1), SimTime(10), &bodies)
        .unwrap();
    assert!(
        world
            .support_intent(&store, PersonId(3), &request, SimTime(10), &bodies, facts)
            .is_none()
    );
}

#[test]
fn action_availability_is_owner_selected_and_expensive_facts_are_lazy() {
    let (world, mut store, mut bodies) = fixture();
    bodies.get_mut(&PersonId(3)).unwrap().position = Position::Area(AreaId(11));
    let facts = AvailableActionFacts {
        independent: true,
        combat_restricted: false,
        moving: false,
        stopped_on_road: false,
        position: Some(Position::Area(AreaId(10))),
    };
    assert_eq!(
        world.available_actions(
            &store,
            PersonId(1),
            SimTime(10),
            facts,
            || Some(bodies.clone()),
            |actor| actor == PersonId(4),
            || panic!("area resident needs no road departure plan"),
        ),
        [
            Action::OfferCarry {
                passenger: PersonId(2)
            },
            Action::AcceptCarry {
                bearers: vec![PersonId(2)]
            },
            Action::RefuseCarry {
                bearer: PersonId(2)
            },
        ]
    );
    assert_eq!(
        world.available_actions(
            &store,
            PersonId(1),
            SimTime(10),
            AvailableActionFacts {
                stopped_on_road: true,
                position: Some(Position::Road(RoadPosition {
                    from: AreaId(10),
                    to: AreaId(11),
                    progress: SimDuration(1),
                    duration: SimDuration(10),
                })),
                ..facts
            },
            || None,
            |_| false,
            || true,
        ),
        [
            Action::Resume,
            Action::ReturnToArea {
                destination: AreaId(10)
            },
            Action::ReturnToArea {
                destination: AreaId(11)
            }
        ]
    );
    assert_eq!(
        world.available_actions(
            &store,
            PersonId(1),
            SimTime(10),
            AvailableActionFacts {
                moving: true,
                ..facts
            },
            || panic!("moving actor does not scan bodies"),
            |_| panic!("moving actor does not enumerate passengers"),
            || panic!("moving actor does not plan departure"),
        ),
        [Action::Stop]
    );
    insert_group(
        &mut store,
        PersonId(1),
        CarryGroup {
            passenger: PersonId(1),
            anchor: PersonId(2),
            bearers: BTreeSet::from([PersonId(2)]),
            authority: Authority::Consented,
            accepted_at: SimTime(10),
        },
    )
    .unwrap();
    assert_eq!(
        world.available_actions(
            &store,
            PersonId(1),
            SimTime(10),
            facts,
            || panic!("carried actor does not scan bodies"),
            |_| false,
            || false
        ),
        [Action::PutDown {
            passenger: PersonId(1)
        }]
    );
    assert!(
        world
            .available_actions(
                &store,
                PersonId(1),
                SimTime(10),
                AvailableActionFacts {
                    combat_restricted: true,
                    ..facts
                },
                || None,
                |_| false,
                || false
            )
            .is_empty()
    );
}

#[test]
fn response_and_support_memories_are_bounded_and_duplicates_do_not_refresh_history() {
    let (mut world, mut store, _) = fixture();
    let response = Response {
        passenger: PersonId(1),
        bearer: PersonId(2),
        kind: ResponseKind::Accepted,
        reason: "accepted".into(),
        observed_at: SimTime(1),
    };
    world.record_response(&mut store, response.clone()).unwrap();
    world
        .record_response(
            &mut store,
            Response {
                observed_at: SimTime(2),
                ..response.clone()
            },
        )
        .unwrap();
    assert_eq!(world.responses, [response]);
    for id in 3..=67 {
        world
            .record_response(
                &mut store,
                Response {
                    passenger: PersonId(id),
                    bearer: PersonId(2),
                    kind: ResponseKind::Refused,
                    reason: "refused".into(),
                    observed_at: SimTime(id),
                },
            )
            .unwrap();
    }
    assert_eq!(world.responses.len(), 64);
    assert_eq!(world.responses[0].passenger, PersonId(4));
    let request = SupportRequest {
        passenger: PersonId(1),
        requester: PersonId(1),
        observed_at: SimTime(10),
        expires: SimTime(100),
    };
    world
        .observe_support(&mut store, &request, (1..=70).map(PersonId))
        .unwrap();
    assert_eq!(support_knowledges(&store).count(), 64);
    for id in 2..=66 {
        world
            .observe_support(
                &mut store,
                &SupportRequest {
                    passenger: PersonId(id),
                    requester: PersonId(id),
                    ..request.clone()
                },
                [PersonId(70)],
            )
            .unwrap();
    }
    assert_eq!(
        support_knowledge(&store, PersonId(70))
            .unwrap()
            .by_passenger
            .len(),
        64
    );
    world
        .observe_support(
            &mut store,
            &SupportRequest {
                observed_at: SimTime(11),
                passenger: PersonId(2),
                requester: PersonId(2),
                ..request
            },
            [PersonId(70)],
        )
        .unwrap();
    assert_eq!(
        support_knowledge(&store, PersonId(70))
            .unwrap()
            .by_passenger[&PersonId(2)]
            .observed_at,
        SimTime(11)
    );
    assert!(
        !support_knowledge(&store, PersonId(70))
            .unwrap()
            .by_passenger
            .contains_key(&PersonId(66))
    );
}

#[test]
fn navigation_origins_and_same_endpoint_choices_preserve_raw_carrier_policy() {
    let area = Location::AtArea(AreaId(10));
    let moving = Location::InTransit {
        from: AreaId(10),
        to: AreaId(11),
        departed: SimTime(0),
        arrives: SimTime(100),
        road_start: None,
    };
    let stopped = Location::StoppedOnRoad {
        from: AreaId(10),
        to: AreaId(11),
        progress: SimDuration(30),
        duration: SimDuration(100),
    };
    let carried = Location::Carried {
        carrier: PersonId(1),
    };
    assert_eq!(navigation_origin(&area).unwrap(), AreaId(10));
    assert_eq!(navigation_origin(&moving).unwrap(), AreaId(11));
    assert_eq!(navigation_origin(&stopped).unwrap(), AreaId(10));
    assert_eq!(navigation_origin(&carried), Err(Error::Unavailable));
    assert!(include_same_origin_destination(&moving, &[AreaId(12)]));
    assert!(!include_same_origin_destination(&moving, &[]));
    for raw in [&area, &stopped, &carried] {
        assert!(!include_same_origin_destination(raw, &[AreaId(12)]));
    }
}
#[test]
fn basic_position_menu_uses_raw_movement_and_resolved_road_work_restrictions() {
    let area = Location::AtArea(AreaId(10));
    let moving = Location::InTransit {
        from: AreaId(10),
        to: AreaId(11),
        departed: SimTime(0),
        arrives: SimTime(100),
        road_start: None,
    };
    let stopped = Location::StoppedOnRoad {
        from: AreaId(10),
        to: AreaId(11),
        progress: SimDuration(30),
        duration: SimDuration(100),
    };
    let carried = Location::Carried {
        carrier: PersonId(1),
    };
    for (raw, resolved, expected) in [
        (
            &area,
            &area,
            vec![
                MenuActionKind::Move,
                MenuActionKind::Work,
                MenuActionKind::Load,
                MenuActionKind::Other,
            ],
        ),
        (
            &moving,
            &moving,
            vec![
                MenuActionKind::Move,
                MenuActionKind::Work,
                MenuActionKind::Load,
                MenuActionKind::Other,
            ],
        ),
        (&stopped, &stopped, vec![MenuActionKind::Other]),
        (
            &carried,
            &area,
            vec![
                MenuActionKind::Work,
                MenuActionKind::Load,
                MenuActionKind::Other,
            ],
        ),
        (
            &carried,
            &moving,
            vec![
                MenuActionKind::Work,
                MenuActionKind::Load,
                MenuActionKind::Other,
            ],
        ),
        (&carried, &stopped, vec![MenuActionKind::Other]),
    ] {
        let mut menu = vec![
            MenuActionKind::Move,
            MenuActionKind::Work,
            MenuActionKind::Load,
            MenuActionKind::Other,
        ];
        filter_position_menu(&mut menu, raw, resolved, |kind| *kind);
        assert_eq!(menu, expected);
    }
}
