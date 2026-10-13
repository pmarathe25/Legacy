use super::*;
use game_ecs::{Schema, StableId, Store};
struct References {
    lifters: BTreeMap<PersonId, (PersonId, SimTime)>,
}
impl TransportReferences for References {
    fn lifting(&self, actor: PersonId) -> Option<(PersonId, SimTime)> {
        self.lifters.get(&actor).copied()
    }
    fn lifters(&self) -> impl Iterator<Item = (PersonId, PersonId, SimTime)> {
        self.lifters
            .iter()
            .map(|(actor, (passenger, at))| (*actor, *passenger, *at))
    }
}
fn fixture() -> (Runtime, Store, BTreeMap<PersonId, Body>) {
    let runtime = Runtime::new(Rules::default()).unwrap();
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_spatial::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let bodies = (1..=3)
        .map(|id| {
            (
                PersonId(id),
                Body {
                    position: Position::Area(AreaId(10)),
                    mass: Quantity(3_000),
                    cargo: Quantity(0),
                    capacity: Quantity(100_000),
                    can_hold: true,
                    can_walk: true,
                    can_bear: true,
                },
            )
        })
        .collect();
    for actor in 1..=3 {
        game_spatial::set_location(&mut store, StableId(actor), Location::AtArea(AreaId(10)))
            .unwrap();
    }
    (runtime, store, bodies)
}
#[test]
fn carried_anchor_and_native_route_are_reciprocal_save_constraints() {
    let (runtime, mut store, bodies) = fixture();
    insert_group(
        &mut store,
        PersonId(3),
        CarryGroup {
            passenger: PersonId(3),
            anchor: PersonId(1),
            bearers: BTreeSet::from([PersonId(1), PersonId(2)]),
            authority: Authority::Newborn,
            accepted_at: SimTime(0),
        },
    )
    .unwrap();
    let references = References {
        lifters: BTreeMap::new(),
    };
    assert!(validate_composition(&runtime, &store, &bodies, &references, SimTime(0)).is_err());
    game_spatial::set_location(
        &mut store,
        StableId(3),
        Location::Carried {
            carrier: PersonId(1),
        },
    )
    .unwrap();
    validate_composition(&runtime, &store, &bodies, &references, SimTime(0)).unwrap();
    prepare_route_replacement(&store, PersonId(3), vec![AreaId(11)])
        .unwrap()
        .commit(&mut store)
        .unwrap();
    assert!(validate_composition(&runtime, &store, &bodies, &references, SimTime(0)).is_err());
    remove_route(&mut store, PersonId(3)).unwrap();
    game_spatial::set_location(
        &mut store,
        StableId(3),
        Location::Carried {
            carrier: PersonId(2),
        },
    )
    .unwrap();
    assert!(validate_composition(&runtime, &store, &bodies, &references, SimTime(0)).is_err());
    remove_group(&mut store, PersonId(3)).unwrap();
    assert!(validate_composition(&runtime, &store, &bodies, &references, SimTime(0)).is_err());
}
#[test]
fn every_actual_lift_and_every_bearer_action_match_both_directions_and_deadline() {
    let (runtime, mut store, bodies) = fixture();
    let lift = Lift {
        passenger: PersonId(3),
        bearers: BTreeSet::from([PersonId(1), PersonId(2)]),
        authority: Authority::Newborn,
        started: SimTime(0),
        completes: SimTime(30_000),
    };
    insert_lift(&mut store, PersonId(3), lift).unwrap();
    let mut references = References {
        lifters: BTreeMap::from([
            (PersonId(1), (PersonId(3), SimTime(30_000))),
            (PersonId(2), (PersonId(3), SimTime(30_000))),
        ]),
    };
    validate_composition(&runtime, &store, &bodies, &references, SimTime(1)).unwrap();
    references.lifters.get_mut(&PersonId(2)).unwrap().1 = SimTime(30_001);
    assert!(validate_composition(&runtime, &store, &bodies, &references, SimTime(1)).is_err());
    references.lifters.get_mut(&PersonId(2)).unwrap().1 = SimTime(30_000);
    references
        .lifters
        .insert(PersonId(3), (PersonId(3), SimTime(30_000)));
    assert!(validate_composition(&runtime, &store, &bodies, &references, SimTime(1)).is_err());
}

