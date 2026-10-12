use super::*;
use std::collections::{BTreeMap, BTreeSet};
struct Memory {
    signal: Option<(Location, Option<ChildhoodInterest>, i64, SimTime)>,
    stocks: BTreeMap<InventoryId, StockKnowledge>,
    carried: BTreeSet<InventoryId>,
}
impl RescueMemory for Memory {
    fn signal(&self, _: PersonId) -> Option<RememberedRescueSignal<'_>> {
        self.signal
            .as_ref()
            .map(
                |(location, preference, urgency, observed_at)| RememberedRescueSignal {
                    location,
                    preference: *preference,
                    urgency: *urgency,
                    observed_at: *observed_at,
                },
            )
    }
    fn stocks(&self) -> impl Iterator<Item = &StockKnowledge> {
        self.stocks.values().rev()
    }
    fn carried(&self, id: InventoryId) -> bool {
        self.carried.contains(&id)
    }
}
fn road() -> Location {
    Location::StoppedOnRoad {
        from: AreaId(10),
        to: AreaId(11),
        progress: SimDuration(5),
        duration: SimDuration(10),
    }
}
fn facts(location: &Location) -> GuardianRescueFacts<'_> {
    GuardianRescueFacts {
        now: SimTime(10),
        guardian_location: location,
        co_present: true,
        moving: false,
        idle_or_resting: true,
        combat_restricted: false,
        fatigue: 10,
        fatigue_limit: 10,
        meal_size: Quantity(10),
        target_independent: false,
        target_health: 100,
    }
}
fn fixture() -> (Runtime, game_ecs::Store, Memory) {
    let mut schema = game_ecs::Schema::new();
    game_transport::register_components(&mut schema).unwrap();
    (
        Runtime::new(game_transport::Rules::default()).unwrap(),
        game_ecs::Store::with_schema(schema),
        Memory {
            signal: None,
            stocks: BTreeMap::new(),
            carried: BTreeSet::new(),
        },
    )
}
struct NoBodies;
impl BodyFacts for NoBodies {
    fn body(&self, _: PersonId) -> Option<game_transport::Body> {
        panic!("remembered immediate care does not enumerate bodies")
    }
}
fn food(id: u64) -> StockKnowledge {
    StockKnowledge {
        inventory: InventoryId(id),
        area: AreaId(10),
        good: Good::Food,
        quantity: Quantity(10),
        accessible: true,
        observed_at: SimTime(10),
        learned_at: SimTime(10),
        source: KnowledgeSource::Observed(PersonId(1)),
    }
}
#[test]
fn fresh_personal_food_precedes_company_rest_and_support_and_selects_first_carried_stock() {
    let (transport, store, mut memory) = fixture();
    let location = road();
    memory.signal = Some((
        location.clone(),
        Some(ChildhoodInterest::Company),
        1,
        SimTime(10),
    ));
    memory.stocks = BTreeMap::from([
        (InventoryId(1), food(1)),
        (InventoryId(2), food(2)),
        (InventoryId(3), food(3)),
    ]);
    memory.carried = BTreeSet::from([InventoryId(2), InventoryId(3)]);
    assert_eq!(
        guardian_rescue(
            &store,
            &transport,
            PersonId(1),
            PersonId(3),
            facts(&location),
            &memory,
            &NoBodies
        ),
        Some(RescueIntent::Care {
            child: PersonId(3),
            source: InventoryId(2)
        })
    );
    memory.carried.clear();
    assert_eq!(
        guardian_rescue(
            &store,
            &transport,
            PersonId(1),
            PersonId(3),
            facts(&location),
            &memory,
            &NoBodies
        ),
        Some(RescueIntent::Comfort { child: PersonId(3) })
    );
}
#[test]
fn stale_or_elsewhere_signals_do_not_bypass_rest_and_contact_admission_is_lazy() {
    let (transport, store, mut memory) = fixture();
    let location = road();
    for (remembered, at) in [
        (location.clone(), SimTime(9)),
        (Location::AtArea(AreaId(10)), SimTime(10)),
    ] {
        memory.signal = Some((remembered, Some(ChildhoodInterest::Company), 1, at));
        memory.stocks = BTreeMap::from([(InventoryId(2), food(2))]);
        memory.carried = BTreeSet::from([InventoryId(2)]);
        assert_eq!(
            guardian_rescue(
                &store,
                &transport,
                PersonId(1),
                PersonId(3),
                facts(&location),
                &memory,
                &NoBodies
            ),
            Some(RescueIntent::Rest)
        );
    }
    assert_eq!(
        guardian_rescue(
            &store,
            &transport,
            PersonId(1),
            PersonId(3),
            GuardianRescueFacts {
                co_present: false,
                ..facts(&location)
            },
            &memory,
            &NoBodies
        ),
        None
    );
}
#[test]
fn owner_rescue_of_an_overloaded_group_composes_into_a_real_shared_store_lift() {
    let (mut transport, mut store, memory) = fixture();
    let location = Location::AtArea(AreaId(10));
    let bodies = (1..=3)
        .map(|actor| {
            (
                PersonId(actor),
                game_transport::Body {
                    position: game_transport::Position::Area(AreaId(10)),
                    mass: Quantity(2_000),
                    cargo: Quantity(0),
                    capacity: Quantity(if actor == 1 { 3_000 } else { 1_000 }),
                    can_hold: true,
                    can_walk: true,
                    can_bear: true,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    game_transport::insert_group(
        &mut store,
        PersonId(3),
        game_transport::CarryGroup {
            passenger: PersonId(3),
            bearers: BTreeSet::from([PersonId(2)]),
            anchor: PersonId(2),
            authority: game_transport::Authority::UnableToConsent,
            accepted_at: SimTime(0),
        },
    )
    .unwrap();
    let selected = guardian_rescue(
        &store,
        &transport,
        PersonId(1),
        PersonId(3),
        GuardianRescueFacts {
            fatigue: 0,
            ..facts(&location)
        },
        &memory,
        &bodies,
    );
    assert_eq!(
        selected,
        Some(RescueIntent::Transport(Action::JoinCarry {
            passenger: PersonId(3)
        }))
    );
    transport
        .offer(&mut store, PersonId(1), PersonId(3), SimTime(10), &bodies)
        .unwrap();
    let bearers = transport.join_bearers(&store, &bodies, PersonId(3), SimTime(10));
    let lift = transport
        .begin_lift(
            &mut store,
            PersonId(3),
            bearers,
            game_transport::Permission {
                unable_to_consent: true,
                ..Default::default()
            },
            SimTime(10),
            &bodies,
        )
        .unwrap();
    let group = transport
        .complete_lift(&mut store, PersonId(3), lift.completes, &bodies)
        .unwrap();
    assert_eq!(group.bearers, BTreeSet::from([PersonId(1), PersonId(2)]));
    assert_eq!(game_transport::group(&store, PersonId(3)), Some(&group));
    assert!(game_transport::lifts(&store).next().is_none());
}
