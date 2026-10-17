mod support;
use game_characters::*;
use game_core::*;
use game_transport::Location;
use std::collections::BTreeSet;
use support::*;

fn prepared() -> Fixture {
    let mut f = setup();
    f.host
        .dispatch(Configure(Rules {
            rest_fatigue: u64::MAX,
            ..Rules::default()
        }))
        .unwrap();
    f.host
        .dispatch(game_lineage::Begin { person: f.ruler })
        .unwrap();
    f.host.dispatch(game_physiology::Start).unwrap();
    f
}

#[test]
fn a_missing_supplier_is_disproved_before_repeated_attempts_can_displace_local_foraging() {
    let mut f = prepared();
    f.host
        .dispatch(game_cargo::Consume {
            inventory: f.store,
            good: game_cargo::FOOD,
            quantity: 1000,
        })
        .unwrap();
    f.host
        .dispatch(game_cargo::Deposit {
            inventory: f.pack,
            good: game_cargo::FOOD,
            quantity: 100,
        })
        .unwrap();
    f.host
        .dispatch(game_property::Permit {
            issuer: f.ruler,
            person: f.other,
            inventory: f.pack,
            allow: true,
        })
        .unwrap();
    let bad = f.pack;
    observe(&mut f, bad);
    let own = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(f.other),
            capacity: 1000,
        })
        .unwrap();
    let patch = f
        .host
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area: f.home,
            good: game_cargo::FOOD,
            stock: 100,
            capacity: 100,
            renew: 0,
        }))
        .unwrap();
    f.host
        .dispatch(game_transport::Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    f.host
        .dispatch(Register {
            person: f.other,
            mind: Default::default(),
        })
        .unwrap();
    run(&mut f.host, SimTime(33 * MINUTE.0));
    f.host.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(42 * MINUTE.0));
    assert_eq!(f.host.get::<Minds>().unwrap().people[&f.other].failures, 1);
    assert_eq!(
        f.host.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock,
        90
    );
    assert_eq!(food(&f.host, bad), 100);
    assert_eq!(food(&f.host, own), 0);
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.other].hunger,
        2
    );
}
fn visit(f: &mut Fixture, destination: AreaId) {
    let arrives = f
        .host
        .dispatch(game_transport::Move {
            person: f.other,
            destination,
        })
        .unwrap();
    run(&mut f.host, arrives);
    f.host
        .dispatch(game_knowledge::ObserveSurroundings { person: f.other })
        .unwrap();
}
fn store(f: &mut Fixture, area: AreaId, quantity: u64) -> InventoryId {
    let inventory = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: 1000,
        })
        .unwrap();
    f.host
        .dispatch(game_property::ClaimInventory {
            claimant: f.other,
            inventory,
            recognized_by: BTreeSet::from([f.other]),
        })
        .unwrap();
    f.host
        .dispatch(game_cargo::Deposit {
            inventory,
            good: game_cargo::FOOD,
            quantity,
        })
        .unwrap();
    inventory
}
fn observe(f: &mut Fixture, inventory: InventoryId) {
    f.host
        .dispatch(game_knowledge::ObserveStore {
            person: f.other,
            inventory,
        })
        .unwrap();
}
fn start(f: &mut Fixture) {
    f.host
        .dispatch(Register {
            person: f.other,
            mind: Mind::default(),
        })
        .unwrap();
    run(&mut f.host, SimTime(31 * MINUTE.0));
    f.host.dispatch(Start).unwrap();
}
fn remote_meal() -> (Fixture, AreaId, InventoryId) {
    let mut f = prepared();
    let third = f
        .host
        .dispatch(game_land::CreateArea {
            name: "orchard".into(),
        })
        .unwrap();
    f.host
        .dispatch(game_land::Link {
            from: f.away,
            to: third,
            duration: SimDuration(2 * MINUTE.0),
        })
        .unwrap();
    let inventory = store(&mut f, third, 100);
    let away = f.away;
    let home = f.home;
    visit(&mut f, away);
    visit(&mut f, third);
    observe(&mut f, inventory);
    visit(&mut f, away);
    visit(&mut f, home);
    start(&mut f);
    (f, third, inventory)
}
#[test]
fn committed_meal_plan_crosses_two_roads_and_survives_saving_in_transit() {
    let (mut f, third, inventory) = remote_meal();
    run(&mut f.host, SimTime(33 * MINUTE.0));
    assert!(matches!(
        f.host.get::<game_transport::Positions>().unwrap().0[&f.other],
        Location::Transit { .. }
    ));
    assert_eq!(
        f.host.get::<Minds>().unwrap().people[&f.other].plan,
        Some(Plan::Meal {
            inventory,
            good: Some(game_cargo::FOOD)
        })
    );
    assert_eq!(food(&f.host, inventory), 100);
    let mut loaded = builder().load(&f.host.save().unwrap(), 4).unwrap();
    run(&mut f.host, SimTime(36 * MINUTE.0));
    for time in [33 * MINUTE.0 + 101, 34 * MINUTE.0, 36 * MINUTE.0] {
        run(&mut loaded, SimTime(time));
    }
    assert_eq!(f.host.save().unwrap(), loaded.save().unwrap());
    assert_eq!(
        f.host.get::<game_transport::Positions>().unwrap().0[&f.other],
        Location::At(third)
    );
    assert_eq!(food(&f.host, inventory), 90);
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.other].hunger,
        0
    );
    assert!(
        f.host.get::<Minds>().unwrap().people[&f.other]
            .plan
            .is_none()
    );
    assert!(
        !f.host
            .get::<game_skills::Skills>()
            .unwrap()
            .levels
            .contains_key(&f.other)
    );
}
#[test]
fn interrupted_travel_resumes_from_the_actual_road_position() {
    let (mut f, third, inventory) = remote_meal();
    run(&mut f.host, SimTime(33 * MINUTE.0));
    f.host
        .dispatch(game_transport::Cancel { person: f.other })
        .unwrap();
    assert!(
        matches!(f.host.get::<game_transport::Positions>().unwrap().0[&f.other], Location::Road { elapsed, .. } if elapsed == MINUTE)
    );
    run(&mut f.host, SimTime(35 * MINUTE.0));
    // Resume immediately, finish the remaining minute, then traverse the next two.
    assert_eq!(food(&f.host, inventory), 100);
    run(&mut f.host, SimTime(36 * MINUTE.0));
    assert_eq!(
        f.host.get::<game_transport::Positions>().unwrap().0[&f.other],
        Location::At(third)
    );
    assert_eq!(food(&f.host, inventory), 90);
}
#[test]
fn stale_meal_failure_revises_the_plan_toward_another_known_source() {
    let mut f = prepared();
    let home = f.home;
    let away = f.away;
    let first = store(&mut f, home, 100);
    let second = store(&mut f, away, 100);
    observe(&mut f, first);
    visit(&mut f, away);
    observe(&mut f, second);
    visit(&mut f, home);
    f.host
        .dispatch(game_cargo::Consume {
            inventory: first,
            good: game_cargo::FOOD,
            quantity: 100,
        })
        .unwrap();
    start(&mut f);
    run(&mut f.host, SimTime(32 * MINUTE.0));
    assert_eq!(f.host.get::<Minds>().unwrap().people[&f.other].failures, 1);
    run(&mut f.host, SimTime(35 * MINUTE.0));
    assert_eq!(food(&f.host, second), 90);
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.other].hunger,
        0
    );
    assert_eq!(f.host.get::<Minds>().unwrap().people[&f.other].failures, 1);
}
#[test]
fn discovering_a_closed_intermediate_road_abandons_the_plan_without_consuming_food() {
    let (mut f, third, inventory) = remote_meal();
    run(&mut f.host, SimTime(33 * MINUTE.0));
    f.host
        .dispatch(game_land::CloseRoad {
            from: f.away,
            to: third,
        })
        .unwrap();
    // The remote change does not update the citizen's knowledge before arrival.
    let before = game_knowledge::view(&f.host, f.other).unwrap();
    assert!(game_knowledge::route(&before, f.away, third).is_ok());
    run(&mut f.host, SimTime(34 * MINUTE.0));
    let mind = &f.host.get::<Minds>().unwrap().people[&f.other];
    assert!(mind.plan.is_none());
    assert_eq!(mind.failures, 1);
    assert!(
        game_knowledge::route(
            &game_knowledge::view(&f.host, f.other).unwrap(),
            f.away,
            third
        )
        .is_err()
    );
    assert_eq!(food(&f.host, inventory), 100);
}
#[test]
fn foraging_plan_travels_works_then_eats_from_its_real_carried_output() {
    let mut f = prepared();
    let pack = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(f.other),
            capacity: 100,
        })
        .unwrap();
    f.host
        .dispatch(game_property::ClaimInventory {
            claimant: f.other,
            inventory: pack,
            recognized_by: BTreeSet::from([f.other]),
        })
        .unwrap();
    observe(&mut f, pack);
    let patch = f
        .host
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area: f.away,
            good: game_cargo::FOOD,
            stock: 100,
            capacity: 100,
            renew: 0,
        }))
        .unwrap();
    let away = f.away;
    let home = f.home;
    visit(&mut f, away);
    visit(&mut f, home);
    start(&mut f);
    run(&mut f.host, SimTime(34 * MINUTE.0));
    assert_eq!(
        f.host.get::<game_sim::Actions>().unwrap().0[&f.other].kind,
        "gather"
    );
    assert_eq!(food(&f.host, pack), 0);
    assert_eq!(
        f.host.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock,
        100
    );
    run(&mut f.host, SimTime(39 * MINUTE.0));
    assert_eq!(
        f.host.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock,
        90
    );
    assert_eq!(
        food(&f.host, pack),
        0,
        "gathered portion was actually eaten"
    );
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.other].hunger,
        0
    );
    assert!(
        f.host.get::<Minds>().unwrap().people[&f.other]
            .plan
            .is_none()
    );
    assert_eq!(
        f.host.get::<game_skills::Skills>().unwrap().levels[&f.other]["forage"],
        5
    );
}
#[test]
fn urgent_hunger_interrupts_rest_and_death_clears_the_plan() {
    let mut f = setup();
    f.host
        .dispatch(game_lineage::Begin { person: f.ruler })
        .unwrap();
    f.host
        .dispatch(game_property::Permit {
            issuer: f.ruler,
            person: f.other,
            inventory: f.store,
            allow: true,
        })
        .unwrap();
    let inventory = f.store;
    observe(&mut f, inventory);
    f.host
        .dispatch(Register {
            person: f.other,
            mind: Mind::default(),
        })
        .unwrap();
    f.host.dispatch(game_physiology::Start).unwrap();
    run(&mut f.host, SimTime(99 * MINUTE.0));
    f.host
        .dispatch(game_physiology::Rest { person: f.other })
        .unwrap();
    f.host.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(100 * MINUTE.0));
    // The original rest was canceled; another decision can finish eating and
    // begin a new rest instead of waiting another review interval.
    let rest = &f.host.get::<game_sim::Actions>().unwrap().0[&f.other];
    assert_eq!(rest.kind, "rest");
    assert_eq!(rest.until, Some(SimTime(160 * MINUTE.0)));
    assert_eq!(food(&f.host, f.store), 990);
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.other].hunger,
        40
    );
    f.host
        .dispatch(game_physiology::Damage {
            person: f.other,
            amount: 100,
        })
        .unwrap();
    assert!(!f.host.get::<Minds>().unwrap().people.contains_key(&f.other));
}

