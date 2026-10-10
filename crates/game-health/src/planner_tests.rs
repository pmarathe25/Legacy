use crate::*;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

fn stock(id: u64, good: Good, area: u64, quantity: u64, here: bool, held: bool) -> RememberedStock {
    RememberedStock {
        fact: StockKnowledge {
            inventory: InventoryId(id),
            area: AreaId(area),
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
fn routes() -> KnownRoutes {
    BTreeMap::from([
        (
            AreaId(1),
            BTreeMap::from([(AreaId(2), SimDuration(100)), (AreaId(3), SimDuration(300))]),
        ),
        (AreaId(2), BTreeMap::new()),
        (AreaId(3), BTreeMap::new()),
    ])
}
fn input<'a>(
    stocks: &'a [RememberedStock],
    springs: &'a [RememberedSpring],
    routes: &'a KnownRoutes,
    searched: &'a BTreeMap<AreaId, SimTime>,
    requests: &'a BTreeSet<InventoryId>,
) -> WaterPlanning<'a> {
    WaterPlanning {
        now: SimTime(1_000),
        area: AreaId(1),
        stationary: true,
        values: WaterValues {
            honesty: 1_000,
            risk_tolerance: 400,
        },
        stocks,
        springs,
        routes,
        searched,
        stock_requests: requests,
    }
}
#[test]
fn unsafe_local_water_uses_real_known_fuel_and_tool_or_carries_a_serving() {
    let mut world = World::default();
    world.clinical.insert(
        PersonId(1),
        Clinical {
            thirst: world.rules.drink_relief,
            ..Clinical::default()
        },
    );
    let stocks = vec![
        stock(10, Good::Water, 1, 5_000, true, false),
        stock(11, Good::Timber, 1, 1_000, true, false),
        stock(12, Good::Tool, 1, 1_000, true, false),
    ];
    let routes = routes();
    let searched = BTreeMap::new();
    let requests = BTreeSet::new();
    let facts = input(&stocks, &[], &routes, &searched, &requests);
    assert_eq!(
        world.plan_need(
            NeedFacts {
                actor: PersonId(1),
                drink_relief: world.rules.drink_relief,
                sheltered: false,
                can_enter_shelter: true
            },
            &facts
        ),
        Some(HealthIntent::Carry {
            source: InventoryId(10),
            quantity: Quantity(1_000)
        })
    );
}
#[test]
fn remote_known_stock_is_a_dated_route_lead_without_fabricating_current_water() {
    let stocks = vec![stock(10, Good::Water, 2, 1_000, false, false)];
    let routes = routes();
    let searched = BTreeMap::new();
    let requests = BTreeSet::new();
    assert_eq!(
        plan_water_procurement(
            &Rules::default(),
            &input(&stocks, &[], &routes, &searched, &requests)
        ),
        Some(HealthIntent::Move {
            destination: AreaId(2)
        })
    );
    let unknown = KnownRoutes::new();
    assert!(
        plan_water_procurement(
            &Rules::default(),
            &input(&[], &[], &unknown, &searched, &requests)
        )
        .is_none()
    );
}
#[test]
fn private_spring_requests_permission_and_only_a_risk_taking_actor_attempts_unauthorized_collection()
 {
    let springs = vec![RememberedSpring {
        id: ResourceId(7),
        area: AreaId(1),
        available: Quantity(1_000),
        accessible: false,
        contamination: 0,
        observed_at: SimTime(0),
        can_gather: true,
        can_request: true,
    }];
    let routes = routes();
    let searched = BTreeMap::new();
    let requests = BTreeSet::new();
    let mut facts = input(&[], &springs, &routes, &searched, &requests);
    assert_eq!(
        plan_spring(&facts, true),
        Some(HealthIntent::RequestResourceAccess {
            resource: ResourceId(7)
        })
    );
    facts.values = WaterValues {
        honesty: 100,
        risk_tolerance: 800,
    };
    assert_eq!(
        plan_spring(&facts, true),
        Some(HealthIntent::GatherWater {
            resource: ResourceId(7),
            quantity: Quantity(1_000),
            unauthorized: true
        })
    );
}
#[test]
fn actual_local_dry_searches_expire_but_reports_and_unknown_resources_add_no_search_history() {
    let routes = routes();
    let requests = BTreeSet::new();
    let mut searched = BTreeMap::from([(AreaId(2), SimTime(999)), (AreaId(3), SimTime(999))]);
    let rules = Rules {
        water_search_retry: SimDuration(100),
        ..Rules::default()
    };
    assert!(
        plan_water_procurement(&rules, &input(&[], &[], &routes, &searched, &requests)).is_none()
    );
    searched.insert(AreaId(2), SimTime(0));
    assert_eq!(
        plan_water_procurement(&rules, &input(&[], &[], &routes, &searched, &requests)),
        Some(HealthIntent::Move {
            destination: AreaId(2)
        })
    );
    assert_eq!(searched[&AreaId(3)], SimTime(999));
}
#[test]
fn grounded_care_can_be_refused_for_own_need_or_lack_of_known_supplies() {
    let stocks = vec![
        stock(20, Good::Herb, 1, 1_000, true, false),
        stock(21, Good::Water, 1, 1_000, true, false),
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
