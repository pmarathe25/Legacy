use super::*;
#[derive(Clone)]
struct Facts {
    roads: KnownRoutes,
    readiness: TravelReadiness,
}
impl BodyFacts for Facts {
    fn body(&self, _: PersonId) -> Option<Body> {
        None
    }
}
impl RoadFacts for Facts {
    fn duration(&self, from: AreaId, to: AreaId) -> Option<SimDuration> {
        self.roads.get(&from)?.get(&to).copied()
    }
    fn known(&self, _: PersonId, from: AreaId, to: AreaId) -> bool {
        self.duration(from, to).is_some()
    }
}
impl JourneyFacts for Facts {
    fn readiness(&self, actor: PersonId) -> Option<TravelReadiness> {
        (actor == PersonId(1)).then(|| self.readiness.clone())
    }
    fn navigation(&self, _: PersonId) -> KnownRoutes {
        self.roads.clone()
    }
    fn area_exists(&self, area: AreaId) -> bool {
        [AreaId(1), AreaId(2), AreaId(3)].contains(&area)
    }
}
fn fixture() -> (Store, Facts, Runtime) {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_spatial::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    game_spatial::set_location(&mut store, StableId(1), Location::AtArea(AreaId(1))).unwrap();
    let facts = Facts {
        roads: KnownRoutes::from([
            (AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(100))])),
            (
                AreaId(2),
                BTreeMap::from([(AreaId(3), SimDuration(150)), (AreaId(1), SimDuration(400))]),
            ),
        ]),
        readiness: TravelReadiness {
            independent: true,
            combat_restricted: false,
            physical: None,
        },
    };
    (store, facts, Runtime::new(Rules::default()).unwrap())
}
#[test]
fn kernel_journey_has_native_pending_steps_without_a_carrying_runtime_or_body_catalog() {
    let (mut store, facts, mut fallback) = fixture();
    let plan = prepare_journey_request(
        &store,
        TravelRequest {
            actor: PersonId(1),
            destination: AreaId(3),
        },
        &facts,
        None,
        SimTime(0),
    )
    .unwrap();
    let mut batch = Batch::new();
    let finish = plan
        .stage_into(None, &store, &facts, SimTime(0), &mut batch)
        .unwrap();
    finish.validate_footprint(&batch).unwrap();
    store.apply_batch(batch).unwrap();
    finish.finish(&mut fallback);
    assert_eq!(route_steps(&store, PersonId(1)), &[AreaId(3)]);
    assert!(matches!(
        game_spatial::location(&store, StableId(1)),
        Some(Location::InTransit {
            from: AreaId(1),
            to: AreaId(2),
            arrives: SimTime(100),
            ..
        })
    ));
    validate_absent(&store).unwrap();
    fallback
        .prepare_arrivals(&store, &[PersonId(1)], SimTime(100))
        .unwrap()
        .commit(&mut fallback, &mut store, &facts, SimTime(100))
        .unwrap();
    let continuation = prepare_continued_journey(
        &store,
        ContinuedJourney {
            actor: PersonId(1),
            origin: AreaId(2),
            destination: AreaId(3),
        },
        &facts,
        None,
        SimTime(100),
    )
    .unwrap();
    assert!(
        !continuation
            .projection()
            .effects()
            .contains(&MotionEffect::Interrupt(PersonId(1)))
    );
    let mut batch = Batch::new();
    let finish = continuation
        .stage_into(None, &store, &facts, SimTime(100), &mut batch)
        .unwrap();
    finish.validate_footprint(&batch).unwrap();
    store.apply_batch(batch).unwrap();
    finish.finish(&mut fallback);
    assert!(route_steps(&store, PersonId(1)).is_empty());
    assert!(matches!(
        game_spatial::location(&store, StableId(1)),
        Some(Location::InTransit {
            from: AreaId(2),
            to: AreaId(3),
            arrives: SimTime(250),
            ..
        })
    ));
}
#[test]
fn selected_readiness_navigation_time_location_and_runtime_presence_are_guarded_before_publish() {
    for failure in 0..5 {
        let (mut store, mut facts, fallback) = fixture();
        let plan = prepare_journey_request(
            &store,
            TravelRequest {
                actor: PersonId(1),
                destination: AreaId(3),
            },
            &facts,
            None,
            SimTime(0),
        )
        .unwrap();
        let mut now = SimTime(0);
        let mut runtime = None;
        match failure {
            0 => facts.readiness.independent = false,
            1 => {
                facts.roads.get_mut(&AreaId(2)).unwrap().remove(&AreaId(3));
            }
            2 => now = SimTime(1),
            3 => {
                *game_spatial::location_mut(&mut store, StableId(1)).unwrap() =
                    Location::AtArea(AreaId(2));
            }
            _ => runtime = Some(&fallback),
        }
        let before = store.snapshot().unwrap();
        assert!(
            plan.stage_into(runtime, &store, &facts, now, &mut Batch::new())
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
}

#[test]
fn failed_continuation_prepares_guarded_route_removal_and_one_interrupted_effect() {
    for failure in 0..4 {
        let (mut store, mut facts, mut runtime) = fixture();
        game_spatial::set_location(&mut store, StableId(1), Location::AtArea(AreaId(2))).unwrap();
        prepare_route_replacement(&store, PersonId(1), vec![AreaId(3)])
            .unwrap()
            .commit(&mut store)
            .unwrap();
        facts.readiness.independent = false;
        let request = ContinuedJourney {
            actor: PersonId(1),
            origin: AreaId(2),
            destination: AreaId(3),
        };
        let plan =
            prepare_continuation_response(&store, request, &facts, None, SimTime(100)).unwrap();
        assert_eq!(
            plan.projection().effects(),
            &[MotionEffect::Interrupted { actor: PersonId(1) }]
        );
        let mut now = SimTime(100);
        match failure {
            1 => facts.readiness.independent = true,
            2 => now = SimTime(101),
            3 => {
                prepare_route_replacement(&store, PersonId(1), Vec::new())
                    .unwrap()
                    .commit(&mut store)
                    .unwrap();
            }
            _ => {}
        }
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let result = plan.stage_into(None, &store, &facts, now, &mut batch);
        if failure > 0 {
            assert!(result.is_err());
            assert_eq!(store.snapshot().unwrap(), before);
            continue;
        }
        let finish = result.unwrap();
        finish.validate_footprint(&batch).unwrap();
        store.apply_batch(batch).unwrap();
        assert_eq!(
            finish.finish(&mut runtime),
            vec![MotionEffect::Interrupted { actor: PersonId(1) }]
        );
        assert!(route_steps(&store, PersonId(1)).is_empty());
        assert_eq!(
            game_spatial::location(&store, StableId(1)),
            Some(&Location::AtArea(AreaId(2)))
        );
    }
}

#[test]
fn pending_arrival_preflight_rejects_overflow_before_any_motion_publication() {
    let (mut store, facts, _) = fixture();
    game_spatial::set_location(
        &mut store,
        StableId(1),
        Location::InTransit {
            from: AreaId(1),
            to: AreaId(2),
            departed: SimTime(0),
            arrives: SimTime(100),
            road_start: None,
        },
    )
    .unwrap();
    prepare_route_replacement(&store, PersonId(1), vec![AreaId(3)])
        .unwrap()
        .commit(&mut store)
        .unwrap();
    let before = store.snapshot().unwrap();
    assert_eq!(
        preflight_arrival_continuation(&store, PersonId(1), SimTime(u64::MAX), &facts),
        Err(Error::Overflow)
    );
    assert_eq!(store.snapshot().unwrap(), before);
    preflight_arrival_continuation(&store, PersonId(1), SimTime(100), &facts).unwrap();
}