struct SaveFacts;
impl RouteReferences for SaveFacts {
    fn location(&self, _: PersonId) -> Option<&Location> {
        None
    }
    fn link(&self, _: AreaId, _: AreaId) -> bool {
        false
    }
}
impl MotionReferences for SaveFacts {
    fn actors(&self) -> impl Iterator<Item = PersonId> {
        [PersonId(1), PersonId(2)].into_iter()
    }
    fn arrivals(&self) -> impl Iterator<Item = (PersonId, SimTime)> {
        std::iter::empty()
    }
    fn alive(&self, _: PersonId) -> Option<bool> {
        Some(true)
    }
    fn physical_enabled(&self) -> bool {
        true
    }
    fn area_exists(&self, area: AreaId) -> bool {
        area == AreaId(10)
    }
    fn duration(&self, _: AreaId, _: AreaId) -> Option<SimDuration> {
        None
    }
}
impl TransportReferences for SaveFacts {
    fn lifting(&self, _: PersonId) -> Option<(PersonId, SimTime)> {
        None
    }
    fn lifters(&self) -> impl Iterator<Item = (PersonId, PersonId, SimTime)> {
        std::iter::empty()
    }
}
impl MeasuredBodyFacts for SaveFacts {
    fn measurements(&self, actor: PersonId, _: SimTime) -> Result<BodyMeasurements, Error> {
        Ok(BodyMeasurements {
            age: (actor == PersonId(2)).then_some((SimDuration(0), SimDuration(100))),
            cargo: Quantity(100),
            capacity: Quantity(100_000),
            alive: true,
            health: 100,
            fatigue: 0,
            fatigue_limit: 100,
            independent: actor == PersonId(1),
            combat_restricted: false,
            work: CarryWork::Idle,
            incapacitation: None,
        })
    }
}
#[test]
fn public_save_validation_and_body_projection_share_canonical_carrier_rows() {
    let runtime = Runtime::new(Rules::default()).unwrap();
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_spatial::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    game_spatial::set_location(&mut store, StableId(1), Location::AtArea(AreaId(10))).unwrap();
    game_spatial::set_location(
        &mut store,
        StableId(2),
        Location::Carried {
            carrier: PersonId(1),
        },
    )
    .unwrap();
    insert_group(
        &mut store,
        PersonId(2),
        CarryGroup {
            passenger: PersonId(2),
            anchor: PersonId(1),
            bearers: BTreeSet::from([PersonId(1)]),
            authority: Authority::Newborn,
            accepted_at: SimTime(0),
        },
    )
    .unwrap();
    let child = runtime
        .measured_body(&store, PersonId(2), SimTime(0), &SaveFacts)
        .unwrap();
    assert_eq!(child.position, Position::Area(AreaId(10)));
    assert_eq!(child.mass, runtime.rules.newborn_mass);
    assert_eq!(child.cargo, Quantity(100));
    assert!(!child.can_walk);
    assert_eq!(
        runtime
            .measured_unplaced_body(
                &Location::AtArea(AreaId(10)),
                SimTime(0),
                SaveFacts.measurements(PersonId(2), SimTime(0)).unwrap()
            )
            .unwrap(),
        child
    );
    validate_save(Some(&runtime), &store, &SaveFacts, SimTime(0)).unwrap();
    assert!(validate_save(None, &store, &SaveFacts, SimTime(0)).is_err());
    let mut malformed = runtime.clone();
    malformed.rules.newborn_mass = Quantity(malformed.rules.adult_mass.0 + 1);
    assert!(matches!(
        validate_save(Some(&malformed), &store, &SaveFacts, SimTime(0)),
        Err(Error::InvalidSave(_))
    ));
    *game_spatial::location_mut(&mut store, StableId(1)).unwrap() = Location::Carried {
        carrier: PersonId(2),
    };
    assert!(validate_save(Some(&runtime), &store, &SaveFacts, SimTime(0)).is_err());
}
