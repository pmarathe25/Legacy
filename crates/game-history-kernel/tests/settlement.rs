use game_history_kernel::*;
fn pool(n: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .num_threads(n)
        .build()
        .unwrap()
}
fn run(world: &mut World, days: u64, pool: &rayon::ThreadPool) -> Vec<Receipt> {
    let until = world.now + days * DAY;
    let mut events = vec![];
    while world.now < until {
        events.extend(world.step(pool).unwrap());
    }
    events
}
#[test]
fn integrated_history_replays_across_workers_and_midpoint_checkpoint() {
    let original = World::fixture(17, 32, 2).unwrap();
    let mut one = original.clone();
    let mut four = original.clone();
    let mut reload = original;
    let first = run(&mut one, 4, &pool(1));
    let second = run(&mut four, 4, &pool(4));
    let mut third = run(&mut reload, 2, &pool(4));
    reload = World::load(&reload.save().unwrap()).unwrap();
    third.extend(run(&mut reload, 2, &pool(1)));
    assert_eq!(first, second);
    assert_eq!(first, third);
    assert_eq!(one.save().unwrap(), four.save().unwrap());
    assert_eq!(one.save().unwrap(), reload.save().unwrap());
    for kind in ["meal", "harvest", "rested", "encounter"] {
        assert!(
            one.totals.get(kind).copied().unwrap_or(0) > 0,
            "missing {kind}"
        );
    }
    for stock in one.economy().settlements() {
        assert_eq!(
            stock.food + stock.consumed + stock.spoiled,
            170 + stock.produced
        );
    }
}
#[test]
fn starvation_and_control_are_exact_and_replacement_actions_are_not_restored() {
    let mut w = World::fixture(3, 2, 1).unwrap();
    let person = w.population().people()[0].id;
    w.control(person, true).unwrap();
    let now = w.now;
    let initial = w.population().get(person).unwrap().hunger;
    let death = now + 1440 - initial + 6000;
    while w.now < death {
        w.step(&pool(1)).unwrap();
    }
    assert_eq!(w.now, death);
    let p = w.population().get(person).unwrap();
    assert!(!p.alive);
    assert_eq!(p.activity, Activity::Idle);
    let other = 2;
    w.injure(other, 1).unwrap();
    assert_eq!(w.population().get(other).unwrap().activity, Activity::Idle);
}
#[test]
fn care_and_stock_matching_are_exclusive_and_children_keep_identity() {
    let w = World::fixture(7, 3, 1).unwrap();
    let now = w.now;
    let mut people = w.population().people().to_vec();
    people[0].born = now - DAY;
    people[0].dependent = true;
    people[0].hunger = 480;
    people[1].born = now - DAY;
    people[1].dependent = true;
    people[1].hunger = 600;
    people[2].hunger = 0;
    let economy = Economy::new(vec![Settlement {
        id: 1,
        name: "Home".into(),
        food: 1,
        capacity: 100,
        produced: 0,
        consumed: 0,
        spoiled: 0,
    }])
    .unwrap();
    let mut w = World::new(
        7,
        now,
        Population::new(people).unwrap(),
        economy,
        w.society().clone(),
    )
    .unwrap();
    let events = w.step(&pool(4)).unwrap();
    let care: Vec<_> = events.iter().filter(|e| e.kind == "care").collect();
    assert_eq!(care.len(), 1);
    assert_eq!(care[0].people, vec![3, 2]);
    assert_eq!(w.economy().get(1).unwrap().consumed, 1);
    assert_eq!(w.population().get(2).unwrap().id, 2);
}
#[test]
fn rejected_boundary_keeps_all_owner_state_and_receipt_sequence() {
    let w = World::fixture(1, 8, 2).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&w.save().unwrap()).unwrap();
    json["sequence"] = serde_json::json!(u64::MAX);
    let mut w = World::load(&json.to_string()).unwrap();
    let before = w.save().unwrap();
    assert!(w.step(&pool(4)).is_err());
    assert_eq!(before, w.save().unwrap());
}
