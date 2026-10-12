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
