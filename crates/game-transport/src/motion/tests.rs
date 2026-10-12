use super::*;
fn fixture() -> (Runtime, Store) {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_spatial::register_components(&mut schema).unwrap();
    schema
        .register::<String>("test/foreign", |value| {
            if value == "reject" {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    for actor in [PersonId(1), PersonId(2)] {
        game_spatial::set_location(
            &mut store,
            StableId(actor.0),
            Location::InTransit {
                from: AreaId(10),
                to: AreaId(11),
                departed: SimTime(0),
                arrives: SimTime(100),
                road_start: None,
            },
        )
        .unwrap();
    }
    game_spatial::set_location(
        &mut store,
        StableId(3),
        Location::Carried {
            carrier: PersonId(1),
        },
    )
    .unwrap();
    insert_group(
        &mut store,
        PersonId(3),
        CarryGroup {
            passenger: PersonId(3),
            anchor: PersonId(1),
            bearers: BTreeSet::from([PersonId(1), PersonId(2)]),
            authority: Authority::UnableToConsent,
            accepted_at: SimTime(0),
        },
    )
    .unwrap();
    (Runtime::new(Rules::default()).unwrap(), store)
}
fn empty_bodies() -> BTreeMap<PersonId, Body> {
    BTreeMap::new()
}
#[test]
fn stopping_keeps_a_carried_passenger_and_pauses_the_exact_approach_point() {
    let (mut runtime, mut store) = fixture();
    let target = RoadPosition {
        from: AreaId(10),
        to: AreaId(11),
        progress: SimDuration(80),
        duration: SimDuration(100),
    };
    insert_approach(
        &mut store,
        PersonId(1),
        Approach {
            target,
            started: SimTime(0),
            completes: SimTime(80),
        },
    )
    .unwrap();
    let effects = runtime
        .prepare_stop(&store, PersonId(1), SimTime(40))
        .unwrap()
        .commit(&mut runtime, &mut store, &empty_bodies(), SimTime(40))
        .unwrap();
    assert_eq!(
        game_spatial::location(&store, StableId(1)),
        Some(&Location::StoppedOnRoad {
            from: AreaId(10),
            to: AreaId(11),
            progress: SimDuration(40),
            duration: SimDuration(100)
        })
    );
    assert_eq!(
        game_spatial::location(&store, StableId(3)),
        Some(&Location::Carried {
            carrier: PersonId(1)
        })
    );
    assert!(group(&store, PersonId(3)).is_some());
    assert_eq!(
        paused_approach(&store, PersonId(1)),
        Some(&PausedApproach { target })
    );
    assert!(approach_job(&store, PersonId(1)).is_none());
    assert_eq!(
        resume_target(&store, PersonId(1)),
        Some(&ResumeTarget { area: AreaId(11) })
    );
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, MotionEffect::Interrupted { .. }))
            .count(),
        2
    );
}
#[test]
fn an_exact_arrival_endpoint_has_no_paused_directional_route() {
    let (mut runtime, mut store) = fixture();
    runtime
        .prepare_stop(&store, PersonId(1), SimTime(100))
        .unwrap()
        .commit(&mut runtime, &mut store, &empty_bodies(), SimTime(100))
        .unwrap();
    assert_eq!(
        game_spatial::location(&store, StableId(1)),
        Some(&Location::AtArea(AreaId(11)))
    );
    assert!(resume_targets(&store).next().is_none());
    assert!(paused_approaches(&store).next().is_none());
}
#[test]
fn stale_bearer_location_and_foreign_schema_rejection_preserve_all_motion_rows() {
    for stale in [true, false] {
        let (runtime, mut store) = fixture();
        let prepared = runtime
            .prepare_put_down(
                &store,
                PersonId(1),
                PersonId(3),
                SimTime(40),
                PutDownFacts {
                    passenger_independent: false,
                    passenger_combat_restricted: false,
                },
            )
            .unwrap();
        if stale {
            *game_spatial::location_mut(&mut store, StableId(2)).unwrap() =
                Location::AtArea(AreaId(10));
        }
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let staged =
            prepared.stage_into(&runtime, &store, &empty_bodies(), SimTime(40), &mut batch);
        if stale {
            assert!(staged.is_err());
        } else {
            let _finish = staged.unwrap();
            batch.insert(StableId(4), "reject".to_owned());
            assert!(store.apply_batch(batch).is_err());
        }
        assert_eq!(before, store.snapshot().unwrap());
        assert!(group(&store, PersonId(3)).is_some());
    }
}

