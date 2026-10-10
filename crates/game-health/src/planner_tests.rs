use crate::*;
use game_core::*;
fn stock(id: u64, good: Good, quantity: u64, here: bool, held: bool) -> RememberedStock {
    RememberedStock {
        fact: StockKnowledge {
            inventory: InventoryId(id),
            area: AreaId(1),
            good,
            quantity: Quantity(quantity),
            accessible: true,
            observed_at: SimTime(0),
            learned_at: SimTime(0),
            source: KnowledgeSource::Observed(PersonId(1)),
        },
        here,
        held,
    }
}
fn planning(stocks: &[RememberedStock]) -> HealthPlanning<'_> {
    HealthPlanning {
        now: SimTime(1_000),
        area: AreaId(1),
        stationary: true,
        values: IntakeValues {
            risk_tolerance: 400,
        },
        stocks,
    }
}
fn thirsty() -> World {
    let mut world = World::default();
    world.clinical.insert(
        PersonId(1),
        Clinical {
            thirst: world.rules.drink_relief,
            ..Clinical::default()
        },
    );
    world
}
fn need(world: &World) -> NeedFacts {
    NeedFacts {
        actor: PersonId(1),
        drink_relief: world.rules.drink_relief,
        sheltered: false,
        can_enter_shelter: true,
    }
}
#[test]
fn missing_local_intake_declares_resource_need_without_choosing_a_source_route_or_collecting() {
    let world = thirsty();
    for stocks in [vec![], vec![stock(10, Good::Water, 1_000, false, false)]] {
        assert_eq!(
            world.plan_need(need(&world), &planning(&stocks)),
            Some(HealthIntent::Supply(SupplyNeed {
                good: Good::Water,
                quantity: Quantity(1_000),
                source: None,
                carried: false,
                consumer: HealthConsumer::Drink
            }))
        );
    }
}
#[test]
fn unsafe_area_water_declares_a_separated_serving_then_actual_held_water_can_be_prepared() {
    let world = thirsty();
    let mut stocks = vec![
        stock(10, Good::Water, 5_000, true, false),
        stock(11, Good::Timber, 1_000, true, false),
        stock(12, Good::Tool, 1_000, true, false),
    ];
    assert_eq!(
        world.plan_need(need(&world), &planning(&stocks)),
        Some(HealthIntent::Supply(SupplyNeed {
            good: Good::Water,
            quantity: Quantity(1_000),
            source: Some(InventoryId(10)),
            carried: true,
            consumer: HealthConsumer::Drink
        }))
    );
    assert_eq!(
        world.consume_supply(
            PersonId(1),
            InventoryId(10),
            &planning(&stocks),
            HealthConsumer::Drink
        ),
        None
    );
    stocks.push(stock(20, Good::Water, 1_000, true, true));
    assert_eq!(
        world.plan_need(need(&world), &planning(&stocks)),
        Some(HealthIntent::Action(HealthAction::Boil {
            source: InventoryId(20),
            fuel: InventoryId(11),
            tool: InventoryId(12)
        }))
    );
    assert_eq!(
        world.clinical[&PersonId(1)].thirst,
        world.rules.drink_relief
    );
}
#[test]
fn native_consumer_rejects_distant_or_insufficient_water_and_remains_a_real_timed_action() {
    let world = thirsty();
    for (quantity, here) in [(1_000, false), (999, true)] {
        let stocks = [stock(10, Good::Water, quantity, here, true)];
        assert_eq!(
            world.consume_supply(
                PersonId(1),
                InventoryId(10),
                &planning(&stocks),
                HealthConsumer::Drink
            ),
            None
        );
    }
    let stocks = [stock(10, Good::Water, 1_000, true, true)];
    assert_eq!(
        world.consume_supply(
            PersonId(1),
            InventoryId(10),
            &planning(&stocks),
            HealthConsumer::Drink
        ),
        Some(HealthAction::Drink {
            source: InventoryId(10)
        })
    );
    assert_eq!(
        world.clinical[&PersonId(1)].thirst,
        world.rules.drink_relief
    );
}
#[test]
fn known_safe_supply_avoids_purification_without_granting_access_or_changing_water() {
    let mut world = thirsty();
    world.known.entry(PersonId(1)).or_default().water.insert(
        InventoryId(10),
        WaterSafetyView {
            inventory: InventoryId(10),
            suspected_unsafe: false,
            purified: true,
            observed_at: SimTime(0),
        },
    );
    let stocks = [
        stock(10, Good::Water, 5_000, true, false),
        stock(11, Good::Timber, 1_000, true, false),
        stock(12, Good::Tool, 1_000, true, false),
    ];
    assert_eq!(
        world.plan_need(need(&world), &planning(&stocks)),
        Some(HealthIntent::Action(HealthAction::Drink {
            source: InventoryId(10)
        }))
    );
    assert_eq!(stocks[0].fact.quantity, Quantity(5_000));
}
#[test]
fn grounded_care_can_be_refused_for_own_need_or_lack_of_known_supplies() {
    let stocks = [
        stock(20, Good::Herb, 1_000, true, false),
        stock(21, Good::Water, 1_000, true, false),
    ];
    let facts = |hungry| WillingCare {
        capable: true,
        medicine: 1,
        busy: false,
        hungry,
        fatigued: false,
        urgent_dependent: false,
        empathy: 800,
        trust: 0,
        stocks: &stocks,
    };
    assert_eq!(
        willing_supplies(&Rules::default(), facts(false)),
        Some((InventoryId(20), InventoryId(21)))
    );
    assert!(willing_supplies(&Rules::default(), facts(true)).is_none());
}

#[test]
fn exposure_pressure_tracks_physical_thresholds_without_selecting_a_provider() {
    assert_eq!(exposure_urgency(0, 1_000), 0);
    assert_eq!(exposure_urgency(249, 1_000), 0);
    assert_eq!(exposure_urgency(250, 1_000), 3_000);
    assert_eq!(exposure_urgency(999, 1_000), 3_000);
    assert_eq!(exposure_urgency(1_000, 1_000), 20_000);
}