#[test]
fn urgent_hunger_interrupts_authored_labor_without_refunding_consumed_inputs() {
    let mut f = prepared();
    f.host
        .dispatch(game_property::Permit {
            issuer: f.ruler,
            person: f.other,
            inventory: f.store,
            allow: true,
        })
        .unwrap();
    let inventory = f.store;
    observe(&mut f, inventory);
    f.host
        .dispatch(Register {
            person: f.other,
            mind: Mind::default(),
        })
        .unwrap();
    run(&mut f.host, SimTime(99 * MINUTE.0));
    f.host
        .dispatch(game_workshop::Define {
            name: "tool".into(),
            recipe: game_workshop::Recipe {
                inputs: std::collections::BTreeMap::from([(game_cargo::TIMBER, 5)]),
                output: GoodId(8),
                quantity: 1,
                duration: HOUR,
                skill: "craft".into(),
            },
        })
        .unwrap();
    f.host
        .dispatch(game_workshop::Craft {
            person: f.other,
            recipe: "tool".into(),
            inventory,
        })
        .unwrap();
    f.host.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(100 * MINUTE.0));
    assert_eq!(food(&f.host, inventory), 980);
    assert_eq!(
        f.host.get::<game_workshop::Workshop>().unwrap().waste[&game_cargo::TIMBER],
        5
    );
    let stock = &f.host.get::<game_cargo::Cargo>().unwrap().0[&inventory];
    assert_eq!(stock.amount(game_cargo::TIMBER), 95);
    assert_eq!(stock.amount(GoodId(8)), 0);
    assert!(stock.reserved.is_empty());
    assert!(
        f.host
            .get::<game_sim::Actions>()
            .unwrap()
            .0
            .get(&f.other)
            .is_none_or(|a| a.kind != "craft")
    );
    run(&mut f.host, SimTime(159 * MINUTE.0));
    assert_eq!(
        f.host.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(GoodId(8)),
        0
    );
    assert!(
        !f.host
            .get::<game_skills::Skills>()
            .unwrap()
            .levels
            .contains_key(&f.other)
    );
}