#[test]
fn disjoint_motion_plans_share_one_envelope_and_undeclared_edges_reject() {
    for foreign in [false, true] {
        let (runtime, mut store) = fixture();
        game_spatial::set_location(
            &mut store,
            StableId(4),
            Location::InTransit {
                from: AreaId(20),
                to: AreaId(21),
                departed: SimTime(0),
                arrives: SimTime(100),
                road_start: None,
            },
        )
        .unwrap();
        let mut envelope = MotionBatch::new(&runtime, SimTime(40));
        envelope
            .push(
                runtime
                    .prepare_stop(&store, PersonId(1), SimTime(40))
                    .unwrap(),
            )
            .unwrap();
        envelope
            .push(
                runtime
                    .prepare_stop(&store, PersonId(4), SimTime(40))
                    .unwrap(),
            )
            .unwrap();
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let finish = envelope
            .stage_into(&runtime, &store, &empty_bodies(), SimTime(40), &mut batch)
            .unwrap();
        if foreign {
            batch.insert(
                StableId(5),
                CarryGroup {
                    passenger: PersonId(5),
                    anchor: PersonId(1),
                    bearers: BTreeSet::from([PersonId(1)]),
                    authority: Authority::Newborn,
                    accepted_at: SimTime(40),
                },
            );
            assert_eq!(finish.validate_footprint(&batch), Err(Error::Unavailable));
            assert_eq!(store.snapshot().unwrap(), before);
        } else {
            finish.validate_footprint(&batch).unwrap();
            store.apply_batch(batch).unwrap();
            assert!(matches!(
                game_spatial::location(&store, StableId(4)),
                Some(Location::StoppedOnRoad {
                    progress: SimDuration(40),
                    ..
                })
            ));
            assert!(matches!(
                game_spatial::location(&store, StableId(1)),
                Some(Location::StoppedOnRoad {
                    progress: SimDuration(40),
                    ..
                })
            ));
        }
    }
}
#[test]
fn opening_topology_time_and_exact_reserved_rows_reject_stale_or_foreign_changes() {
    for failure in 0..4 {
        let (runtime, mut store) = fixture();
        let prepared = runtime
            .prepare_stop(&store, PersonId(1), SimTime(40))
            .unwrap();
        if failure == 0 {
            insert_group(
                &mut store,
                PersonId(5),
                CarryGroup {
                    passenger: PersonId(5),
                    anchor: PersonId(1),
                    bearers: BTreeSet::from([PersonId(1)]),
                    authority: Authority::Newborn,
                    accepted_at: SimTime(40),
                },
            )
            .unwrap();
        }
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        if failure == 2 {
            batch.remove::<Location>(StableId(3));
        }
        let now = if failure == 1 {
            SimTime(41)
        } else {
            SimTime(40)
        };
        let admitted = prepared.stage_into(&runtime, &store, &empty_bodies(), now, &mut batch);
        if failure < 3 {
            assert!(admitted.is_err());
        } else {
            let finish = admitted.unwrap();
            batch.remove::<Route>(StableId(3));
            assert_eq!(finish.validate_footprint(&batch), Err(Error::Unavailable));
            assert!(store.apply_batch(batch).is_err());
        }
        assert_eq!(store.snapshot().unwrap(), before);
    }
}
fn road_bodies(store: &Store, now: SimTime) -> BTreeMap<PersonId, Body> {
    game_spatial::locations(store)
        .map(|(actor, _)| {
            let position =
                position_from_location(&game_spatial::resolve(store, actor).unwrap(), now).unwrap();
            (
                PersonId(actor.0),
                Body {
                    position,
                    mass: Quantity(3_000),
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
struct Roads {
    reverse: u64,
    known: bool,
}
impl RoadFacts for Roads {
    fn duration(&self, from: AreaId, to: AreaId) -> Option<SimDuration> {
        match (from.0, to.0) {
            (10, 11) => Some(SimDuration(100)),
            (11, 10) => Some(SimDuration(self.reverse)),
            _ => None,
        }
    }
    fn known(&self, _: PersonId, _: AreaId, _: AreaId) -> bool {
        self.known
    }
}
#[test]
fn reverse_return_stages_connected_body_motion_with_exact_asymmetric_geometry() {
    let (runtime, mut store) = fixture();
    let facts = road_bodies(&store, SimTime(25));
    let roads = Roads {
        reverse: 300,
        known: false,
    };
    let plan = runtime
        .prepare_road_return(&store, PersonId(1), AreaId(10), SimTime(25), &facts, &roads)
        .unwrap();
    let mut batch = Batch::new();
    let finish = plan
        .stage_into(&runtime, &store, &facts, &roads, SimTime(25), &mut batch)
        .unwrap();
    finish.validate_footprint(&batch).unwrap();
    store.apply_batch(batch).unwrap();
    let location = game_spatial::location(&store, StableId(1)).unwrap();
    assert!(matches!(
        location,
        Location::InTransit {
            arrives: SimTime(100),
            from: AreaId(11),
            to: AreaId(10),
            ..
        }
    ));
    assert_eq!(
        position_from_location(location, SimTime(25)).unwrap(),
        facts[&PersonId(1)].position
    );
    assert_eq!(
        game_spatial::location(&store, StableId(1)),
        game_spatial::location(&store, StableId(2))
    );
    assert!(matches!(
        game_spatial::location(&store, StableId(3)),
        Some(Location::Carried {
            carrier: PersonId(1)
        })
    ));
}
#[test]
fn selected_road_and_body_changes_reject_departure_before_publication() {
    for stale_road in [false, true] {
        let (runtime, store) = fixture();
        let mut facts = road_bodies(&store, SimTime(25));
        let mut roads = Roads {
            reverse: 300,
            known: true,
        };
        let plan = runtime
            .prepare_road_return(&store, PersonId(1), AreaId(10), SimTime(25), &facts, &roads)
            .unwrap();
        if stale_road {
            roads.reverse = 301;
        } else {
            facts.get_mut(&PersonId(2)).unwrap().cargo = Quantity(1);
        }
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        assert!(
            plan.stage_into(&runtime, &store, &facts, &roads, SimTime(25), &mut batch)
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
}
#[test]
fn real_birth_beside_carried_gestator_uses_existing_bearers_then_overload_stops_every_bearer() {
    let (mut runtime, mut store) = fixture();
    game_spatial::set_location(
        &mut store,
        StableId(4),
        Location::StoppedOnRoad {
            from: AreaId(10),
            to: AreaId(11),
            progress: SimDuration(40),
            duration: SimDuration(100),
        },
    )
    .unwrap();
    let mut bodies = road_bodies(&store, SimTime(40));
    bodies.get_mut(&PersonId(4)).unwrap().mass = Quantity(200_000);
    runtime
        .prepare_birth_motion(&store, PersonId(4), PersonId(3), SimTime(40), &bodies)
        .unwrap()
        .commit(&mut runtime, &mut store, &bodies, SimTime(40))
        .unwrap();
    assert_eq!(
        group(&store, PersonId(4)).unwrap().bearers,
        group(&store, PersonId(3)).unwrap().bearers
    );
    assert_eq!(
        game_spatial::location(&store, StableId(4)),
        Some(&Location::Carried {
            carrier: PersonId(1)
        })
    );
    struct Facts(BTreeMap<PersonId, Body>);
    impl BodyFacts for Facts {
        fn body(&self, actor: PersonId) -> Option<Body> {
            self.0.get(&actor).cloned()
        }
    }
    impl ReconciliationFacts for Facts {
        fn actors(&self) -> impl Iterator<Item = PersonId> {
            self.0.keys().copied()
        }
        fn alive(&self, _: PersonId) -> bool {
            true
        }
    }
    let facts = Facts(bodies);
    let plan = runtime
        .prepare_reconciliation(&store, SimTime(40), &facts)
        .unwrap();
    let mut batch = Batch::new();
    let finish = plan
        .stage_into(&runtime, &store, &facts, SimTime(40), &mut batch)
        .unwrap();
    finish.validate_footprint(&batch).unwrap();
    store.apply_batch(batch).unwrap();
    finish.finish(&mut runtime);
    for actor in [1, 2] {
        assert!(matches!(
            game_spatial::location(&store, StableId(actor)),
            Some(Location::StoppedOnRoad {
                progress: SimDuration(40),
                ..
            })
        ));
    }
    for actor in [3, 4] {
        assert_eq!(
            game_spatial::location(&store, StableId(actor)),
            Some(&Location::Carried {
                carrier: PersonId(1)
            })
        );
    }
}
#[test]
fn real_timed_lift_owns_final_carrier_route_and_arrival_cleanup() {
    let (mut runtime, mut store) = fixture();
    remove_group(&mut store, PersonId(3)).unwrap();
    for actor in 1..=3 {
        game_spatial::set_location(&mut store, StableId(actor), Location::AtArea(AreaId(10)))
            .unwrap();
    }
    let bodies = road_bodies(&store, SimTime(0));
    for actor in [1, 2] {
        runtime
            .offer(
                &mut store,
                PersonId(actor),
                PersonId(3),
                SimTime(0),
                &bodies,
            )
            .unwrap();
    }
    runtime
        .begin_lift(
            &mut store,
            PersonId(3),
            BTreeSet::from([PersonId(1), PersonId(2)]),
            Permission {
                passenger_accepted: true,
                ..Permission::default()
            },
            SimTime(0),
            &bodies,
        )
        .unwrap();
    prepare_route_replacement(&store, PersonId(3), vec![AreaId(11)])
        .unwrap()
        .commit(&mut store)
        .unwrap();
    let activities = BTreeMap::from([(PersonId(3), false)]);
    let plan = runtime
        .prepare_completed_lift_motion(&store, PersonId(3), SimTime(30_000), &bodies, &activities)
        .unwrap();
    assert!(
        plan.effects()
            .contains(&MotionEffect::ClearArrival(PersonId(3)))
    );
    let stale_plan = runtime
        .prepare_completed_lift_motion(&store, PersonId(3), SimTime(30_000), &bodies, &activities)
        .unwrap();
    let changed_actions = BTreeMap::from([(PersonId(3), true)]);
    let before = store.snapshot().unwrap();
    assert!(
        stale_plan
            .stage_into(
                &runtime,
                &store,
                &bodies,
                &changed_actions,
                SimTime(30_000),
                &mut Batch::new()
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    let effects = plan
        .commit(
            &mut runtime,
            &mut store,
            &bodies,
            &activities,
            SimTime(30_000),
        )
        .unwrap();
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, MotionEffect::Interrupt(_)))
            .count(),
        3
    );
    assert!(lift(&store, PersonId(3)).is_none());
    assert!(route_steps(&store, PersonId(3)).is_empty());
    assert_eq!(
        game_spatial::location(&store, StableId(3)),
        Some(&Location::Carried {
            carrier: PersonId(1)
        })
    );
}

#[test]
fn motion_admission_is_bound_to_its_batch_even_with_identical_foreign_contents() {
    let (runtime, store) = fixture();
    let mut original = Batch::new();
    let admitted = runtime
        .prepare_stop(&store, PersonId(1), SimTime(40))
        .unwrap()
        .stage_into(
            &runtime,
            &store,
            &empty_bodies(),
            SimTime(40),
            &mut original,
        )
        .unwrap();
    let mut foreign = Batch::new();
    let _other = runtime
        .prepare_stop(&store, PersonId(1), SimTime(40))
        .unwrap()
        .stage_into(&runtime, &store, &empty_bodies(), SimTime(40), &mut foreign)
        .unwrap();
    assert_eq!(
        admitted.validate_footprint(&foreign),
        Err(Error::Unavailable)
    );
    original.insert(StableId(4), "independent".to_owned());
    admitted.validate_footprint(&original).unwrap();
    admitted
        .route_admission(PersonId(1))
        .unwrap()
        .validate_footprint(&original)
        .unwrap();
    assert!(admitted.route_admission(PersonId(3)).is_none());
}
#[test]
fn a_deadline_wave_preserves_outsider_event_order_between_coupled_bearers() {
    let (mut runtime, mut store) = fixture();
    game_spatial::set_location(
        &mut store,
        StableId(4),
        Location::InTransit {
            from: AreaId(10),
            to: AreaId(11),
            departed: SimTime(0),
            arrives: SimTime(100),
            road_start: None,
        },
    )
    .unwrap();
    group_mut(&mut store, PersonId(3)).unwrap().bearers =
        BTreeSet::from([PersonId(1), PersonId(4)]);
    let target = RoadPosition {
        from: AreaId(10),
        to: AreaId(11),
        progress: SimDuration(80),
        duration: SimDuration(100),
    };
    for actor in [1, 2, 4] {
        insert_approach(
            &mut store,
            PersonId(actor),
            Approach {
                target,
                started: SimTime(0),
                completes: SimTime(80),
            },
        )
        .unwrap();
    }
    let effects = runtime
        .prepare_arrivals(
            &store,
            &[PersonId(1), PersonId(2), PersonId(4)],
            SimTime(80),
        )
        .unwrap()
        .commit(&mut runtime, &mut store, &empty_bodies(), SimTime(80))
        .unwrap();
    let interrupted = effects
        .into_iter()
        .filter_map(|effect| match effect {
            MotionEffect::Interrupted { actor } => Some(actor),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(interrupted, [PersonId(1), PersonId(2), PersonId(4)]);
    assert_eq!(
        game_spatial::location(&store, StableId(1)),
        game_spatial::location(&store, StableId(4))
    );
    assert!(approaches(&store).next().is_none());
}
